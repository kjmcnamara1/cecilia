//! Application state machine: buffers, panes, tabs, modes and key handling.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use cecilia_core::{Beat, Clef, Note, Score, Staff, StaffClef, Track, TrackKind, staff};

use crate::command::Command;
use crate::config::{Config, Lookup, Mode};
use crate::keys::{Key, KeyCode};
use crate::layout::{Axis, Node, PaneId, Rect, neighbour};

/// An open score.
#[derive(Debug, Clone)]
pub struct Buffer {
    pub path: Option<PathBuf>,
    pub name: String,
    pub score: Score,
    /// Changed since it was opened or last written.
    pub modified: bool,
    pub history: crate::history::History,
    /// Incremented on every change, so views know to re-layout.
    pub version: u64,
    /// The file's size and modification time when this buffer last read or wrote it (none: the
    /// file did not exist), to notice changes made outside the app (see `external.rs`).
    pub disk: Option<(SystemTime, u64)>,
}

/// What a score pane displays, reported by the frontend for half-page movement.
#[derive(Debug, Clone, Default)]
pub struct ViewInfo {
    /// All bars flow along one line (the horizontal layout): `visible_bars` counts.
    pub horizontal: bool,
    /// The bars of every row (system) of the cursor's track, in order.
    pub rows: Vec<std::ops::Range<usize>>,
    /// Rows at least partly on screen.
    pub visible_rows: usize,
    /// Bars at least partly on screen.
    pub visible_bars: usize,
    /// In the page layout, the most rows on one page: half-page movement counts these, not
    /// the rows on screen, so it does not depend on the zoom. Zero in the other layouts.
    pub page_rows: usize,
}

/// A position in a score: track, bar, beat index (voice 1) and string.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Cursor {
    pub track: usize,
    pub bar: usize,
    pub beat: usize,
    pub string: usize,
    /// Voice being edited (0 = voice 1).
    pub voice: usize,
    /// Which of the track's staves the cursor is on (0 = top); [`Cursor::HOME`] (or any staff
    /// the track does not have) means its home staff ([`home_staff`]).
    pub staff: usize,
    /// Position on a staff of lines: a written diatonic number on a standard or slash staff,
    /// a GABC position on chant. On a tab (or drum lanes) the cursor is at `string` instead.
    pub line: i32,
}

impl Cursor {
    /// The track's home staff: its tab if it has one, else its top staff.
    pub const HOME: usize = usize::MAX;
}

impl Default for Cursor {
    fn default() -> Self {
        Self {
            track: 0,
            bar: 0,
            beat: 0,
            string: 0,
            voice: 0,
            staff: Cursor::HOME,
            line: 0,
        }
    }
}

/// A note of a beat, and the staff the cursor is on it from ([`App::linked_note`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LinkedNote {
    pub track: usize,
    pub bar: usize,
    pub voice: usize,
    pub beat: usize,
    pub string: u8,
    pub staff: usize,
}

/// The staff the cursor is on: `staff` if the track has it, else its home staff (its first
/// tab, or its top staff).
pub fn staff_index(track: &Track, staff: usize) -> usize {
    let staves = track.staves();
    if staff < staves.len() {
        staff
    } else {
        staves
            .iter()
            .position(|&s| uses_strings(track, s))
            .unwrap_or(0)
    }
}

/// A place the cursor can sit on a track: a staff and a position on its lines, or a staff and
/// one of its strings (or drum lanes).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Row {
    Line(usize, i32),
    String(usize, usize),
}

/// What the command palette shows (see [`App::palette`]).
#[derive(Debug, Clone, PartialEq)]
pub struct Palette {
    /// Shown before the input: ":" for commands, "buffer ›" etc. for pickers.
    pub prompt: String,
    /// Title of the box.
    pub title: String,
    pub input: String,
    pub items: Vec<PaletteItem>,
    pub selected: Option<usize>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PaletteItem {
    pub text: String,
    /// Short description (of a command, or a picker entry).
    pub description: Option<String>,
    /// A color shown beside the text (the color picker).
    pub swatch: Option<crate::theme::Rgba>,
    /// An icon shown beside the text.
    pub icon: Option<crate::icons::Icon>,
    /// Character positions that matched the typed text (highlighted).
    pub matched: Vec<usize>,
}

#[derive(Debug, Clone)]
pub struct Pane {
    /// What the pane shows: a score, or a tool working on the view's score pane.
    pub kind: PaneKind,
    /// A tool pane's own height and width (viewport units), kept whatever other panes come
    /// and go; score panes share the room left.
    pub size: (Option<u16>, Option<u16>),
    /// The size was set by hand: it is no longer fitted to the contents (`ui.auto_resize`).
    pub manual_size: bool,
    /// A local tool pane's score pane: the one it opened under and always shows. Global
    /// tool panes (and score panes) have none and follow the view's score pane.
    pub owner: Option<PaneId>,
    pub buffer: usize,
    pub cursor: Cursor,
    /// First visible row, maintained by the frontend.
    pub scroll: usize,
    /// GUI score layout of this pane: "page", "vertical" or "horizontal".
    pub layout: String,
    /// GUI: show the audio waveform strip at the bottom of this pane.
    pub waveform: bool,
    /// GUI zoom of this pane's score (1.0 = 100%).
    pub zoom: f64,
    /// Buffers shown in this pane before, most recent first, with their cursors (switching
    /// back restores the cursor; closing a buffer falls back to the most recent one).
    pub recent: Vec<(usize, Cursor)>,
    /// Where the cursor was before the last jump (`''`).
    pub jump_back: Option<(usize, Cursor)>,
    /// Where `zz` / `zt` / `zb` asked the cursor to sit in the view, until the frontend that
    /// owns the scroll position has applied it (`App::take_scroll_request`).
    pub scroll_request: Option<ScrollTo>,
    /// A waveform pane's pending zoom (`:zoom-in`, `:zoom-out`; 0 resets it), until the
    /// frontend that owns its view has applied it ([`App::take_wave_zoom`]).
    pub wave_zoom: Option<f64>,
}

/// Where in a score pane's view the cursor is placed (`zz`, `zt`, `zb`): the middle, the top
/// (the left in the horizontal layout) or the bottom (the right).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScrollTo {
    Center,
    Top,
    Bottom,
}

/// What a pane shows. Tool panes (all but `Score`) work on their view's score pane: the score
/// pane focused last.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Hash)]
pub enum PaneKind {
    #[default]
    Score,
    Tracks,
    /// The audio track's waveform.
    Audio,
    Piano,
    Fretboard,
    Tuner,
    /// The score's, the cursor track's and the lyrics' properties, docked along the right.
    Properties,
    /// The editing palette (durations, rests, effects …), docked along the left.
    Palette,
}

impl PaneKind {
    pub const TOOLS: [PaneKind; 7] = [
        PaneKind::Tracks,
        PaneKind::Audio,
        PaneKind::Piano,
        PaneKind::Fretboard,
        PaneKind::Tuner,
        PaneKind::Properties,
        PaneKind::Palette,
    ];

    pub fn name(self) -> &'static str {
        match self {
            PaneKind::Score => "score",
            PaneKind::Tracks => "tracks",
            PaneKind::Audio => "waveform",
            PaneKind::Piano => "piano",
            PaneKind::Fretboard => "fretboard",
            PaneKind::Tuner => "tuner",
            PaneKind::Properties => "properties",
            PaneKind::Palette => "palette",
        }
    }

    pub fn parse(s: &str) -> Option<PaneKind> {
        Some(match s {
            "score" => PaneKind::Score,
            "tracks" => PaneKind::Tracks,
            "waveform" => PaneKind::Audio,
            "piano" => PaneKind::Piano,
            "fretboard" => PaneKind::Fretboard,
            "tuner" => PaneKind::Tuner,
            "properties" => PaneKind::Properties,
            "palette" => PaneKind::Palette,
            _ => return None,
        })
    }
}

/// Adds a pane along the bottom of a layout (a tool docked at the window's bottom).
/// `Ave Maria (2nd)` → `ave-maria-2nd`: lowercase words joined by single dashes.
fn kebab_case(s: &str) -> String {
    s.split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .map(str::to_lowercase)
        .collect::<Vec<_>>()
        .join("-")
}

fn dock_bottom(root: Node, id: PaneId) -> Node {
    match root {
        Node::Split {
            axis: Axis::Stacked,
            mut children,
            mut weights,
        } => {
            children.push(Node::Leaf(id));
            weights.push(1.0);
            Node::Split {
                axis: Axis::Stacked,
                children,
                weights,
            }
        }
        other => Node::Split {
            axis: Axis::Stacked,
            children: vec![other, Node::Leaf(id)],
            weights: vec![1.0, 1.0],
        },
    }
}

/// Takes the leaf of a pane out of a root split along the right (what remains is the first
/// result); a root without it is returned as it is.
fn split_off_leaf(root: Node, id: Option<PaneId>) -> (Node, Option<Node>) {
    let Some(id) = id else {
        return (root, None);
    };
    match root {
        Node::Split {
            axis: Axis::SideBySide,
            mut children,
            ..
        } if children.contains(&Node::Leaf(id)) => {
            let at = children.iter().position(|c| *c == Node::Leaf(id)).unwrap();
            let leaf = children.remove(at);
            let main = if children.len() == 1 {
                children.remove(0)
            } else {
                Node::Split {
                    axis: Axis::SideBySide,
                    weights: vec![1.0; children.len()],
                    children,
                }
            };
            (main, Some(leaf))
        }
        root => (root, None),
    }
}

/// Puts a pane along the left of everything but the properties pane: the bottom tools sit
/// inside it, as wide as the scores.
fn dock_left(main: Node, id: PaneId) -> Node {
    Node::Split {
        axis: Axis::SideBySide,
        children: vec![Node::Leaf(id), main],
        weights: vec![1.0, 1.0],
    }
}

/// Takes the tracks pane off the bottom of a root stack (what remains is the first result); a
/// root without it is returned as it is.
fn split_off_tracks(root: Node, id: Option<PaneId>) -> (Node, Option<Node>) {
    let Some(id) = id else {
        return (root, None);
    };
    match root {
        Node::Split {
            axis: Axis::Stacked,
            mut children,
            mut weights,
        } if children.last() == Some(&Node::Leaf(id)) => {
            let leaf = children.pop();
            weights.pop();
            let main = if children.len() == 1 {
                children.remove(0)
            } else {
                Node::Split {
                    axis: Axis::Stacked,
                    children,
                    weights,
                }
            };
            (main, leaf)
        }
        root => (root, None),
    }
}

/// Puts the tracks pane along the bottom of everything, at the full width of the window.
fn with_tracks(main: Node, tracks: Option<Node>) -> Node {
    match tracks {
        Some(Node::Leaf(id)) => dock_bottom(main, id),
        _ => main,
    }
}

/// Puts a bottom tool under the scores: inside the palette along the left, if there is one.
fn dock_tool(main: Node, id: PaneId, palette: Option<PaneId>) -> Node {
    match (main, palette) {
        (
            Node::Split {
                axis: Axis::SideBySide,
                mut children,
                weights,
            },
            Some(p),
        ) if children.first() == Some(&Node::Leaf(p)) && children.len() == 2 => {
            let body = children.remove(1);
            children.push(dock_bottom(body, id));
            Node::Split {
                axis: Axis::SideBySide,
                children,
                weights,
            }
        }
        (main, _) => dock_bottom(main, id),
    }
}

/// A layout of panes with its own set of loaded buffers (the bufferline's tabs).
#[derive(Debug, Clone)]
pub struct View {
    pub root: Node,
    /// The focused pane (a score or a tool).
    pub focus: PaneId,
    /// The score pane focused last: the one tool panes and editing commands work on.
    pub score: PaneId,
    /// Buffers loaded in this view, in bufferline order.
    pub buffers: Vec<usize>,
    pub name: Option<String>,
    /// A pane shown alone, filling the view (`:maximize`); the others come back when it is
    /// toggled off or another pane is focused.
    pub maximized: Option<PaneId>,
    /// The view shows the dashboard instead of its panes (it has no files).
    pub dashboard: bool,
}

/// One tab of the bufferline (see [`App::bufferline`]).
#[derive(Debug, Clone, PartialEq)]
pub struct BufferTab {
    pub buffer: usize,
    pub name: String,
    /// File type, from the extension: "guitar-pro", "musicxml", "midi", "chant", "cecilia" or "new".
    pub kind: &'static str,
    pub modified: bool,
    /// Shown in the focused pane.
    pub active: bool,
    /// This file is the one playing (only one plays at a time)…
    pub playing: bool,
    /// …or is paused.
    pub paused: bool,
}

/// Extension of cecilia's native format for a score: `ccc` for chant, `ccm` for modern.
pub fn native_extension(score: &Score) -> &'static str {
    if score.is_chant() { "ccc" } else { "ccm" }
}

/// File type of a buffer, for its bufferline icon.
pub fn file_kind(path: Option<&Path>) -> &'static str {
    let ext = path
        .and_then(|p| p.extension())
        .and_then(|e| e.to_str())
        .map(str::to_ascii_lowercase)
        .unwrap_or_default();
    match ext.as_str() {
        "gp3" | "gp4" | "gp5" | "gpx" | "gp" => "guitar-pro",
        "musicxml" | "xml" | "mxl" => "musicxml",
        "mid" | "midi" => "midi",
        "gabc" => "chant",
        "cc" | "ccc" | "ccm" => "cecilia",
        "" => "new",
        _ => "other",
    }
}

/// A name shortened to `max` characters by dropping the middle: the first `prefix` and last
/// `suffix` characters are kept around an ellipsis.
pub fn truncate_middle(name: &str, max: usize, prefix: usize, suffix: usize) -> String {
    let chars: Vec<char> = name.chars().collect();
    if chars.len() <= max.max(1) {
        return name.to_string();
    }
    // Keep within `max` even if the prefix and suffix settings add up to more.
    let budget = max.saturating_sub(1);
    let prefix = prefix.min(budget);
    let suffix = suffix.min(budget - prefix);
    let mut out: String = chars[..prefix].iter().collect();
    out.push('…');
    out.extend(&chars[chars.len() - suffix..]);
    out
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Message {
    pub text: String,
    pub error: bool,
}

/// What a save prompt is for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Closing {
    Buffer(usize),
    View,
    Quit,
}

/// A pending "save changes?" prompt: the buffer asked about, and those already answered `n`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Confirm {
    pub action: Closing,
    pub asking: usize,
    pub skip: Vec<usize>,
}

/// Requests for the frontend, drained with [`App::take_effects`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Effect {
    /// Start playback of a buffer from a bar/tick position.
    Play {
        buffer: usize,
        bar: usize,
        tick: u32,
    },
    Pause,
    /// Move the play position (playing or paused) without restarting playback.
    Seek {
        buffer: usize,
        bar: usize,
        tick: u32,
    },
    /// Continue paused playback from where it stopped.
    Resume,
    /// Pause and rewind to where playback started.
    Stop {
        buffer: usize,
        bar: usize,
        tick: u32,
    },
    /// The SoundFont setting changed: the player is recreated on next use.
    ConfigReloaded,
    /// A buffer's score was edited.
    ScoreChanged {
        buffer: usize,
    },
    /// A buffer's audio track was attached, detached or changed.
    AudioChanged {
        buffer: usize,
    },
    /// Show the system file picker (starting in `dir`) and open the files chosen.
    OpenDialog {
        dir: Option<PathBuf>,
    },
    /// Show the system save dialog (starting in `dir`, suggesting `name`) and save the buffer
    /// under the path chosen (see `App::save_as`).
    SaveDialog {
        dir: Option<PathBuf>,
        name: String,
    },
    /// Start (true) or stop listening for the tuner.
    Tuner(bool),
    /// Mixer, loop, metronome, speed or pitch settings changed.
    PlaybackSettings,
    /// Record a sync point for the next bar at the current audio position.
    SyncTap,
    /// Generate sync points from beat detection.
    SyncDetect,
    /// Sync the chant note under the cursor at the playhead.
    SyncHere,
    /// The mouse went down on the waveform in Sync mode (`App::wave_press`): the cell under the
    /// cursor gets a point there (snapped to the amplitude) unless one is close, which is then
    /// grabbed; the audio plays from there until the button is released.
    WavePress,
}

pub const DEFAULT_TRACKS_ROWS: usize = 8;
pub const DEFAULT_WAVEFORM_HEIGHT: u16 = 64;
pub const MIN_WAVEFORM_HEIGHT: u16 = 24;
pub const MAX_WAVEFORM_HEIGHT: u16 = 600;

pub struct App {
    pub config: Config,
    pub config_path: Option<PathBuf>,
    config_mtime: Option<SystemTime>,
    /// When the theme file (`[theme] file`) last changed: edits to it apply live too.
    theme_mtime: Option<SystemTime>,
    pub buffers: Vec<Buffer>,
    pub panes: HashMap<PaneId, Pane>,
    pub(crate) next_pane: PaneId,
    pub views: Vec<View>,
    pub active_view: usize,
    pub mode: Mode,
    /// A close or quit waiting on the answer to "save changes?" (`y`, `n` or anything else to
    /// cancel); it takes the next key.
    pub confirm_close: Option<Confirm>,
    /// Keys typed so far of a multi-key binding.
    pub pending: Vec<Key>,
    pub count: Option<usize>,
    pub cmdline: String,
    /// The command line was opened from Visual mode: a line run with Enter finds the
    /// selection again, so commands apply to it.
    cmd_from_visual: bool,
    /// Syncing audio (`:sync`): Sync mode is the mode while nothing else (Visual, the command
    /// line) is, and the waveform shows the sync points and rulers.
    pub syncing: bool,
    /// The sync point being dragged along the waveform (a chant cell's id).
    pub(crate) wave_grab: Option<u32>,
    /// A press on the waveform waiting for the player: buffer, audio seconds, tolerance.
    pub(crate) wave_press: Option<(usize, f64, f64)>,
    /// The line was begun by a palette tool that asks for an argument: `.` repeats the tool.
    pub(crate) cmd_from_palette: bool,
    history: Vec<String>,
    history_pos: Option<usize>,
    /// Bookmarks (`m{a-z}`, `m{A-Z}`).
    pub marks: crate::marks::Marks,
    /// The open picker (Picker mode).
    pub(crate) picker: Option<crate::picker::Picker>,
    /// Command-line completion in progress: candidates, where they start and which is shown.
    completion: Option<(crate::complete::Completions, Option<usize>)>,
    pub message: Option<Message>,
    pub(crate) swap: crate::swap::SwapState,
    pub show_help: bool,
    /// Set by the frontend to reflect the player state.
    pub playing: bool,
    /// Buffer being played, set when playback starts.
    pub playing_buffer: Option<usize>,
    play_start: Option<(PaneId, Cursor)>,
    /// Playback is paused (Play mode stays on until stopped).
    pub paused: bool,
    /// Where the cursor was when playback paused: resuming carries on only if it's still there.
    pub(crate) paused_at: Option<(PaneId, usize, Cursor)>,
    /// When the cursor last moved the play position: playhead updates from just before the
    /// player caught up are ignored, so the cursor doesn't bounce back.
    seeked: Option<std::time::Instant>,
    /// A mouse press on a score, which a drag turns into a Visual selection.
    pub(crate) mouse_sel: Option<crate::mouse::MouseSel>,
    /// The toolbar is shown (`ui.toolbar`, `:toolbar`).
    pub toolbar: bool,
    /// Design mode: the page layout can be adjusted by hand (`:design`).
    pub design: bool,
    /// The design as it was when a mouse drag began (the drag previews without undo steps).
    pub(crate) design_drag: Option<cecilia_core::design::DesignOverrides>,
    /// The tuner's latest reading while a tuner pane is shown (set by the frontend).
    pub tuner_reading: Option<cecilia_audio::tuner::Reading>,
    /// Why the tuner can't listen, if it can't.
    pub tuner_error: Option<String>,
    /// The tracks panel's selected row (an index into [`App::track_rows`]), while it has the
    /// focus.
    pub tracks_selected: usize,
    /// The properties pane's tab, selection and fold state.
    pub props: crate::properties::PropsState,
    /// The editing palette's selection and fold state.
    pub palette: crate::palette::PaletteState,
    /// Height of a row of text in viewport units (1 in a terminal, pixels in the GUI), for
    /// sizing new tool panes.
    pub row_height: u16,
    /// Until when the tracks show for a moment after the track changed, while no tracks pane
    /// is shown (`ui.tracks_peek`).
    tracks_peek_until: Option<std::time::Instant>,
    /// Width of a column of text in viewport units (1 in a terminal, pixels in the GUI).
    pub col_width: u16,
    /// What a pane's frame adds to its height and width (viewport units).
    pub pane_frame: u16,
    /// Tool panes added before the frontend first set the viewport: sized again then.
    unfitted: Vec<PaneId>,
    viewport_known: bool,
    /// A track being renamed in place in the tracks pane: its index and the name typed so far.
    pub renaming: Option<(usize, String)>,
    /// Height of the GUI's waveform strips and bar in logical pixels (`:waveform-resize`).
    pub waveform_height: u16,
    /// When each pane was last focused (`focus_clock` ticks), for directional focus.
    focus_recency: HashMap<PaneId, u64>,
    focus_clock: u64,
    /// Screen area for panes, set by the frontend.
    pub viewport: Rect,
    /// Bars moved by page-down/up when a pane's [`ViewInfo`] is not known yet.
    pub page_bars: usize,
    /// What each score pane displays, as last reported by the frontend (`set_view_info`).
    pub(crate) view_info: HashMap<PaneId, ViewInfo>,
    pub(crate) effects: Vec<Effect>,
    pub quit: bool,
    /// The dashboard's focused button.
    pub(crate) dashboard_focus: usize,
    /// Files closed lately, the most recent first (the dashboard's Open Recent).
    pub(crate) recent: Vec<PathBuf>,
    /// Layout for panes opened without another to copy from (`ui.layout`); each pane keeps
    /// its own (`Pane::layout`).
    pub score_layout: String,
    pub(crate) ed: crate::editor::EditorState,
    /// Play-along settings.
    pub pa: crate::playalong::PlayAlong,
    /// The active workspace file (none: no workspace; `:workspace-save NAME` starts one).
    pub workspace_file: Option<PathBuf>,
    /// A pending "delete this workspace?" prompt.
    pub confirm_delete_workspace: Option<PathBuf>,
    /// A pending "file changed on disk: reload?" prompt, for this buffer.
    pub confirm_reload: Option<usize>,
    /// When the open files were last looked at for outside changes.
    pub(crate) disk_checked: Option<std::time::Instant>,
    /// Record the active workspace for the next start (`cecilia` without arguments reopens it).
    /// Off in tests and screenshots.
    pub remember_workspace: bool,
}

impl App {
    pub fn new(config: Config, config_path: Option<PathBuf>) -> Self {
        let toolbar = config.file.ui.toolbar;
        let (score_layout, zoom) = (config.file.ui.layout.clone(), config.file.ui.zoom);
        let config_mtime = config_path.as_deref().and_then(mtime);
        let theme_file = config.file.theme.file.as_str();
        let theme_mtime = (!theme_file.is_empty())
            .then(|| crate::theme::theme_path(std::path::Path::new(theme_file)))
            .and_then(|p| mtime(&p));
        let mut panes = HashMap::new();
        panes.insert(
            1,
            Pane {
                kind: PaneKind::Score,
                size: (None, None),
                manual_size: false,
                owner: None,
                buffer: 0,
                cursor: Cursor::default(),
                scroll: 0,
                layout: score_layout.clone(),
                waveform: true,
                zoom,
                recent: Vec::new(),
                jump_back: None,
                scroll_request: None,
                wave_zoom: None,
            },
        );
        let mut app = Self {
            config,
            config_path,
            config_mtime,
            theme_mtime,
            buffers: vec![Buffer::empty()],
            panes,
            next_pane: 2,
            views: vec![View {
                maximized: None,
                dashboard: false,
                root: Node::Leaf(1),
                focus: 1,
                score: 1,
                buffers: vec![0],
                name: None,
            }],
            active_view: 0,
            mode: Mode::Normal,
            pending: Vec::new(),
            count: None,
            cmdline: String::new(),
            cmd_from_visual: false,
            syncing: false,
            wave_grab: None,
            wave_press: None,
            cmd_from_palette: false,
            history: Vec::new(),
            history_pos: None,
            completion: None,
            picker: None,
            marks: Default::default(),
            message: None,
            confirm_close: None,
            swap: Default::default(),
            show_help: false,
            playing: false,
            playing_buffer: None,
            play_start: None,
            paused: false,
            paused_at: None,
            seeked: None,
            mouse_sel: None,
            toolbar,
            design: false,
            design_drag: None,
            tuner_reading: None,
            tuner_error: None,
            tracks_selected: 0,
            props: Default::default(),
            palette: Default::default(),
            renaming: None,
            row_height: 1,
            col_width: 1,
            pane_frame: 2,
            tracks_peek_until: None,
            unfitted: Vec::new(),
            viewport_known: false,
            waveform_height: DEFAULT_WAVEFORM_HEIGHT,
            focus_recency: HashMap::new(),
            focus_clock: 0,
            viewport: Rect {
                x: 0,
                y: 0,
                w: 80,
                h: 24,
            },
            page_bars: 4,
            view_info: HashMap::new(),
            effects: Vec::new(),
            quit: false,
            dashboard_focus: 0,
            recent: Vec::new(),
            score_layout,
            ed: Default::default(),
            pa: Default::default(),
            workspace_file: None,
            confirm_delete_workspace: None,
            confirm_reload: None,
            disk_checked: None,
            remember_workspace: false,
        };
        // Tool panes the config shows from the start.
        let ui = &app.config.file.ui;
        let tools = [
            (PaneKind::Tracks, ui.tracks_panel),
            (PaneKind::Fretboard, ui.fretboard),
            (PaneKind::Piano, ui.piano),
            (PaneKind::Properties, ui.properties),
            (PaneKind::Palette, ui.palette),
        ];
        for (kind, on) in tools {
            if on {
                app.set_tool(kind, Some(true));
            }
        }
        app.set_focus(1);
        app
    }

    pub fn take_effects(&mut self) -> Vec<Effect> {
        std::mem::take(&mut self.effects)
    }

    pub fn view(&self) -> &View {
        &self.views[self.active_view]
    }

    /// The view's score pane: the one editing, playback and the tool panes work on.
    pub fn focused(&self) -> PaneId {
        self.view().score
    }

    /// The focused pane, a score or a tool.
    pub fn window_focus(&self) -> PaneId {
        self.view().focus
    }

    /// What the focused pane shows.
    pub fn focused_kind(&self) -> PaneKind {
        self.panes
            .get(&self.window_focus())
            .map_or(PaneKind::Score, |p| p.kind)
    }

    /// Marks a pane as the most recently focused, for directional focus to prefer.
    fn touch_focus(&mut self, id: PaneId) {
        self.focus_clock += 1;
        self.focus_recency.insert(id, self.focus_clock);
    }

    /// Focuses a pane of the active view; a score pane becomes the one tools work on.
    /// A local tool pane makes its own score pane the view's.
    pub fn set_focus(&mut self, id: PaneId) {
        let (kind, owner) = self
            .panes
            .get(&id)
            .map_or((None, None), |p| (Some(p.kind), p.owner));
        let was_props = self.focused_kind() == PaneKind::Properties;
        let was_score = self.focused_kind() == PaneKind::Score;
        self.touch_focus(id);
        let v = &mut self.views[self.active_view];
        v.focus = id;
        if kind == Some(PaneKind::Score) {
            v.score = id;
        } else if let Some(o) = owner {
            v.score = o;
        }
        // Leaving the properties pane leaves its lyrics text area (keeping its caret); coming
        // back to the text area takes it up again where it was.
        let now_props = kind == Some(PaneKind::Properties);
        if was_props && !now_props && self.props.edit.as_ref().is_some_and(|e| e.modal) {
            self.end_edit(false);
        } else if now_props && !was_props {
            self.enter_text_area();
            if was_score && self.config.file.ui.lyrics_select_cursor {
                self.select_cursor_in_text();
            }
        }
    }

    /// The score pane a tool pane shows: its own for a local tool, else the view's.
    pub fn tool_score(&self, id: PaneId) -> PaneId {
        self.panes
            .get(&id)
            .and_then(|p| p.owner)
            .filter(|o| self.panes.contains_key(o))
            .unwrap_or_else(|| self.focused())
    }

    /// The pane of a tool kind that applies here, if shown: the view's global one, or (for a
    /// local tool) the one under the view's score pane.
    pub fn tool_pane(&self, kind: PaneKind) -> Option<PaneId> {
        let owner = (!self.is_global(kind)).then(|| self.focused());
        self.view().root.panes().into_iter().find(|id| {
            self.panes
                .get(id)
                .is_some_and(|p| p.kind == kind && p.owner == owner)
        })
    }

    /// A tool pane's natural height and width (viewport units): what its contents need, in
    /// rows of `row_height` and columns of `col_width` (set by the frontend), frame included.
    fn natural_size(&self, kind: PaneKind) -> (u16, u16) {
        let strings = self
            .buffer()
            .score
            .tracks
            .get(self.pane().cursor.track)
            .map_or(6, |t| t.strings.len().max(1));
        let (rows, cols) = match kind {
            PaneKind::Tracks => (self.track_rows().len() + 1, 72),
            PaneKind::Fretboard => (strings + 1, 60),
            PaneKind::Piano | PaneKind::Tuner => (3, 40),
            PaneKind::Audio => (4, 40),
            PaneKind::Properties => (10, crate::properties::WIDTH as usize),
            PaneKind::Palette => (10, crate::palette::WIDTH as usize),
            PaneKind::Score => (10, 40),
        };
        let frame = self.pane_frame as usize;
        (
            (rows * self.row_height as usize + frame).min(u16::MAX as usize) as u16,
            (cols * self.col_width as usize + frame).min(u16::MAX as usize) as u16,
        )
    }

    /// A new tool pane's size: its natural one, except that the properties and palette panes
    /// (docked at the sides) have a width only.
    fn tool_size(&self, kind: PaneKind) -> (Option<u16>, Option<u16>) {
        let (h, w) = self.natural_size(kind);
        match kind {
            PaneKind::Properties | PaneKind::Palette => (None, Some(w)),
            _ => (Some(h), Some(w)),
        }
    }

    /// Whether a tool docks along the bottom of the whole view, showing the last active score
    /// pane (`ui.global_tools`), else under its own score pane.
    pub fn is_global(&self, kind: PaneKind) -> bool {
        matches!(kind, PaneKind::Properties | PaneKind::Palette)
            || self
                .config
                .file
                .ui
                .global_tools
                .iter()
                .any(|t| PaneKind::parse(t) == Some(kind))
    }

    /// After a change to `ui.global_tools`: open panes of `kind` docked the old way close, and
    /// one opens where the tool now docks (along the window bottom, or under the current
    /// score pane).
    fn redock_tool(&mut self, kind: PaneKind) {
        if matches!(
            kind,
            PaneKind::Score | PaneKind::Properties | PaneKind::Palette
        ) {
            return;
        }
        let global = self.is_global(kind);
        let equalize = self.config.file.ui.equalize_on_close;
        let stale: Vec<PaneId> = self
            .panes
            .iter()
            .filter(|(_, p)| p.kind == kind && p.owner.is_some() == global)
            .map(|(id, _)| *id)
            .collect();
        for id in &stale {
            for v in &mut self.views {
                v.root.remove_with(*id, equalize);
                if v.focus == *id {
                    v.focus = v.score;
                }
            }
            self.panes.remove(id);
        }
        if !stale.is_empty() {
            self.set_tool(kind, Some(true));
        }
    }

    /// A pane's fixed size along an axis: a tool pane's own height or width.
    pub fn fixed_size(&self, id: PaneId, axis: Axis) -> Option<u16> {
        let p = self.panes.get(&id).filter(|p| p.kind != PaneKind::Score)?;
        match axis {
            Axis::Stacked => p.size.0,
            Axis::SideBySide => p.size.1,
        }
    }

    /// Sets the area panes are laid out in (each frame, by the frontend). The first time, tool
    /// panes added before it was known take their natural height.
    /// Records what a score pane displays (each frame, by the frontend), for page-down/up.
    pub fn set_view_info(&mut self, pane: PaneId, info: Option<ViewInfo>) {
        match info {
            Some(i) => self.view_info.insert(pane, i),
            None => self.view_info.remove(&pane),
        };
    }

    /// The pending `zz` / `zt` / `zb` request of a pane, which the frontend applies to its own
    /// scroll position; taking it clears it, so it applies once.
    pub fn take_wave_zoom(&mut self, pane: PaneId) -> Option<f64> {
        self.panes.get_mut(&pane)?.wave_zoom.take()
    }

    pub fn take_scroll_request(&mut self, pane: PaneId) -> Option<ScrollTo> {
        self.panes.get_mut(&pane)?.scroll_request.take()
    }

    /// Moves the focused pane's cursor half a screen (`down` or up): across half the displayed
    /// bars in a horizontal layout, else down or up half the displayed rows of bars (rounded up,
    /// at least one), staying at the same place in the bar. Without a reported view, `page_bars`.
    fn half_page(&mut self, down: bool) {
        let info = self.view_info.get(&self.focused()).cloned();
        let Some(info) = info.filter(|i| i.horizontal || !i.rows.is_empty()) else {
            let n = self.page_bars.max(1);
            self.move_cursor(|s, c| {
                set_bar(
                    s,
                    c,
                    if down {
                        c.bar + n
                    } else {
                        c.bar.saturating_sub(n)
                    },
                )
            });
            return;
        };
        if info.horizontal {
            let n = (info.visible_bars / 2).max(1);
            self.move_cursor(|s, c| {
                set_bar(
                    s,
                    c,
                    if down {
                        c.bar + n
                    } else {
                        c.bar.saturating_sub(n)
                    },
                )
            });
            return;
        }
        let rows = if info.page_rows > 0 {
            info.page_rows
        } else {
            info.visible_rows
        };
        let n = rows.div_ceil(2).max(1);
        self.move_cursor(|s, c| {
            let Some(from) = info.rows.iter().position(|r| r.contains(&c.bar)) else {
                return;
            };
            let to = if down {
                (from + n).min(info.rows.len() - 1)
            } else {
                from.saturating_sub(n)
            };
            let (a, b) = (&info.rows[from], &info.rows[to]);
            let bar = (b.start + (c.bar - a.start)).min(b.end.saturating_sub(1));
            c.bar = bar.min(s.master_bars.len().saturating_sub(1));
            c.beat = c.beat.min(beats(s, c).len().saturating_sub(1));
        });
    }

    pub fn set_viewport(&mut self, viewport: Rect) {
        self.viewport = viewport;
        if self.viewport_known {
            self.fit_tracks_panes();
            return;
        }
        self.viewport_known = true;
        for id in std::mem::take(&mut self.unfitted) {
            let Some(kind) = self.panes.get(&id).map(|p| p.kind) else {
                continue;
            };
            let size = self.tool_size(kind);
            if let Some(p) = self.panes.get_mut(&id) {
                p.size = size;
            }
        }
    }

    /// Nests every view's global tool panes the way `set_tool` builds them, whatever shape the
    /// tree was saved or edited in: the tracks pane along the bottom at the window's full width; above
    /// it the properties pane outermost along the right, the palette along the left of what is
    /// left of it, and between them the scores with the other bottom tools under them (as wide
    /// as the scores). Sizes belong to the panes, so they are kept; a tree
    /// already in that shape is left alone (split weights included).
    pub(crate) fn normalize_layouts(&mut self) {
        for v in 0..self.views.len() {
            let owner_of = |id: &PaneId| self.panes.get(id).map(|p| (p.kind, p.owner));
            let panes = self.views[v].root.panes();
            let find = |kind| {
                panes
                    .iter()
                    .copied()
                    .find(|id| owner_of(id) == Some((kind, None)))
            };
            let (props, palette) = (find(PaneKind::Properties), find(PaneKind::Palette));
            let tracks = find(PaneKind::Tracks);
            let tools: Vec<PaneId> = panes
                .iter()
                .copied()
                .filter(|id| match owner_of(id) {
                    Some((kind, None)) => !matches!(
                        kind,
                        PaneKind::Score
                            | PaneKind::Properties
                            | PaneKind::Palette
                            | PaneKind::Tracks
                    ),
                    _ => false,
                })
                .collect();
            if props.is_none() && palette.is_none() {
                continue;
            }
            let mut main = self.views[v].root.clone();
            let mut removed = true;
            for id in props.iter().chain(&palette).chain(&tools).chain(&tracks) {
                removed &= main.remove(*id);
            }
            if !removed {
                continue;
            }
            for t in &tools {
                main = dock_bottom(main, *t);
            }
            if let Some(p) = palette {
                main = dock_left(main, p);
            }
            let root = match props {
                Some(p) => Node::Split {
                    axis: Axis::SideBySide,
                    children: vec![main, Node::Leaf(p)],
                    weights: vec![1.0, 1.0],
                },
                None => main,
            };
            let root = with_tracks(root, tracks.map(Node::Leaf));
            if root.shape() != self.views[v].root.shape() {
                self.views[v].root = root;
            }
        }
    }

    /// Whether the active view shows a pane of a tool kind.
    pub fn has_tool(&self, kind: PaneKind) -> bool {
        self.tool_pane(kind).is_some()
    }

    /// Shows (docked along the bottom of the view) or hides a tool pane; `None` toggles.
    pub fn set_tool(&mut self, kind: PaneKind, on: Option<bool>) {
        let shown = self.tool_pane(kind);
        let on = on.unwrap_or(shown.is_none());
        match (shown, on) {
            (Some(id), false) => {
                if self.views[self.active_view].maximized == Some(id) {
                    self.views[self.active_view].maximized = None;
                }
                self.close_pane_id(id);
            }
            (None, true) => {
                let id = self.new_pane(self.pane().buffer);
                let size = self.tool_size(kind);
                let score = self.focused();
                let global = self.is_global(kind);
                let pane = self.panes.get_mut(&id).unwrap();
                pane.kind = kind;
                pane.size = size;
                pane.owner = (!global).then_some(score);
                if !self.viewport_known {
                    self.unfitted.push(id);
                }
                let palette = self.view().root.panes().into_iter().find(|i| {
                    self.panes
                        .get(i)
                        .is_some_and(|p| p.kind == PaneKind::Palette)
                });
                let props = self.view().root.panes().into_iter().find(|i| {
                    self.panes
                        .get(i)
                        .is_some_and(|p| p.kind == PaneKind::Properties)
                });
                let tracks = self.view().root.panes().into_iter().find(|i| {
                    self.panes
                        .get(i)
                        .is_some_and(|p| p.kind == PaneKind::Tracks && p.owner.is_none())
                });
                let view = &mut self.views[self.active_view];
                if !global {
                    // Under the score pane it works on, as wide as it.
                    view.root.split(score, id, Axis::Stacked);
                } else {
                    // The tracks pane is along the bottom of the whole window. Above it the
                    // properties pane is outermost, along the right at the full height;
                    // then the palette along the left; between them the scores with the
                    // other bottom tools under them.
                    let root = std::mem::replace(&mut view.root, Node::Leaf(0));
                    let (root, tracks_leaf) = if kind == PaneKind::Tracks {
                        (root, Some(Node::Leaf(id)))
                    } else {
                        split_off_tracks(root, tracks)
                    };
                    let (main, props_leaf) = split_off_leaf(root, props);
                    let main = match kind {
                        PaneKind::Properties => main,
                        PaneKind::Palette => dock_left(main, id),
                        PaneKind::Tracks => main,
                        _ => dock_tool(main, id, palette),
                    };
                    let side = if kind == PaneKind::Properties {
                        Some(Node::Leaf(id))
                    } else {
                        props_leaf
                    };
                    let root = match side {
                        Some(side) => Node::Split {
                            axis: Axis::SideBySide,
                            children: vec![main, side],
                            weights: vec![1.0, 1.0],
                        },
                        None => main,
                    };
                    view.root = with_tracks(root, tracks_leaf);
                }
            }
            _ => return,
        }
        if kind == PaneKind::Tuner {
            self.tuner_reading = None;
            self.tuner_error = None;
            self.effects.push(Effect::Tuner(on));
        }
    }

    /// The kind of score a buffer holds, as the pane title and status line name it: `chant` or
    /// `modern`.
    pub fn score_kind_name(&self, buffer: usize) -> &'static str {
        if self.buffers.get(buffer).is_some_and(|b| b.score.is_chant()) {
            "chant"
        } else {
            "modern"
        }
    }

    /// The active view's bufferline: its loaded buffers in order.
    pub fn bufferline(&self) -> Vec<BufferTab> {
        let current = self.pane().buffer;
        self.view()
            .buffers
            .iter()
            .filter_map(|&b| {
                let buf = self.buffers.get(b)?;
                Some(BufferTab {
                    buffer: b,
                    name: buf.name.clone(),
                    // A chant score shows the chant icon whatever its file (`.cc`, unsaved).
                    kind: if buf.score.is_chant() {
                        "chant"
                    } else {
                        file_kind(buf.path.as_deref())
                    },
                    modified: buf.modified,
                    active: b == current,
                    playing: self.playing_buffer == Some(b) && self.playing && !self.paused,
                    paused: self.playing_buffer == Some(b) && self.paused,
                })
            })
            .collect()
    }

    /// Moves the tab at position `from` of the active view's tabline to position `to`
    /// (0-based; clamped).
    pub fn move_tab(&mut self, from: usize, to: usize) {
        let tabs = &mut self.views[self.active_view].buffers;
        if from >= tabs.len() {
            return;
        }
        let to = to.min(tabs.len() - 1);
        let b = tabs.remove(from);
        tabs.insert(to, b);
    }

    /// The view labels the tabline shows: none for a lone view with `tabline.hide_single_view`.
    pub fn tabline_views(&self) -> Vec<String> {
        if self.views.len() == 1 && self.config.file.tabline.hide_single_view {
            Vec::new()
        } else {
            self.view_labels()
        }
    }

    /// Labels of the views, for the right of the tabline: the name, or the number.
    pub fn view_labels(&self) -> Vec<String> {
        self.views
            .iter()
            .enumerate()
            .map(|(i, v)| v.name.clone().unwrap_or_else(|| (i + 1).to_string()))
            .collect()
    }

    /// The view's score pane (see [`App::focused`]).
    pub fn pane(&self) -> &Pane {
        &self.panes[&self.focused()]
    }

    pub(crate) fn pane_mut(&mut self) -> &mut Pane {
        let id = self.focused();
        self.panes.get_mut(&id).unwrap()
    }

    /// The note under a pane's cursor, on any staff: the other staves of its track show it in
    /// the mode's color. Found as the editor finds the note an edit hits: the cursor's string
    /// on a tab, the note at the cursor's position on a staff of lines, the beat's note on a
    /// slash or chant staff.
    pub fn linked_note(&self, pane: &Pane) -> Option<LinkedNote> {
        let c = pane.cursor;
        let score = &self.buffers.get(pane.buffer)?.score;
        let track = score.tracks.get(c.track)?;
        let staff = staff_index(track, c.staff);
        let kind = *track.staves().get(staff)?;
        let beat = track
            .bars
            .get(c.bar)?
            .voices
            .get(c.voice)?
            .beats
            .get(c.beat)?;
        let string = if uses_strings(track, kind) {
            u8::try_from(c.string).ok()?
        } else if matches!(kind, Staff::Slash) || track.chant.is_some() {
            beat.notes.first()?.string
        } else {
            let key = score
                .master_bars
                .get(c.bar)
                .map_or(0, |m| m.key_signature.accidentals);
            beat.notes
                .iter()
                .find(|n| {
                    let written = track.pitch_of(n) as i32 + track.staff_clef().transpose();
                    cecilia_core::staff::diatonic(written, key) == c.line
                })?
                .string
        };
        beat.notes
            .iter()
            .any(|n| n.string == string)
            .then_some(LinkedNote {
                track: c.track,
                bar: c.bar,
                voice: c.voice,
                beat: c.beat,
                string,
                staff,
            })
    }

    pub fn buffer(&self) -> &Buffer {
        &self.buffers[self.pane().buffer]
    }

    pub fn layout(&self) -> Vec<(PaneId, Rect)> {
        if self.view().dashboard {
            return Vec::new();
        }
        if let Some(id) = self.maximized() {
            return vec![(id, self.viewport)];
        }
        self.pane_rects()
    }

    /// Where every pane of the view is, maximized or not (for moving between them).
    fn pane_rects(&self) -> Vec<(PaneId, Rect)> {
        let mut out = Vec::new();
        let fixed = |id, axis| self.fixed_size(id, axis);
        self.view()
            .root
            .layout_with(self.viewport, &mut out, &fixed);
        out
    }

    /// Jumps `n` sections on or back (as far as there are), remembered for `''`.
    fn section_jump(&mut self, forward: bool, n: usize) {
        let bars = &self.buffer().score.master_bars;
        let at = self.pane().cursor.bar;
        let starts: Vec<usize> = if forward {
            (at + 1..bars.len())
                .filter(|&b| bars[b].marker.is_some())
                .collect()
        } else {
            (0..at)
                .rev()
                .filter(|&b| bars[b].marker.is_some())
                .collect()
        };
        match starts.get(n.max(1) - 1).or(starts.last()) {
            Some(&b) => self.run(Command::GotoBar(b + 1)),
            None => self.info(if forward {
                "no section after this bar"
            } else {
                "no section before this bar"
            }),
        }
    }

    /// The borders between panes of the active view, for resizing with the mouse.
    pub fn dividers(&self) -> Vec<crate::layout::Divider> {
        if self.maximized().is_some() {
            return Vec::new();
        }
        let fixed = |id, axis| self.fixed_size(id, axis);
        self.view().root.dividers(self.viewport, &fixed)
    }

    /// Whether the edit cursor in a buffer is hidden: while it plays with follow on, only the
    /// playhead shows (`ui.hide_cursor_while_following`).
    pub fn cursor_hidden(&self, buffer: usize) -> bool {
        self.config.file.ui.hide_cursor_while_following
            && self.playing
            && self.config.file.ui.follow_playback
            && self.playing_buffer == Some(buffer)
    }

    /// Whether a pane shows its own waveform strip.
    pub fn pane_waveform(&self, pane: &Pane) -> bool {
        pane.waveform
    }

    /// The tracks show for a moment after the track changed (no tracks pane being shown):
    /// frontends draw them along the bottom of the window, `peek_height` tall.
    pub fn tracks_peeking(&self) -> bool {
        !self.has_tool(PaneKind::Tracks)
            && self
                .tracks_peek_until
                .is_some_and(|t| std::time::Instant::now() < t)
    }

    /// Height of the peeking tracks (viewport units).
    pub fn peek_height(&self) -> u16 {
        self.natural_size(PaneKind::Tracks).0
    }

    /// After a key: if it changed the score pane's track (same file) while no tracks pane is
    /// shown, the tracks show for `ui.tracks_peek` seconds.
    pub(crate) fn peek_tracks(&mut self, before: (PaneId, usize, Cursor)) {
        let pane = self.pane();
        let secs = self.config.file.ui.tracks_peek;
        if self.has_tool(PaneKind::Tracks)
            || secs <= 0.0
            || !secs.is_finite()
            || (self.focused(), pane.buffer) != (before.0, before.1)
            || pane.cursor.track == before.2.track
        {
            return;
        }
        self.tracks_peek_until =
            Some(std::time::Instant::now() + std::time::Duration::from_secs_f64(secs.min(3600.0)));
    }

    /// Tool panes without a size of their own (from older workspaces) get their natural one.
    pub(crate) fn size_tools(&mut self) {
        let without: Vec<PaneId> = self
            .panes
            .iter()
            .filter(|(_, p)| match p.kind {
                PaneKind::Score => false,
                PaneKind::Properties | PaneKind::Palette => p.size.1.is_none(),
                _ => p.size.0.is_none(),
            })
            .map(|(id, _)| *id)
            .collect();
        for id in without {
            if self.viewport_known {
                let size = self.tool_size(self.panes[&id].kind);
                self.panes.get_mut(&id).unwrap().size = size;
            } else {
                self.unfitted.push(id);
            }
        }
    }

    /// The pane maximized in the active view, while it has the focus.
    pub fn maximized(&self) -> Option<PaneId> {
        let v = self.view();
        v.maximized
            .filter(|&id| id == v.focus && v.root.panes().len() > 1)
    }

    /// Multiplies a pane's zoom by `factor` (kept within 20%–600%).
    pub fn zoom_pane(&mut self, id: PaneId, factor: f64) {
        if let Some(p) = self.panes.get_mut(&id) {
            p.zoom = (p.zoom * factor).clamp(0.2, 6.0);
        }
    }

    /// The zoom of the pane being worked on.
    pub fn zoom(&self) -> f64 {
        self.pane().zoom
    }

    /// Whether a pane is in the maximized state: it is the view's maximized pane, even when
    /// focus has moved to another pane (it will be maximized again on return).
    pub fn is_maximized(&self, id: PaneId) -> bool {
        self.view().maximized == Some(id)
    }

    /// Moves a pane border (from [`App::dividers`]) to `pos`.
    pub fn drag_divider(&mut self, d: &crate::layout::Divider, pos: u16) {
        let v = self.active_view;
        // A tool pane on either side takes the new size itself; else the panes share anew.
        let fixed = |id, axis| self.fixed_size(id, axis);
        let root = &self.views[v].root;
        let rects = root.child_rects(&d.path, d.area, &fixed);
        let children = match root.node_at(&d.path) {
            Some(Node::Split { children, .. }) => children.clone(),
            _ => return,
        };
        // Between two tool panes: they share the border's move, the rest stays as it is.
        let leaf = |k: usize| match children.get(k) {
            Some(Node::Leaf(id)) if fixed(*id, d.axis).is_some() => Some(*id),
            _ => None,
        };
        if let (Some(a), Some(b), Some(ra), Some(rb)) = (
            leaf(d.index),
            leaf(d.index + 1),
            rects.get(d.index),
            rects.get(d.index + 1),
        ) {
            let (start, end) = match d.axis {
                Axis::Stacked => (ra.y, rb.y + rb.h),
                Axis::SideBySide => (ra.x, rb.x + rb.w),
            };
            let pos = pos.clamp(start + 2, end.saturating_sub(2).max(start + 2));
            self.set_tool_size(a, d.axis, pos - start);
            self.set_tool_size(b, d.axis, end.saturating_sub(pos).max(2));
            return;
        }
        for (k, before) in [(d.index, true), (d.index + 1, false)] {
            let (Some(Node::Leaf(id)), Some(r)) = (children.get(k), rects.get(k)) else {
                continue;
            };
            if fixed(*id, d.axis).is_none() {
                continue;
            }
            let (start, end) = match d.axis {
                Axis::Stacked => (r.y, r.y + r.h),
                Axis::SideBySide => (r.x, r.x + r.w),
            };
            let size = if before {
                pos.saturating_sub(start)
            } else {
                end.saturating_sub(pos)
            }
            .max(2);
            let id = *id;
            self.set_tool_size(id, d.axis, size);
            return;
        }
        self.views[v].root.drag_divider(d, pos);
    }

    /// Sets a tool pane's height (stacked) or width (side by side).
    fn set_tool_size(&mut self, id: PaneId, axis: Axis, size: u16) {
        if let Some(p) = self.panes.get_mut(&id) {
            p.manual_size = true;
            match axis {
                Axis::Stacked => p.size.0 = Some(size),
                Axis::SideBySide => p.size.1 = Some(size),
            }
        }
    }

    pub fn info(&mut self, text: impl Into<String>) {
        self.message = Some(Message {
            text: text.into(),
            error: false,
        });
    }

    pub fn error(&mut self, text: impl Into<String>) {
        self.message = Some(Message {
            text: text.into(),
            error: true,
        });
    }

    // ---- files ----

    /// Opens a file into a buffer (reusing one already open) and returns its index.
    pub fn open(&mut self, path: &Path) -> Result<usize, String> {
        let canonical = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
        if let Some(i) = self
            .buffers
            .iter()
            .position(|b| b.path.as_deref() == Some(&canonical))
        {
            return Ok(i);
        }
        let score =
            cecilia_formats::read_file(path).map_err(|e| format!("{}: {e}", path.display()))?;
        let name = path.file_name().map_or_else(
            || path.display().to_string(),
            |n| n.to_string_lossy().into(),
        );
        // Replace the initial empty buffer if nothing refers to anything else yet.
        let canonical_path = canonical.clone();
        let mut buffer = Buffer {
            path: Some(canonical),
            name,
            score,
            modified: false,
            history: Default::default(),
            version: 0,
            disk: file_stamp(&canonical_path),
        };
        let recovered = self.swap_check_open(buffer.path.as_deref().unwrap());
        if let Some((score, history)) = recovered.clone() {
            buffer.score = score;
            buffer.modified = true;
            if let Some(history) = history {
                buffer.history = history;
            }
        }
        let index = if self.buffers.len() == 1 && self.buffers[0].path.is_none() {
            self.buffers[0] = buffer;
            0
        } else {
            self.buffers.push(buffer);
            self.buffers.len() - 1
        };
        if recovered.is_some() {
            // Own the swap file now, so closing the buffer removes it.
            self.swap_write(index);
        }
        Ok(index)
    }

    /// Shows a buffer in the focused pane, adding it to the view's bufferline if needed. The
    /// pane remembers the buffer it showed (and its cursor) to come back to.
    pub(crate) fn show_buffer(&mut self, buffer: usize) {
        let view = &mut self.views[self.active_view];
        if view.dashboard {
            // Leaving the dashboard: its placeholder score is not one to come back to.
            view.dashboard = false;
            let focused = self.focused();
            if let Some(p) = self.panes.get_mut(&focused) {
                p.recent.clear();
            }
        }
        let view = &mut self.views[self.active_view];
        if !view.buffers.contains(&buffer) {
            view.buffers.push(buffer);
        }
        self.switch_pane_buffer(self.focused(), buffer);
        self.palette_follow_kind();
        let b = &self.buffers[buffer];
        let s = &b.score;
        let msg = format!(
            "\"{}\" {} — {} tracks, {} bars, tempo {}",
            b.name,
            if s.meta.title.is_empty() {
                ""
            } else {
                &s.meta.title
            },
            s.tracks.len(),
            s.master_bars.len(),
            s.tempo
        );
        let hint = (s.audio.is_none())
            .then(|| b.path.as_deref().and_then(crate::playalong::find_audio_for))
            .flatten()
            .map(|a| {
                format!(
                    " — matching audio {}, :audio to attach",
                    a.file_name().unwrap_or_default().to_string_lossy()
                )
            });
        self.info(msg + &hint.unwrap_or_default());
    }

    /// Points a pane at a buffer, restoring the cursor it last had there.
    fn switch_pane_buffer(&mut self, pane_id: PaneId, buffer: usize) {
        let Some(pane) = self.panes.get_mut(&pane_id) else {
            return;
        };
        if pane.buffer == buffer {
            return;
        }
        let previous = (pane.buffer, pane.cursor);
        pane.recent.retain(|(b, _)| *b != previous.0);
        pane.recent.insert(0, previous);
        let cursor = match pane.recent.iter().position(|(b, _)| *b == buffer) {
            Some(i) => pane.recent.remove(i).1,
            None => Cursor::default(),
        };
        pane.buffer = buffer;
        pane.cursor = cursor;
        pane.scroll = 0;
        let mut c = pane.cursor;
        clamp_cursor(&self.buffers[buffer].score, &mut c);
        self.panes.get_mut(&pane_id).unwrap().cursor = c;
        self.fit_tracks_panes();
    }

    /// Gives each tracks pane of the active view the height its score's rows need (after
    /// the score it shows changed).
    fn fit_tracks_panes(&mut self) {
        if !self.viewport_known || !self.config.file.ui.auto_resize {
            return;
        }
        let active = self.active_view;
        let saved = self.views[active].score;
        for id in self.views[active].root.panes() {
            if self
                .panes
                .get(&id)
                .is_none_or(|p| p.kind != PaneKind::Tracks || p.manual_size)
            {
                continue;
            }
            self.views[active].score = self.tool_score(id);
            let (h, _) = self.natural_size(PaneKind::Tracks);
            self.panes.get_mut(&id).unwrap().size.0 = Some(h);
        }
        self.views[active].score = saved;
    }

    /// Shows the buffer `steps` tabs along the bufferline (wrapping with `tabline.wrap`) in the focused pane.
    fn cycle_buffer(&mut self, steps: isize) {
        let list = &self.view().buffers;
        if list.is_empty() {
            return;
        }
        let n = list.len() as isize;
        let at = list
            .iter()
            .position(|&b| b == self.pane().buffer)
            .unwrap_or(0) as isize;
        let to = if self.config.file.tabline.wrap {
            (at + steps).rem_euclid(n)
        } else {
            (at + steps).clamp(0, n - 1)
        };
        let next = list[to as usize];
        self.show_buffer(next);
    }

    /// Closes a buffer's tab in the active view. Panes showing it switch to the most recent
    /// other buffer they showed that is still in the view, or close if there is none. The
    /// buffer is unloaded once no view has it; unsaved changes then stop the close unless
    /// `force`.
    pub fn close_buffer(&mut self, buffer: usize, force: bool) {
        let in_other_views = self
            .views
            .iter()
            .enumerate()
            .any(|(i, v)| i != self.active_view && v.buffers.contains(&buffer));
        let b = &self.buffers[buffer];
        if !in_other_views && b.modified && !force {
            return self.ask_save(Closing::Buffer(buffer), buffer, Vec::new());
        }
        if self.playing_buffer == Some(buffer) && !in_other_views && (self.playing || self.paused) {
            self.run(Command::Stop);
        }
        let name = self.buffers[buffer].name.clone();
        let view = &mut self.views[self.active_view];
        let pos = view.buffers.iter().position(|&b| b == buffer).unwrap_or(0);
        view.buffers.retain(|&b| b != buffer);
        let remaining = view.buffers.clone();
        // The neighbouring tab, for a view's only pane with nothing else to go back to.
        let neighbour = remaining
            .get(pos.min(remaining.len().saturating_sub(1)))
            .copied();
        let only_pane = view.root.panes().len() == 1;
        let mut to_close = Vec::new();
        for id in view.root.panes() {
            let Some(pane) = self.panes.get_mut(&id) else {
                continue;
            };
            pane.recent
                .retain(|(b, _)| *b != buffer && remaining.contains(b));
            if pane.buffer == buffer {
                match pane.recent.first().map(|(b, _)| *b) {
                    Some(next) => {
                        // Not via switch_pane_buffer: the closed buffer is not remembered.
                        let (b, mut cursor) = pane.recent.remove(0);
                        clamp_cursor(&self.buffers[next].score, &mut cursor);
                        let pane = self.panes.get_mut(&id).unwrap();
                        pane.buffer = b;
                        pane.cursor = cursor;
                        pane.scroll = 0;
                    }
                    None => match neighbour.filter(|_| only_pane) {
                        Some(next) => {
                            pane.buffer = next;
                            pane.cursor = Cursor::default();
                            pane.scroll = 0;
                        }
                        None => to_close.push(id),
                    },
                }
            }
        }
        if !in_other_views {
            let path = self.buffers[buffer].path.clone();
            self.remember_closed(path);
        }
        for id in to_close {
            self.close_pane_id(id);
            if self.quit {
                return;
            }
        }
        if !in_other_views {
            self.forget_marks(buffer);
            self.swap_release(buffer);
            // Unloaded: the slot stays (indices are shared) but holds nothing.
            self.buffers[buffer] = Buffer {
                name: "[closed]".into(),
                ..Buffer::empty()
            };
        }
        self.info(format!("closed \"{name}\""));
    }

    /// A new view as `tabline.new_view` says: a copy of the active view's bufferline, just the
    /// current buffer, or an empty start.
    fn new_view(&mut self) {
        let current = self.pane().buffer;
        let mut buffers = match self.config.file.tabline.new_view.as_str() {
            "current" => vec![current],
            "empty" => {
                self.buffers.push(Buffer::empty());
                vec![self.buffers.len() - 1]
            }
            _ => self.view().buffers.clone(),
        };
        // Without `ui.keep_files_in_new_view` (or any file to keep) the new view is the
        // dashboard, on a placeholder score no tab lists.
        let dashboard = !self.config.file.ui.keep_files_in_new_view || buffers.is_empty();
        if dashboard {
            self.buffers.push(Buffer::empty());
            buffers = Vec::new();
        }
        let shown = if dashboard {
            self.buffers.len() - 1
        } else if buffers.contains(&current) {
            current
        } else {
            buffers[0]
        };
        let id = self.new_pane(shown);
        self.views.insert(
            self.active_view + 1,
            View {
                maximized: None,
                dashboard,
                root: Node::Leaf(id),
                focus: id,
                score: id,
                buffers,
                name: None,
            },
        );
        self.active_view += 1;
        if dashboard {
            self.dashboard_focus = 0;
        }
    }

    /// Closing the only view closes all its buffers: what remains is one score pane showing an
    /// empty score, behind the dashboard.
    fn close_last_view(&mut self, orphans: &[usize]) {
        if self.playing_buffer.is_some_and(|b| orphans.contains(&b))
            && (self.playing || self.paused)
        {
            self.run(Command::Stop);
        }
        for &b in orphans {
            self.forget_marks(b);
            self.swap_release(b);
            self.buffers[b] = Buffer {
                name: "[closed]".into(),
                ..Buffer::empty()
            };
        }
        let keep = self.focused();
        let equalize = self.config.file.ui.equalize_on_close;
        let view = &mut self.views[self.active_view];
        for id in view.root.panes() {
            if id != keep {
                view.root.remove_with(id, equalize);
                self.panes.remove(&id);
            }
        }
        view.maximized = None;
        view.focus = keep;
        self.buffers.push(Buffer::empty());
        let empty = self.buffers.len() - 1;
        let pane = self.panes.get_mut(&keep).unwrap();
        pane.buffer = empty;
        pane.cursor = Cursor::default();
        pane.scroll = 0;
        pane.recent.clear();
        self.show_dashboard();
    }

    /// Closes the active view. Buffers no other view has are unloaded; unsaved changes in
    /// them stop it unless `force`.
    fn close_view(&mut self, force: bool) {
        self.close_view_skipping(force, Vec::new());
    }

    /// [`App::close_view`], not asking about the buffers in `skip` (answered `n`).
    fn close_view_skipping(&mut self, force: bool, skip: Vec<usize>) {
        let last = self.views.len() == 1;
        let view = &self.views[self.active_view];
        let orphans: Vec<usize> = view
            .buffers
            .iter()
            .copied()
            .filter(|b| {
                !self
                    .views
                    .iter()
                    .enumerate()
                    .any(|(i, v)| i != self.active_view && v.buffers.contains(b))
            })
            .collect();
        if !force {
            let unsaved = orphans
                .iter()
                .find(|&&b| self.buffers[b].modified && !skip.contains(&b));
            if let Some(&b) = unsaved {
                return self.ask_save(Closing::View, b, skip);
            }
        }
        for &b in &orphans {
            let path = self.buffers[b].path.clone();
            self.remember_closed(path);
        }
        if last {
            self.close_last_view(&orphans);
            return;
        }
        let view = self.views.remove(self.active_view);
        for id in view.root.panes() {
            self.panes.remove(&id);
        }
        for b in orphans {
            self.forget_marks(b);
            self.buffers[b] = Buffer {
                name: "[closed]".into(),
                ..Buffer::empty()
            };
        }
        self.active_view = self.active_view.min(self.views.len() - 1);
    }

    // ---- config ----

    /// Reloads the config file if it changed on disk.
    pub fn poll_config(&mut self) {
        self.swap_tick(std::time::Instant::now());
        self.poll_files();
        let Some(path) = self.config_path.clone() else {
            return;
        };
        let m = mtime(&path);
        let theme_file = self.config.file.theme.file.as_str();
        let theme = (!theme_file.is_empty())
            .then(|| crate::theme::theme_path(std::path::Path::new(theme_file)))
            .and_then(|p| mtime(&p));
        if m != self.config_mtime || theme != self.theme_mtime {
            self.config_mtime = m;
            self.theme_mtime = theme;
            self.reload_config();
        }
    }

    /// Makes `config` current: tools that changed `ui.global_tools` membership move to their
    /// new dock, completion candidates found under the old matching rules are dropped, and a
    /// changed SoundFont is reported to the player.
    fn install_config(&mut self, config: Config) {
        let old = std::mem::replace(&mut self.config, config);
        let (was, now) = (&old.file.ui.global_tools, &self.config.file.ui.global_tools);
        let moved: Vec<PaneKind> = was
            .iter()
            .filter(|t| !now.contains(t))
            .chain(now.iter().filter(|t| !was.contains(t)))
            .filter_map(|t| PaneKind::parse(t))
            .collect();
        for kind in &moved {
            self.redock_tool(*kind);
        }
        if !moved.is_empty() {
            self.normalize_layouts();
        }
        let (a, b) = (&old.file.ui, &self.config.file.ui);
        if a.completion_ignore_case != b.completion_ignore_case
            || a.completion_fuzzy != b.completion_fuzzy
        {
            self.completion = None;
        }
        // Only a new SoundFont needs a new player (dropping it would also drop a paused one).
        if old.file.audio.soundfont != self.config.file.audio.soundfont {
            self.effects.push(Effect::ConfigReloaded);
        }
        // A paused or running player keeps the speed it was last given.
        if old.file.audio.speed != self.config.file.audio.speed {
            self.effects.push(Effect::PlaybackSettings);
        }
    }

    /// `:set`: applies one option to a copy of the config file and rebuilds the config from it.
    fn set_option(&mut self, name: &str, value: Option<&str>) {
        let mut file = self.config.file.clone();
        match crate::options::set(&mut file, name, value) {
            Ok(v) => {
                let mut overrides = self.config.overrides.clone();
                overrides.insert(name.to_string(), v.clone());
                let old_errors = Config::from_file(self.config.file.clone()).1;
                let (mut config, errors) = Config::from_file(file);
                if let Some(e) = errors.iter().find(|e| !old_errors.contains(e)) {
                    self.error(format!("{name}: {e}"));
                    return;
                }
                config.overrides = overrides;
                self.install_config(config);
                self.info(format!("{name} = {v}"));
            }
            Err(e) => self.error(e),
        }
    }

    pub fn reload_config(&mut self) {
        let Some(path) = self.config_path.clone() else {
            self.error("no config file");
            return;
        };
        let overrides: Vec<(String, String)> = self
            .config
            .overrides
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        match Config::load(&path, &overrides) {
            Ok((config, errors)) => {
                self.install_config(config);
                if errors.is_empty() {
                    self.info(format!("config reloaded: {}", path.display()));
                } else {
                    self.error(errors.join("; "));
                }
            }
            Err(e) => self.error(format!("config not reloaded: {e}")),
        }
    }

    // ---- keys ----

    pub fn handle_key(&mut self, key: Key) {
        self.palette_follow_kind();
        if let Some(path) = self.confirm_delete_workspace.take() {
            return self.answer_delete_workspace(path, key);
        }
        if let Some(buffer) = self.confirm_reload.take() {
            return self.answer_reload(buffer, key);
        }
        if let Some(buffer) = self.confirm_close.take() {
            return self.answer_close(buffer, key);
        }
        if self.dashboard_key(key) {
            return;
        }
        self.dispatch_key(key);
        if self.dashboard_visible() && matches!(self.mode, Mode::Insert | Mode::Visual) {
            self.mode = Mode::Normal;
            self.ed.visual_anchor = None;
        }
    }

    /// The chord policy of `:modern-to-chant` and `cecilia convert … .gabc` (`ui.chant_chords`).
    pub fn chord_policy(&self) -> cecilia_core::melody::ChordPolicy {
        cecilia_core::melody::ChordPolicy::parse(&self.config.file.ui.chant_chords)
            .unwrap_or_default()
    }

    /// Shows the save prompt for a buffer and waits for the answer.
    fn ask_save(&mut self, action: Closing, asking: usize, skip: Vec<usize>) {
        let text = format!(
            "\"{}\" has unsaved changes: save? (y/n, Esc cancels)",
            self.buffers[asking].name
        );
        self.confirm_close = Some(Confirm {
            action,
            asking,
            skip,
        });
        self.info(text);
    }

    /// The answer to a save prompt: `y` writes the buffer, `n` leaves it unsaved, any other
    /// key cancels; then the close or quit carries on (asking about the next changed buffer).
    fn answer_close(&mut self, mut c: Confirm, key: Key) {
        self.message = None;
        let Some(b) = self.buffers.get(c.asking).filter(|b| b.modified) else {
            return;
        };
        match key.code {
            KeyCode::Char('y' | 'Y') => {
                if b.path.is_none() {
                    return self.error(format!(
                        "\"{}\" has no file name (:w <file>, then try again)",
                        b.name
                    ));
                }
                self.write_buffer(c.asking, None);
                if self.buffers[c.asking].modified {
                    return;
                }
            }
            KeyCode::Char('n' | 'N') => c.skip.push(c.asking),
            _ => return self.info("cancelled"),
        }
        match c.action {
            Closing::Buffer(b) => self.close_buffer(b, c.skip.contains(&b)),
            Closing::View => self.close_view_skipping(false, c.skip),
            Closing::Quit => self.quit_skipping(c.skip),
        }
    }

    pub(crate) fn handle_command_key(&mut self, key: Key) {
        let bound = match self.config.keymap(Mode::Command).lookup(&[key]) {
            Lookup::Exact(cmds) => Some(cmds.to_vec()),
            _ => None,
        };
        if let Some(cmds) = bound {
            for cmd in cmds {
                self.run(cmd);
            }
            return;
        }
        if !matches!(
            key.code,
            KeyCode::Tab | KeyCode::BackTab | KeyCode::Up | KeyCode::Down
        ) && !(key.alt && matches!(key.code, KeyCode::Char('j' | 'k')))
        {
            self.completion = None;
        }
        match key.code {
            KeyCode::Tab => self.complete_step(true),
            KeyCode::BackTab => self.complete_step(false),
            KeyCode::Char('j') if key.alt => self.complete_step(true),
            KeyCode::Char('k') if key.alt => self.complete_step(false),
            KeyCode::Esc => self.leave_command_mode(),
            KeyCode::Char('c') if key.ctrl => self.leave_command_mode(),
            KeyCode::Char('u') if key.ctrl => self.cmdline.clear(),
            KeyCode::Enter => {
                let line = std::mem::take(&mut self.cmdline);
                let from_visual = std::mem::take(&mut self.cmd_from_visual);
                let from_palette = std::mem::take(&mut self.cmd_from_palette);
                self.leave_command_mode();
                if from_visual && self.ed.visual_anchor.is_some() {
                    self.mode = Mode::Visual;
                }
                if !line.trim().is_empty() {
                    self.history.push(line.clone());
                }
                match self.parse_for_score(&line) {
                    Ok(cmd) if from_palette => self.run_tool(cmd),
                    Ok(cmd) => self.run(cmd),
                    Err(e) => self.error(e),
                }
            }
            KeyCode::Backspace => {
                if self.cmdline.pop().is_none() {
                    self.leave_command_mode();
                }
            }
            // Up/Down move through the palette's candidates; <C-p>/<C-n> recall history.
            KeyCode::Down => self.complete_step(true),
            KeyCode::Up => self.complete_step(false),
            KeyCode::Char('p' | 'n') if key.ctrl && !self.history.is_empty() => {
                let back = key.code == KeyCode::Char('p');
                let last = self.history.len() - 1;
                let pos = match (back, self.history_pos) {
                    (true, None) => Some(last),
                    (true, Some(p)) => Some(p.saturating_sub(1)),
                    (_, Some(p)) if p < last => Some(p + 1),
                    _ => None,
                };
                self.history_pos = pos;
                self.cmdline = pos.map(|p| self.history[p].clone()).unwrap_or_default();
            }
            KeyCode::Char(c) if !key.ctrl && !key.alt && !key.sup => self.cmdline.push(c),
            _ => {}
        }
    }

    /// Parses a command line and checks that its command is for the kind of score being edited
    /// (`capo: not in a chant score`, `chant-mod: not in a modern score`).
    pub(crate) fn parse_for_score(&self, line: &str) -> Result<Command, String> {
        let cmd = Command::parse(line)?;
        let name = line
            .trim()
            .trim_start_matches(':')
            .split_whitespace()
            .next()
            .unwrap_or("");
        crate::complete::check_scope(name, self.buffer().score.is_chant())?;
        Ok(cmd)
    }

    /// Opens the command line with some text already typed.
    pub(crate) fn open_command_line(&mut self, text: &str) {
        self.cmd_from_visual = self.mode == Mode::Visual && self.ed.visual_anchor.is_some();
        self.cmd_from_palette = false;
        self.mode = Mode::Command;
        self.cmdline = text.to_string();
        self.completion = None;
        self.history_pos = None;
    }

    /// Leaves Sync mode: tapping stops (the sync points stay).
    pub(crate) fn leave_sync_mode(&mut self) {
        self.syncing = false;
        self.mode = Mode::Normal;
        self.pending.clear();
        self.ed.operator = None;
        self.pa.chant_tap = None;
        self.pa.beat_trail.clear();
        self.info("sync mode off");
    }

    fn leave_command_mode(&mut self) {
        self.mode = if self.syncing {
            Mode::Sync
        } else {
            Mode::Normal
        };
        self.cmdline.clear();
        self.history_pos = None;
        self.completion = None;
    }

    /// Tab (or Shift-Tab backwards): extend the word to the candidates' common prefix, then
    /// cycle through them.
    fn complete_step(&mut self, forward: bool) {
        use crate::complete::{common_prefix, complete_in};
        if let Some((c, index)) = self.completion.as_mut() {
            if c.items.len() > 1 {
                let n = c.items.len();
                let i = match (*index, forward) {
                    (None, true) => 0,
                    (None, false) => n - 1,
                    (Some(i), true) => (i + 1) % n,
                    (Some(i), false) => (i + n - 1) % n,
                };
                *index = Some(i);
                self.cmdline.truncate(c.start);
                self.cmdline.push_str(&c.items[i]);
            }
            return;
        }
        let cwd = std::env::current_dir().unwrap_or_default();
        let score_dir = self
            .buffer()
            .path
            .as_deref()
            .and_then(Path::parent)
            .map(Path::to_path_buf);
        let kind = Some(self.buffer().score.is_chant());
        let c = complete_in(
            &self.cmdline,
            &cwd,
            score_dir.as_deref(),
            self.matcher(),
            kind,
        );
        let typed = self.cmdline[c.start..].to_string();
        // One candidate starting with what was typed completes outright, whatever fuzzy
        // matching also found.
        let prefixed: Vec<usize> = (0..c.items.len())
            .filter(|&i| self.matcher().is_prefix(&c.items[i], &typed))
            .collect();
        let unique = match (c.items.len(), prefixed.as_slice()) {
            (1, _) => Some(0),
            (_, [only]) if !typed.is_empty() => Some(*only),
            _ => None,
        };
        if let Some(i) = unique {
            self.cmdline.truncate(c.start);
            self.cmdline.push_str(&c.items[i]);
            if c.commands || !c.items[i].ends_with('/') {
                self.cmdline.push(' ');
            }
            return;
        }
        match c.items.len() {
            0 => self.error(format!("no completions for '{typed}'")),
            1 => {
                self.cmdline.truncate(c.start);
                self.cmdline.push_str(&c.items[0]);
                if c.commands || !c.items[0].ends_with('/') {
                    self.cmdline.push(' ');
                }
            }
            _ => {
                // Fill in what all candidates share, if that extends what was typed (with fuzzy
                // matching they may share a start unrelated to it).
                let prefix = common_prefix(&c.items, self.config.file.ui.completion_ignore_case);
                if prefix.chars().count() > typed.chars().count()
                    && self.matcher().is_prefix(&prefix, &typed)
                {
                    self.cmdline.truncate(c.start);
                    self.cmdline.push_str(&prefix);
                    self.completion = Some((c, None));
                } else {
                    self.completion = Some((c, None));
                    self.complete_step(forward);
                }
            }
        }
    }

    /// How completion matches, from the case and fuzzy settings.
    pub fn matcher(&self) -> crate::complete::Matcher {
        crate::complete::Matcher {
            ignore_case: self.config.file.ui.completion_ignore_case,
            fuzzy: self.config.file.ui.completion_fuzzy,
        }
    }

    /// The command palette while in Command mode: the typed line, the completions of the word
    /// being typed (updated as you type; Tab cycles through them) and which one is selected.
    pub fn palette(&self) -> Option<Palette> {
        use crate::complete::{complete_in, describe};
        if self.mode == Mode::Picker {
            return self.picker_palette();
        }
        if self.mode != Mode::Command {
            return None;
        }
        let (c, selected) = match &self.completion {
            Some((c, i)) => (c.clone(), *i),
            None => {
                let cwd = std::env::current_dir().unwrap_or_default();
                let score_dir = self.buffer().path.as_deref().and_then(Path::parent);
                let kind = Some(self.buffer().score.is_chant());
                (
                    complete_in(&self.cmdline, &cwd, score_dir, self.matcher(), kind),
                    None,
                )
            }
        };
        let items = c
            .items
            .into_iter()
            .zip(c.positions)
            .map(|(text, matched)| PaletteItem {
                description: if c.commands {
                    describe(&text).map(String::from)
                } else {
                    None
                },
                text,
                swatch: None,
                icon: None,
                matched,
            })
            .collect();
        Some(Palette {
            prompt: ":".into(),
            title: "Command".into(),
            input: self.cmdline.clone(),
            items,
            selected,
        })
    }

    pub(crate) fn run_counted(&mut self, cmd: Command, count: Option<usize>) {
        match (&cmd, count) {
            (Command::GotoFirst | Command::GotoLast, Some(n)) => self.run(Command::GotoBar(n)),
            // A counted section motion is one jump (for `''`), n sections away.
            (Command::SectionNext | Command::SectionPrev, Some(n)) => {
                self.section_jump(matches!(cmd, Command::SectionNext), n)
            }
            (c, Some(n)) if c.repeatable() => {
                for _ in 0..n.min(10_000) {
                    self.run(cmd.clone());
                }
            }
            _ => self.run(cmd),
        }
    }

    /// Executes a command.
    pub fn run(&mut self, cmd: Command) {
        use Command::*;
        self.palette_follow_kind();
        // Chant first: it takes over note deletion on chant staves.
        if self.run_chant(&cmd)
            || self.run_edit(&cmd)
            || self.run_playalong(&cmd)
            || self.run_tracks(&cmd)
            || self.run_properties(&cmd)
            || self.run_palette(&cmd)
        {
            return;
        }
        match cmd {
            BeatNext => self.move_cursor(|s, c| {
                step_beat(s, c, true);
            }),
            BeatPrev => self.move_cursor(|s, c| {
                step_beat(s, c, false);
            }),
            NoteNext => self.move_cursor(|s, c| step_note(s, c, true)),
            NotePrev => self.move_cursor(|s, c| step_note(s, c, false)),
            RowNext => self.move_cursor(row_next),
            RowEnd => self.move_cursor(row_end),
            RowPrev => self.move_cursor(row_prev),
            BarFirstNote => self.move_cursor(|s, c| {
                c.beat = beats(s, c)
                    .iter()
                    .position(|b| !b.notes.is_empty())
                    .unwrap_or(0)
            }),
            BarLastNote => self.move_cursor(|s, c| {
                let all = beats(s, c);
                c.beat = all
                    .iter()
                    .rposition(|b| !b.notes.is_empty())
                    .unwrap_or(all.len().saturating_sub(1))
            }),
            StringDown | StringUp => {
                let (down, between) = (
                    cmd == StringDown,
                    self.config.file.ui.jk_moves_between_staves,
                );
                self.move_cursor(|s, c| move_row(s, c, down, between));
            }
            StaffNext | StaffPrev => {
                let forward = cmd == StaffNext;
                self.move_cursor(|s, c| move_staff(s, c, forward));
            }
            BarNext => self.move_cursor(|s, c| set_bar(s, c, c.bar + 1)),
            BarPrev => self.move_cursor(|s, c| set_bar(s, c, c.bar.saturating_sub(1))),
            BarStart => self.move_cursor(|_, c| c.beat = 0),
            BarEnd => self.move_cursor(|s, c| c.beat = beats(s, c).len().saturating_sub(1)),
            // To the next / previous section's first bar (a jump, remembered for `''`).
            SectionNext => self.section_jump(true, 1),
            SectionPrev => self.section_jump(false, 1),
            // Jumps (remembered for `''`).
            GotoFirst | GotoLast | GotoBar(_) | GotoPos(..) => {
                let here = (self.pane().buffer, self.pane().cursor);
                match cmd {
                    GotoFirst => self.move_cursor(|s, c| set_bar(s, c, 0)),
                    GotoLast => self.move_cursor(|s, c| set_bar(s, c, usize::MAX)),
                    GotoBar(n) => self.move_cursor(|s, c| set_bar(s, c, n.saturating_sub(1))),
                    // A chant: `bar:syllable.note` (a missing note is the first, a division 0).
                    GotoPos(n, pos) if self.cursor_on_chant() => {
                        match self.chant_place_target(n, pos) {
                            Some((bar, beat)) => self.move_cursor(|_, c| {
                                c.bar = bar;
                                c.beat = beat;
                            }),
                            None => self.error(format!("no such place: {n}:{pos}")),
                        }
                    }
                    GotoPos(n, pos) => self.move_cursor(|s, c| {
                        set_bar(s, c, n.saturating_sub(1));
                        let Some(track) = s.tracks.get(c.track) else {
                            return;
                        };
                        // The last beat that starts at or before the position.
                        let n = beats(s, c).len();
                        c.beat = (0..n)
                            .rev()
                            .find(|&b| {
                                cecilia_core::position::beat_position(s, track, c.bar, c.voice, b)
                                    <= pos + 1e-6
                            })
                            .unwrap_or(0);
                    }),
                    _ => unreachable!(),
                }
                if self.pane().cursor != here.1 {
                    self.pane_mut().jump_back = Some(here);
                }
            }
            Mark(name) => self.set_mark(name),
            DeleteMarks(names) => self.delete_marks(names.as_deref()),
            TrackNext => self.move_cursor(|s, c| set_track(s, c, c.track + 1)),
            TrackPrev => self.move_cursor(|s, c| set_track(s, c, c.track.saturating_sub(1))),
            Track(n) => self.move_cursor(|s, c| set_track(s, c, n.saturating_sub(1))),
            PageDown => self.half_page(true),
            PageUp => self.half_page(false),
            PlayToggle => {
                let here = (self.focused(), self.pane().buffer, self.pane().cursor);
                if self.playing && self.playing_buffer == Some(here.1) {
                    self.run(Pause)
                } else if self.playing {
                    // Another file is playing: it stops, and this one starts.
                    self.run(Play)
                } else if self.paused && self.paused_at == Some(here) && !self.syncing {
                    // The cursor is still where playback paused: carry on exactly from there
                    // (Sync mode always starts at the cursor).
                    self.paused = false;
                    self.effects.push(Effect::Resume);
                } else {
                    // Playing always starts at the edit cursor.
                    self.run(Play)
                }
            }
            Play => {
                let pane = self.pane().clone();
                let score = &self.buffers[pane.buffer].score;
                if score.tracks.is_empty() {
                    self.error("nothing to play");
                    return;
                }
                let tick = cursor_tick(score, &pane.cursor);
                self.play_start = Some((self.focused(), pane.cursor));
                self.playing_buffer = Some(pane.buffer);
                self.paused = false;
                self.paused_at = None;
                self.effects.push(Effect::Play {
                    buffer: pane.buffer,
                    bar: pane.cursor.bar,
                    tick,
                });
            }
            Pause => {
                if self.playing || self.paused {
                    self.paused = true;
                    self.paused_at = Some((self.focused(), self.pane().buffer, self.pane().cursor));
                }
                self.effects.push(Effect::Pause);
            }
            Stop => {
                self.paused = false;
                self.paused_at = None;
                let follow = self.config.file.ui.follow_playback;
                if let Some((pane_id, cursor)) = self.play_start.take() {
                    if let Some(p) = self.panes.get_mut(&pane_id) {
                        // A cursor that followed the playhead goes back with it; one moved
                        // independently stays where it is.
                        if follow {
                            p.cursor = cursor;
                        }
                        let tick = cursor_tick(&self.buffers[p.buffer].score, &cursor);
                        self.effects.push(Effect::Stop {
                            buffer: p.buffer,
                            bar: cursor.bar,
                            tick,
                        });
                        return;
                    }
                }
                self.effects.push(Effect::Pause);
            }
            Pick(kind) => self.open_picker(kind),
            Split => self.split(Axis::Stacked),
            // A tool pane changes its own size (in percent of the view's); score panes share.
            Resize(axis, amount) if self.focused_kind() != PaneKind::Score => {
                let id = self.window_focus();
                let total = match axis {
                    Axis::Stacked => self.viewport.h,
                    Axis::SideBySide => self.viewport.w,
                } as i64;
                let now = self.fixed_size(id, axis).unwrap_or(0) as i64;
                let size = match amount {
                    crate::command::Amount::To(n) => n as i64 * total / 100,
                    crate::command::Amount::By(d) => now + d as i64 * total / 100,
                };
                self.set_tool_size(id, axis, size.clamp(2, total.max(2)) as u16);
            }
            Resize(axis, amount) => {
                let id = self.window_focus();
                let (to, by) = match amount {
                    crate::command::Amount::To(n) => (Some(n as f32), 0.0),
                    crate::command::Amount::By(d) => (None, d as f32),
                };
                let view = &mut self.views[self.active_view];
                if !view.root.resize(id, axis, to, by) {
                    self.error(match axis {
                        Axis::Stacked => "resize: no pane above or below this one",
                        Axis::SideBySide => "vertical-resize: no pane beside this one",
                    });
                }
            }
            ResizeEqual => self.views[self.active_view].root.equalize(),
            Toolbar(on) => self.toolbar = on.unwrap_or(!self.toolbar),
            Set(name, value) => self.set_option(&name, value.as_deref()),
            Get(name) => match crate::options::get(&self.config.file, &name) {
                Ok(v) => self.info(format!("{name} = {v}")),
                Err(e) => self.error(e),
            },
            Fretboard(on) => self.set_tool(PaneKind::Fretboard, on),
            Design(cmd) => self.run_design(cmd),
            Piano(on) => self.set_tool(PaneKind::Piano, on),
            Tuner(on) => self.set_tool(PaneKind::Tuner, on),
            Maximize => {
                let on = self.maximized().is_none();
                let v = &mut self.views[self.active_view];
                if on && v.root.panes().len() < 2 {
                    return self.info("only one pane in this view");
                }
                v.maximized = on.then_some(v.focus);
                self.info(if on {
                    "pane maximized (again to restore)"
                } else {
                    "panes restored"
                });
            }
            // The tracks pane taller or shorter, as `:resize` on it.
            // The tracks pane showing N rows (or N more / fewer).
            TracksResize(amount) => {
                self.set_tool(PaneKind::Tracks, Some(true));
                let id = self.tool_pane(PaneKind::Tracks).unwrap();
                let row = self.row_height.max(1) as i64;
                let now = self.fixed_size(id, Axis::Stacked).unwrap_or(0) as i64 / row - 3;
                let rows = amount.apply(now).max(1);
                self.set_tool_size(id, Axis::Stacked, ((rows + 3) * row) as u16);
            }
            WaveformResize(amount) => {
                let h = amount.apply(self.waveform_height as i64);
                self.waveform_height =
                    h.clamp(MIN_WAVEFORM_HEIGHT as i64, MAX_WAVEFORM_HEIGHT as i64) as u16;
            }
            VSplit => self.split(Axis::SideBySide),
            Focus(dir) => {
                let from = self.window_focus();
                self.touch_focus(from);
                let recency = |id| self.focus_recency.get(&id).copied().unwrap_or(0);
                if let Some(id) = neighbour(&self.pane_rects(), from, dir, recency) {
                    self.set_focus(id);
                }
            }
            FocusCycle(forward) => {
                let panes = self.view().root.panes();
                let from = self.window_focus();
                if let Some(i) = panes.iter().position(|p| *p == from) {
                    let n = panes.len();
                    self.set_focus(panes[(i + if forward { 1 } else { n - 1 }) % n]);
                }
            }
            FocusLast => {
                let from = self.window_focus();
                let last = self
                    .view()
                    .root
                    .panes()
                    .into_iter()
                    .filter(|p| *p != from)
                    .max_by_key(|p| self.focus_recency.get(p).copied().unwrap_or(0));
                match last {
                    Some(id) => self.set_focus(id),
                    None => self.info("only one pane in this view"),
                }
            }
            PaneMove(dir) => {
                let (id, v) = (self.window_focus(), self.active_view);
                self.views[v].maximized = None;
                if !self.views[v].root.move_to_edge(id, dir) {
                    self.info("only one pane in this view");
                }
            }
            PaneExchange | PaneRotate(_) => {
                let (id, v) = (self.window_focus(), self.active_view);
                self.views[v].maximized = None;
                let root = &mut self.views[v].root;
                let moved = match cmd {
                    PaneExchange => root.exchange(id),
                    _ => root.rotate(id, matches!(cmd, PaneRotate(true))),
                };
                if !moved {
                    self.info("only one pane in this view");
                }
            }
            ClosePane => self.close_pane(),
            Quit { force } => self.try_quit(force),
            WriteAll => {
                self.write_all();
            }
            WriteQuit => {
                if self.write_all() {
                    self.try_quit(false);
                }
            }
            WorkspaceSave(target, force) => self.save_workspace(target.as_deref(), force),
            WorkspaceOpen(name, discard) => self.open_workspace(&name, discard),
            WorkspaceDelete(name, force) => self.delete_workspace(name.as_deref(), force),
            ViewNext => self.active_view = (self.active_view + 1) % self.views.len(),
            ViewPrev => {
                self.active_view = (self.active_view + self.views.len() - 1) % self.views.len()
            }
            ViewGoto(n) => match n.checked_sub(1).filter(|&i| i < self.views.len()) {
                Some(i) => self.active_view = i,
                None => self.error(format!("no view {n} ({} views)", self.views.len())),
            },
            ViewNew(path) => {
                let file = match path {
                    Some(p) => match self.open(&p) {
                        Ok(b) => Some(b),
                        Err(e) => return self.error(e),
                    },
                    None => None,
                };
                self.new_view();
                if let Some(b) = file {
                    self.show_buffer(b);
                }
            }
            ViewRename(name) => {
                self.views[self.active_view].name = name.filter(|n| !n.trim().is_empty());
            }
            BufferNext => self.cycle_buffer(1),
            BufferPrev => self.cycle_buffer(-1),
            BufferMove(amount) => {
                let b = self.pane().buffer;
                let tabs = &self.view().buffers;
                let Some(from) = tabs.iter().position(|&t| t == b) else {
                    return self.error("this file has no tab here");
                };
                let to = match amount {
                    crate::command::Amount::To(n) => n as i64 - 1,
                    crate::command::Amount::By(d) => from as i64 + d as i64,
                };
                self.move_tab(from, to.clamp(0, tabs.len() as i64 - 1) as usize);
            }
            BufferGoto(n) => match n.checked_sub(1).and_then(|i| self.view().buffers.get(i)) {
                Some(&b) => self.show_buffer(b),
                None => self.error(format!("no buffer tab {n}")),
            },
            BufferClose(n, force) => {
                let target = match n {
                    None => Some(self.pane().buffer),
                    Some(n) => n
                        .checked_sub(1)
                        .and_then(|i| self.view().buffers.get(i).copied()),
                };
                match target {
                    Some(b) => self.close_buffer(b, force),
                    None => self.error("no such buffer tab"),
                }
            }
            ViewClose(force) => self.close_view(force),
            Recover(path, force) => self.recover(path, force),
            SwapDiscard => self.swap_discard(),
            Edit(path) => match self.open(&path) {
                Ok(b) => {
                    self.show_buffer(b);
                    self.swap_show_notices();
                }
                Err(e) => self.error(e),
            },
            OpenDialog => {
                let dir = self
                    .buffer()
                    .path
                    .as_deref()
                    .and_then(Path::parent)
                    .map(Path::to_path_buf);
                self.effects.push(Effect::OpenDialog { dir });
            }
            Write(path) => self.write(path),
            SaveAs(Some(path)) => self.save_as(path),
            SaveAs(None) => self.request_save_dialog(),
            Source => self.reload_config(),
            CommandMode => self.open_command_line(""),
            Normal => {
                if self.mode == Mode::Sync {
                    return self.leave_sync_mode();
                }
                self.leave_command_mode();
                self.ed.visual_anchor = None;
            }
            Help => self.show_help = !self.show_help,
            Layout(name) => {
                // Only the focused pane changes.
                let pane = self.pane_mut();
                pane.layout = name.unwrap_or_else(|| {
                    match pane.layout.as_str() {
                        "vertical" => "page",
                        "page" => "horizontal",
                        _ => "vertical",
                    }
                    .to_string()
                });
                let msg = format!("layout: {}", pane.layout);
                self.info(msg);
            }
            ScrollCursor(to) => {
                let id = self.focused();
                if let Some(p) = self.panes.get_mut(&id) {
                    p.scroll_request = Some(to);
                }
            }
            // The waveform pane, when it has the focus, zooms its own timeline.
            ZoomIn | ZoomOut | ZoomReset if self.focused_kind() == PaneKind::Audio => {
                let id = self.window_focus();
                if let Some(p) = self.panes.get_mut(&id) {
                    p.wave_zoom = Some(match cmd {
                        ZoomIn => 1.25,
                        ZoomOut => 0.8,
                        _ => 0.0,
                    });
                }
            }
            ZoomIn => self.zoom_pane(self.focused(), 1.15),
            ZoomOut => self.zoom_pane(self.focused(), 1.0 / 1.15),
            ZoomReset => {
                let z = self.config.file.ui.zoom;
                let id = self.focused();
                if let Some(p) = self.panes.get_mut(&id) {
                    p.zoom = z;
                }
            }
            Nop => {}
            _ => {}
        }
    }

    /// Writes the focused buffer. Without a path it writes back to the buffer's file, but only
    /// when there are changes, so opening and saving never rewrites an untouched original.
    fn write(&mut self, path: Option<PathBuf>) {
        let index = self.pane().buffer;
        // A buffer with no file has nowhere to be written back to: ask for one.
        if path.is_none() && self.buffers[index].path.is_none() {
            return self.request_save_dialog();
        }
        // `:write FILE` is `:save-as FILE`: the buffer takes the new file.
        if let Some(path) = path {
            return self.save_as(path);
        }
        self.write_buffer(index, path);
    }

    /// Asks the frontend for a file to save the focused buffer as (see [`Effect::SaveDialog`]).
    fn request_save_dialog(&mut self) {
        let buffer = self.buffer();
        let dir = buffer
            .path
            .as_deref()
            .and_then(Path::parent)
            .map(Path::to_path_buf);
        // The file's name, else the title in kebab-case (a new file), else a placeholder.
        let slug = kebab_case(&buffer.score.meta.title);
        let ext = native_extension(&buffer.score);
        let name = if buffer.path.is_some() {
            buffer.name.clone()
        } else if !slug.is_empty() {
            format!("{slug}.{ext}")
        } else {
            format!("untitled.{ext}")
        };
        self.effects.push(Effect::SaveDialog { dir, name });
    }

    /// `:save-as`: writes the focused buffer to `path` (format by extension) and makes that its
    /// file, so a later `:w` writes there. The buffer counts as saved. Nothing changes if the
    /// write fails or another buffer already has that file.
    pub fn save_as(&mut self, mut path: PathBuf) {
        // A name without an extension is a native file.
        if path.extension().is_none() {
            path.set_extension(native_extension(&self.buffer().score));
        }
        let index = self.pane().buffer;
        let canonical = path.canonicalize().ok();
        if let Some(other) = self.buffers.iter().enumerate().position(|(i, b)| {
            i != index && b.path.is_some() && b.path.as_deref() == canonical.as_deref()
        }) {
            let name = self.buffers[other].name.clone();
            return self.error(format!("\"{name}\" is already open in another buffer tab"));
        }
        if !self.write_buffer(index, Some(path.clone())) {
            return;
        }
        let canonical = path.canonicalize().unwrap_or(path.clone());
        // The old swap file (named by the old path or slot) goes before the name changes.
        self.swap_release(index);
        let buffer = &mut self.buffers[index];
        buffer.name = path.file_name().map_or_else(
            || path.display().to_string(),
            |n| n.to_string_lossy().into(),
        );
        buffer.disk = file_stamp(&canonical);
        buffer.path = Some(canonical);
        buffer.modified = false;
        buffer.history.mark_saved();
    }

    /// `:wall`: writes every changed file. False if one could not be written (or has no name).
    fn write_all(&mut self) -> bool {
        let changed: Vec<usize> = (0..self.buffers.len())
            .filter(|&b| self.buffers[b].modified)
            .collect();
        let mut written = 0;
        for b in changed {
            if self.buffers[b].path.is_none() {
                self.error(format!(
                    "\"{}\" has no file name (show it and :w <file>)",
                    self.buffers[b].name
                ));
                return false;
            }
            self.write_buffer(b, None);
            if self.buffers[b].modified {
                return false;
            }
            written += 1;
        }
        self.info(match written {
            0 => "no changes to write".to_string(),
            1 => "1 file written".to_string(),
            n => format!("{n} files written"),
        });
        true
    }

    /// Quits cecilia; with unsaved changes (unless forced) it asks about each changed file first.
    fn try_quit(&mut self, force: bool) {
        if force {
            self.swap_clear();
            self.quit = true;
        } else {
            self.quit_skipping(Vec::new());
        }
    }

    /// Quits unless a buffer not in `skip` (answered `n`) has unsaved changes: that is asked
    /// about.
    fn quit_skipping(&mut self, skip: Vec<usize>) {
        match (0..self.buffers.len()).find(|&b| self.buffers[b].modified && !skip.contains(&b)) {
            Some(b) => self.ask_save(Closing::Quit, b, skip),
            None => {
                self.swap_clear();
                self.autosave_workspace_on_quit();
                self.quit = true;
            }
        }
    }

    /// Writes buffer `index`; false if nothing was written (an error was reported).
    fn write_buffer(&mut self, index: usize, path: Option<PathBuf>) -> bool {
        let buffer = &self.buffers[index];
        let target = match path {
            Some(p) => p,
            None => {
                let Some(p) = buffer.path.clone() else {
                    self.error("no file name (use :w <file>)");
                    return false;
                };
                if !buffer.modified {
                    self.info("no changes to write");
                    return true;
                }
                p
            }
        };
        // Guitar Pro keeps the backing track inside the file: embed an attached recording.
        let mut embedded;
        let mut score = &buffer.score;
        let is_gp = target
            .extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("gp"));
        if let Some(audio) = score
            .audio
            .as_ref()
            .filter(|a| is_gp && a.embedded.is_none())
            && let Some(file) = audio.file.as_deref()
        {
            let file = buffer
                .path
                .as_deref()
                .and_then(Path::parent)
                .map_or_else(|| PathBuf::from(file), |d| d.join(file));
            match std::fs::read(&file) {
                Ok(bytes) => {
                    embedded = score.clone();
                    embedded.audio.as_mut().unwrap().embedded = Some(std::sync::Arc::new(bytes));
                    score = &embedded;
                }
                Err(e) => {
                    self.error(format!("{}: {e}", file.display()));
                    return false;
                }
            }
        }
        let ext = target
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or_default()
            .to_string();
        let lost = cecilia_formats::voices_lost(score, &ext);
        let rows_lost = cecilia_formats::lyric_rows_lost(score, &ext);
        let audio_lost = cecilia_formats::audio_lost(score, &ext);
        let native = native_extension(score);
        match cecilia_formats::write_file(score, &target) {
            Ok(()) => {
                let size = std::fs::metadata(&target).map(|m| m.len()).unwrap_or(0);
                if buffer.path.as_deref() == Some(target.as_path()) {
                    self.buffers[index].disk = file_stamp(&target);
                    self.buffers[index].modified = false;
                    self.buffers[index].history.mark_saved();
                    self.swap_release(index);
                }
                let mut note = if lost > 0 {
                    format!(
                        " (warning: {lost} voice bar{} beyond what .{ext} stores left out)",
                        if lost == 1 { "" } else { "s" }
                    )
                } else {
                    String::new()
                };
                if rows_lost > 0 {
                    note.push_str(&format!(
                        " (warning: {rows_lost} verse row{} beyond the five lyric lines .{ext} stores left out)",
                        if rows_lost == 1 { "" } else { "s" }
                    ));
                }
                if audio_lost {
                    note.push_str(&format!(
                        " (warning: the audio and its sync are not saved in .{ext}; save as \
                         .{native} to keep them)"
                    ));
                }
                self.info(format!(
                    "\"{}\" written, {size} bytes{note}",
                    target.display()
                ));
                true
            }
            Err(e) => {
                self.error(format!("{}: {e}", target.display()));
                false
            }
        }
    }

    /// Puts a pane's cursor back in range after a frontend set its fields directly (a click).
    /// On a slash staff this puts it on the staff's one line, wherever the click fell.
    pub fn normalize_cursor(&mut self, pane: PaneId) {
        if let Some(p) = self.panes.get_mut(&pane) {
            if let Some(b) = self.buffers.get(p.buffer) {
                clamp_cursor(&b.score, &mut p.cursor);
            }
        }
    }

    fn move_cursor(&mut self, f: impl FnOnce(&Score, &mut Cursor)) {
        let id = self.focused();
        let pane = self.panes.get_mut(&id).unwrap();
        let score = &self.buffers[pane.buffer].score;
        if score.tracks.is_empty() {
            return;
        }
        let before = (pane.cursor.track, pane.cursor.bar, pane.cursor.beat);
        let insert = self.mode == Mode::Insert;
        f(score, &mut pane.cursor);
        clamp_cursor(score, &mut pane.cursor);
        // On a chant the cursor stands on the staff line of the note it moved to (up and down
        // then choose the pitch of a note to enter).
        let c = &mut pane.cursor;
        // (In Insert mode the cursor just moves along: its staff line is the pitch of the next
        // note entered.)
        if !insert && (c.track, c.bar, c.beat) != before {
            if let Some(chant) = cursor_chant(score, c) {
                let t = cecilia_core::chant::transcribe(chant);
                if let Some(note) = t.note_at(c.bar, c.beat).and_then(|n| chant.note(n)) {
                    c.line = note.pitch as i32;
                }
            }
        }
    }

    /// A new score pane on `buffer`, with the score pane's layout.
    fn new_pane(&mut self, buffer: usize) -> PaneId {
        let id = self.next_pane;
        self.next_pane += 1;
        let (layout, waveform, zoom) = self.panes.get(&self.focused()).map_or_else(
            || (self.score_layout.clone(), true, self.config.file.ui.zoom),
            |p| (p.layout.clone(), p.waveform, p.zoom),
        );
        self.panes.insert(
            id,
            Pane {
                kind: PaneKind::Score,
                size: (None, None),
                manual_size: false,
                owner: None,
                buffer,
                cursor: Cursor::default(),
                scroll: 0,
                layout,
                waveform,
                zoom,
                recent: Vec::new(),
                jump_back: None,
                scroll_request: None,
                wave_zoom: None,
            },
        );
        id
    }

    fn split(&mut self, axis: Axis) {
        let current = self.pane().clone();
        let id = self.new_pane(current.buffer);
        let pane = self.panes.get_mut(&id).unwrap();
        pane.cursor = current.cursor;
        pane.recent = current.recent;
        // The focused pane (a score or a tool) gives up half its room to a new score pane.
        let focus = self.window_focus();
        self.views[self.active_view].root.split(focus, id, axis);
        self.set_focus(id);
    }

    /// The score panes of the active view.
    fn score_panes(&self) -> Vec<PaneId> {
        self.view()
            .root
            .panes()
            .into_iter()
            .filter(|id| {
                self.panes
                    .get(id)
                    .is_some_and(|p| p.kind == PaneKind::Score)
            })
            .collect()
    }

    /// `:pane-close` on the focused pane: closes it, its view with the last pane. The last pane
    /// of the last view stays (closing the last buffer is what shows the dashboard).
    fn close_pane(&mut self) {
        let focus = self.window_focus();
        // A tool pane, or one of several score panes, just closes; the last score pane closes
        // the view.
        let tool = self.focused_kind() != PaneKind::Score;
        if tool || self.score_panes().len() > 1 {
            self.close_pane_id(focus);
        } else if self.views.len() > 1 {
            self.close_view(false);
        } else {
            self.error("cannot close the last pane");
        }
    }

    /// Closes a pane of the active view (focus moves to its nearest neighbour). The last pane
    /// closes the view; the last pane of the last view shows an empty score instead.
    fn close_pane_id(&mut self, id: PaneId) {
        let rects = self.pane_rects();
        let tool = self
            .panes
            .get(&id)
            .is_some_and(|p| p.kind != PaneKind::Score);
        if !tool && self.score_panes().len() <= 1 {
            if self.views.len() > 1 {
                self.close_view(true);
            } else {
                self.buffers.push(Buffer::empty());
                let empty = self.buffers.len() - 1;
                self.show_dashboard();
                let pane = self.panes.get_mut(&id).unwrap();
                pane.buffer = empty;
                pane.cursor = Cursor::default();
                pane.recent.clear();
            }
            return;
        }
        // A score pane's local tools close with it.
        let tools: Vec<PaneId> = self
            .panes
            .iter()
            .filter(|(_, p)| p.owner == Some(id))
            .map(|(t, _)| *t)
            .collect();
        let equalize = self.config.file.ui.equalize_on_close;
        let view = &mut self.views[self.active_view];
        for t in &tools {
            view.root.remove_with(*t, equalize);
            if view.focus == *t {
                view.focus = id;
            }
        }
        for t in &tools {
            self.panes.remove(t);
        }
        let view = &mut self.views[self.active_view];
        view.root.remove_with(id, equalize);
        let (focused, score) = (view.focus == id, view.score == id);
        let remaining = view.root.panes();
        self.panes.remove(&id);
        if score {
            let other = self.score_panes()[0];
            self.views[self.active_view].score = other;
        }
        if focused {
            // Focus the nearest remaining pane, as vim does.
            let next = [
                crate::command::Direction::Left,
                crate::command::Direction::Up,
                crate::command::Direction::Right,
                crate::command::Direction::Down,
            ]
            .into_iter()
            .find_map(|d| {
                neighbour(&rects, id, d, |p| {
                    self.focus_recency.get(&p).copied().unwrap_or(0)
                })
                .filter(|n| remaining.contains(n))
            })
            .unwrap_or(remaining[0]);
            self.set_focus(next);
        }
    }

    // ---- playback feedback ----

    /// Records whether the player is running (called by the frontend).
    pub fn set_playing(&mut self, playing: bool) {
        if self.playing && !playing && !self.paused {
            // The song ended by itself.
            self.play_start = None;
        }
        self.playing = playing;
    }

    /// While following playback, keeps the focused pane's cursor on the playing beat (the
    /// frontend calls this each frame while playing). With following off the cursor stays put,
    /// so turning it off mid-playback leaves the cursor where the music was at that moment.
    /// While playing (or paused) with follow on, a cursor moved by hand takes the play position
    /// with it.
    pub(crate) fn seek_to_cursor(&mut self, before: (PaneId, usize, Cursor)) {
        let pane = self.pane();
        let now = (self.focused(), pane.buffer, pane.cursor);
        // Syncing: moving the cursor (Enter moves it on) does not move the playback, which
        // runs on by itself; the cursor follows it when following is on.
        if !(self.playing || self.paused)
            || !self.config.file.ui.follow_playback
            || self.syncing
            || now == before
            || self.playing_buffer != Some(now.1)
            // Play and Stop already put the playhead at the cursor.
            || self.effects.iter().any(|e| matches!(e, Effect::Play { .. } | Effect::Stop { .. }))
        {
            return;
        }
        let tick = cursor_tick(&self.buffers[now.1].score, &now.2);
        self.seeked = Some(std::time::Instant::now());
        self.effects.push(Effect::Seek {
            buffer: now.1,
            bar: now.2.bar,
            tick,
        });
    }

    pub fn follow_playback(&mut self, buffer: usize, bar: usize, tick: u32) {
        if !self.config.file.ui.follow_playback {
            return;
        }
        if let Some(t) = self.seeked {
            if t.elapsed() < std::time::Duration::from_millis(100) {
                return;
            }
            self.seeked = None;
        }
        let score = &self.buffers[buffer].score;
        let focused = self.focused();
        if let Some(p) = self.panes.get_mut(&focused).filter(|p| p.buffer == buffer) {
            if p.cursor.bar != bar
                || cursor_tick(score, &p.cursor) > tick
                || beat_end(score, &p.cursor) <= tick
            {
                p.cursor.bar = bar;
                p.cursor.beat = beat_at_tick(score, p.cursor.track, bar, tick);
            }
        }
    }
}

/// A file's size and modification time (none: it does not exist).
pub(crate) fn file_stamp(path: &Path) -> Option<(SystemTime, u64)> {
    let m = std::fs::metadata(path).ok()?;
    Some((m.modified().ok()?, m.len()))
}

fn mtime(path: &Path) -> Option<SystemTime> {
    std::fs::metadata(path).and_then(|m| m.modified()).ok()
}

// ---- cursor helpers ----

/// Beats of voice 1 at the cursor's track and bar.
pub fn beats<'a>(score: &'a Score, c: &Cursor) -> &'a [Beat] {
    score
        .tracks
        .get(c.track)
        .and_then(|t| t.bars.get(c.bar))
        .and_then(|b| b.voices.get(c.voice))
        .map_or(&[], |v| v.beats.as_slice())
}

/// Tick within the bar at which the cursor's beat starts.
pub fn cursor_tick(score: &Score, c: &Cursor) -> u32 {
    beats(score, c).iter().take(c.beat).map(Beat::ticks).sum()
}

fn beat_end(score: &Score, c: &Cursor) -> u32 {
    cursor_tick(score, c) + beats(score, c).get(c.beat).map_or(0, |b| b.ticks().max(1))
}

/// Index of the beat sounding at `tick` in a bar.
pub fn beat_at_tick(score: &Score, track: usize, bar: usize, tick: u32) -> usize {
    let c = Cursor {
        track,
        bar,
        ..Cursor::default()
    };
    let mut t = 0;
    for (i, b) in beats(score, &c).iter().enumerate() {
        t += b.ticks();
        if t > tick {
            return i;
        }
    }
    beats(score, &c).len().saturating_sub(1)
}

impl Buffer {
    /// The placeholder buffer of a session without files.
    pub fn empty() -> Self {
        Self {
            path: None,
            name: "[No file]".into(),
            score: Score::default(),
            modified: false,
            history: Default::default(),
            version: 0,
            disk: None,
        }
    }
}

pub(crate) fn clamp_cursor(score: &Score, c: &mut Cursor) {
    c.track = c.track.min(score.tracks.len().saturating_sub(1));
    c.bar = c.bar.min(score.master_bars.len().saturating_sub(1));
    c.beat = c.beat.min(beats(score, c).len().saturating_sub(1));
    let strings = score
        .tracks
        .get(c.track)
        .map_or(1, |t| t.strings.len().max(1));
    c.string = c.string.min(strings - 1);
    // The cursor's staff exists, and a position left from another kind of staff goes back
    // to the middle.
    let Some(track) = score.tracks.get(c.track) else {
        return;
    };
    let staves = track.staves();
    c.staff = staff_index(track, c.staff);
    let staff = staves[c.staff];
    if !uses_strings(track, staff) {
        let middle = staff_middle(track, staff);
        // A slash staff is a single line: the cursor always sits on it.
        if staff == Staff::Slash || (c.line - middle).abs() > 24 {
            c.line = middle;
        }
    }
}

/// Whether the cursor sits on strings on a staff: a tab, or a drum staff's lanes.
pub(crate) fn uses_strings(track: &Track, staff: Staff) -> bool {
    match staff {
        Staff::Tab | Staff::TabRhythm => !track.strings.is_empty(),
        Staff::Standard(_) => track.kind == TrackKind::Drums,
        Staff::Slash => false,
    }
}

/// Middle of a staff (its treble half on a grand staff; a GABC position on a chant track).
fn staff_middle(track: &Track, staff: Staff) -> i32 {
    if track.chant.is_some() {
        return 6;
    }
    match staff {
        Staff::Standard(clef) => track.written_clef(clef, false).bottom_line() + 4,
        _ => track.written_clef(Clef::Treble, false).bottom_line() + 4,
    }
}

/// Rows of one staff, top first: its strings, or its lines and spaces (with those just above
/// and below it, and as far as the notes of the cursor beat on that staff reach).
fn staff_rows(score: &Score, c: &Cursor, track: &Track, index: usize, staff: Staff) -> Vec<Row> {
    if uses_strings(track, staff) {
        return (0..track.strings.len().max(1))
            .map(|s| Row::String(index, s))
            .collect();
    }
    let lines = |lo: i32, hi: i32| (lo..=hi).rev().map(move |p| Row::Line(index, p));
    let key = score
        .master_bars
        .get(c.bar)
        .map_or(0, |m| m.key_signature.accidentals);
    let notes: Vec<u8> = beats(score, c)
        .get(c.beat)
        .map(|b| b.notes.iter().map(|n| track.pitch_of(n)).collect())
        .unwrap_or_default();
    // A staff's lines in a clef, widened to the notes it shows.
    let range = |clef: StaffClef, shows: &dyn Fn(u8) -> bool| {
        let bottom = clef.bottom_line();
        let (mut lo, mut hi) = (bottom - 1, bottom + 9);
        for &p in notes.iter().filter(|&&p| shows(p)) {
            let d = staff::diatonic(p as i32 + clef.transpose(), key);
            lo = lo.min(d);
            hi = hi.max(d);
        }
        (lo, hi)
    };
    if track.chant.is_some() {
        return lines(1, 11).collect();
    }
    match staff {
        Staff::Slash => vec![Row::Line(index, staff_middle(track, staff))],
        Staff::Standard(clef @ Clef::Grand { split }) => {
            let (t_lo, t_hi) = range(track.written_clef(clef, false), &|p| p >= split);
            let (b_lo, b_hi) = range(track.written_clef(clef, true), &|p| p < split);
            lines(t_lo, t_hi)
                .chain(lines(b_lo, b_hi.min(t_lo - 1)))
                .collect()
        }
        Staff::Standard(clef) => {
            let (lo, hi) = range(track.written_clef(clef, false), &|_| true);
            lines(lo, hi).collect()
        }
        // A tab without strings is shown as a treble staff.
        Staff::Tab | Staff::TabRhythm => {
            let (lo, hi) = range(track.written_clef(Clef::Treble, false), &|_| true);
            lines(lo, hi).collect()
        }
    }
}

/// Every row of the cursor's track, top first: each staff's rows in turn.
fn rows(score: &Score, c: &Cursor) -> Vec<Row> {
    let Some(track) = score.tracks.get(c.track) else {
        return Vec::new();
    };
    track
        .staves()
        .into_iter()
        .enumerate()
        .flat_map(|(i, staff)| staff_rows(score, c, track, i, staff))
        .collect()
}

/// The cursor's row.
pub fn cursor_row(score: &Score, c: &Cursor) -> Row {
    let on_strings = score
        .tracks
        .get(c.track)
        .and_then(|t| {
            t.staves()
                .get(staff_index(t, c.staff))
                .map(|&s| uses_strings(t, s))
        })
        .unwrap_or(true);
    if on_strings {
        Row::String(c.staff, c.string)
    } else {
        Row::Line(c.staff, c.line)
    }
}

fn set_row(c: &mut Cursor, row: Row) {
    match row {
        Row::Line(staff, p) => {
            c.staff = staff;
            c.line = p;
        }
        Row::String(staff, s) => {
            c.staff = staff;
            c.string = s;
        }
    }
}

/// Ledger-line positions a staff offers beyond its own rows when the cursor keeps to it.
const LEDGER_ROWS: i32 = 4;

/// The rows of the cursor's staff alone, with ledger positions above and below a line staff.
fn own_rows(score: &Score, c: &Cursor) -> Vec<Row> {
    let Some(track) = score.tracks.get(c.track) else {
        return Vec::new();
    };
    let staves = track.staves();
    let index = staff_index(track, c.staff);
    let staff = staves[index];
    let mut rows = staff_rows(score, c, track, index, staff);
    if !(track.chant.is_some() || staff == Staff::Slash) && !uses_strings(track, staff) {
        if let (Some(&Row::Line(_, top)), Some(&Row::Line(_, bottom))) = (rows.first(), rows.last())
        {
            let above = (top + 1..=top + LEDGER_ROWS)
                .rev()
                .map(|p| Row::Line(index, p));
            let below = (bottom - LEDGER_ROWS..bottom)
                .rev()
                .map(|p| Row::Line(index, p));
            rows = above.chain(rows).chain(below).collect();
        }
    }
    rows
}

/// The cursor to the next or previous staff of its track (it stops at the first and last).
fn move_staff(score: &Score, c: &mut Cursor, forward: bool) {
    let Some(track) = score.tracks.get(c.track) else {
        return;
    };
    let staves = track.staves();
    let here = staff_index(track, c.staff);
    let to = if forward {
        (here + 1).min(staves.len() - 1)
    } else {
        here.saturating_sub(1)
    };
    c.staff = to;
    // A line left over from another kind of staff goes back to the middle.
    if to != here && !uses_strings(track, staves[to]) {
        let shown = staff_rows(score, c, track, to, staves[to]);
        if !shown.contains(&Row::Line(to, c.line)) {
            c.line = staff_middle(track, staves[to]);
        }
    }
}

/// One row down or up through the track's staves; it stops at the first and last row.
///
/// With `between_staves` false it stays on the cursor's staff, where a line staff goes on a
/// few positions past its edges for ledger lines; true goes on into the neighbouring staff.
fn move_row(score: &Score, c: &mut Cursor, down: bool, between_staves: bool) {
    if let Some(t) = score.tracks.get(c.track) {
        c.staff = staff_index(t, c.staff);
    }
    let here = if between_staves {
        rows(score, c)
    } else {
        own_rows(score, c)
    };
    let row = cursor_row(score, c);
    // A position off the list (a ledger line left by an edit) steps to its neighbour.
    let i = here.iter().position(|r| *r == row).or_else(|| match row {
        Row::Line(staff, p) => here
            .iter()
            .position(|r| matches!(r, Row::Line(s, q) if *s == staff && *q < p)),
        Row::String(..) => None,
    });
    let target = match (i, down) {
        (Some(i), true) if i + 1 < here.len() => Some(here[i + 1]),
        (Some(i), false) if i > 0 => Some(here[i - 1]),
        (None, _) if !here.is_empty() => Some(here[0]),
        _ => None,
    };
    if let Some(r) = target {
        set_row(c, r);
    }
}

/// A cursor's place as `bar:beat.fraction` (`5:3.5`).
pub fn position_text(score: &Score, c: &Cursor) -> String {
    // A chant: `bar:syllable.note`, as everywhere (the start of the song, before any note, is
    // `1:0`).
    if let Some(chant) = score.tracks.get(c.track).and_then(|t| t.chant.as_deref()) {
        let t = cecilia_core::chant::transcribe(chant);
        return t
            .cell_near(c.bar, c.beat)
            .and_then(|i| chant.places().of_cell(&t.cells[i]))
            .map_or_else(|| "1:0".to_string(), |p| p.label());
    }
    let pos = score.tracks.get(c.track).map_or(1.0 + c.beat as f64, |t| {
        cecilia_core::position::beat_position(score, t, c.bar, c.voice, c.beat)
    });
    cecilia_core::position::format_position(c.bar, pos, false)
}

impl App {
    /// The `(bar, beat)` of the cell of a chant at place `n:pos` (`pos` as `syllable.note`).
    fn chant_place_target(&self, n: usize, pos: f64) -> Option<(usize, usize)> {
        let c = self.pane().cursor;
        let chant = self.buffer().score.tracks.get(c.track)?.chant.as_deref()?;
        let (syllable, note) = (
            pos.floor() as usize,
            ((pos.fract() * 10.0).round() as usize).max(1),
        );
        let want = cecilia_core::chant::Place {
            bar: n,
            syllable,
            note: if syllable == 0 { 0 } else { note },
        };
        let (t, places) = (cecilia_core::chant::transcribe(chant), chant.places());
        t.cells
            .iter()
            .find(|cell| {
                places.of_cell(cell).is_some_and(|p| {
                    (p.bar, p.syllable, p.note.max(usize::from(p.syllable > 0)))
                        == (want.bar, want.syllable, want.note)
                })
            })
            .map(|cell| (cell.bar, cell.beat))
    }
}

fn set_bar(score: &Score, c: &mut Cursor, bar: usize) {
    c.bar = bar.min(score.master_bars.len().saturating_sub(1));
    c.beat = 0;
}

fn set_track(score: &Score, c: &mut Cursor, track: usize) {
    let tick = cursor_tick(score, c);
    c.track = track.min(score.tracks.len().saturating_sub(1));
    c.beat = beat_at_tick(score, c.track, c.bar, tick);
    c.staff = Cursor::HOME;
}

fn step_beat(score: &Score, c: &mut Cursor, forward: bool) -> bool {
    if forward {
        if c.beat + 1 < beats(score, c).len() {
            c.beat += 1;
        } else if c.bar + 1 < score.master_bars.len() {
            c.bar += 1;
            c.beat = 0;
        } else {
            return false;
        }
    } else if c.beat > 0 {
        c.beat -= 1;
    } else if c.bar > 0 {
        c.bar -= 1;
        c.beat = beats(score, c).len().saturating_sub(1);
    } else {
        return false;
    }
    true
}

fn step_note(score: &Score, c: &mut Cursor, forward: bool) {
    let start = *c;
    while step_beat(score, c, forward) {
        if beats(score, c)
            .get(c.beat)
            .is_some_and(|b| !b.notes.is_empty())
        {
            return;
        }
    }
    *c = start;
}

/// The note of the beat at the cursor that sits on the cursor's row: its string on a tab, its
/// line or space on a standard staff, the beat's first note on a slash staff or a chant track.
pub(crate) fn row_note<'a>(score: &'a Score, c: &Cursor) -> Option<&'a Note> {
    let track = score.tracks.get(c.track)?;
    let kind = *track.staves().get(staff_index(track, c.staff))?;
    let beat = beats(score, c).get(c.beat)?;
    if uses_strings(track, kind) {
        beat.notes.iter().find(|n| n.string as usize == c.string)
    } else if matches!(kind, Staff::Slash) || track.chant.is_some() {
        beat.notes.first()
    } else {
        let key = score
            .master_bars
            .get(c.bar)
            .map_or(0, |m| m.key_signature.accidentals);
        beat.notes.iter().find(|n| {
            let written = track.pitch_of(n) as i32 + track.staff_clef().transpose();
            cecilia_core::staff::diatonic(written, key) == c.line
        })
    }
}

/// Whether the note on the cursor's row is tied from the one before it.
fn row_tied(score: &Score, c: &Cursor) -> bool {
    row_note(score, c).is_some_and(|n| n.tie)
}

/// The last note of the tie the cursor's note starts or continues (the cursor stays when the
/// note is not tied on).
fn tie_end(score: &Score, c: &mut Cursor) {
    loop {
        let mut next = *c;
        if !step_beat(score, &mut next, true) || !row_tied(score, &next) {
            return;
        }
        *c = next;
    }
}

/// The first note of the tie the cursor's note continues.
fn tie_start(score: &Score, c: &mut Cursor) {
    while row_tied(score, c) {
        let mut prev = *c;
        if !step_beat(score, &mut prev, false) || row_note(score, &prev).is_none() {
            return;
        }
        *c = prev;
    }
}

/// Steps through the beats from the cursor until one has a note on the cursor's row that
/// `wanted` accepts; the cursor stays where it is if there is none.
fn find_row_note(
    score: &Score,
    c: &mut Cursor,
    forward: bool,
    wanted: impl Fn(&Note) -> bool,
) -> bool {
    let mut at = *c;
    while step_beat(score, &mut at, forward) {
        if row_note(score, &at).is_some_and(&wanted) {
            *c = at;
            return true;
        }
    }
    false
}

/// The chant of the cursor's track, when it has one (a chant score's only track).
fn cursor_chant<'a>(score: &'a Score, c: &Cursor) -> Option<&'a cecilia_core::chant::Chant> {
    score.tracks.get(c.track)?.chant.as_deref()
}

/// Moves the cursor of a chant track to the first beat of note `n` (`end`: the last beat it
/// sounds), keeping its staff position.
fn chant_goto(score: &Score, c: &mut Cursor, n: usize, end: bool) {
    let Some(chant) = cursor_chant(score, c) else {
        return;
    };
    let t = cecilia_core::chant::transcribe(chant);
    if let Some((bar, beat)) = t.place_of(n) {
        c.bar = bar;
        c.beat = if end {
            beat + t.lengths[n].max(1) as usize - 1
        } else {
            beat
        };
    }
}

/// A `(bar, beat)` place.
type Place = (usize, usize);

/// The notes of a chant track on the cursor's staff position, in reading order, as
/// `(note, first beat, last beat)` places.
fn chant_row_notes(score: &Score, c: &Cursor) -> Vec<(usize, Place, Place)> {
    let Some(chant) = cursor_chant(score, c) else {
        return Vec::new();
    };
    let t = cecilia_core::chant::transcribe(chant);
    (0..t.positions.len())
        .filter(|&n| chant.note(n).is_some_and(|x| x.pitch as i32 == c.line))
        .map(|n| {
            let (bar, beat) = t.positions[n];
            (
                n,
                (bar, beat),
                (bar, beat + t.lengths[n].max(1) as usize - 1),
            )
        })
        .collect()
}

/// `w`: the next note on the row that begins a tie (or is not tied at all). On a chant, the
/// next note on the cursor's staff position.
fn row_next(score: &Score, c: &mut Cursor) {
    if cursor_chant(score, c).is_some() {
        let here = (c.bar, c.beat);
        if let Some(&(n, ..)) = chant_row_notes(score, c).iter().find(|r| r.1 > here) {
            chant_goto(score, c, n, false);
        }
        return;
    }
    find_row_note(score, c, true, |n| !n.tie);
}

/// `e`: the end of the tie the cursor is in, else the end of the next note's tie. On a chant,
/// the end of the note the cursor is in (a stropha sounds several beats), else the end of the
/// next note.
fn row_end(score: &Score, c: &mut Cursor) {
    if cursor_chant(score, c).is_some() {
        let here = (c.bar, c.beat);
        let notes = chant_row_notes(score, c);
        if let Some(&(n, ..)) = notes.iter().find(|r| r.2 > here) {
            chant_goto(score, c, n, true);
        }
        return;
    }
    let here = *c;
    tie_end(score, c);
    if *c == here && find_row_note(score, c, true, |_| true) {
        tie_end(score, c);
    }
}

/// `b`: the start of the tie the cursor is in, else the start of the previous note's tie. On a
/// chant, the start of the note the cursor is in, else the previous note.
fn row_prev(score: &Score, c: &mut Cursor) {
    if cursor_chant(score, c).is_some() {
        let here = (c.bar, c.beat);
        let notes = chant_row_notes(score, c);
        if let Some(&(n, ..)) = notes.iter().rev().find(|r| r.1 < here) {
            chant_goto(score, c, n, false);
        }
        return;
    }
    let here = *c;
    tie_start(score, c);
    if *c == here && find_row_note(score, c, false, |_| true) {
        tie_start(score, c);
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use cecilia_core::*;

    pub(crate) fn test_app() -> App {
        let beat = |fret| Beat {
            duration: Duration::new(DurationValue::Quarter),
            notes: vec![Note::new(0, fret)],
            ..Beat::default()
        };
        let rest = Beat::rest(Duration::new(DurationValue::Quarter));
        let bar = Bar {
            voices: vec![Voice {
                beats: vec![beat(1), rest.clone(), rest, beat(3)],
            }],
            ..Default::default()
        };
        let track = Track {
            strings: vec![64, 59, 55, 50, 45, 40],
            bars: vec![bar; 8],
            ..Track::default()
        };
        let score = Score {
            tempo: 120,
            master_bars: vec![MasterBar::default(); 8],
            tracks: vec![track.clone(), track],
            ..Score::default()
        };
        let mut app = App::new(Config::default(), None);
        app.buffers[0] = Buffer {
            path: None,
            name: "test".into(),
            score,
            modified: false,
            history: Default::default(),
            version: 0,
            disk: None,
        };
        // The properties pane is on by default; most tests are about panes without it.
        app.set_tool(PaneKind::Properties, Some(false));
        app
    }

    pub(crate) fn keys(app: &mut App, s: &str) {
        for k in crate::keys::parse_keys(s, &[Key::char(' ')]).unwrap() {
            app.handle_key(k);
        }
    }

    #[test]
    fn ctrl_and_ctrl_shift_bind_separately_and_macros_keep_shift() {
        use crate::config::Lookup;
        let toml = "[keymap.normal]\n\"<C-S-q>\" = \"quit!\"\n";
        let (config, errors) = Config::parse(toml, &[]).unwrap();
        assert!(errors.is_empty(), "{errors:?}");
        let map = config.keymap(Mode::Normal);
        let k = |s| crate::keys::parse_keys(s, &[]).unwrap();
        let cmd = |s| match map.lookup(&k(s)) {
            Lookup::Exact(c) => Some(c.to_vec()),
            _ => None,
        };
        assert_eq!(cmd("<C-S-q>"), Some(vec![Command::parse("quit!").unwrap()]));
        let plain = cmd("<C-q>");
        assert_ne!(plain, cmd("<C-S-q>"));
        assert_eq!(plain, Some(vec![Command::parse("quit").unwrap()]));
        // A macro replays the shifted key as recorded.
        let mut app = App::new(Config::default(), None);
        app.run(Command::New);
        keys(&mut app, "qa<C-S-x><S-F5><D-x>q");
        assert_eq!(
            crate::keys::format_keys(&app.ed.macros[&'a']),
            "<C-S-x><S-F5><D-x>"
        );
    }

    #[test]
    fn linked_note_follows_the_cursor_on_a_tab() {
        let mut app = test_app();
        let at = |app: &App| app.linked_note(app.pane());
        // The first beat has a note on string 0; the cursor starts there on the tab.
        let n = at(&app).unwrap();
        assert_eq!((n.bar, n.beat, n.string), (0, 0, 0));
        // On another string of the beat, or a rest, there is no note.
        keys(&mut app, "j");
        assert_eq!(at(&app), None);
        keys(&mut app, "kl");
        assert_eq!(at(&app), None);
    }

    #[test]
    fn linked_note_on_other_staves() {
        let mut app = test_app();
        app.buffers[0].score.tracks[0].staves = vec![Staff::Standard(Clef::Treble), Staff::Tab];
        let t = &app.buffers[0].score.tracks[0];
        let n = &t.bars[0].voices[0].beats[0].notes[0];
        let p = cecilia_core::staff::diatonic(t.pitch_of(n) as i32 + t.staff_clef().transpose(), 0);
        app.pane_mut().cursor.staff = 0;
        app.pane_mut().cursor.line = p;
        let n = app.linked_note(app.pane()).unwrap();
        assert_eq!((n.string, n.staff), (0, 0));
        // A position with no note there, and a rest, light nothing.
        app.pane_mut().cursor.line = p + 1;
        assert_eq!(app.linked_note(app.pane()), None);
        app.pane_mut().cursor.line = p;
        app.pane_mut().cursor.beat = 1;
        assert_eq!(app.linked_note(app.pane()), None);
        // A slash staff shows the beat's one note.
        app.buffers[0].score.tracks[0].staves = vec![Staff::Tab, Staff::Slash];
        app.pane_mut().cursor.staff = 1;
        app.pane_mut().cursor.beat = 0;
        assert_eq!(app.linked_note(app.pane()).map(|n| n.staff), Some(1));
    }

    #[test]
    fn motions_and_counts() {
        let mut app = test_app();
        keys(&mut app, "l");
        assert_eq!(app.pane().cursor.beat, 1);
        keys(&mut app, "w");
        assert_eq!((app.pane().cursor.bar, app.pane().cursor.beat), (0, 3));
        keys(&mut app, "w");
        assert_eq!((app.pane().cursor.bar, app.pane().cursor.beat), (1, 0));
        keys(&mut app, "3}");
        assert_eq!(app.pane().cursor.bar, 4);
        keys(&mut app, "G");
        assert_eq!(app.pane().cursor.bar, 7);
        keys(&mut app, "2G");
        assert_eq!(app.pane().cursor.bar, 1);
        keys(&mut app, "gg$");
        assert_eq!((app.pane().cursor.bar, app.pane().cursor.beat), (0, 3));
        keys(&mut app, "0jjjjjjjj");
        assert_eq!(app.pane().cursor.string, 5);
        keys(&mut app, "J");
        assert_eq!(app.pane().cursor.track, 1);
    }

    #[test]
    fn jk_stay_on_the_staff_and_staff_commands_change_it() {
        let mut app = test_app();
        let staves = app.buffer().score.tracks[0].staves();
        assert!(staves.len() >= 2, "{staves:?}");
        let first_line = |app: &App| {
            let t = &app.buffer().score.tracks[0];
            uses_strings(t, t.staves()[staff_index(t, app.pane().cursor.staff)])
        };
        // Keep to the first staff: `k` past its top never leaves it.
        keys(&mut app, ":staff-prev<CR>");
        let staff = staff_index(&app.buffer().score.tracks[0], app.pane().cursor.staff);
        assert_eq!(staff, 0);
        let _ = first_line(&app);
        for _ in 0..80 {
            keys(&mut app, "j");
        }
        assert_eq!(
            staff_index(&app.buffer().score.tracks[0], app.pane().cursor.staff),
            0
        );
        let low = app.pane().cursor;
        for _ in 0..80 {
            keys(&mut app, "k");
        }
        assert_eq!(
            staff_index(&app.buffer().score.tracks[0], app.pane().cursor.staff),
            0
        );
        if !first_line(&app) {
            // Ledger positions beyond the staff's own rows.
            assert!(app.pane().cursor.line > low.line + 8, "{low:?}");
        }
        keys(&mut app, "<A-j>");
        assert_eq!(
            staff_index(&app.buffer().score.tracks[0], app.pane().cursor.staff),
            1
        );
        keys(&mut app, "<A-k>");
        assert_eq!(
            staff_index(&app.buffer().score.tracks[0], app.pane().cursor.staff),
            0
        );
        // The old behaviour: j runs on into the next staff.
        app.config.file.ui.jk_moves_between_staves = true;
        for _ in 0..80 {
            keys(&mut app, "j");
        }
        assert_eq!(
            staff_index(&app.buffer().score.tracks[0], app.pane().cursor.staff),
            staves.len() - 1
        );
    }

    #[test]
    fn alt_j_k_step_through_pickers_and_completions() {
        let mut app = test_app();
        app.open_picker(crate::picker::PickerKind::Keymap);
        keys(&mut app, "<A-j><A-j>");
        assert_eq!(app.picker.as_ref().unwrap().selected, 2);
        keys(&mut app, "<A-k>");
        assert_eq!(app.picker.as_ref().unwrap().selected, 1);
        assert!(app.picker.as_ref().unwrap().query.is_empty());
        keys(&mut app, "<Esc>:fo<A-j>");
        let first = app.cmdline.clone();
        keys(&mut app, "<A-j>");
        assert_ne!(app.cmdline, first);
        keys(&mut app, "<A-k>");
        assert_eq!(app.cmdline, first);
        assert!(app.cmdline.starts_with("fo"));
    }

    #[test]
    fn focus_cycles_and_returns_to_the_previous_pane() {
        let mut app = test_app();
        keys(&mut app, "<C-w>v<C-w>s");
        let ids = app.view().root.panes();
        assert_eq!(ids.len(), 3);
        app.set_focus(ids[0]);
        keys(&mut app, "<C-w>w");
        assert_eq!(app.focused(), ids[1]);
        keys(&mut app, "<C-w><C-w><C-w>w");
        assert_eq!(app.focused(), ids[0]);
        keys(&mut app, "<C-w>W");
        assert_eq!(app.focused(), ids[2]);
        // Back and forth between the two last focused.
        app.set_focus(ids[1]);
        keys(&mut app, "<C-w>p");
        assert_eq!(app.focused(), ids[2]);
        keys(&mut app, "<C-w>p");
        assert_eq!(app.focused(), ids[1]);
        keys(&mut app, "<C-w><C-p>");
        assert_eq!(app.focused(), ids[2]);
    }

    #[test]
    fn directional_focus_returns_to_last_focused_pane() {
        // Two panes on the left, one on the right; the bottom-left one was focused last.
        let mut app = test_app();
        keys(&mut app, "<C-w>v<C-w>h<C-w>s");
        let right = app.view().root.panes()[2];
        let (top, bottom) = (app.view().root.panes()[0], app.view().root.panes()[1]);
        assert_ne!(top, bottom);
        app.set_focus(bottom);
        keys(&mut app, "<C-w>l");
        assert_eq!(app.focused(), right);
        keys(&mut app, "<C-w>h");
        assert_eq!(app.focused(), bottom);
        app.set_focus(top);
        keys(&mut app, "<C-w>l<C-w>h");
        assert_eq!(app.focused(), top);
        // Three on the left: whichever was last focused comes back, not the middle one.
        keys(&mut app, "<C-w>s");
        let col: Vec<_> = {
            let mut ids = app.view().root.panes();
            ids.retain(|i| *i != right);
            ids
        };
        assert_eq!(col.len(), 3);
        for id in col {
            app.set_focus(id);
            keys(&mut app, "<C-w>l");
            assert_eq!(app.focused(), right);
            keys(&mut app, "<C-w>h");
            assert_eq!(app.focused(), id);
        }
    }

    #[test]
    fn command_line_and_panes() {
        let mut app = test_app();
        keys(&mut app, ":5<CR>");
        assert_eq!(app.pane().cursor.bar, 4);
        assert_eq!(app.mode, Mode::Normal);
        keys(&mut app, "<C-w>v");
        assert_eq!(app.view().root.panes().len(), 2);
        let right = app.focused();
        keys(&mut app, "<C-w>h");
        assert_ne!(app.focused(), right);
        keys(&mut app, ":pane-close<CR>");
        assert_eq!(app.view().root.panes(), vec![right]);
        assert!(!app.quit);
        keys(&mut app, ":view-new<CR>gt");
        assert_eq!(app.active_view, 0);
        keys(&mut app, ":bogus<CR>");
        assert!(app.message.as_ref().is_some_and(|m| m.error));
        keys(&mut app, ":qa<CR>");
        assert!(app.quit);
    }

    #[test]
    fn write_exports_and_protects_untouched_originals() {
        let dir = std::env::temp_dir().join(format!("cecilia-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let mut app = test_app();
        let out = dir.join("out.gp5");
        app.run(Command::Write(Some(out.clone())));
        assert!(out.exists(), "{:?}", app.message);
        app.buffers[0].path = Some(out.clone());
        app.run(Command::Write(None));
        assert_eq!(app.message.as_ref().unwrap().text, "no changes to write");
        app.run(Command::Write(Some(dir.join("out.unknown"))));
        assert!(app.message.as_ref().unwrap().error);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// Files `name` (empty scores) in a temp folder.
    fn files(dir: &Path, names: &[&str]) -> Vec<PathBuf> {
        std::fs::create_dir_all(dir).unwrap();
        let mut app = App::new(Config::default(), None);
        app.set_tool(PaneKind::Properties, Some(false));
        app.run(Command::New);
        names
            .iter()
            .map(|n| {
                let p = dir.join(n);
                app.run(Command::Write(Some(p.clone())));
                p
            })
            .collect()
    }

    #[test]
    fn buffers_belong_to_views_and_close_back_to_recent() {
        let dir = std::env::temp_dir().join(format!("cecilia-views-{}", std::process::id()));
        let f = files(&dir, &["a.ccm", "b.ccm", "c.ccm"]);
        let mut app = App::new(Config::default(), None);
        app.set_tool(PaneKind::Properties, Some(false));
        for p in &f {
            app.run(Command::Edit(p.clone()));
        }
        // Three tabs; the pane shows c and remembers b then a.
        let names = |app: &App| -> Vec<String> {
            app.bufferline().iter().map(|t| t.name.clone()).collect()
        };
        assert_eq!(names(&app), ["a.ccm", "b.ccm", "c.ccm"]);
        assert!(app.bufferline()[2].active);
        // Cursors are remembered per buffer in the pane.
        app.run(Command::BufferGoto(1));
        app.panes.get_mut(&app.focused()).unwrap().cursor.string = 3;
        app.run(Command::BufferNext);
        assert_eq!(app.buffer().name, "b.ccm");
        app.run(Command::BufferPrev);
        assert_eq!(app.pane().cursor.string, 3);
        // A new view copies the bufferline (tabline.new_view = "copy"), sharing the buffers.
        app.run(Command::ViewNew(None));
        assert_eq!(names(&app), ["a.ccm", "b.ccm", "c.ccm"]);
        assert_eq!(app.view_labels(), ["1", "2"]);
        // Closing a tab here leaves it loaded for view 1.
        app.run(Command::BufferClose(Some(3), false));
        assert_eq!(names(&app), ["a.ccm", "b.ccm"]);
        app.run(Command::ViewPrev);
        assert_eq!(names(&app), ["a.ccm", "b.ccm", "c.ccm"]);
        // Closing the shown buffer: the pane goes back to the one it showed before.
        app.run(Command::BufferGoto(2));
        app.run(Command::BufferGoto(3));
        app.run(Command::BufferClose(None, false));
        assert_eq!(app.buffer().name, "b.ccm");
        // Unsaved changes stop a close that would unload the buffer.
        let b = app.pane().buffer;
        app.buffers[b].modified = true;
        app.run(Command::ViewNext);
        app.run(Command::BufferClose(Some(2), false));
        app.run(Command::ViewPrev);
        app.run(Command::BufferClose(None, false));
        // It asks first; anything but y/n cancels.
        assert_eq!(app.confirm_close.as_ref().map(|c| c.asking), Some(b));
        keys(&mut app, "x");
        assert!(app.confirm_close.is_none());
        assert_eq!(app.buffer().name, "b.ccm");
        // `n` discards the changes.
        app.run(Command::BufferClose(None, false));
        keys(&mut app, "n");
        assert_eq!(app.buffer().name, "a.ccm");
        app.buffers[b].modified = false;
        app.run(Command::BufferGoto(2));
        app.run(Command::BufferClose(None, true));
        assert_eq!(app.buffer().name, "a.ccm");
        // A split pane goes back to what it showed before…
        app.run(Command::VSplit);
        app.run(Command::Edit(f[2].clone()));
        app.run(Command::BufferClose(None, false));
        assert_eq!(app.view().root.panes().len(), 2);
        assert_eq!(app.buffer().name, "a.ccm");
        // …and closes when it has nothing to go back to.
        app.run(Command::Edit(f[2].clone()));
        let id = app.focused();
        app.panes.get_mut(&id).unwrap().recent.clear();
        app.run(Command::BufferClose(None, false));
        assert_eq!(app.view().root.panes().len(), 1);
        assert!(!app.panes.contains_key(&id));
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn long_names_lose_their_middle() {
        assert_eq!(truncate_middle("short.gp5", 28, 14, 10), "short.gp5");
        assert_eq!(
            truncate_middle("A very long song title that does not fit.cc", 28, 14, 10),
            "A very long so…not fit.cc"
        );
        // Prefix and suffix are cut to fit the maximum.
        assert_eq!(truncate_middle("abcdefghij", 5, 4, 4), "abcd…");
        assert_eq!(file_kind(Some(Path::new("x.GP5"))), "guitar-pro");
        assert_eq!(file_kind(None), "new");
    }

    #[test]
    fn waveform_strips_audio_pane_global_tools_and_escape() {
        let mut app = App::new(Config::default(), None);
        let first = app.focused();
        app.run(Command::Split);
        let second = app.focused();
        app.run(Command::Waveform);
        assert!(!app.panes[&second].waveform);
        assert!(app.panes[&first].waveform);
        assert!(!app.has_tool(PaneKind::Audio));
        app.run(Command::AudioPane(None));
        assert!(app.has_tool(PaneKind::Audio));
        // Escape in a tool pane goes back to the score pane worked on last.
        let audio = app.tool_pane(PaneKind::Audio).unwrap();
        app.set_focus(audio);
        app.handle_key(crate::keys::Key::new(crate::keys::KeyCode::Esc));
        assert_eq!(app.window_focus(), second);
        // Taking it out of ui.global_tools moves the open pane under its score pane, and back.
        assert!(app.is_global(PaneKind::Audio));
        app.run(Command::parse("set ui.global_tools -waveform").unwrap());
        assert!(!app.is_global(PaneKind::Audio));
        let local = app.tool_pane(PaneKind::Audio).unwrap();
        assert_eq!(app.panes[&local].owner, Some(second));
        app.run(Command::parse("set ui.global_tools +waveform").unwrap());
        assert!(
            app.panes[&app.tool_pane(PaneKind::Audio).unwrap()]
                .owner
                .is_none()
        );
        app.run(Command::AudioPane(Some(false)));
        assert!(!app.has_tool(PaneKind::Audio));
        // The old setting still loads, with a warning.
        let (_, errors) = Config::parse("[ui]\nglobal_waveform = true\n", &[]).unwrap();
        assert!(errors[0].contains("ui.global_waveform"), "{errors:?}");
        // The scrollbar is on by default and toggles.
        assert!(app.config.file.ui.scrollbar);
        app.run(Command::parse("set ui.scrollbar toggle").unwrap());
        assert!(!app.config.file.ui.scrollbar);
        app.run(Command::parse("set ui.scrollbar true").unwrap());
        assert!(app.config.file.ui.scrollbar);
        // The progress bar is on by default and toggles.
        assert!(app.config.file.ui.progress_bar);
        app.run(Command::parse("set ui.progress_bar toggle").unwrap());
        assert!(!app.config.file.ui.progress_bar);
    }

    #[test]
    fn playing_keeps_the_editing_modes() {
        let mut app = App::new(Config::default(), None);
        app.run(Command::New);
        keys(&mut app, "i3<Space>l5<Esc>0");
        keys(&mut app, "<Space><Space>");
        app.set_playing(true);
        assert_eq!(app.mode, Mode::Normal);
        // With following off, the cursor moves and edits on its own while the music plays.
        keys(&mut app, "<Space>f");
        app.take_effects();
        keys(&mut app, "lx");
        assert_eq!(app.pane().cursor.beat, 1);
        assert!(app.buffer().modified);
        keys(&mut app, "v");
        assert_eq!(app.mode, Mode::Visual);
        keys(&mut app, "<Esc>");
        assert!(
            !app.take_effects()
                .iter()
                .any(|e| matches!(e, Effect::Seek { .. } | Effect::Play { .. }))
        );
        // Pausing and resuming with the cursor where it paused carries on…
        keys(&mut app, "<Space><Space>");
        app.set_playing(false);
        assert!(app.paused);
        app.take_effects();
        keys(&mut app, "<Space><Space>");
        assert_eq!(app.take_effects(), vec![Effect::Resume]);
        app.set_playing(true);
        // …but once it has moved, playing starts at the cursor.
        keys(&mut app, "<Space><Space>");
        app.set_playing(false);
        keys(&mut app, "0");
        app.take_effects();
        keys(&mut app, "<Space><Space>");
        assert_eq!(
            app.take_effects(),
            vec![Effect::Play {
                buffer: 1,
                bar: 0,
                tick: 0
            }]
        );
        // The playing file's tab says so.
        app.set_playing(true);
        let tab = |app: &App, b: usize| {
            app.bufferline()
                .into_iter()
                .find(|t| t.buffer == b)
                .unwrap()
        };
        assert!(tab(&app, 1).playing);
        // Only one file plays: starting another stops this one.
        app.run(Command::New);
        app.take_effects();
        keys(&mut app, "<Space><Space>");
        assert_eq!(
            app.take_effects(),
            vec![Effect::Play {
                buffer: 2,
                bar: 0,
                tick: 0
            }]
        );
        assert!(tab(&app, 2).playing && !tab(&app, 1).playing);
        // A song that ends by itself just stops.
        app.set_playing(false);
        assert!(!tab(&app, 2).playing && !tab(&app, 2).paused);
        assert_eq!(app.mode, Mode::Normal);
    }

    #[test]
    fn undoing_back_to_the_saved_state_is_unchanged() {
        let dir = std::env::temp_dir().join(format!("cecilia-undo-saved-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("a.ccm");
        let mut app = App::new(Config::default(), None);
        app.run(Command::New);
        app.run(Command::Write(Some(file.clone())));
        let mut app = App::new(Config::default(), None);
        app.run(Command::Edit(file.clone()));
        // Edit, then undo back to where it was opened: unchanged; redo: changed again.
        keys(&mut app, "i5<Esc>");
        assert!(app.buffer().modified);
        keys(&mut app, "u");
        assert!(!app.buffer().modified);
        keys(&mut app, "U");
        assert!(app.buffer().modified);
        // Save, edit, undo: unchanged; one more undo goes before the save: changed.
        app.run(Command::Write(None));
        assert!(!app.buffer().modified);
        keys(&mut app, "l");
        keys(&mut app, "i7<Esc>");
        assert!(app.buffer().modified);
        keys(&mut app, "u");
        assert!(!app.buffer().modified);
        keys(&mut app, "u");
        assert!(app.buffer().modified);
        // A new score is unchanged until edited, and again once undone to the start.
        let mut app = App::new(Config::default(), None);
        app.run(Command::New);
        assert!(!app.buffer().modified);
        keys(&mut app, "i5<Esc>");
        assert!(app.buffer().modified);
        keys(&mut app, "u");
        assert!(!app.buffer().modified);
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn closing_a_changed_buffer_offers_to_save() {
        let dir = std::env::temp_dir().join(format!("cecilia-close-save-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("a.ccm");
        let mut app = App::new(Config::default(), None);
        app.run(Command::New);
        app.run(Command::Write(Some(file.clone())));
        app.run(Command::New);
        // An untouched new score closes without asking.
        app.run(Command::BufferClose(None, false));
        assert!(app.confirm_close.is_none());
        app.run(Command::New);
        keys(&mut app, "i5<Esc>");
        // A changed one with no file name can't be saved from the prompt.
        app.run(Command::BufferClose(None, false));
        keys(&mut app, "y");
        assert!(app.message.as_ref().unwrap().error);
        assert!(app.buffer().modified);
        // A named one is written, then closed.
        app.run(Command::Edit(file.clone()));
        keys(&mut app, "i5<Esc>");
        let b = app.pane().buffer;
        app.run(Command::BufferClose(None, false));
        keys(&mut app, "y");
        assert!(!app.buffers[b].modified);
        assert_ne!(app.pane().buffer, b);
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn quitting_is_refused_with_unsaved_changes() {
        let dir = std::env::temp_dir().join(format!("cecilia-quit-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("a.ccm");
        let mut app = App::new(Config::default(), None);
        app.run(Command::New);
        app.run(Command::Write(Some(file.clone())));
        let mut app = App::new(Config::default(), None);
        app.run(Command::Edit(file.clone()));
        keys(&mut app, "<C-q>");
        assert!(app.quit);
        // A change: <C-q>, :q refuse…
        let mut app = App::new(Config::default(), None);
        app.run(Command::Edit(file.clone()));
        keys(&mut app, "i5<Esc>");
        keys(&mut app, "<C-q>");
        assert!(!app.quit && app.confirm_close.is_some());
        // Any other key cancels; `n` discards and quits; `y` writes and quits.
        keys(&mut app, "x");
        assert!(!app.quit && app.confirm_close.is_none());
        app.run(Command::Quit { force: false });
        keys(&mut app, "n");
        assert!(app.quit);
        let mut app = App::new(Config::default(), None);
        app.run(Command::Edit(file.clone()));
        keys(&mut app, "i5<Esc>");
        app.run(Command::Quit { force: false });
        keys(&mut app, "y");
        assert!(app.quit && !app.buffer().modified);
        let mut app = App::new(Config::default(), None);
        app.run(Command::Edit(file.clone()));
        keys(&mut app, "i5<Esc>");
        // …:qa! discards, :wqa writes it first.
        app.run(Command::Quit { force: true });
        assert!(app.quit);
        let mut app = App::new(Config::default(), None);
        app.run(Command::Edit(file.clone()));
        keys(&mut app, "i7<Esc>");
        app.run(Command::WriteQuit);
        assert!(app.quit && !app.buffer().modified);
        // An unnamed new score can't be written by :wqa.
        let mut app = App::new(Config::default(), None);
        app.run(Command::New);
        keys(&mut app, "i5<Esc>");
        app.run(Command::WriteQuit);
        assert!(!app.quit);
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn panes_move_like_vim_windows() {
        let mut app = App::new(Config::default(), None);
        app.viewport = Rect {
            x: 0,
            y: 0,
            w: 90,
            h: 40,
        };
        keys(&mut app, "<C-w>v<C-w>v");
        let ids = app.view().root.panes();
        let me = app.focused();
        // <C-w>J: across the bottom.
        keys(&mut app, "<C-w>J");
        let r = app
            .layout()
            .into_iter()
            .find(|(id, _)| *id == me)
            .unwrap()
            .1;
        assert_eq!((r.y, r.w), (20, 90));
        // <C-w>L: down the right side.
        keys(&mut app, "<C-w>L");
        let r = app
            .layout()
            .into_iter()
            .find(|(id, _)| *id == me)
            .unwrap()
            .1;
        assert_eq!((r.x + r.w, r.h), (90, 40));
        // <C-w>x and <C-w>r keep the panes, in another order.
        let before = app.view().root.panes();
        keys(&mut app, "<C-w>x");
        assert_ne!(app.view().root.panes(), before);
        keys(&mut app, "<C-w>r<C-w>R");
        let mut after = app.view().root.panes();
        after.sort();
        let mut all = ids.clone();
        all.sort();
        assert_eq!(after, all);
        assert_eq!(app.focused(), me);
    }

    #[test]
    fn tabs_move_by_command_and_key() {
        let mut app = App::new(Config::default(), None);
        app.run(Command::New);
        app.run(Command::New);
        app.run(Command::New);
        let order = |app: &App| {
            app.bufferline()
                .iter()
                .map(|t| t.buffer)
                .collect::<Vec<_>>()
        };
        let start = order(&app);
        let shown = app.pane().buffer;
        assert_eq!(*start.last().unwrap(), shown);
        // To the front, then one right with the key, then back to the end.
        app.run(Command::BufferMove(crate::command::Amount::To(1)));
        assert_eq!(order(&app)[0], shown);
        keys(&mut app, "<Space>>");
        assert_eq!(order(&app)[1], shown);
        app.run(Command::BufferMove(crate::command::Amount::By(99)));
        assert_eq!(order(&app), start);
        keys(&mut app, "<Space><lt>");
        assert_eq!(order(&app)[start.len() - 2], shown);
        // Dragging uses move_tab directly.
        app.move_tab(0, 5);
        assert_eq!(*order(&app).last().unwrap(), start[0]);
    }

    #[test]
    fn maximize_shows_the_focused_pane_alone() {
        let mut app = App::new(Config::default(), None);
        app.set_tool(PaneKind::Properties, Some(false));
        app.viewport = Rect {
            x: 0,
            y: 0,
            w: 80,
            h: 40,
        };
        app.run(Command::VSplit);
        let id = app.focused();
        keys(&mut app, "<A-m>");
        assert_eq!(app.layout(), vec![(id, app.viewport)]);
        assert!(app.dividers().is_empty());
        keys(&mut app, "<A-m>");
        assert_eq!(app.layout().len(), 2);
        // Focusing another pane brings them all back.
        keys(&mut app, "<C-w>z<C-w>h");
        assert_eq!(app.layout().len(), 2);
    }

    #[test]
    fn layout_belongs_to_the_focused_pane() {
        let mut app = App::new(Config::default(), None);
        let first = app.focused();
        assert_eq!(app.pane().layout, "vertical");
        // A split starts with the layout of the pane it came from.
        app.run(Command::Layout(Some("page".into())));
        app.run(Command::Split);
        let second = app.focused();
        assert_ne!(first, second);
        assert_eq!(app.pane().layout, "page");
        // Changing it changes only the focused pane.
        app.run(Command::Layout(None));
        assert_eq!(app.panes[&second].layout, "horizontal");
        assert_eq!(app.panes[&first].layout, "page");
    }

    /// Three score panes stacked in the left column, one in the right; returns the left three
    /// and the right one.
    fn three_left_one_right(app: &mut App) -> ([PaneId; 3], PaneId) {
        let first = app.focused();
        app.run(Command::VSplit);
        let right = app.focused();
        app.set_focus(first);
        app.run(Command::Split);
        let second = app.focused();
        app.run(Command::Split);
        let third = app.focused();
        app.set_viewport(Rect {
            x: 0,
            y: 0,
            w: 200,
            h: 90,
        });
        ([first, second, third], right)
    }

    fn rect_of(app: &App, id: PaneId) -> Rect {
        app.pane_rects().iter().find(|(p, _)| *p == id).unwrap().1
    }

    #[test]
    fn closing_a_pane_equalizes_its_split() {
        let mut app = App::new(Config::default(), None);
        let ([a, b, c], right) = three_left_one_right(&mut app);
        let right_before = rect_of(&app, right);
        app.close_pane_id(b);
        app.set_viewport(Rect {
            x: 0,
            y: 0,
            w: 200,
            h: 90,
        });
        assert_eq!(rect_of(&app, a).h, rect_of(&app, c).h);
        assert_eq!(rect_of(&app, right), right_before);
    }

    #[test]
    fn set_toggles_equalize_on_close() {
        let mut app = App::new(Config::default(), None);
        assert!(app.config.file.ui.equalize_on_close);
        app.run(Command::parse("set ui.equalize_on_close toggle").unwrap());
        assert!(!app.config.file.ui.equalize_on_close);
        app.run(Command::parse("set ui.equalize_on_close true").unwrap());
        assert!(app.config.file.ui.equalize_on_close);
        app.run(Command::parse("set ui.equalize_on_close false").unwrap());
        assert!(!app.config.file.ui.equalize_on_close);
    }

    fn msg(app: &App) -> (String, bool) {
        let m = app.message.as_ref().expect("a message");
        (m.text.clone(), m.error)
    }

    #[test]
    fn set_toggles_and_shows_value() {
        let mut app = App::new(Config::default(), None);
        assert!(app.config.file.tabline.wrap);
        app.run(Command::parse("set tabline.wrap toggle").unwrap());
        assert!(!app.config.file.tabline.wrap);
        assert_eq!(msg(&app), ("tabline.wrap = false".into(), false));
        app.run(Command::parse("get tabline.wrap").unwrap());
        assert_eq!(msg(&app), ("tabline.wrap = false".into(), false));
    }

    #[test]
    fn set_error_keeps_value_and_reports() {
        let mut app = App::new(Config::default(), None);
        let before = app.config.file.ui.bar_numbers.clone();
        app.run(Command::parse("set ui.bar_numbers bogus").unwrap());
        assert_eq!(app.config.file.ui.bar_numbers, before);
        let (text, error) = msg(&app);
        assert!(error);
        assert!(text.contains("every"), "{text}");
        assert!(Command::parse("set").is_err());
        assert!(Command::parse("get").is_err());
    }

    #[test]
    fn set_rejects_values_that_break_the_config() {
        let mut app = App::new(Config::default(), None);
        let leader = app.config.leader.clone();
        app.run(Command::parse("set general.leader <Nope>").unwrap());
        assert_eq!(app.config.leader, leader);
        assert!(app.config.overrides.is_empty());
        assert!(app.message.as_ref().is_some_and(|m| m.error));
        app.message = None;
        app.run(Command::parse("set toolbar.items +nonsense").unwrap());
        assert!(app.config.overrides.is_empty());
        assert!(app.message.as_ref().is_some_and(|m| m.error));
    }

    #[test]
    fn set_rebuilds_keymaps() {
        use crate::config::Lookup;
        let mut app = App::new(Config::default(), None);
        app.run(Command::parse("set general.leader ,").unwrap());
        let k = |s| crate::keys::parse_keys(s, &[]).unwrap();
        match app.config.keymap(Mode::Normal).lookup(&k(",p")) {
            Lookup::Exact(c) => assert_eq!(c, [Command::parse("properties").unwrap()]),
            _ => panic!(",p should be bound"),
        }
    }

    #[test]
    fn set_survives_reload() {
        let dir = std::env::temp_dir().join(format!("cecilia-set-reload-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("config.toml");
        std::fs::write(&path, "[tabline]\nwrap = true\n").unwrap();
        let (config, _) = Config::load(&path, &[]).unwrap();
        let mut app = App::new(config, Some(path.clone()));
        app.run(Command::parse("set tabline.wrap false").unwrap());
        std::fs::write(
            &path,
            "[tabline]\nwrap = true\n[ui]\nbar_numbers = \"every\"\n",
        )
        .unwrap();
        app.reload_config();
        assert!(!app.config.file.tabline.wrap);
        assert_eq!(app.config.file.ui.bar_numbers, "every");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn set_default_resets() {
        let mut app = App::new(Config::default(), None);
        app.run(Command::parse("set tabline.wrap false").unwrap());
        app.run(Command::parse("set tabline.wrap default").unwrap());
        assert!(app.config.file.tabline.wrap);
        assert_eq!(msg(&app), ("tabline.wrap = true".into(), false));
    }

    #[test]
    fn buffer_next_and_prev_wrap_with_tabline_wrap() {
        for wrap in [true, false] {
            let dir =
                std::env::temp_dir().join(format!("cecilia-wrap-{wrap}-{}", std::process::id()));
            let f = files(&dir, &["a.ccm", "b.ccm"]);
            let (config, _) = Config::parse(&format!("[tabline]\nwrap = {wrap}\n"), &[]).unwrap();
            let mut app = App::new(config, None);
            app.set_tool(PaneKind::Properties, Some(false));
            for p in &f {
                app.run(Command::Edit(p.clone()));
            }
            assert_eq!(app.buffer().name, "b.ccm");
            app.run(Command::BufferNext);
            assert_eq!(app.buffer().name, if wrap { "a.ccm" } else { "b.ccm" });
            app.run(Command::BufferGoto(1));
            app.run(Command::BufferPrev);
            assert_eq!(app.buffer().name, if wrap { "b.ccm" } else { "a.ccm" });
        }
    }

    #[test]
    fn appearance_settings_default_to_the_built_in_look_and_clamp() {
        let (c, errors) = Config::parse("", &[]).unwrap();
        assert!(errors.is_empty());
        assert!(c.file.tabline.wrap);
        assert_eq!(c.file.ui.focus_ring_width, 4.0);
        assert_eq!(
            c.focus_ring_color(c.theme.mode_color(Mode::Normal)),
            c.theme.mode_color(Mode::Normal)
        );
        let (c, errors) = Config::parse(
            "[ui]\nfocus_ring_width = 20\nfocus_ring_color = \"#102030\"\n",
            &[],
        )
        .unwrap();
        assert_eq!(errors.len(), 1, "{errors:?}");
        assert_eq!(c.file.ui.focus_ring_width, 8.0);
        assert_eq!(
            c.focus_ring_color(c.theme.mode_color(Mode::Insert)),
            crate::theme::Rgba::rgb(0x10, 0x20, 0x30)
        );
        let (c, errors) = Config::parse("[ui]\nfocus_ring_color = \"red\"\n", &[]).unwrap();
        assert_eq!(errors.len(), 1);
        assert_eq!(
            c.focus_ring_color(c.theme.mode_color(Mode::Normal)),
            c.theme.mode_color(Mode::Normal)
        );
    }

    #[test]
    fn appearance_options_set_reset_and_reject() {
        let mut app = App::new(Config::default(), None);
        let run = |app: &mut App, c: &str| app.run(Command::parse(c).unwrap());
        run(&mut app, "set ui.focus_ring_width 4");
        assert_eq!(app.config.file.ui.focus_ring_width, 4.0);
        run(&mut app, "set ui.focus_ring_width 99");
        assert_eq!(
            app.config.file.ui.focus_ring_width, 4.0,
            "out of range: unchanged"
        );
        assert!(msg(&app).1, "an error");
        run(&mut app, "set ui.focus_ring_width 0");
        assert_eq!(app.config.file.ui.focus_ring_width, 0.0);
        run(&mut app, "set ui.focus_ring_width default");
        assert_eq!(app.config.file.ui.focus_ring_width, 4.0);
        run(&mut app, "set ui.focus_ring_width wide");
        assert_eq!(app.config.file.ui.focus_ring_width, 4.0);

        run(&mut app, "set ui.focus_ring_color #102030");
        assert_eq!(
            app.focus_ring_color(),
            crate::theme::Rgba::rgb(0x10, 0x20, 0x30)
        );
        run(&mut app, "set ui.focus_ring_color red");
        assert_eq!(app.config.file.ui.focus_ring_color, "#102030");
        run(&mut app, "set ui.focus_ring_color default");
        assert!(app.config.file.ui.focus_ring_color.is_empty());

        run(&mut app, "set tabline.wrap toggle");
        assert!(!app.config.file.tabline.wrap);
        run(&mut app, "set tabline.wrap true");
        assert!(app.config.file.tabline.wrap);
        run(&mut app, "set tabline.wrap maybe");
        assert!(app.config.file.tabline.wrap);
    }

    #[test]
    fn closing_a_pane_hands_its_space_over_without_equalize_on_close() {
        let (config, _) = Config::parse("[ui]\nequalize_on_close = false\n", &[]).unwrap();
        let mut app = App::new(config, None);
        let ([a, b, c], right) = three_left_one_right(&mut app);
        let right_before = rect_of(&app, right);
        app.close_pane_id(b);
        app.set_viewport(Rect {
            x: 0,
            y: 0,
            w: 200,
            h: 90,
        });
        assert_ne!(rect_of(&app, a).h, rect_of(&app, c).h);
        assert_eq!(rect_of(&app, right), right_before);
    }

    #[test]
    fn the_edit_cursor_hides_while_following_playback() {
        let mut app = test_app();
        assert!(!app.cursor_hidden(0));
        keys(&mut app, "<Space><Space>");
        app.set_playing(true);
        assert!(app.cursor_hidden(0));
        // Following off: it shows (and moves on its own).
        keys(&mut app, "<Space>f");
        assert!(!app.cursor_hidden(0));
        // The setting turns it off (following stays off, as `<Space>f` left it).
        let (config, _) = Config::parse(
            "[ui]\nhide_cursor_while_following = false\nfollow_playback = false\n",
            &[],
        )
        .unwrap();
        app.config = config;
        keys(&mut app, "<Space>f");
        assert!(app.config.file.ui.follow_playback && !app.cursor_hidden(0));
    }

    #[test]
    fn moving_the_cursor_while_playing_moves_the_play_position() {
        let mut app = test_app();
        keys(&mut app, "<Space><Space>");
        app.take_effects();
        app.set_playing(true);
        assert!(app.config.file.ui.follow_playback);
        // `}` jumps the playback to the next bar at once…
        keys(&mut app, "}");
        assert_eq!(
            app.take_effects(),
            vec![Effect::Seek {
                buffer: 0,
                bar: 1,
                tick: 0
            }]
        );
        // …and a playhead update from before the player caught up doesn't pull it back.
        app.follow_playback(0, 0, 960);
        assert_eq!(app.pane().cursor.bar, 1);
        // Beat moves seek too.
        keys(&mut app, "l");
        assert_eq!(
            app.take_effects(),
            vec![Effect::Seek {
                buffer: 0,
                bar: 1,
                tick: 960
            }]
        );
        // A key that doesn't move the cursor doesn't seek.
        keys(&mut app, "<Space>f");
        assert!(app.take_effects().is_empty());
        // Without following, the cursor moves on its own.
        keys(&mut app, "}");
        assert_eq!(app.pane().cursor.bar, 2);
        assert!(app.take_effects().is_empty());
    }

    #[test]
    fn play_and_stop_effects() {
        let mut app = test_app();
        keys(&mut app, "}ll<Space><Space>");
        assert_eq!(
            app.take_effects(),
            vec![Effect::Play {
                buffer: 0,
                bar: 1,
                tick: 1920
            }]
        );
        // Following playback moves the cursor with the playhead…
        app.playing = true;
        app.follow_playback(0, 3, 1000);
        assert_eq!((app.pane().cursor.bar, app.pane().cursor.beat), (3, 1));
        // …until following is turned off: then it stays where it was.
        keys(&mut app, "<Space>f");
        assert!(!app.config.file.ui.follow_playback);
        app.follow_playback(0, 5, 0);
        assert_eq!((app.pane().cursor.bar, app.pane().cursor.beat), (3, 1));
        keys(&mut app, "<Space>f");
        keys(&mut app, "<Space>s");
        assert_eq!(
            app.take_effects(),
            vec![Effect::Stop {
                buffer: 0,
                bar: 1,
                tick: 1920
            }]
        );
        assert_eq!(app.pane().cursor.bar, 1);
    }

    #[test]
    fn tracks_pane_fits_until_resized_by_hand() {
        let mut app = test_app();
        app.row_height = 10;
        app.col_width = 5;
        app.set_viewport(Rect {
            x: 0,
            y: 0,
            w: 200,
            h: 200,
        });
        app.run(Command::Tracks(Some(true)));
        let id = app.tool_pane(PaneKind::Tracks).unwrap();
        let fitted = app.panes[&id].size.0;
        // A track added later makes the pane taller.
        let track = app.buffer().score.tracks[0].clone();
        app.buffers[0].score.tracks.push(track);
        app.set_viewport(Rect {
            x: 0,
            y: 0,
            w: 200,
            h: 200,
        });
        let taller = app.panes[&id].size.0;
        assert!(taller > fitted);
        // Resized by hand, it stays; with auto_resize off nothing follows.
        app.set_tool_size(id, Axis::Stacked, 30);
        let track = app.buffer().score.tracks[0].clone();
        app.buffers[0].score.tracks.push(track);
        app.set_viewport(Rect {
            x: 0,
            y: 0,
            w: 200,
            h: 200,
        });
        assert_eq!(app.panes[&id].size.0, Some(30));
        app.panes.get_mut(&id).unwrap().manual_size = false;
        app.config.file.ui.auto_resize = false;
        app.set_viewport(Rect {
            x: 0,
            y: 0,
            w: 200,
            h: 200,
        });
        assert_eq!(app.panes[&id].size.0, Some(30));
    }

    fn app_with_bars(n: usize) -> App {
        let mut app = App::new(Config::default(), None);
        app.run(Command::New);
        // A new score has a few bars; add until there are `n`.
        while app.buffer().score.master_bars.len() < n {
            app.run(Command::BarInsert { after: true });
        }
        app.run(Command::GotoFirst);
        app
    }

    #[test]
    fn scroll_keys_request_a_scroll_taken_once() {
        let mut app = app_with_bars(12);
        let id = app.focused();
        for (k, to) in [
            ("zz", ScrollTo::Center),
            ("zt", ScrollTo::Top),
            ("zb", ScrollTo::Bottom),
        ] {
            keys(&mut app, k);
            assert_eq!(app.pane().scroll_request, Some(to), "{k}");
            assert_eq!(app.take_scroll_request(id), Some(to));
            assert_eq!(app.take_scroll_request(id), None);
        }
    }

    #[test]
    fn scroll_commands_parse_and_ignore_counts() {
        let mut app = app_with_bars(12);
        let id = app.focused();
        keys(&mut app, "3zz");
        assert_eq!(app.take_scroll_request(id), Some(ScrollTo::Center));
        assert_eq!(app.pane().cursor.bar, 0);
        for (c, to) in [
            ("scroll-center", ScrollTo::Center),
            ("scroll-top", ScrollTo::Top),
            ("scroll-bottom", ScrollTo::Bottom),
        ] {
            app.run(Command::parse(c).unwrap());
            assert_eq!(app.take_scroll_request(id), Some(to));
        }
        assert!(!Command::ScrollCursor(ScrollTo::Top).repeatable());
    }

    #[test]
    fn half_page_falls_back_to_page_bars() {
        let mut app = app_with_bars(12);
        app.run(Command::PageDown);
        assert_eq!(app.pane().cursor.bar, app.page_bars);
        app.run(Command::PageUp);
        assert_eq!(app.pane().cursor.bar, 0);
    }

    #[test]
    fn half_page_in_the_page_layout_counts_rows_per_page_not_on_screen() {
        let mut app = app_with_bars(20);
        let id = app.focused();
        // Ten rows of two bars; five rows to a page, however many are on screen.
        for visible_rows in [2, 5, 9] {
            app.set_view_info(
                id,
                Some(ViewInfo {
                    rows: (0..10).map(|r| r * 2..r * 2 + 2).collect(),
                    visible_rows,
                    page_rows: 5,
                    ..Default::default()
                }),
            );
            app.pane_mut().cursor.bar = 0;
            app.run(Command::PageDown);
            assert_eq!(app.pane().cursor.bar, 6, "{visible_rows} rows visible");
        }
    }

    #[test]
    fn half_page_horizontal_moves_half_the_visible_bars() {
        let mut app = app_with_bars(20);
        let id = app.focused();
        let info = |visible_bars| ViewInfo {
            horizontal: true,
            visible_bars,
            ..Default::default()
        };
        app.set_view_info(id, Some(info(6)));
        app.run(Command::PageDown);
        assert_eq!(app.pane().cursor.bar, 3);
        app.run(Command::PageUp);
        assert_eq!(app.pane().cursor.bar, 0);
        // At least one bar.
        app.set_view_info(id, Some(info(1)));
        app.run(Command::PageDown);
        assert_eq!(app.pane().cursor.bar, 1);
    }

    #[test]
    fn half_page_vertical_jumps_rows_and_keeps_the_column() {
        // 12 bars, 3 per row: 4 rows.
        let mut app = app_with_bars(12);
        let id = app.focused();
        let rows = (0..4).map(|r| r * 3..r * 3 + 3).collect::<Vec<_>>();
        let info = |visible_rows| ViewInfo {
            rows: rows.clone(),
            visible_rows,
            ..Default::default()
        };
        app.set_view_info(id, Some(info(3)));
        app.pane_mut().cursor.bar = 1;
        app.pane_mut().cursor.string = 2;
        // 3 rows shown: down by 2 rows, same place in the row and on the same string.
        app.run(Command::PageDown);
        assert_eq!((app.pane().cursor.bar, app.pane().cursor.string), (7, 2));
        // Clamped to the last row.
        app.run(Command::PageDown);
        assert_eq!(app.pane().cursor.bar, 10);
        app.run(Command::PageUp);
        assert_eq!(app.pane().cursor.bar, 4);
        // One row shown: still a row.
        app.set_view_info(id, Some(info(1)));
        app.run(Command::PageUp);
        assert_eq!(app.pane().cursor.bar, 1);
    }

    #[test]
    fn half_page_vertical_clamps_to_a_shorter_row() {
        let mut app = app_with_bars(5);
        let id = app.focused();
        app.set_view_info(
            id,
            Some(ViewInfo {
                rows: vec![0..3, 3..5],
                visible_rows: 2,
                ..Default::default()
            }),
        );
        app.pane_mut().cursor.bar = 2;
        app.run(Command::PageDown);
        assert_eq!(app.pane().cursor.bar, 4);
    }

    #[test]
    fn incomplete_bars_option() {
        let mut app = App::new(Config::default(), None);
        let on = |app: &App| app.config.file.ui.highlight_incomplete_bars;
        assert!(!on(&app));
        app.run(Command::parse("set ui.highlight_incomplete_bars true").unwrap());
        assert!(on(&app));
        app.run(Command::parse("set ui.highlight_incomplete_bars toggle").unwrap());
        assert!(!on(&app));
        app.run(Command::parse("set ui.highlight_incomplete_bars maybe").unwrap());
        assert!(!on(&app));
        let (config, errors) =
            Config::parse("[ui]\nhighlight_incomplete_bars = true\n", &[]).unwrap();
        assert!(errors.is_empty());
        assert!(on(&App::new(config, None)));
        let (config, _) = Config::parse("[theme]\nincomplete_bar = \"#112233\"\n", &[]).unwrap();
        assert_eq!(
            config.theme.incomplete_bar,
            crate::theme::Rgba::rgb(0x11, 0x22, 0x33)
        );
    }

    #[test]
    fn bar_numbers_option() {
        let mut app = App::new(Config::default(), None);
        assert_eq!(app.config.file.ui.bar_numbers, "system");
        app.run(Command::parse("set ui.bar_numbers every").unwrap());
        assert_eq!(app.config.file.ui.bar_numbers, "every");
        app.run(Command::parse("set ui.bar_numbers sometimes").unwrap());
        assert_eq!(app.config.file.ui.bar_numbers, "every");
        app.run(Command::parse("set ui.bar_numbers system").unwrap());
        assert_eq!(app.config.file.ui.bar_numbers, "system");
        let (config, errors) = Config::parse("[ui]\nbar_numbers = \"every\"\n", &[]).unwrap();
        assert!(errors.is_empty());
        assert_eq!(App::new(config, None).config.file.ui.bar_numbers, "every");
        let (_, errors) = Config::parse("[ui]\nbar_numbers = \"odd\"\n", &[]).unwrap();
        assert!(errors[0].contains("ui.bar_numbers"), "{errors:?}");
    }
}
