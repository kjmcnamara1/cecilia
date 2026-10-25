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
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ConfigFile {
    pub general: GeneralConfig,
    pub ui: UiConfig,
    pub audio: AudioConfig,
    pub keymap: KeymapConfig,
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
}

impl Default for AudioConfig {
    fn default() -> Self {
        Self {
            soundfont: None,
            speed: 1.0,
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
    pub visual: HashMap<String, Binding>,
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
    ("<C-w>s", "split"),
    ("<C-w>v", "vsplit"),
    ("<C-w>h", "focus-left"),
    ("<C-w>j", "focus-down"),
    ("<C-w>k", "focus-up"),
    ("<C-w>l", "focus-right"),
    ("<C-w>q", "close-pane"),
    ("<C-w>c", "close-pane"),
    ("gt", "tab-next"),
    ("gT", "tab-prev"),
    (":", "command-mode"),
    ("ZQ", "quit!"),
    ("ZZ", "wq"),
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
#[derive(Debug, Clone)]
pub struct Config {
    pub file: ConfigFile,
    pub keymaps: HashMap<Mode, Keymap>,
    pub leader: Vec<Key>,
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
            normal.bindings.insert(
                keys,
                vec![Command::parse(cmd).expect("valid default command")],
            );
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
                map.bindings.insert(
                    keys,
                    vec![Command::parse(cmd).expect("valid default command")],
                );
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
        keymaps.insert(Mode::Normal, normal);
        keymaps.insert(Mode::Command, command);
        (
            Self {
                file,
                keymaps,
                leader,
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
            }
            Ok(cmds) => {
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
    fn invalid_toml_is_an_error() {
        assert!(Config::parse("[general]\nleader = ").is_err());
        assert!(Config::parse("[nope]\n").is_err());
    }
}
