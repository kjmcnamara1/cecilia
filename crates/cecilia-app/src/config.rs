//! TOML configuration and keymaps.

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::command::Command;
use crate::keys::{Key, parse_keys};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Mode {
    Normal,
    Command,
    /// Typing notes: digits set frets, letters toggle techniques.
    Insert,
    /// Selecting beats (or whole bars).
    Visual,
    /// Choosing a buffer, view or workspace from a filtered list.
    Picker,
    /// Names the tracks pane's keymap (`[keymap.tracks]`), used while that pane has the
    /// focus in Normal mode; never the current mode.
    Tracks,
    /// Names the properties pane's keymap (`[keymap.properties]`), like [`Mode::Tracks`].
    Properties,
    /// Names the editing palette's keymap (`[keymap.palette]`), like [`Mode::Tracks`].
    Palette,
    /// Syncing audio to the score (`:sync`): the waveform shows the sync points and rulers, and
    /// the keys of `[keymap.sync]` apply (the motions of Normal mode too).
    Sync,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct ConfigFile {
    pub general: GeneralConfig,
    pub ui: UiConfig,
    pub tabline: TablineConfig,
    pub audio: AudioConfig,
    pub swap: SwapConfig,
    pub notifications: NotificationsConfig,
    pub popups: PopupsConfig,
    pub theme: crate::theme::ThemeConfig,
    pub toolbar: ToolbarConfig,
    pub statusline: StatuslineConfig,
    pub score: ScoreConfig,
    pub keymap: KeymapConfig,
}

/// Notifications (`[notifications]`): where the toast cards go and how long they stay.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct NotificationsConfig {
    /// The corner or side the cards stack against: "top_left", "top", "top_right", "right",
    /// "bottom_right", "bottom", "bottom_left" or "left".
    pub anchor: String,
    /// What the cards sit in: "scores" (the area of the score panes) or "window".
    pub area: String,
    /// Seconds an information card stays (0: until dismissed).
    pub info_timeout: f64,
    /// Seconds an error card stays (0: until dismissed).
    pub error_timeout: f64,
    /// Most cards shown at once; the rest are counted in a "+N more" line.
    pub max_visible: usize,
    /// Notifications kept in the history (0: no history).
    pub history_size: usize,
    /// How the history picker shows when a notification appeared: "relative" (5m ago) or
    /// "absolute" (the date and time).
    pub history_time: String,
}

impl Default for NotificationsConfig {
    fn default() -> Self {
        Self {
            anchor: "top_right".into(),
            area: "scores".into(),
            info_timeout: 4.0,
            error_timeout: 8.0,
            max_visible: 5,
            history_size: 500,
            history_time: "relative".into(),
        }
    }
}

/// How popups (pickers, the command palette, notification cards) look (`[popups]`).
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct PopupsConfig {
    /// How opaque their fill is, in percent: 0 (invisible) to 100.
    pub opacity: f64,
    /// How far the page behind them is blurred (pixels; 0: not blurred).
    pub blur: f64,
    /// How dark the shadow under them is, in percent: 0 (none) to 100.
    pub shadow_opacity: f64,
    /// How far the shadow is blurred (pixels).
    pub shadow_blur: f64,
    /// How far the shadow is pushed down (pixels).
    pub shadow_offset: f64,
}

impl Default for PopupsConfig {
    fn default() -> Self {
        Self {
            opacity: 85.0,
            blur: 20.0,
            shadow_opacity: 45.0,
            shadow_blur: 15.0,
            shadow_offset: 7.0,
        }
    }
}

/// Score defaults (`[score]`).
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct ScoreConfig {
    /// Clef of a standard staff added without one: "treble", "bass" or "grand".
    pub clef: String,
}

impl Default for ScoreConfig {
    fn default() -> Self {
        Self {
            clef: "treble".into(),
        }
    }
}

/// The status line: how the terminal separates its sections: "arrow" (default), "round" or
/// "none" (the GUI draws rounded chips).
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct StatuslineConfig {
    pub separators: String,
}

impl Default for StatuslineConfig {
    fn default() -> Self {
        Self {
            separators: "arrow".into(),
        }
    }
}

/// The toolbar's buttons: command lines in order, `"|"` between groups.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct ToolbarConfig {
    pub items: Vec<String>,
}

impl Default for ToolbarConfig {
    fn default() -> Self {
        Self {
            items: crate::toolbar::DEFAULT_ITEMS
                .iter()
                .map(|s| s.to_string())
                .collect(),
        }
    }
}

/// The tabline: the active view's buffers as tabs on the left, the views on the right.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct TablineConfig {
    /// What a new view starts with: "copy" (the current view's buffers), "current" (the
    /// current buffer only) or "empty".
    pub new_view: String,
    /// File-type icons in the terminal UI: "nerd" (Nerd Font glyphs), "unicode" or "none".
    pub icons: String,
    /// Hide the views on the right of the tabline while there is only one.
    pub hide_single_view: bool,
    /// `:buffer-next` / `:buffer-prev` wrap around at the ends of the tabline (off: they stop
    /// at the last / first tab).
    pub wrap: bool,
    pub truncation: TruncationConfig,
}

impl Default for TablineConfig {
    fn default() -> Self {
        Self {
            new_view: "copy".into(),
            icons: "nerd".into(),
            hide_single_view: true,
            wrap: true,
            truncation: TruncationConfig::default(),
        }
    }
}

/// Long file names in tabs keep their first `prefix_width` and last `suffix_width`
/// characters around an ellipsis, to at most `max_width` characters.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct TruncationConfig {
    pub max_width: usize,
    pub prefix_width: usize,
    pub suffix_width: usize,
}

impl Default for TruncationConfig {
    fn default() -> Self {
        Self {
            max_width: 28,
            prefix_width: 14,
            suffix_width: 10,
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct GeneralConfig {
    pub leader: String,
}

impl Default for GeneralConfig {
    fn default() -> Self {
        Self {
            leader: "<Space>".into(),
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct UiConfig {
    /// TUI: characters per sixteenth note in the tab view.
    pub tab_width: u8,
    /// Scroll to keep the playhead in view while playing.
    pub follow_playback: bool,
    /// `j`/`k` go on from the last row of a staff into the next staff, as `:string-down` and
    /// `:string-up` once always did; off, they keep to the cursor's staff (`:staff-next` and
    /// `:staff-prev` change staff).
    pub jk_moves_between_staves: bool,
    /// Going from a chant score to its lyrics text area (`<leader>P`, a pane switch) puts the
    /// caret (Normal mode) on the note the cursor is on (`:lyrics-select` does so always).
    pub lyrics_select_cursor: bool,
    /// Command-line Tab completion ignores letter case.
    pub completion_ignore_case: bool,
    /// Command-line completion matches the typed letters in order anywhere in a candidate.
    pub completion_fuzzy: bool,
    /// How far down the window (percent) the command palette's and pickers' input line sits;
    /// the pickers' lists grow down from it, the command history's up.
    pub picker_position: u8,
    /// The most candidates the command palette and pickers list at once.
    pub picker_max_items: u8,
    /// Order command completion and the buffer, recent file and workspace pickers by how often
    /// and how lately each was used (zoxide's way), not alphabetically or by position.
    pub frecency: bool,
    /// Command lines kept in the command history (0: no history).
    pub command_history_size: usize,
    /// How the command history shows when a line was run: "relative" (5m ago) or "absolute"
    /// (the date and time).
    pub command_history_time: String,
    /// No longer used (`global_tools` lists the audio pane instead); kept so old configs
    /// still load, with a warning.
    #[serde(skip_serializing)]
    pub global_waveform: Option<bool>,
    /// A progress bar along the bottom of every score pane: sections and the play position.
    pub progress_bar: bool,
    /// A scrollbar on each score pane: down its right side, or along the bottom in the
    /// horizontal layout.
    pub scrollbar: bool,
    /// The tracks panel (tracks with mixer and a bar overview) at the bottom of the window.
    pub tracks_panel: bool,
    /// The properties pane (score, tracks and lyrics) along the right of the window.
    pub properties: bool,
    /// The editing palette (durations, rests, effects …) along the left of the window.
    pub palette: bool,
    /// The palette's keyboard motions wrap around at its edges (down from the last line to the
    /// first, right from the last button of a line to the first of the next …).
    pub palette_wrap: bool,
    /// Tool panes that dock along the bottom of the whole view, at its full width, and show
    /// the last active score pane ("tracks", "waveform", "fretboard", "piano", "tuner"); the
    /// others dock under the score pane they open from, as wide as it, and always show it.
    pub global_tools: Vec<String>,
    /// Seconds the tracks show along the bottom after a key or command changes the track,
    /// while no tracks pane is shown (0: off).
    pub tracks_peek: f64,
    /// Tracks panes follow the height their score needs, until resized by hand.
    pub auto_resize: bool,
    /// Closing a pane gives the split it was in equal weights again, so the remaining panes
    /// share the space equally (off: the neighbour takes the closed pane's space).
    pub equalize_on_close: bool,
    /// `:view-new` starts the new view with the current files (off: with the dashboard).
    pub keep_files_in_new_view: bool,
    /// Save the active workspace (`:workspace-save`) when quitting; `:quit!` never does.
    pub workspace_autosave_on_quit: bool,
    /// Starting without arguments loads the last workspace (off: the dashboard, with no workspace).
    pub workspace_restore_last: bool,
    /// The toolbar of common commands at the top of the window.
    pub toolbar: bool,
    /// The fretboard and piano panels (notes at the cursor or playhead).
    pub fretboard: bool,
    pub piano: bool,
    /// While playing with follow on, hide the edit cursor: only the red playhead shows.
    pub hide_cursor_while_following: bool,
    /// GUI score layout: "page", "vertical" or "horizontal".
    pub layout: String,
    /// GUI zoom factor.
    pub zoom: f64,
    /// Which note of a chord `:modern-to-chant` keeps (chant is monophonic): "top" (the
    /// default), "bottom", or "error" (the conversion refuses, a message names the chord).
    pub chant_chords: String,
    /// Bar numbers on the score: `"system"` (the first bar of each line) or `"every"` bar.
    pub bar_numbers: String,
    /// The rhythm row under a tab: `"auto"` (only when the track has no standard or slash
    /// staff), `"on"` or `"off"`.
    pub tab_rhythm: String,
    /// While `'` waits for a mark letter: where the marks are drawn on the score, `"exact"`
    /// (on the position a jump lands on), `"above"` (over the staff, at that beat) or `"off"`.
    pub mark_labels: MarkLabels,
    /// Draw the staff lines of bars that don't hold their full count in red.
    pub highlight_incomplete_bars: bool,
    /// Thickness in pixels of the ring around the focused pane in the GUI (0: none; 0 to 8).
    pub focus_ring_width: f64,
    /// Color of that ring (#rgb, #rrggbb or #rrggbbaa); empty: the current mode's color.
    pub focus_ring_color: String,
    /// GUI: the font family of the lyrics text area in the properties pane: the app's text font
    /// (Source Sans 3) by default, or any installed font family.
    pub lyrics_font: String,
    /// GUI: its size in points.
    pub lyrics_font_size: f64,
    /// The font of the lyrics of a chant score: `noto-serif` (default) or `source-sans`. The
    /// versicle ℣, response ℟, dagger † and barred Ⱥ are Noto Serif's either way.
    pub chant_text_font: String,
    /// GUI: the waveform's time ruler along the bottom (a fixed scale): `all` (a tick each second,
    /// a tall one every five), `major` or `off`.
    pub waveform_time_ticks: Ticks,
    /// GUI: the waveform's score ruler along the top, which stretches with the sync points (a
    /// chant's bars are its major ticks, its syllables the minor ones).
    pub waveform_beat_ticks: Ticks,
    /// GUI: label the major ticks of the time ruler (minutes left of the tick, seconds right).
    pub waveform_time_labels: bool,
    /// GUI: label the ticks of the score ruler (the bar left of a major tick, the syllable right).
    pub waveform_beat_labels: bool,
    /// Sync mode: how far (milliseconds, 0 to 200) a sync point is moved to reach the strongest
    /// onset of the audio near where it was put; 0 puts it exactly there.
    pub sync_snap: f64,
}

/// Where the marks show on the score while `'` waits for a letter.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum MarkLabels {
    #[default]
    Exact,
    Above,
    Off,
}

/// Which ticks of a waveform ruler are drawn.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Ticks {
    #[default]
    All,
    Major,
    Off,
}

impl Ticks {
    /// Whether a major (`true`) or minor tick is drawn.
    pub fn shows(self, major: bool) -> bool {
        match self {
            Ticks::All => true,
            Ticks::Major => major,
            Ticks::Off => false,
        }
    }
}

impl Default for UiConfig {
    fn default() -> Self {
        Self {
            tab_width: 2,
            follow_playback: true,
            jk_moves_between_staves: false,
            lyrics_select_cursor: true,
            completion_ignore_case: true,
            completion_fuzzy: true,
            picker_position: 25,
            picker_max_items: 12,
            frecency: true,
            command_history_size: 500,
            command_history_time: "relative".into(),
            global_waveform: None,
            progress_bar: true,
            scrollbar: true,
            tracks_panel: false,
            properties: false,
            palette: false,
            palette_wrap: true,
            global_tools: ["tracks", "waveform", "fretboard", "piano", "tuner"]
                .map(String::from)
                .to_vec(),
            tracks_peek: 1.0,
            auto_resize: true,
            equalize_on_close: true,
            keep_files_in_new_view: true,
            workspace_autosave_on_quit: true,
            workspace_restore_last: true,
            toolbar: true,
            fretboard: false,
            piano: false,
            hide_cursor_while_following: true,
            layout: "vertical".into(),
            chant_chords: "top".into(),
            bar_numbers: "system".into(),
            tab_rhythm: "auto".into(),
            highlight_incomplete_bars: false,
            focus_ring_width: 4.0,
            focus_ring_color: String::new(),
            zoom: 1.0,
            lyrics_font: "Source Sans 3".into(),
            lyrics_font_size: 12.0,
            chant_text_font: "noto-serif".into(),
            mark_labels: MarkLabels::Exact,
            waveform_time_ticks: Ticks::All,
            waveform_beat_ticks: Ticks::All,
            waveform_time_labels: true,
            waveform_beat_labels: true,
            sync_snap: 70.0,
        }
    }
}

/// Swap files (`[swap]`): recovery copies of buffers with unsaved changes.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct SwapConfig {
    /// Write swap files.
    pub enabled: bool,
    /// Seconds after the last change before the swap file is written (and at the latest six
    /// times this while editing continues).
    pub interval: f64,
    /// Opening a file with a newer swap file (not owned by a running cecilia) restores it as
    /// unsaved changes; otherwise a message points to `:recover` and `:swap-discard`.
    pub auto_recover: bool,
    /// Where swap files go (`$XDG_STATE_HOME/cecilia/swap`, else `~/.local/state/cecilia/swap`, when unset).
    pub dir: Option<PathBuf>,
}

impl Default for SwapConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            interval: 5.0,
            auto_recover: true,
            dir: None,
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct AudioConfig {
    /// SoundFont (.sf2) for MIDI playback; the built-in synth is used when unset.
    pub soundfont: Option<PathBuf>,
    /// Playback speed multiplier (1.0 = written tempo).
    pub speed: f64,
    /// The tuner's reference pitch for A4, in Hz.
    pub tuner_a4: f64,
}

impl Default for AudioConfig {
    fn default() -> Self {
        Self {
            soundfont: None,
            speed: 1.0,
            tuner_a4: 440.0,
        }
    }
}

/// A keymap value: one command or a list of commands run in order.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(untagged)]
pub enum Binding {
    One(String),
    Many(Vec<String>),
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct KeymapConfig {
    pub normal: HashMap<String, Binding>,
    pub command: HashMap<String, Binding>,
    pub insert: HashMap<String, Binding>,
    /// Normal-mode keys for the notation under the cursor, looked up before `normal`.
    #[serde(rename = "normal-tab")]
    pub normal_tab: HashMap<String, Binding>,
    #[serde(rename = "normal-standard")]
    pub normal_standard: HashMap<String, Binding>,
    #[serde(rename = "normal-slash")]
    pub normal_slash: HashMap<String, Binding>,
    #[serde(rename = "normal-chant", alias = "chant")]
    pub normal_chant: HashMap<String, Binding>,
    /// Insert-mode keys for the notation under the cursor, looked up before `insert`.
    #[serde(rename = "insert-tab")]
    pub insert_tab: HashMap<String, Binding>,
    #[serde(rename = "insert-standard")]
    pub insert_standard: HashMap<String, Binding>,
    #[serde(rename = "insert-slash")]
    pub insert_slash: HashMap<String, Binding>,
    #[serde(rename = "insert-chant")]
    pub insert_chant: HashMap<String, Binding>,
    pub visual: HashMap<String, Binding>,
    pub tracks: HashMap<String, Binding>,
    pub properties: HashMap<String, Binding>,
    pub palette: HashMap<String, Binding>,
    pub sync: HashMap<String, Binding>,
    /// Keys that apply while design mode is on, looked up before the normal ones.
    pub design: HashMap<String, Binding>,
    /// No longer used (Play mode was removed); kept so old configs still load, with a warning.
    pub play: HashMap<String, Binding>,
}

const DEFAULT_NORMAL: &[(&str, &str)] = &[
    ("S", "sync"),
    ("h", "beat-prev"),
    ("<Left>", "beat-prev"),
    ("l", "beat-next"),
    ("<Right>", "beat-next"),
    ("j", "string-down"),
    ("<Down>", "string-down"),
    ("k", "string-up"),
    ("<Up>", "string-up"),
    ("<A-j>", "staff-next"),
    ("<A-k>", "staff-prev"),
    ("w", "note-row-next"),
    ("e", "note-row-end"),
    ("b", "note-row-prev"),
    ("W", "note-next"),
    ("B", "note-prev"),
    ("^", "bar-first-note"),
    ("gh", "bar-first-note"),
    ("$", "bar-last-note"),
    ("gl", "bar-last-note"),
    ("}", "bar-next"),
    ("{", "bar-prev"),
    ("0", "bar-start"),
    ("<Home>", "bar-start"),
    ("<End>", "bar-end"),
    ("gg", "goto-first"),
    ("G", "goto-last"),
    ("]v", "voice-next"),
    ("[v", "voice-prev"),
    ("<A-F1>", "voice 1"),
    ("<A-F2>", "voice 2"),
    ("<A-F3>", "voice 3"),
    ("<A-F4>", "voice 4"),
    ("J", "track-next"),
    ("K", "track-prev"),
    ("<C-d>", "page-down"),
    ("<PageDown>", "page-down"),
    ("<C-u>", "page-up"),
    ("<PageUp>", "page-up"),
    ("zz", "scroll-center"),
    ("zt", "scroll-top"),
    ("zb", "scroll-bottom"),
    ("<leader><leader>", "play-toggle"),
    ("<leader>s", "stop"),
    ("<leader>f", "set ui.follow_playback toggle"),
    ("<C-o>", "open"),
    ("<C-q>", "quit"),
    ("<C-w>s", "split"),
    ("<C-w>+", "resize +5"),
    ("<C-w>-", "resize -5"),
    ("<C-w>>", "vertical-resize +5"),
    ("<C-w><lt>", "vertical-resize -5"),
    ("<C-w>=", "resize-equal"),
    ("<C-w>H", "pane-move-left"),
    ("<C-w>J", "pane-move-down"),
    ("<C-w>K", "pane-move-up"),
    ("<C-w>L", "pane-move-right"),
    ("<C-w>x", "pane-exchange"),
    ("<C-w>r", "pane-rotate"),
    ("<C-w>R", "pane-rotate-back"),
    ("<A-m>", "maximize"),
    ("<C-w>z", "maximize"),
    ("<C-w>t", "tracks-focus"),
    ("<leader>T", "tracks"),
    ("<leader>p", "properties"),
    ("<leader>P", "properties-focus"),
    ("<leader>e", "palette"),
    ("<leader>E", "palette-focus"),
    ("<leader>B", "toolbar"),
    ("]s", "section-next"),
    ("[s", "section-prev"),
    ("<leader>S", "pick-section"),
    ("<leader>F", "fretboard"),
    ("<leader>K", "piano"),
    ("<leader>u", "tuner"),
    ("<leader>>", "buffer-move-next"),
    ("<leader><lt>", "buffer-move-prev"),
    ("<leader>?", "pick-keymap"),
    ("<leader>nh", "notifications"),
    ("<leader>nd", "notify-dismiss"),
    ("<C-w>v", "vsplit"),
    ("<C-w>h", "focus-left"),
    ("<C-w>j", "focus-down"),
    ("<C-w>k", "focus-up"),
    ("<C-w>l", "focus-right"),
    ("<C-w>w", "focus-next"),
    ("<C-w><C-w>", "focus-next"),
    ("<C-w>W", "focus-prev"),
    ("<C-w>p", "focus-last"),
    ("<C-w><C-p>", "focus-last"),
    ("<C-w>q", "pane-close"),
    ("<C-w>c", "pane-close"),
    ("gt", "view-next"),
    ("gT", "view-prev"),
    ("L", "buffer-next"),
    ("H", "buffer-prev"),
    ("<leader>x", "buffer-close"),
    ("<leader>b", "pick-buffer"),
    ("<leader>'", "buffer-alternate"),
    ("'", "jump-mark"),
    ("`", "set-mark"),
    ("<leader>V", "pick-view"),
    ("<leader>W", "pick-workspace"),
    (":", "command-mode"),
    ("ZQ", "quit!"),
    ("ZZ", "write-quit"),
    // Editing
    ("i", "insert"),
    ("a", "append"),
    ("I", "insert-bar-start"),
    ("A", "append-bar-end"),
    ("gi", "insert-last"),
    ("R", "replace-mode"),
    ("gv", "visual-reselect"),
    ("x", "delete-note"),
    ("d", "delete"),
    ("y", "yank"),
    ("c", "change"),
    ("p", "put"),
    ("P", "put-before"),
    ("u", "undo"),
    ("U", "redo"),
    ("g-", "undo-older"),
    ("g+", "undo-newer"),
    (".", "repeat"),
    ("q", "record-macro"),
    ("@", "play-macro"),
    ("\"", "register"),
    ("v", "visual"),
    ("V", "visual-bar"),
    ("o", "bar-open-after"),
    ("O", "bar-open-before"),
    ("+", "duration-longer"),
    ("-", "duration-shorter"),
    ("<C-a>", "pitch-up"),
    ("<C-x>", "pitch-down"),
    ("r", "rest"),
    ("t.", "dot"),
    ("t3", "tuplet 3"),
    ("<F1>", "help"),
    ("<leader>v", "layout"),
    ("<leader>d", "design"),
    ("<leader>a", "mix"),
    ("<leader>m", "metronome"),
    ("<leader>c", "count-in"),
    ("<leader>-", "speed-down"),
    ("<leader>=", "speed-up"),
    ("<leader>l", "loop-toggle"),
    ("<leader>t", "sync-tap"),
    ("<leader>w", "waveform"),
    ("<C-=>", "zoom-in"),
    ("<C-+>", "zoom-in"),
    ("<C-->", "zoom-out"),
    ("<C-0>", "zoom-reset"),
];

/// Normal mode on a tab (and drum lanes): the technique keys of a fretted instrument.
const DEFAULT_NORMAL_TAB: &[(&str, &str)] = &[
    ("th", "toggle hammer"),
    ("tb", "toggle bend"),
    ("ts", "toggle slide"),
    ("tv", "toggle vibrato"),
    ("tx", "toggle dead"),
    ("tm", "toggle palm-mute"),
    ("tl", "toggle let-ring"),
    ("tg", "toggle ghost"),
    ("ta", "toggle accent"),
    ("tn", "toggle harmonic"),
    ("tt", "toggle tie"),
];

/// Normal mode on a standard staff.
const DEFAULT_NORMAL_STANDARD: &[(&str, &str)] = &[
    ("th", "toggle hammer"),
    ("ts", "toggle slide"),
    ("tv", "toggle vibrato"),
    ("tx", "toggle dead"),
    ("tg", "toggle ghost"),
    ("ta", "toggle accent"),
    ("tt", "toggle tie"),
    ("t#", "accidental sharp"),
    ("t&", "accidental flat"),
    ("t=", "accidental natural"),
];

/// Normal mode on a slash staff.
const DEFAULT_NORMAL_SLASH: &[(&str, &str)] = &[("ta", "toggle accent"), ("tt", "toggle tie")];

/// Normal mode on a chant score (`[keymap.normal-chant]`): the `t` keys put signs on the cursor
/// note (heads, rhythmic signs, alterations), `tc` / `tF` set the clef and `td` adds a division.
/// They are looked up before `[keymap.normal]` and only on chant scores, so no modern key is lost.
const DEFAULT_NORMAL_CHANT: &[(&str, &str)] = &[
    ("<A-i>", "lyric-mode"),
    ("<A-J>", "chant-join"),
    ("<A-S>", "chant-split"),
    ("<S-Del>", "lyrics-delete"),
    ("tp", "chant-shape punctum"),
    ("ti", "chant-shape inclinatum"),
    ("tv", "chant-shape virga"),
    ("tV", "chant-shape bivirga"),
    ("ts", "chant-shape stropha"),
    ("tq", "chant-shape quilisma"),
    ("to", "chant-shape oriscus"),
    ("tu", "chant-shape quadratum"),
    ("t~", "chant-liquescence deminutus"),
    ("t<", "chant-liquescence ascendens"),
    ("t>", "chant-liquescence descendens"),
    ("tm", "chant-mora"),
    ("t.", "chant-mora"),
    ("te", "chant-episema"),
    ("tt", "chant-ictus"),
    ("tb", "chant-alter flat"),
    ("tn", "chant-alter natural"),
    ("t*", "chant-symbol *"),
    ("t+", "chant-symbol +"),
    ("td,", "chant-division ,"),
    ("td;", "chant-division ;"),
    ("td:", "chant-division :"),
    ("tD", "chant-division ::"),
    ("tc1", "chant-clef c1"),
    ("tc2", "chant-clef c2"),
    ("tc3", "chant-clef c3"),
    ("tc4", "chant-clef c4"),
    ("tcb", "chant-clef cb3"),
    ("tF1", "chant-clef f1"),
    ("tF2", "chant-clef f2"),
    ("tF3", "chant-clef f3"),
    ("tF4", "chant-clef f4"),
    // Audio sync by hand (`<leader>t` taps, `:sync` starts tapping).
    ("<leader>g", "lyrics-select"),
    ("<leader>yh", "sync-here"),
    ("<leader>yu", "sync-undo"),
    ("<leader>y[", "sync-nudge -10"),
    ("<leader>y]", "sync-nudge 10"),
    ("<leader>y{", "sync-nudge -50"),
    ("<leader>y}", "sync-nudge 50"),
    ("<leader>yy", "sync-unit syllable"),
    ("<leader>yn", "sync-unit note"),
];

const DEFAULT_INSERT: &[(&str, &str)] = &[
    ("<Esc>", "normal"),
    ("<Left>", "beat-prev"),
    ("<Right>", "beat-next"),
    ("<Space>", "insert-next"),
    ("<Up>", "string-up"),
    ("<Down>", "string-down"),
    ("<BS>", "delete-note"),
    ("<Del>", "delete-beat"),
    ("<CR>", "insert-enter"),
    ("<A-CR>", "beat-new"),
    ("<A-v>", "voice-next"),
    ("<A-V>", "voice-prev"),
    ("<A-F1>", "voice 1"),
    ("<A-F2>", "voice 2"),
    ("<A-F3>", "voice 3"),
    ("<A-F4>", "voice 4"),
    ("h", "beat-prev"),
    ("j", "string-down"),
    ("k", "string-up"),
    ("l", "beat-next"),
    ("r", "rest"),
    (".", "dot"),
    ("+", "duration-longer"),
    ("-", "duration-shorter"),
    ("<A-1>", "duration 1"),
    ("<A-2>", "duration 2"),
    ("<A-3>", "duration 4"),
    ("<A-4>", "duration 8"),
    ("<A-5>", "duration 16"),
    ("<A-6>", "duration 32"),
    ("<A-7>", "duration 64"),
    ("<C-t>", "tuplet 3"),
    ("<C-a>", "pitch-up"),
    ("<C-x>", "pitch-down"),
];

/// Insert mode on a tab (and drum lanes): digits type frets, letters toggle effects.
const DEFAULT_INSERT_TAB: &[(&str, &str)] = &[
    ("H", "toggle hammer"),
    ("b", "toggle bend"),
    ("s", "toggle slide"),
    ("~", "toggle vibrato"),
    ("x", "toggle dead"),
    ("m", "toggle palm-mute"),
    ("L", "toggle let-ring"),
    ("g", "toggle ghost"),
    ("a", "toggle accent"),
    ("n", "toggle harmonic"),
    ("t", "toggle tie"),
];

/// Insert mode on a standard staff: letters enter notes nearest the cursor.
const DEFAULT_INSERT_STANDARD: &[(&str, &str)] = &[
    ("a", "note-letter a"),
    ("b", "note-letter b"),
    ("c", "note-letter c"),
    ("d", "note-letter d"),
    ("e", "note-letter e"),
    ("f", "note-letter f"),
    ("g", "note-letter g"),
    ("#", "accidental sharp"),
    ("&", "accidental flat"),
    ("=", "accidental natural"),
    ("t", "toggle tie"),
    ("H", "toggle hammer"),
    ("s", "toggle slide"),
    ("~", "toggle vibrato"),
    (">", "toggle accent"),
    ("(", "toggle ghost"),
    ("x", "toggle dead"),
];

/// Insert mode on a slash staff: rhythm only.
const DEFAULT_INSERT_SLASH: &[(&str, &str)] =
    &[("/", "slash"), ("t", "toggle tie"), (">", "toggle accent")];

/// Insert mode on chant: GABC pitch letters and signs.
const DEFAULT_INSERT_CHANT: &[(&str, &str)] = &[
    ("G", "chant-shape inclinatum"),
    ("v", "chant-mod v"),
    ("V", "chant-mod V"),
    ("s", "chant-mod s"),
    ("o", "chant-mod o"),
    ("O", "chant-mod O"),
    ("q", "chant-mod q"),
    ("w", "chant-mod w"),
    ("~", "chant-mod ~"),
    ("<lt>", "chant-liquescence ascendens"),
    (">", "chant-liquescence descendens"),
    ("+", "chant-custos"),
    ("r", "chant-mod r"),
    ("x", "chant-alter flat"),
    ("y", "chant-alter natural"),
    ("#", "chant-alter sharp"),
    ("z", "chant-linebreak z"),
    ("Z", "chant-linebreak Z"),
    (".", "chant-mod ."),
    ("_", "chant-mod _"),
    ("'", "chant-mod '"),
    ("-", "chant-mod -"),
    (",", "chant-bar ,"),
    (";", "chant-bar ;"),
    (":", "chant-bar :"),
    ("<Space>", "insert-next"),
    ("<Right>", "beat-next"),
    ("/", "chant-sep /"),
    ("!", "chant-sep !"),
    ("<A-J>", "chant-join"),
    ("<A-S>", "chant-split"),
    ("<Del>", "delete-note"),
];

/// The tracks panel, while it has the focus.
const DEFAULT_TRACKS: &[(&str, &str)] = &[
    ("j", "tracks-down"),
    ("<Down>", "tracks-down"),
    ("k", "tracks-up"),
    ("<Up>", "tracks-up"),
    ("l", "bar-next"),
    ("<Right>", "bar-next"),
    ("h", "bar-prev"),
    ("<Left>", "bar-prev"),
    ("gg", "goto-first"),
    ("G", "goto-last"),
    ("s", "solo"),
    ("m", "mute"),
    ("+", "track-volume-up"),
    ("=", "track-volume-up"),
    ("-", "track-volume-down"),
    ("c", "track-color"),
    ("t", "track-rename-start"),
    ("i", "track-instrument"),
    ("<CR>", "tracks-open"),
    ("<Esc>", "tracks-focus"),
    ("q", "tracks-focus"),
    ("<leader><leader>", "play-toggle"),
    ("<leader>s", "stop"),
    ("<leader>f", "set ui.follow_playback toggle"),
    ("<leader>T", "tracks"),
    ("<leader>?", "pick-keymap"),
    ("<C-q>", "quit"),
    (":", "command-mode"),
];

/// Normal-mode keys that apply while design mode is on (page layout), over the normal ones.
const DEFAULT_DESIGN: &[(&str, &str)] = &[
    ("<", "bar-width -10"),
    (">", "bar-width +10"),
    ("=", "bar-width auto"),
    ("<A-h>", "beat-nudge -2"),
    ("<A-l>", "beat-nudge +2"),
    ("<A-0>", "beat-nudge reset"),
    ("|", "line-break"),
    ("_", "line-join"),
    ("+", "bars-per-line +1"),
    ("-", "bars-per-line -1"),
    ("<A-=>", "bars-per-line auto"),
    ("<leader>d", "design off"),
];

const DEFAULT_SYNC: &[(&str, &str)] = &[
    ("<Esc>", "normal"),
    ("<Space>", "play-toggle"),
    ("<CR>", "sync-tap"),
    ("u", "sync-undo"),
    ("<BS>", "sync-undo"),
    ("x", "sync-remove"),
    ("[", "sync-nudge -10"),
    ("]", "sync-nudge 10"),
    ("{", "sync-nudge -50"),
    ("}", "sync-nudge 50"),
    ("y", "sync-unit syllable"),
    ("n", "sync-unit note"),
];

const DEFAULT_PALETTE: &[(&str, &str)] = &[
    ("gg", "palette-first"),
    ("G", "palette-last"),
    ("j", "palette-down"),
    ("<Down>", "palette-down"),
    ("<Tab>", "palette-down"),
    ("k", "palette-up"),
    ("<Up>", "palette-up"),
    ("<S-Tab>", "palette-up"),
    ("l", "palette-right"),
    ("<Right>", "palette-right"),
    ("h", "palette-left"),
    ("<Left>", "palette-left"),
    ("<CR>", "palette-activate"),
    ("<Space>", "palette-activate"),
    ("/", "palette-search"),
    ("n", "palette-search-next"),
    ("N", "palette-search-prev"),
    ("za", "palette-fold"),
    ("zc", "palette-fold close"),
    ("zo", "palette-fold open"),
    ("<Esc>", "palette-focus"),
    ("q", "palette-focus"),
    ("<leader>e", "palette"),
    ("<leader>E", "palette-focus"),
];

const DEFAULT_PROPERTIES: &[(&str, &str)] = &[
    ("j", "properties-down"),
    ("<Down>", "properties-down"),
    ("<Tab>", "properties-down"),
    ("k", "properties-up"),
    ("<Up>", "properties-up"),
    ("<S-Tab>", "properties-up"),
    ("gg", "properties-first"),
    ("G", "properties-last"),
    ("zz", "properties-center"),
    ("l", "properties-box +1"),
    ("<Right>", "properties-box +1"),
    ("h", "properties-box -1"),
    ("<Left>", "properties-box -1"),
    ("<A-l>", "properties-step +1"),
    ("<A-h>", "properties-step -1"),
    ("L", "properties-tab"),
    ("<C-Tab>", "properties-tab"),
    ("H", "properties-tab-prev"),
    ("<C-S-Tab>", "properties-tab-prev"),
    ("<A-j>", "properties-staff-move +1"),
    ("<A-k>", "properties-staff-move -1"),
    ("1", "properties-tab score"),
    ("2", "properties-tab track"),
    ("3", "properties-tab lyrics"),
    ("<CR>", "properties-edit"),
    ("i", "properties-edit"),
    ("a", "properties-edit"),
    ("c", "properties-change"),
    ("za", "properties-fold"),
    ("zc", "properties-fold close"),
    ("zo", "properties-fold open"),
    ("x", "properties-clear"),
    ("d", "properties-clear"),
    ("/", "properties-search"),
    ("n", "properties-search-next"),
    ("N", "properties-search-prev"),
    ("<Esc>", "properties-focus"),
    ("q", "properties-focus"),
    ("<leader><leader>", "play-toggle"),
    ("<leader>s", "stop"),
    ("<leader>f", "set ui.follow_playback toggle"),
    ("<leader>p", "properties"),
    ("<leader>P", "properties-focus"),
    ("<leader>?", "pick-keymap"),
    ("<C-q>", "quitall"),
    (":", "command-mode"),
];

const DEFAULT_VISUAL: &[(&str, &str)] = &[
    ("<Esc>", "normal"),
    ("d", "delete"),
    ("x", "delete"),
    ("y", "yank"),
    ("c", "change"),
    ("v", "visual"),
    ("V", "visual-bar"),
    ("+", "duration-longer"),
    ("-", "duration-shorter"),
    ("<A-+>", "dynamic-step +1"),
    ("<A-->", "dynamic-step -1"),
    ("i", "visual-insert"),
    ("a", "visual-append"),
    ("o", "visual-other-end"),
];

/// Resolved key bindings for one mode.
#[derive(Debug, Clone, Default)]
pub struct Keymap {
    pub bindings: HashMap<Vec<Key>, Vec<Command>>,
    /// The commands of each binding as written (for listing keymaps).
    pub names: HashMap<Vec<Key>, Vec<String>>,
}

impl Keymap {
    fn bind_default(&mut self, keys: Vec<Key>, cmd: &str) {
        let parsed = Command::parse(cmd).expect("valid default command");
        self.names.insert(keys.clone(), vec![cmd.to_string()]);
        self.bindings.insert(keys, vec![parsed]);
    }
}

pub enum Lookup<'a> {
    Exact(&'a [Command]),
    Prefix,
    None,
}

impl Keymap {
    pub fn lookup(&self, keys: &[Key]) -> Lookup<'_> {
        let longer = self
            .bindings
            .keys()
            .any(|k| k.len() > keys.len() && k.starts_with(keys));
        match self.bindings.get(keys) {
            Some(cmds) if !longer => Lookup::Exact(cmds),
            _ if longer => Lookup::Prefix,
            Some(cmds) => Lookup::Exact(cmds),
            None => Lookup::None,
        }
    }

    /// Continuations of a pending prefix, for a which-key style hint.
    pub fn continuations(&self, prefix: &[Key]) -> Vec<(Vec<Key>, &[Command])> {
        let mut v: Vec<_> = self
            .bindings
            .iter()
            .filter(|(k, _)| k.len() > prefix.len() && k.starts_with(prefix))
            .map(|(k, c)| (k[prefix.len()..].to_vec(), c.as_slice()))
            .collect();
        v.sort_by(|a, b| a.0.cmp(&b.0));
        v
    }
}

/// Effective configuration: parsed file plus resolved keymaps.
/// Which insert-mode keymap applies: the notation under the cursor.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum InsertKind {
    Tab,
    Standard,
    Slash,
    Chant,
}

#[derive(Debug, Clone)]
pub struct Config {
    pub file: ConfigFile,
    pub keymaps: HashMap<Mode, Keymap>,
    /// Insert-mode keys per notation, looked up before the shared insert keymap.
    pub insert_keymaps: HashMap<InsertKind, Keymap>,
    /// Normal-mode keys per notation, looked up before the shared normal keymap.
    pub normal_keymaps: HashMap<InsertKind, Keymap>,
    /// Normal-mode keys while design mode is on, looked up before all the others.
    pub design_keymap: Keymap,
    pub leader: Vec<Key>,
    /// Interface colors (`[theme]`).
    pub theme: crate::theme::Theme,
    /// Option values given on the command line (or `:set` sessions): resolved name -> value.
    /// They win over the file and survive a config reload.
    pub overrides: BTreeMap<String, String>,
}

impl Default for Config {
    fn default() -> Self {
        Self::from_file(ConfigFile::default()).0
    }
}

impl Config {
    /// Builds a config, collecting (not failing on) invalid keymap entries.
    pub fn from_file(mut file: ConfigFile) -> (Self, Vec<String>) {
        let mut errors = Vec::new();
        let leader = parse_keys(&file.general.leader, &[]).unwrap_or_else(|e| {
            errors.push(format!("general.leader: {e}"));
            vec![Key::char(' ')]
        });
        let mut keymaps = HashMap::new();
        let mut normal = Keymap::default();
        for (keys, cmd) in DEFAULT_NORMAL {
            let keys = parse_keys(keys, &leader).expect("valid default key");
            normal.bind_default(keys, cmd);
        }
        apply_user(
            &mut normal,
            &file.keymap.normal,
            &leader,
            "keymap.normal",
            &mut errors,
        );
        let mut insert = Keymap::default();
        let mut visual = Keymap::default();
        for (map, defaults) in [(&mut insert, DEFAULT_INSERT), (&mut visual, DEFAULT_VISUAL)] {
            for (keys, cmd) in defaults {
                let keys = parse_keys(keys, &leader).expect("valid default key");
                map.bind_default(keys, cmd);
            }
        }
        apply_user(
            &mut insert,
            &file.keymap.insert,
            &leader,
            "keymap.insert",
            &mut errors,
        );
        apply_user(
            &mut visual,
            &file.keymap.visual,
            &leader,
            "keymap.visual",
            &mut errors,
        );
        let mut tracks = Keymap::default();
        for (keys, cmd) in DEFAULT_TRACKS {
            let keys = parse_keys(keys, &leader).expect("valid default key");
            tracks.bind_default(keys, cmd);
        }
        apply_user(
            &mut tracks,
            &file.keymap.tracks,
            &leader,
            "keymap.tracks",
            &mut errors,
        );
        keymaps.insert(Mode::Tracks, tracks);
        let mut properties = Keymap::default();
        for (keys, cmd) in DEFAULT_PROPERTIES {
            let keys = parse_keys(keys, &leader).expect("valid default key");
            properties.bind_default(keys, cmd);
        }
        apply_user(
            &mut properties,
            &file.keymap.properties,
            &leader,
            "keymap.properties",
            &mut errors,
        );
        keymaps.insert(Mode::Properties, properties);
        let mut palette = Keymap::default();
        for (keys, cmd) in DEFAULT_PALETTE {
            let keys = parse_keys(keys, &leader).expect("valid default key");
            palette.bind_default(keys, cmd);
        }
        apply_user(
            &mut palette,
            &file.keymap.palette,
            &leader,
            "keymap.palette",
            &mut errors,
        );
        keymaps.insert(Mode::Palette, palette);
        let mut sync = Keymap::default();
        for (keys, cmd) in DEFAULT_SYNC {
            let keys = parse_keys(keys, &leader).expect("valid default key");
            sync.bind_default(keys, cmd);
        }
        apply_user(
            &mut sync,
            &file.keymap.sync,
            &leader,
            "keymap.sync",
            &mut errors,
        );
        keymaps.insert(Mode::Sync, sync);
        let mut design_keymap = Keymap::default();
        for (keys, cmd) in DEFAULT_DESIGN {
            let keys = parse_keys(keys, &leader).expect("valid default key");
            design_keymap.bind_default(keys, cmd);
        }
        apply_user(
            &mut design_keymap,
            &file.keymap.design,
            &leader,
            "keymap.design",
            &mut errors,
        );
        let mut insert_keymaps = HashMap::new();
        for (kind, defaults, user, name) in [
            (
                InsertKind::Tab,
                DEFAULT_INSERT_TAB,
                &file.keymap.insert_tab,
                "keymap.insert-tab",
            ),
            (
                InsertKind::Standard,
                DEFAULT_INSERT_STANDARD,
                &file.keymap.insert_standard,
                "keymap.insert-standard",
            ),
            (
                InsertKind::Slash,
                DEFAULT_INSERT_SLASH,
                &file.keymap.insert_slash,
                "keymap.insert-slash",
            ),
            (
                InsertKind::Chant,
                DEFAULT_INSERT_CHANT,
                &file.keymap.insert_chant,
                "keymap.insert-chant",
            ),
        ] {
            let mut map = Keymap::default();
            for (keys, cmd) in defaults {
                let keys = parse_keys(keys, &leader).expect("valid default key");
                map.bind_default(keys, cmd);
            }
            apply_user(&mut map, user, &leader, name, &mut errors);
            insert_keymaps.insert(kind, map);
        }
        let mut normal_keymaps = HashMap::new();
        for (kind, defaults, user, name) in [
            (
                InsertKind::Tab,
                DEFAULT_NORMAL_TAB,
                &file.keymap.normal_tab,
                "keymap.normal-tab",
            ),
            (
                InsertKind::Standard,
                DEFAULT_NORMAL_STANDARD,
                &file.keymap.normal_standard,
                "keymap.normal-standard",
            ),
            (
                InsertKind::Slash,
                DEFAULT_NORMAL_SLASH,
                &file.keymap.normal_slash,
                "keymap.normal-slash",
            ),
            (
                InsertKind::Chant,
                DEFAULT_NORMAL_CHANT,
                &file.keymap.normal_chant,
                "keymap.normal-chant",
            ),
        ] {
            let mut map = Keymap::default();
            for (keys, cmd) in defaults {
                let keys = parse_keys(keys, &leader).expect("valid default key");
                map.bind_default(keys, cmd);
            }
            apply_user(&mut map, user, &leader, name, &mut errors);
            normal_keymaps.insert(kind, map);
        }
        keymaps.insert(Mode::Insert, insert);
        keymaps.insert(Mode::Visual, visual);
        let mut command = Keymap::default();
        apply_user(
            &mut command,
            &file.keymap.command,
            &leader,
            "keymap.command",
            &mut errors,
        );
        errors.extend(crate::options::validate(&mut file));
        keymaps.insert(Mode::Normal, normal);
        keymaps.insert(Mode::Command, command);
        let (theme, theme_errors) = crate::theme::Theme::from_config(&file.theme);
        errors.extend(theme_errors);
        for item in &file.toolbar.items {
            if item.trim() != "|" {
                if let Err(e) = Command::parse(item) {
                    errors.push(format!("toolbar.items: {e}"));
                }
            }
        }
        (
            Self {
                file,
                keymaps,
                insert_keymaps,
                normal_keymaps,
                design_keymap,
                leader,
                theme,
                overrides: BTreeMap::new(),
            },
            errors,
        )
    }

    /// Parses TOML text. Hard errors (invalid TOML/unknown fields) return `Err`; invalid
    /// keymap entries are skipped and reported in the returned list.
    pub fn parse(
        text: &str,
        overrides: &[(String, String)],
    ) -> Result<(Self, Vec<String>), String> {
        Self::parse_from(text, overrides, None)
    }

    /// [`Config::parse`], prefixing errors in the TOML text (not in `overrides`) with `origin`.
    fn parse_from(
        text: &str,
        overrides: &[(String, String)],
        origin: Option<&Path>,
    ) -> Result<(Self, Vec<String>), String> {
        let mut file: ConfigFile = toml::from_str(text).map_err(|e| match origin {
            Some(p) => format!("{}: {e}", p.display()),
            None => e.to_string(),
        })?;
        // Checked on the parsed file: applying options rebuilds it and drops these fields.
        let mut deprecated = Vec::new();
        if file.ui.global_waveform.is_some() {
            deprecated.push(
                "ui.global_waveform: no longer used; list \"waveform\" in ui.global_tools and open it with :waveform".into(),
            );
        }
        if !file.keymap.play.is_empty() {
            deprecated.push(
                "keymap.play: there is no Play mode any more; bind these in [keymap.normal]".into(),
            );
        }
        let mut resolved = BTreeMap::new();
        for (name, value) in overrides {
            let v = crate::options::set(&mut file, name, Some(value))?;
            resolved.insert(name.clone(), v);
        }
        let (mut config, errors) = Self::from_file(file);
        config.overrides = resolved;
        deprecated.extend(errors);
        Ok((config, deprecated))
    }

    /// Loads the config file at `path` (defaults when it doesn't exist), then applies the
    /// `overrides` (raw name/value pairs) on top.
    pub fn load(
        path: &Path,
        overrides: &[(String, String)],
    ) -> Result<(Self, Vec<String>), String> {
        match std::fs::read_to_string(path) {
            Ok(text) => Self::parse_from(&text, overrides, Some(path)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Self::parse("", overrides),
            Err(e) => Err(format!("{}: {e}", path.display())),
        }
    }

    /// The focus ring's color: `ui.focus_ring_color` when set, else `fallback` (the mode's color,
    /// see [`crate::App::focus_ring_color`]).
    pub fn focus_ring_color(&self, fallback: crate::theme::Rgba) -> crate::theme::Rgba {
        crate::theme::Rgba::parse(&self.file.ui.focus_ring_color).unwrap_or(fallback)
    }

    pub fn keymap(&self, mode: Mode) -> &Keymap {
        &self.keymaps[&mode]
    }

    /// Clef of a standard staff added without one (`score.clef`).
    pub fn default_clef(&self) -> cecilia_core::Clef {
        match self.file.score.clef.as_str() {
            "bass" => cecilia_core::Clef::Bass,
            "grand" => cecilia_core::Clef::Grand {
                split: cecilia_core::GRAND_SPLIT,
            },
            _ => cecilia_core::Clef::Treble,
        }
    }

    pub fn insert_keymap(&self, kind: InsertKind) -> &Keymap {
        &self.insert_keymaps[&kind]
    }

    pub fn normal_keymap(&self, kind: InsertKind) -> &Keymap {
        &self.normal_keymaps[&kind]
    }

    /// Looks keys up in Normal mode: the keys of the notation under the cursor first, then
    /// the shared ones.
    pub fn normal_lookup(&self, kind: InsertKind, keys: &[Key]) -> Lookup<'_> {
        match self.normal_keymap(kind).lookup(keys) {
            Lookup::None => self.keymap(Mode::Normal).lookup(keys),
            found => found,
        }
    }
}

fn apply_user(
    map: &mut Keymap,
    user: &HashMap<String, Binding>,
    leader: &[Key],
    section: &str,
    errors: &mut Vec<String>,
) {
    for (keys, binding) in user {
        let keys = match parse_keys(keys, leader) {
            Ok(k) => k,
            Err(e) => {
                errors.push(format!("{section}: {e}"));
                continue;
            }
        };
        let names = match binding {
            Binding::One(s) => vec![s.clone()],
            Binding::Many(v) => v.clone(),
        };
        let cmds: Result<Vec<_>, _> = names.iter().map(|n| Command::parse(n)).collect();
        match cmds {
            Ok(cmds) if cmds.iter().all(|c| *c == Command::Nop) => {
                map.bindings.remove(&keys);
                map.names.remove(&keys);
            }
            Ok(cmds) => {
                map.names.insert(keys.clone(), names);
                map.bindings.insert(keys, cmds);
            }
            Err(e) => errors.push(format!("{section}: {e}")),
        }
    }
}

/// Largest `ui.focus_ring_width`.
pub const FOCUS_RING_WIDTH_MAX: f64 = 8.0;

/// Clamps a numeric setting to `lo..=hi`; NaN gives `default`.
pub fn clamp_setting(v: f64, lo: f64, hi: f64, default: f64) -> f64 {
    if v.is_nan() { default } else { v.clamp(lo, hi) }
}

/// Default config file location (`$XDG_CONFIG_HOME/cecilia/config.toml` on Linux).
pub fn default_config_path() -> Option<PathBuf> {
    directories::ProjectDirs::from("", "", "cecilia").map(|d| d.config_dir().join("config.toml"))
}

/// Looks for a SoundFont in conventional system locations.
pub fn find_system_soundfont() -> Option<PathBuf> {
    let mut dirs: Vec<PathBuf> = Vec::new();
    if let Some(d) = directories::ProjectDirs::from("", "", "cecilia") {
        dirs.push(d.data_dir().join("soundfonts"));
    }
    dirs.extend(
        [
            "/usr/share/soundfonts",
            "/usr/share/sounds/sf2",
            "/usr/share/sounds/sf3",
        ]
        .into_iter()
        .map(PathBuf::from),
    );
    for preferred in ["default.sf2", "FluidR3_GM.sf2", "GeneralUser-GS.sf2"] {
        for d in &dirs {
            let p = d.join(preferred);
            if p.is_file() {
                return Some(p);
            }
        }
    }
    dirs.iter()
        .filter_map(|d| std::fs::read_dir(d).ok())
        .flatten()
        .flatten()
        .map(|e| e.path())
        .find(|p| p.extension().is_some_and(|e| e.eq_ignore_ascii_case("sf2")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn user_bindings_override_and_unbind() {
        let (cfg, errors) = Config::parse(
            r#"
            [general]
            leader = ","
            [keymap.normal]
            "<leader>p" = "play-toggle"
            "x" = ["bar-next", "bar-next"]
            "h" = "nop"
            "q" = "not-a-command"
            "#,
            &[],
        )
        .unwrap();
        assert_eq!(errors.len(), 1);
        let n = cfg.keymap(Mode::Normal);
        assert!(matches!(
            n.lookup(&[Key::char(','), Key::char('p')]),
            Lookup::Exact([Command::PlayToggle])
        ));
        assert!(matches!(n.lookup(&[Key::char('x')]), Lookup::Exact(c) if c.len() == 2));
        assert!(matches!(n.lookup(&[Key::char('h')]), Lookup::None));
        // Default leader bindings moved to the new leader.
        assert!(matches!(n.lookup(&[Key::char(',')]), Lookup::Prefix));
    }

    #[test]
    fn insert_keymaps_per_notation() {
        let (cfg, errors) = Config::parse(
            r#"
            [keymap.insert-standard]
            "a" = "accidental sharp"
            "#,
            &[],
        )
        .unwrap();
        assert!(errors.is_empty(), "{errors:?}");
        let a = [Key::char('a')];
        assert!(matches!(
            cfg.insert_keymap(InsertKind::Standard).lookup(&a),
            Lookup::Exact([Command::Accidental(1)])
        ));
        // Other notations keep their own defaults for the same key.
        assert!(matches!(
            cfg.insert_keymap(InsertKind::Chant)
                .lookup(&[Key::char('v')]),
            Lookup::Exact([Command::ChantMod(_)])
        ));
        assert!(matches!(
            cfg.insert_keymap(InsertKind::Standard)
                .lookup(&[Key::char('c')]),
            Lookup::Exact([Command::NoteLetter('c')])
        ));
    }

    #[test]
    fn invalid_toml_is_an_error() {
        assert!(Config::parse("[general]\nleader = ", &[]).is_err());
        assert!(Config::parse("[nope]\n", &[]).is_err());
    }

    #[test]
    fn the_waveform_rulers_are_settings() {
        let c = Config::default();
        assert!(c.file.ui.waveform_time_ticks.shows(false));
        let (c, errors) = Config::parse(
            "[ui]\nwaveform_time_ticks = \"major\"\nwaveform_beat_ticks = \"off\"\nwaveform_time_labels = false\n",
            &[],
        )
        .unwrap();
        assert!(errors.is_empty());
        assert!(!c.file.ui.waveform_time_ticks.shows(false));
        assert!(c.file.ui.waveform_time_ticks.shows(true));
        assert!(!c.file.ui.waveform_beat_ticks.shows(true));
        assert!(!c.file.ui.waveform_time_labels && c.file.ui.waveform_beat_labels);
    }

    #[test]
    fn the_chant_text_font_is_noto_serif_or_source_sans() {
        let (c, errors) = Config::parse("", &[]).unwrap();
        assert!(errors.is_empty());
        assert_eq!(c.file.ui.chant_text_font, "noto-serif");
        let (_, errors) = Config::parse("[ui]\nchant_text_font = \"source-sans\"\n", &[]).unwrap();
        assert!(errors.is_empty(), "{errors:?}");
        let (_, errors) = Config::parse("[ui]\nchant_text_font = \"comic\"\n", &[]).unwrap();
        assert_eq!(errors.len(), 1, "{errors:?}");
    }

    #[test]
    fn the_example_config_loads_cleanly() {
        let (_, errors) =
            Config::parse(include_str!("../../../docs/config.example.toml"), &[]).unwrap();
        assert!(errors.is_empty(), "{errors:?}");
    }

    #[test]
    fn an_old_play_keymap_still_loads_with_a_warning() {
        let (_, errors) = Config::parse("[keymap.play]\n\"p\" = \"play-toggle\"\n", &[]).unwrap();
        assert!(errors[0].contains("keymap.play"), "{errors:?}");
    }

    fn pairs(v: &[(&str, &str)]) -> Vec<(String, String)> {
        v.iter()
            .map(|(a, b)| (a.to_string(), b.to_string()))
            .collect()
    }

    #[test]
    fn invalid_option_in_file_warns_and_defaults() {
        let (c, errors) =
            Config::parse("[ui]\nbar_numbers = \"x\"\ntab_width = 99\n", &[]).unwrap();
        assert_eq!(c.file.ui.bar_numbers, "system");
        assert_eq!(c.file.ui.tab_width, 8);
        assert!(
            errors.iter().any(|e| e.starts_with("ui.bar_numbers:")),
            "{errors:?}"
        );
        assert!(
            errors.iter().any(|e| e.starts_with("ui.tab_width:")),
            "{errors:?}"
        );
    }

    #[test]
    fn override_errors_carry_no_path() {
        let dir = std::env::temp_dir().join(format!("cecilia-ovr-err-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("config.toml");
        std::fs::write(&path, "").unwrap();
        let e = Config::load(&path, &[("nope.nothing".into(), "1".into())])
            .err()
            .unwrap();
        assert!(!e.contains("config.toml"), "{e}");
        std::fs::write(&path, "[ui\n").unwrap();
        let e = Config::load(&path, &[]).err().unwrap();
        assert!(e.starts_with(&path.display().to_string()), "{e}");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn overrides_beat_the_file() {
        let (c, errors) = Config::parse(
            "[tabline]\nwrap = true\n",
            &pairs(&[("tabline.wrap", "false")]),
        )
        .unwrap();
        assert!(errors.is_empty(), "{errors:?}");
        assert!(!c.file.tabline.wrap);
        assert_eq!(
            c.overrides.get("tabline.wrap").map(String::as_str),
            Some("false")
        );
    }

    #[test]
    fn bad_override_is_an_error() {
        let e = Config::parse("", &pairs(&[("tabline.wrap", "maybe")]))
            .err()
            .unwrap();
        assert!(e.contains("tabline.wrap"), "{e}");
        assert!(Config::parse("", &pairs(&[("nope.nothing", "1")])).is_err());
    }

    #[test]
    fn deprecated_settings_warn_even_with_overrides() {
        let (_, errors) = Config::parse(
            "[ui]\nglobal_waveform = true\n",
            &pairs(&[("tabline.wrap", "false")]),
        )
        .unwrap();
        assert!(
            errors.iter().any(|e| e.starts_with("ui.global_waveform")),
            "{errors:?}"
        );
    }
}
