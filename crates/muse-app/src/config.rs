//! TOML configuration and keymaps.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde::Deserialize;

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
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ConfigFile {
    pub general: GeneralConfig,
    pub ui: UiConfig,
    pub tabline: TablineConfig,
    pub audio: AudioConfig,
    pub theme: crate::theme::ThemeConfig,
    pub toolbar: ToolbarConfig,
    pub statusline: StatuslineConfig,
    pub score: ScoreConfig,
    pub keymap: KeymapConfig,
}

/// Score defaults (`[score]`).
#[derive(Debug, Clone, Deserialize)]
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
#[derive(Debug, Clone, Deserialize)]
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
#[derive(Debug, Clone, Deserialize)]
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
#[derive(Debug, Clone, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct TablineConfig {
    /// What a new view starts with: "copy" (the current view's buffers), "current" (the
    /// current buffer only) or "empty".
    pub new_view: String,
    /// File-type icons in the terminal UI: "nerd" (Nerd Font glyphs), "unicode" or "none".
    pub icons: String,
    /// Hide the views on the right of the tabline while there is only one.
    pub hide_single_view: bool,
    pub truncation: TruncationConfig,
}

impl Default for TablineConfig {
    fn default() -> Self {
        Self {
            new_view: "copy".into(),
            icons: "nerd".into(),
            hide_single_view: true,
            truncation: TruncationConfig::default(),
        }
    }
}

/// Long file names in tabs keep their first `prefix_width` and last `suffix_width`
/// characters around an ellipsis, to at most `max_width` characters.
#[derive(Debug, Clone, Deserialize)]
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

#[derive(Debug, Clone, Deserialize)]
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

#[derive(Debug, Clone, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct UiConfig {
    /// TUI: characters per sixteenth note in the tab view.
    pub tab_width: u8,
    /// Scroll to keep the playhead in view while playing.
    pub follow_playback: bool,
    /// Command-line Tab completion ignores letter case.
    pub completion_ignore_case: bool,
    /// Command-line completion matches the typed letters in order anywhere in a candidate.
    pub completion_fuzzy: bool,
    /// No longer used (`global_tools` lists the audio pane instead); kept so old configs
    /// still load, with a warning.
    #[serde(skip_serializing)]
    pub global_waveform: Option<bool>,
    /// A progress bar along the bottom of every score pane: sections and the play position.
    pub progress_bar: bool,
    /// The tracks panel (tracks with mixer and a bar overview) at the bottom of the window.
    pub tracks_panel: bool,
    /// The properties pane (score, tracks and lyrics) along the right of the window.
    pub properties: bool,
    /// Tool panes that dock along the bottom of the whole view, at its full width, and show
    /// the last active score pane ("tracks", "waveform", "fretboard", "piano", "tuner"); the
    /// others dock under the score pane they open from, as wide as it, and always show it.
    pub global_tools: Vec<String>,
    /// Seconds the tracks show along the bottom after a key or command changes the track,
    /// while no tracks pane is shown (0: off).
    pub tracks_peek: f64,
    /// Tracks panes follow the height their score needs, until resized by hand.
    pub auto_resize: bool,
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
}

impl Default for UiConfig {
    fn default() -> Self {
        Self {
            tab_width: 2,
            follow_playback: true,
            completion_ignore_case: true,
            completion_fuzzy: true,
            global_waveform: None,
            progress_bar: true,
            tracks_panel: false,
            properties: true,
            global_tools: ["tracks", "waveform", "fretboard", "piano", "tuner"]
                .map(String::from)
                .to_vec(),
            tracks_peek: 1.0,
            auto_resize: true,
            toolbar: true,
            fretboard: false,
            piano: false,
            hide_cursor_while_following: true,
            layout: "vertical".into(),
            zoom: 1.0,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
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
#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum Binding {
    One(String),
    Many(Vec<String>),
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct KeymapConfig {
    pub normal: HashMap<String, Binding>,
    pub command: HashMap<String, Binding>,
    pub insert: HashMap<String, Binding>,
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
    /// No longer used (Play mode was removed); kept so old configs still load, with a warning.
    pub play: HashMap<String, Binding>,
}

const DEFAULT_NORMAL: &[(&str, &str)] = &[
    ("h", "beat-prev"),
    ("<Left>", "beat-prev"),
    ("l", "beat-next"),
    ("<Right>", "beat-next"),
    ("j", "string-down"),
    ("<Down>", "string-down"),
    ("k", "string-up"),
    ("<Up>", "string-up"),
    ("w", "note-next"),
    ("b", "note-prev"),
    ("}", "bar-next"),
    ("{", "bar-prev"),
    ("0", "bar-start"),
    ("<Home>", "bar-start"),
    ("$", "bar-end"),
    ("<End>", "bar-end"),
    ("gg", "goto-first"),
    ("G", "goto-last"),
    ("J", "track-next"),
    ("K", "track-prev"),
    ("<C-d>", "page-down"),
    ("<PageDown>", "page-down"),
    ("<C-u>", "page-up"),
    ("<PageUp>", "page-up"),
    ("<leader><leader>", "play-toggle"),
    ("<leader>s", "stop"),
    ("<leader>f", "follow-toggle"),
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
    ("<C-w>v", "vsplit"),
    ("<C-w>h", "focus-left"),
    ("<C-w>j", "focus-down"),
    ("<C-w>k", "focus-up"),
    ("<C-w>l", "focus-right"),
    ("<C-w>q", "pane-close"),
    ("<C-w>c", "pane-close"),
    ("gt", "view-next"),
    ("gT", "view-prev"),
    ("L", "buffer-next"),
    ("H", "buffer-prev"),
    ("<leader>x", "buffer-close"),
    ("<leader>b", "pick-buffer"),
    ("m", "set-mark"),
    ("'", "jump-mark-bar"),
    ("`", "jump-mark"),
    ("<leader>V", "pick-view"),
    ("<leader>W", "pick-workspace"),
    (":", "command-mode"),
    ("ZQ", "quit!"),
    ("ZZ", "write-quit"),
    // Editing
    ("i", "insert"),
    ("a", "append"),
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
    ("o", "bar-insert-after"),
    ("O", "bar-insert-before"),
    ("+", "duration-longer"),
    ("-", "duration-shorter"),
    ("<C-a>", "pitch-up"),
    ("<C-x>", "pitch-down"),
    ("r", "rest"),
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
    ("t.", "dot"),
    ("t3", "tuplet 3"),
    ("<F1>", "help"),
    ("<leader>v", "layout"),
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

const DEFAULT_INSERT: &[(&str, &str)] = &[
    ("<Esc>", "normal"),
    ("<Left>", "beat-prev"),
    ("<Right>", "insert-next"),
    ("<Space>", "insert-next"),
    ("<Up>", "string-up"),
    ("<Down>", "string-down"),
    ("<BS>", "delete-note"),
    ("<Del>", "delete-note"),
    ("<CR>", "beat-new"),
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
    ("h", "toggle hammer"),
    ("b", "toggle bend"),
    ("s", "toggle slide"),
    ("~", "toggle vibrato"),
    ("x", "toggle dead"),
    ("m", "toggle palm-mute"),
    ("l", "toggle let-ring"),
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
    ("h", "toggle hammer"),
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
    ("a", "chant-note a"),
    ("b", "chant-note b"),
    ("c", "chant-note c"),
    ("d", "chant-note d"),
    ("e", "chant-note e"),
    ("f", "chant-note f"),
    ("g", "chant-note g"),
    ("h", "chant-note h"),
    ("i", "chant-note i"),
    ("j", "chant-note j"),
    ("k", "chant-note k"),
    ("l", "chant-note l"),
    ("m", "chant-note m"),
    ("A", "chant-note A"),
    ("B", "chant-note B"),
    ("C", "chant-note C"),
    ("D", "chant-note D"),
    ("E", "chant-note E"),
    ("F", "chant-note F"),
    ("G", "chant-note G"),
    ("H", "chant-note H"),
    ("I", "chant-note I"),
    ("J", "chant-note J"),
    ("K", "chant-note K"),
    ("L", "chant-note L"),
    ("M", "chant-note M"),
    ("v", "chant-mod v"),
    ("s", "chant-mod s"),
    ("o", "chant-mod o"),
    ("q", "chant-mod q"),
    ("w", "chant-mod w"),
    ("~", "chant-mod ~"),
    (".", "chant-mod ."),
    ("_", "chant-mod _"),
    ("'", "chant-mod '"),
    ("-", "chant-mod -"),
    (",", "chant-bar ,"),
    (";", "chant-bar ;"),
    (":", "chant-bar :"),
    ("<Space>", "chant-append"),
    ("<Right>", "beat-next"),
    ("/", "chant-break"),
    ("<CR>", "chant-syllable"),
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
    ("<leader>f", "follow-toggle"),
    ("<leader>T", "tracks"),
    ("<leader>?", "pick-keymap"),
    ("<C-q>", "quit"),
    (":", "command-mode"),
];

const DEFAULT_PROPERTIES: &[(&str, &str)] = &[
    ("j", "properties-down"),
    ("<Down>", "properties-down"),
    ("k", "properties-up"),
    ("<Up>", "properties-up"),
    ("l", "properties-tab"),
    ("<Right>", "properties-tab"),
    ("<Tab>", "properties-tab"),
    ("h", "properties-tab-prev"),
    ("<Left>", "properties-tab-prev"),
    ("<S-Tab>", "properties-tab-prev"),
    ("1", "properties-tab score"),
    ("2", "properties-tab track"),
    ("3", "properties-tab lyrics"),
    ("<CR>", "properties-edit"),
    ("i", "properties-edit"),
    ("a", "properties-edit"),
    ("za", "properties-fold"),
    ("zc", "properties-fold close"),
    ("zo", "properties-fold open"),
    ("x", "properties-clear"),
    ("n", "track-next"),
    ("p", "track-prev"),
    ("<Esc>", "properties-focus"),
    ("q", "properties-focus"),
    ("<leader><leader>", "play-toggle"),
    ("<leader>s", "stop"),
    ("<leader>f", "follow-toggle"),
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
    pub leader: Vec<Key>,
    /// Interface colors (`[theme]`).
    pub theme: crate::theme::Theme,
}

impl Default for Config {
    fn default() -> Self {
        Self::from_file(ConfigFile::default()).0
    }
}

impl Config {
    /// Builds a config, collecting (not failing on) invalid keymap entries.
    pub fn from_file(file: ConfigFile) -> (Self, Vec<String>) {
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
        if file.ui.global_waveform.is_some() {
            errors.push(
                "ui.global_waveform: no longer used; list \"waveform\" in ui.global_tools and open it with :waveform".into(),
            );
        }
        if !file.keymap.play.is_empty() {
            errors.push(
                "keymap.play: there is no Play mode any more; bind these in [keymap.normal]".into(),
            );
        }
        keymaps.insert(Mode::Normal, normal);
        keymaps.insert(Mode::Command, command);
        let (theme, theme_errors) = crate::theme::Theme::from_config(&file.theme);
        errors.extend(theme_errors);
        if !["treble", "bass", "grand"].contains(&file.score.clef.as_str()) {
            errors.push(format!(
                "score.clef: '{}' is not treble, bass or grand",
                file.score.clef
            ));
        }
        if !["arrow", "round", "none"].contains(&file.statusline.separators.as_str()) {
            errors.push(format!(
                "statusline.separators: '{}' is not arrow, round or none",
                file.statusline.separators
            ));
        }
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
                leader,
                theme,
            },
            errors,
        )
    }

    /// Parses TOML text. Hard errors (invalid TOML/unknown fields) return `Err`; invalid
    /// keymap entries are skipped and reported in the returned list.
    pub fn parse(text: &str) -> Result<(Self, Vec<String>), String> {
        let file: ConfigFile = toml::from_str(text).map_err(|e| e.to_string())?;
        Ok(Self::from_file(file))
    }

    pub fn load(path: &Path) -> Result<(Self, Vec<String>), String> {
        match std::fs::read_to_string(path) {
            Ok(text) => Self::parse(&text).map_err(|e| format!("{}: {e}", path.display())),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok((Self::default(), vec![])),
            Err(e) => Err(format!("{}: {e}", path.display())),
        }
    }

    pub fn keymap(&self, mode: Mode) -> &Keymap {
        &self.keymaps[&mode]
    }

    /// Clef of a standard staff added without one (`score.clef`).
    pub fn default_clef(&self) -> muse_core::Clef {
        match self.file.score.clef.as_str() {
            "bass" => muse_core::Clef::Bass,
            "grand" => muse_core::Clef::Grand {
                split: muse_core::GRAND_SPLIT,
            },
            _ => muse_core::Clef::Treble,
        }
    }

    pub fn insert_keymap(&self, kind: InsertKind) -> &Keymap {
        &self.insert_keymaps[&kind]
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

/// Default config file location (`$XDG_CONFIG_HOME/muse/config.toml` on Linux).
pub fn default_config_path() -> Option<PathBuf> {
    directories::ProjectDirs::from("", "", "muse").map(|d| d.config_dir().join("config.toml"))
}

/// Looks for a SoundFont in conventional system locations.
pub fn find_system_soundfont() -> Option<PathBuf> {
    let mut dirs: Vec<PathBuf> = Vec::new();
    if let Some(d) = directories::ProjectDirs::from("", "", "muse") {
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
            cfg.insert_keymap(InsertKind::Chant).lookup(&a),
            Lookup::Exact([Command::ChantNote('a')])
        ));
        assert!(matches!(
            cfg.insert_keymap(InsertKind::Standard)
                .lookup(&[Key::char('c')]),
            Lookup::Exact([Command::NoteLetter('c')])
        ));
    }

    #[test]
    fn invalid_toml_is_an_error() {
        assert!(Config::parse("[general]\nleader = ").is_err());
        assert!(Config::parse("[nope]\n").is_err());
    }

    #[test]
    fn the_example_config_loads_cleanly() {
        let (_, errors) = Config::parse(include_str!("../../../docs/config.example.toml")).unwrap();
        assert!(errors.is_empty(), "{errors:?}");
    }

    #[test]
    fn an_old_play_keymap_still_loads_with_a_warning() {
        let (_, errors) = Config::parse("[keymap.play]\n\"p\" = \"play-toggle\"\n").unwrap();
        assert!(errors[0].contains("keymap.play"), "{errors:?}");
    }
}
