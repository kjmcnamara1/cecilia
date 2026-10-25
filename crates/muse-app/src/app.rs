//! Application state machine: buffers, panes, tabs, modes and key handling.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use muse_core::{Beat, Score};

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
}

/// A position in a score: track, bar, beat index (voice 1) and string.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Cursor {
    pub track: usize,
    pub bar: usize,
    pub beat: usize,
    pub string: usize,
    /// Voice being edited (0 = voice 1).
    pub voice: usize,
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
    /// Character positions that matched the typed text (highlighted).
    pub matched: Vec<usize>,
}

#[derive(Debug, Clone)]
pub struct Pane {
    pub buffer: usize,
    pub cursor: Cursor,
    /// First visible row, maintained by the frontend.
    pub scroll: usize,
    /// GUI score layout of this pane: "page", "vertical" or "horizontal".
    pub layout: String,
    /// GUI: show the audio waveform strip at the bottom of this pane.
    pub waveform: bool,
    /// Buffers shown in this pane before, most recent first, with their cursors (switching
    /// back restores the cursor; closing a buffer falls back to the most recent one).
    pub recent: Vec<(usize, Cursor)>,
    /// Where the cursor was before the last jump (`''`).
    pub jump_back: Option<(usize, Cursor)>,
}

/// A layout of panes with its own set of loaded buffers (the bufferline's tabs).
#[derive(Debug, Clone)]
pub struct View {
    pub root: Node,
    pub focus: PaneId,
    /// Buffers loaded in this view, in bufferline order.
    pub buffers: Vec<usize>,
    pub name: Option<String>,
    /// A pane shown alone, filling the view (`:maximize`); the others come back when it is
    /// toggled off or another pane is focused.
    pub maximized: Option<PaneId>,
}

/// One tab of the bufferline (see [`App::bufferline`]).
#[derive(Debug, Clone, PartialEq)]
pub struct BufferTab {
    pub buffer: usize,
    pub name: String,
    /// File type, from the extension: "guitar-pro", "musicxml", "midi", "chant", "muse" or "new".
    pub kind: &'static str,
    pub modified: bool,
    /// Shown in the focused pane.
    pub active: bool,
    /// This file is the one playing (only one plays at a time)…
    pub playing: bool,
    /// …or is paused.
    pub paused: bool,
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
        "muse" => "muse",
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
    /// The configuration changed (e.g. a new SoundFont).
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
    /// Mixer, loop, metronome, speed or pitch settings changed.
    PlaybackSettings,
    /// Record a sync point for the next bar at the current audio position.
    SyncTap,
    /// Generate sync points from beat detection.
    SyncDetect,
}

pub const DEFAULT_TRACKS_ROWS: usize = 8;
pub const MAX_TRACKS_ROWS: usize = 64;
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
    /// Keys typed so far of a multi-key binding.
    pub pending: Vec<Key>,
    pub count: Option<usize>,
    pub cmdline: String,
    history: Vec<String>,
    history_pos: Option<usize>,
    /// Bookmarks (`m{a-z}`, `m{A-Z}`).
    pub marks: crate::marks::Marks,
    /// The open picker (Picker mode).
    pub(crate) picker: Option<crate::picker::Picker>,
    /// Command-line completion in progress: candidates, where they start and which is shown.
    completion: Option<(crate::complete::Completions, Option<usize>)>,
    pub message: Option<Message>,
    pub show_help: bool,
    /// Set by the frontend to reflect the player state.
    pub playing: bool,
    /// Buffer being played, set when playback starts.
    pub playing_buffer: Option<usize>,
    play_start: Option<(PaneId, Cursor)>,
    /// Playback is paused (Play mode stays on until stopped).
    pub paused: bool,
    /// Where the cursor was when playback paused: resuming carries on only if it's still there.
    paused_at: Option<(PaneId, usize, Cursor)>,
    pub follow: bool,
    /// When the cursor last moved the play position: playhead updates from just before the
    /// player caught up are ignored, so the cursor doesn't bounce back.
    seeked: Option<std::time::Instant>,
    /// Command-line completion ignores case (`ui.completion_ignore_case`).
    pub completion_ignore_case: bool,
    /// Command-line completion is fuzzy (`ui.completion_fuzzy`).
    pub completion_fuzzy: bool,
    /// GUI: a waveform bar across the whole window above the status line, for the focused
    /// pane's file (`ui.global_waveform`).
    pub global_waveform: bool,
    /// The tracks panel is shown (`ui.tracks_panel`).
    pub tracks_panel: bool,
    /// The toolbar is shown (`ui.toolbar`, `:toolbar`).
    pub toolbar: bool,
    /// Until when the hidden tracks panel shows after the track changed (`ui.tracks_peek`).
    tracks_peek_until: Option<std::time::Instant>,
    /// Tracks the tracks panel shows at once (`:tracks-resize`, or drag its top edge).
    pub tracks_rows: usize,
    /// Height of the GUI's waveform strips and bar in logical pixels (`:waveform-resize`).
    pub waveform_height: u16,
    /// Screen area for panes, set by the frontend.
    pub viewport: Rect,
    /// Bars moved by page-down/up, set by the frontend.
    pub page_bars: usize,
    pub(crate) effects: Vec<Effect>,
    pub quit: bool,
    /// Layout for panes opened without another to copy from (`ui.layout`); each pane keeps
    /// its own (`Pane::layout`).
    pub score_layout: String,
    /// Chant tracks show square notation (false: their modern transcription).
    pub chant_view: bool,
    pub zoom: f64,
    pub(crate) ed: crate::editor::EditorState,
    /// Play-along settings.
    pub pa: crate::playalong::PlayAlong,
    /// The active workspace file, saved again as the session changes.
    pub workspace_file: Option<PathBuf>,
    /// The workspace as last written, to save only when something changed.
    pub(crate) workspace_saved: Option<String>,
    /// Record the active workspace for the next start (`muse` without arguments reopens it).
    /// Off in tests and screenshots.
    pub remember_workspace: bool,
}

impl App {
    pub fn new(config: Config, config_path: Option<PathBuf>) -> Self {
        let follow = config.file.ui.follow_playback;
        let completion_ignore_case = config.file.ui.completion_ignore_case;
        let completion_fuzzy = config.file.ui.completion_fuzzy;
        let global_waveform = config.file.ui.global_waveform;
        let tracks_panel = config.file.ui.tracks_panel;
        let toolbar = config.file.ui.toolbar;
        let (score_layout, zoom) = (config.file.ui.layout.clone(), config.file.ui.zoom);
        let config_mtime = config_path.as_deref().and_then(mtime);
        let theme_mtime = config
            .file
            .theme
            .file
            .as_deref()
            .map(crate::theme::theme_path)
            .and_then(|p| mtime(&p));
        let mut panes = HashMap::new();
        panes.insert(
            1,
            Pane {
                buffer: 0,
                cursor: Cursor::default(),
                scroll: 0,
                layout: score_layout.clone(),
                waveform: true,
                recent: Vec::new(),
                jump_back: None,
            },
        );
        Self {
            config,
            config_path,
            config_mtime,
            theme_mtime,
            buffers: vec![Buffer::empty()],
            panes,
            next_pane: 2,
            views: vec![View {
                maximized: None,
                root: Node::Leaf(1),
                focus: 1,
                buffers: vec![0],
                name: None,
            }],
            active_view: 0,
            mode: Mode::Normal,
            pending: Vec::new(),
            count: None,
            cmdline: String::new(),
            history: Vec::new(),
            history_pos: None,
            completion: None,
            picker: None,
            marks: Default::default(),
            message: None,
            show_help: false,
            playing: false,
            playing_buffer: None,
            play_start: None,
            paused: false,
            paused_at: None,
            follow,
            seeked: None,
            completion_ignore_case,
            completion_fuzzy,
            global_waveform,
            tracks_panel,
            toolbar,
            tracks_rows: DEFAULT_TRACKS_ROWS,
            tracks_peek_until: None,
            waveform_height: DEFAULT_WAVEFORM_HEIGHT,
            viewport: Rect {
                x: 0,
                y: 0,
                w: 80,
                h: 24,
            },
            page_bars: 4,
            effects: Vec::new(),
            quit: false,
            score_layout,
            chant_view: true,
            zoom,
            ed: Default::default(),
            pa: Default::default(),
            workspace_file: None,
            workspace_saved: None,
            remember_workspace: false,
        }
    }

    pub fn take_effects(&mut self) -> Vec<Effect> {
        std::mem::take(&mut self.effects)
    }

    pub fn view(&self) -> &View {
        &self.views[self.active_view]
    }

    pub fn focused(&self) -> PaneId {
        self.view().focus
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
                    kind: file_kind(buf.path.as_deref()),
                    modified: buf.modified,
                    active: b == current,
                    playing: self.playing_buffer == Some(b) && self.playing && !self.paused,
                    paused: self.playing_buffer == Some(b) && self.paused,
                })
            })
            .collect()
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

    pub fn pane(&self) -> &Pane {
        &self.panes[&self.focused()]
    }

    pub(crate) fn pane_mut(&mut self) -> &mut Pane {
        let id = self.focused();
        self.panes.get_mut(&id).unwrap()
    }

    pub fn buffer(&self) -> &Buffer {
        &self.buffers[self.pane().buffer]
    }

    pub fn layout(&self) -> Vec<(PaneId, Rect)> {
        if let Some(id) = self.maximized() {
            return vec![(id, self.viewport)];
        }
        self.pane_rects()
    }

    /// Where every pane of the view is, maximized or not (for moving between them).
    fn pane_rects(&self) -> Vec<(PaneId, Rect)> {
        let mut out = Vec::new();
        self.view().root.layout(self.viewport, &mut out);
        out
    }

    /// The borders between panes of the active view, for resizing with the mouse.
    pub fn dividers(&self) -> Vec<crate::layout::Divider> {
        if self.maximized().is_some() {
            return Vec::new();
        }
        self.view().root.dividers(self.viewport)
    }

    /// Whether the tracks panel is on screen: shown, or peeking after a change of track.
    pub fn tracks_shown(&self) -> bool {
        self.tracks_panel || self.tracks_peeking()
    }

    /// The hidden tracks panel is showing for a moment after the track changed.
    pub fn tracks_peeking(&self) -> bool {
        self.tracks_peek_until
            .is_some_and(|t| std::time::Instant::now() < t)
    }

    /// After a key: if it changed the focused pane's track (same file), the hidden tracks
    /// panel shows for `ui.tracks_peek` seconds.
    pub(crate) fn peek_tracks(&mut self, before: (PaneId, usize, Cursor)) {
        let pane = self.pane();
        let secs = self.config.file.ui.tracks_peek;
        if self.tracks_panel
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

    /// The pane maximized in the active view, while it has the focus.
    pub fn maximized(&self) -> Option<PaneId> {
        let v = self.view();
        v.maximized
            .filter(|&id| id == v.focus && v.root.panes().len() > 1)
    }

    /// Moves a pane border (from [`App::dividers`]) to `pos`.
    pub fn drag_divider(&mut self, d: &crate::layout::Divider, pos: u16) {
        let v = self.active_view;
        self.views[v].root.drag_divider(d, pos);
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
            muse_formats::read_file(path).map_err(|e| format!("{}: {e}", path.display()))?;
        let name = path.file_name().map_or_else(
            || path.display().to_string(),
            |n| n.to_string_lossy().into(),
        );
        // Replace the initial empty buffer if nothing refers to anything else yet.
        let buffer = Buffer {
            path: Some(canonical),
            name,
            score,
            modified: false,
            history: Default::default(),
            version: 0,
        };
        if self.buffers.len() == 1 && self.buffers[0].path.is_none() {
            self.buffers[0] = buffer;
            return Ok(0);
        }
        self.buffers.push(buffer);
        Ok(self.buffers.len() - 1)
    }

    /// Shows a buffer in the focused pane, adding it to the view's bufferline if needed. The
    /// pane remembers the buffer it showed (and its cursor) to come back to.
    pub(crate) fn show_buffer(&mut self, buffer: usize) {
        let view = &mut self.views[self.active_view];
        if !view.buffers.contains(&buffer) {
            view.buffers.push(buffer);
        }
        self.switch_pane_buffer(self.focused(), buffer);
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
    }

    /// Shows the buffer `steps` tabs along the bufferline (wrapping) in the focused pane.
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
        let next = list[(at + steps).rem_euclid(n) as usize];
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
            return self.error(format!(
                "\"{}\" has unsaved changes (:w, or :bdelete! to discard)",
                b.name
            ));
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
        for id in to_close {
            self.close_pane_id(id);
            if self.quit {
                return;
            }
        }
        if !in_other_views {
            self.forget_marks(buffer);
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
        let buffers = match self.config.file.tabline.new_view.as_str() {
            "current" => vec![current],
            "empty" => {
                self.buffers.push(Buffer::empty());
                vec![self.buffers.len() - 1]
            }
            _ => self.view().buffers.clone(),
        };
        let shown = if buffers.contains(&current) {
            current
        } else {
            buffers[0]
        };
        let id = self.new_pane(shown);
        self.views.insert(
            self.active_view + 1,
            View {
                maximized: None,
                root: Node::Leaf(id),
                focus: id,
                buffers,
                name: None,
            },
        );
        self.active_view += 1;
    }

    /// Closes the active view. Buffers no other view has are unloaded; unsaved changes in
    /// them stop it unless `force`.
    fn close_view(&mut self, force: bool) {
        if self.views.len() == 1 {
            return self.error("cannot close the last view");
        }
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
            if let Some(b) = orphans.iter().find(|&&b| self.buffers[b].modified) {
                return self.error(format!(
                    "\"{}\" has unsaved changes (:w, or :view-close! to discard)",
                    self.buffers[*b].name
                ));
            }
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
        self.autosave_workspace();
        let Some(path) = self.config_path.clone() else {
            return;
        };
        let m = mtime(&path);
        let theme = self
            .config
            .file
            .theme
            .file
            .as_deref()
            .map(crate::theme::theme_path)
            .and_then(|p| mtime(&p));
        if m != self.config_mtime || theme != self.theme_mtime {
            self.config_mtime = m;
            self.theme_mtime = theme;
            self.reload_config();
        }
    }

    pub fn reload_config(&mut self) {
        let Some(path) = self.config_path.clone() else {
            self.error("no config file");
            return;
        };
        match Config::load(&path) {
            Ok((config, errors)) => {
                self.config = config;
                self.follow = self.config.file.ui.follow_playback;
                self.completion_ignore_case = self.config.file.ui.completion_ignore_case;
                self.completion_fuzzy = self.config.file.ui.completion_fuzzy;
                self.global_waveform = self.config.file.ui.global_waveform;
                self.effects.push(Effect::ConfigReloaded);
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
        self.dispatch_key(key);
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
        ) {
            self.completion = None;
        }
        match key.code {
            KeyCode::Tab => self.complete_step(true),
            KeyCode::BackTab => self.complete_step(false),
            KeyCode::Esc => self.leave_command_mode(),
            KeyCode::Char('c') if key.ctrl => self.leave_command_mode(),
            KeyCode::Char('u') if key.ctrl => self.cmdline.clear(),
            KeyCode::Enter => {
                let line = std::mem::take(&mut self.cmdline);
                self.leave_command_mode();
                if !line.trim().is_empty() {
                    self.history.push(line.clone());
                }
                match Command::parse(&line) {
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
            KeyCode::Char(c) if !key.ctrl && !key.alt => self.cmdline.push(c),
            _ => {}
        }
    }

    fn leave_command_mode(&mut self) {
        self.mode = Mode::Normal;
        self.cmdline.clear();
        self.history_pos = None;
        self.completion = None;
    }

    /// Tab (or Shift-Tab backwards): extend the word to the candidates' common prefix, then
    /// cycle through them.
    fn complete_step(&mut self, forward: bool) {
        use crate::complete::{common_prefix, complete};
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
        let c = complete(&self.cmdline, &cwd, score_dir.as_deref(), self.matcher());
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
                let prefix = common_prefix(&c.items, self.completion_ignore_case);
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
            ignore_case: self.completion_ignore_case,
            fuzzy: self.completion_fuzzy,
        }
    }

    /// The command palette while in Command mode: the typed line, the completions of the word
    /// being typed (updated as you type; Tab cycles through them) and which one is selected.
    pub fn palette(&self) -> Option<Palette> {
        use crate::complete::{complete, describe};
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
                (
                    complete(&self.cmdline, &cwd, score_dir, self.matcher()),
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
        if self.run_edit(&cmd)
            || self.run_playalong(&cmd)
            || self.run_chant(&cmd)
            || self.run_tracks(&cmd)
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
            StringDown => self.move_cursor(|s, c| {
                let n = s.tracks.get(c.track).map_or(1, |t| t.strings.len().max(1));
                c.string = (c.string + 1).min(n - 1);
            }),
            StringUp => self.move_cursor(|_, c| c.string = c.string.saturating_sub(1)),
            BarNext => self.move_cursor(|s, c| set_bar(s, c, c.bar + 1)),
            BarPrev => self.move_cursor(|s, c| set_bar(s, c, c.bar.saturating_sub(1))),
            BarStart => self.move_cursor(|_, c| c.beat = 0),
            BarEnd => self.move_cursor(|s, c| c.beat = beats(s, c).len().saturating_sub(1)),
            // Jumps (remembered for `''`).
            GotoFirst | GotoLast | GotoBar(_) => {
                let here = (self.pane().buffer, self.pane().cursor);
                match cmd {
                    GotoFirst => self.move_cursor(|s, c| set_bar(s, c, 0)),
                    GotoLast => self.move_cursor(|s, c| set_bar(s, c, usize::MAX)),
                    GotoBar(n) => self.move_cursor(|s, c| set_bar(s, c, n.saturating_sub(1))),
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
            PageDown => {
                let n = self.page_bars.max(1);
                self.move_cursor(|s, c| set_bar(s, c, c.bar + n))
            }
            PageUp => {
                let n = self.page_bars.max(1);
                self.move_cursor(|s, c| set_bar(s, c, c.bar.saturating_sub(n)))
            }
            PlayToggle => {
                let here = (self.focused(), self.pane().buffer, self.pane().cursor);
                if self.playing && self.playing_buffer == Some(here.1) {
                    self.run(Pause)
                } else if self.playing {
                    // Another file is playing: it stops, and this one starts.
                    self.run(Play)
                } else if self.paused && self.paused_at == Some(here) {
                    // The cursor is still where playback paused: carry on exactly from there.
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
                let follow = self.follow;
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
            CompletionFuzzy(on) => {
                self.completion_fuzzy = on.unwrap_or(!self.completion_fuzzy);
                self.completion = None;
                self.info(if self.completion_fuzzy {
                    "fuzzy completion on"
                } else {
                    "fuzzy completion off (prefix matching)"
                });
            }
            CompletionIgnoreCase(on) => {
                self.completion_ignore_case = on.unwrap_or(!self.completion_ignore_case);
                // Candidates found under the old setting no longer apply.
                self.completion = None;
                self.info(if self.completion_ignore_case {
                    "completion ignores case"
                } else {
                    "completion matches case"
                });
            }
            FollowToggle => {
                self.follow = !self.follow;
                self.info(if self.follow {
                    "follow playback on"
                } else {
                    "follow playback off"
                });
            }
            Split => self.split(Axis::Stacked),
            // In the tracks panel, its height is resized.
            Resize(Axis::Stacked, amount) if self.mode == Mode::Tracks => {
                self.run(TracksResize(amount))
            }
            Resize(axis, amount) => {
                let id = self.focused();
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
            TracksResize(amount) => {
                let rows = amount.apply(self.tracks_rows as i64);
                self.tracks_rows = rows.clamp(1, MAX_TRACKS_ROWS as i64) as usize;
                self.tracks_panel = true;
            }
            WaveformResize(amount) => {
                let h = amount.apply(self.waveform_height as i64);
                self.waveform_height =
                    h.clamp(MIN_WAVEFORM_HEIGHT as i64, MAX_WAVEFORM_HEIGHT as i64) as u16;
            }
            VSplit => self.split(Axis::SideBySide),
            Focus(dir) => {
                if let Some(id) = neighbour(&self.pane_rects(), self.focused(), dir) {
                    self.views[self.active_view].focus = id;
                }
            }
            ClosePane => self.close_pane(false),
            Quit { force } => self.close_pane(force),
            QuitAll { force } => {
                if self.may_quit(force) {
                    self.autosave_workspace();
                    self.quit = true;
                }
            }
            WriteAll => {
                self.write_all();
            }
            WriteQuitAll => {
                if self.write_all() && self.may_quit(false) {
                    self.autosave_workspace();
                    self.quit = true;
                }
            }
            WorkspaceSave(target) => self.save_workspace(target.as_deref()),
            WorkspaceOpen(name, force) => self.open_workspace(&name, force),
            WorkspaceClose => self.close_workspace(),
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
            Edit(path) => match self.open(&path) {
                Ok(b) => self.show_buffer(b),
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
            WriteQuit => {
                if self.buffer().modified {
                    self.write(None);
                    if self.message.as_ref().is_some_and(|m| m.error) {
                        return;
                    }
                }
                self.close_pane(false);
            }
            Source => self.reload_config(),
            CommandMode => {
                self.mode = Mode::Command;
                self.cmdline.clear();
            }
            Normal => {
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
            ZoomIn => self.zoom = (self.zoom * 1.15).min(6.0),
            ZoomOut => self.zoom = (self.zoom / 1.15).max(0.2),
            ZoomReset => self.zoom = self.config.file.ui.zoom,
            Nop => {}
            _ => {}
        }
    }

    /// Writes the focused buffer. Without a path it writes back to the buffer's file, but only
    /// when there are changes, so opening and saving never rewrites an untouched original.
    fn write(&mut self, path: Option<PathBuf>) {
        let index = self.pane().buffer;
        self.write_buffer(index, path);
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

    /// Quitting is refused while a file has unsaved changes (unless forced): the message
    /// names it. True if muse may quit.
    fn may_quit(&mut self, force: bool) -> bool {
        if force {
            return true;
        }
        match self.buffers.iter().find(|b| b.modified) {
            Some(b) => {
                let name = b.name.clone();
                self.error(format!(
                    "\"{name}\" has unsaved changes (:wqa to write all and quit, :qa! to discard)"
                ));
                false
            }
            None => true,
        }
    }

    fn write_buffer(&mut self, index: usize, path: Option<PathBuf>) {
        let buffer = &self.buffers[index];
        let target = match path {
            Some(p) => p,
            None => {
                let Some(p) = buffer.path.clone() else {
                    return self.error("no file name (use :w <file>)");
                };
                if !buffer.modified {
                    return self.info("no changes to write");
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
                Err(e) => return self.error(format!("{}: {e}", file.display())),
            }
        }
        match muse_formats::write_file(score, &target) {
            Ok(()) => {
                let size = std::fs::metadata(&target).map(|m| m.len()).unwrap_or(0);
                if buffer.path.as_deref() == Some(target.as_path()) {
                    self.buffers[index].modified = false;
                }
                self.info(format!("\"{}\" written, {size} bytes", target.display()));
            }
            Err(e) => self.error(format!("{}: {e}", target.display())),
        }
    }

    fn move_cursor(&mut self, f: impl FnOnce(&Score, &mut Cursor)) {
        let id = self.focused();
        let pane = self.panes.get_mut(&id).unwrap();
        let score = &self.buffers[pane.buffer].score;
        if score.tracks.is_empty() {
            return;
        }
        f(score, &mut pane.cursor);
        clamp_cursor(score, &mut pane.cursor);
    }

    /// A new pane on `buffer`, with the focused pane's layout.
    fn new_pane(&mut self, buffer: usize) -> PaneId {
        let id = self.next_pane;
        self.next_pane += 1;
        let (layout, waveform) = self.panes.get(&self.focused()).map_or_else(
            || (self.score_layout.clone(), true),
            |p| (p.layout.clone(), p.waveform),
        );
        self.panes.insert(
            id,
            Pane {
                buffer,
                cursor: Cursor::default(),
                scroll: 0,
                layout,
                waveform,
                recent: Vec::new(),
                jump_back: None,
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
        let focus = self.focused();
        let view = &mut self.views[self.active_view];
        view.root.split(focus, id, axis);
        view.focus = id;
    }

    /// `:q` on the focused pane: closes it, its view with the last pane, muse with the last view.
    fn close_pane(&mut self, force: bool) {
        let focus = self.focused();
        if self.view().root.panes().len() > 1 {
            self.close_pane_id(focus);
        } else if self.views.len() > 1 {
            self.close_view(false);
        } else if self.may_quit(force) {
            self.autosave_workspace();
            self.quit = true;
        }
    }

    /// Closes a pane of the active view (focus moves to its nearest neighbour). The last pane
    /// closes the view; the last pane of the last view shows an empty score instead.
    fn close_pane_id(&mut self, id: PaneId) {
        let rects = self.pane_rects();
        if self.view().root.panes().len() <= 1 {
            if self.views.len() > 1 {
                self.close_view(true);
            } else {
                self.buffers.push(Buffer::empty());
                let empty = self.buffers.len() - 1;
                let view = &mut self.views[self.active_view];
                view.buffers.push(empty);
                let pane = self.panes.get_mut(&id).unwrap();
                pane.buffer = empty;
                pane.cursor = Cursor::default();
                pane.recent.clear();
            }
            return;
        }
        let view = &mut self.views[self.active_view];
        view.root.remove(id);
        self.panes.remove(&id);
        if view.focus == id {
            let remaining = view.root.panes();
            // Focus the nearest remaining pane, as vim does.
            view.focus = [
                crate::command::Direction::Left,
                crate::command::Direction::Up,
                crate::command::Direction::Right,
                crate::command::Direction::Down,
            ]
            .into_iter()
            .find_map(|d| neighbour(&rects, id, d).filter(|n| remaining.contains(n)))
            .unwrap_or(remaining[0]);
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
        if !(self.playing || self.paused)
            || !self.follow
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
        if !self.follow {
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
        beat: 0,
        string: 0,
        voice: 0,
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
}

fn set_bar(score: &Score, c: &mut Cursor, bar: usize) {
    c.bar = bar.min(score.master_bars.len().saturating_sub(1));
    c.beat = 0;
}

fn set_track(score: &Score, c: &mut Cursor, track: usize) {
    let tick = cursor_tick(score, c);
    c.track = track.min(score.tracks.len().saturating_sub(1));
    c.beat = beat_at_tick(score, c.track, c.bar, tick);
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

#[cfg(test)]
mod tests {
    use super::*;
    use muse_core::*;

    fn test_app() -> App {
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
        };
        app
    }

    fn keys(app: &mut App, s: &str) {
        for k in crate::keys::parse_keys(s, &[Key::char(' ')]).unwrap() {
            app.handle_key(k);
        }
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
        keys(&mut app, ":q<CR>");
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
        let dir = std::env::temp_dir().join(format!("muse-test-{}", std::process::id()));
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
        let dir = std::env::temp_dir().join(format!("muse-views-{}", std::process::id()));
        let f = files(&dir, &["a.muse", "b.muse", "c.muse"]);
        let mut app = App::new(Config::default(), None);
        for p in &f {
            app.run(Command::Edit(p.clone()));
        }
        // Three tabs; the pane shows c and remembers b then a.
        let names = |app: &App| -> Vec<String> {
            app.bufferline().iter().map(|t| t.name.clone()).collect()
        };
        assert_eq!(names(&app), ["a.muse", "b.muse", "c.muse"]);
        assert!(app.bufferline()[2].active);
        // Cursors are remembered per buffer in the pane.
        app.run(Command::BufferGoto(1));
        app.panes.get_mut(&app.focused()).unwrap().cursor.string = 3;
        app.run(Command::BufferNext);
        assert_eq!(app.buffer().name, "b.muse");
        app.run(Command::BufferPrev);
        assert_eq!(app.pane().cursor.string, 3);
        // A new view copies the bufferline (tabline.new_view = "copy"), sharing the buffers.
        app.run(Command::ViewNew(None));
        assert_eq!(names(&app), ["a.muse", "b.muse", "c.muse"]);
        assert_eq!(app.view_labels(), ["1", "2"]);
        // Closing a tab here leaves it loaded for view 1.
        app.run(Command::BufferClose(Some(3), false));
        assert_eq!(names(&app), ["a.muse", "b.muse"]);
        app.run(Command::ViewPrev);
        assert_eq!(names(&app), ["a.muse", "b.muse", "c.muse"]);
        // Closing the shown buffer: the pane goes back to the one it showed before.
        app.run(Command::BufferGoto(2));
        app.run(Command::BufferGoto(3));
        app.run(Command::BufferClose(None, false));
        assert_eq!(app.buffer().name, "b.muse");
        // Unsaved changes stop a close that would unload the buffer.
        let b = app.pane().buffer;
        app.buffers[b].modified = true;
        app.run(Command::ViewNext);
        app.run(Command::BufferClose(Some(2), false));
        app.run(Command::ViewPrev);
        app.run(Command::BufferClose(None, false));
        assert!(app.message.as_ref().unwrap().error);
        assert_eq!(app.buffer().name, "b.muse");
        app.run(Command::BufferClose(None, true));
        assert_eq!(app.buffer().name, "a.muse");
        // A split pane goes back to what it showed before…
        app.run(Command::VSplit);
        app.run(Command::Edit(f[2].clone()));
        app.run(Command::BufferClose(None, false));
        assert_eq!(app.view().root.panes().len(), 2);
        assert_eq!(app.buffer().name, "a.muse");
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
            truncate_middle("A very long song title that does not fit.muse", 28, 14, 10),
            "A very long so…t fit.muse"
        );
        // Prefix and suffix are cut to fit the maximum.
        assert_eq!(truncate_middle("abcdefghij", 5, 4, 4), "abcd…");
        assert_eq!(file_kind(Some(Path::new("x.GP5"))), "guitar-pro");
        assert_eq!(file_kind(None), "new");
    }

    #[test]
    fn waveform_strip_is_per_pane_and_window_bar_toggles() {
        let mut app = App::new(Config::default(), None);
        let first = app.focused();
        app.run(Command::Split);
        let second = app.focused();
        app.run(Command::Waveform);
        assert!(!app.panes[&second].waveform);
        assert!(app.panes[&first].waveform);
        assert!(!app.global_waveform);
        app.run(Command::WaveformGlobal(None));
        assert!(app.global_waveform);
        app.run(Command::WaveformGlobal(Some(false)));
        assert!(!app.global_waveform);
        let (config, _) = Config::parse("[ui]\nglobal_waveform = true\n").unwrap();
        assert!(App::new(config, None).global_waveform);
    }

    #[test]
    fn playing_keeps_the_editing_modes() {
        let mut app = App::new(Config::default(), None);
        app.run(Command::New);
        keys(&mut app, "i3<Space>5<Esc>0");
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
    fn quitting_is_refused_with_unsaved_changes() {
        let dir = std::env::temp_dir().join(format!("muse-quit-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("a.muse");
        let mut app = App::new(Config::default(), None);
        app.run(Command::New);
        app.run(Command::Write(Some(file.clone())));
        let mut app = App::new(Config::default(), None);
        app.run(Command::Edit(file.clone()));
        keys(&mut app, "<C-q>");
        assert!(app.quit);
        // A change: <C-q>, :qa and :q on the last pane refuse…
        let mut app = App::new(Config::default(), None);
        app.run(Command::Edit(file.clone()));
        keys(&mut app, "i5<Esc>");
        keys(&mut app, "<C-q>");
        assert!(!app.quit && app.message.as_ref().unwrap().error);
        app.run(Command::Quit { force: false });
        assert!(!app.quit);
        // …:qa! discards, :wqa writes it first.
        app.run(Command::QuitAll { force: true });
        assert!(app.quit);
        let mut app = App::new(Config::default(), None);
        app.run(Command::Edit(file.clone()));
        keys(&mut app, "i7<Esc>");
        app.run(Command::WriteQuitAll);
        assert!(app.quit && !app.buffer().modified);
        // An unnamed new score can't be written by :wqa.
        let mut app = App::new(Config::default(), None);
        app.run(Command::New);
        keys(&mut app, "i5<Esc>");
        app.run(Command::WriteQuitAll);
        assert!(!app.quit);
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn maximize_shows_the_focused_pane_alone() {
        let mut app = App::new(Config::default(), None);
        app.viewport = Rect {
            x: 0,
            y: 0,
            w: 80,
            h: 40,
        };
        app.run(Command::VSplit);
        let id = app.focused();
        keys(&mut app, "<C-m>");
        assert_eq!(app.layout(), vec![(id, app.viewport)]);
        assert!(app.dividers().is_empty());
        keys(&mut app, "<C-m>");
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

    #[test]
    fn moving_the_cursor_while_playing_moves_the_play_position() {
        let mut app = test_app();
        keys(&mut app, "<Space><Space>");
        app.take_effects();
        app.set_playing(true);
        assert!(app.follow);
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
        assert!(!app.follow);
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
}
