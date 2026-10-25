//! Tab completion for the command line: command names, then each command's arguments (file
//! paths, layouts, mixer modes, tunings, techniques, ...).

use std::path::Path;

/// Every command name the parser accepts, long names first where there are aliases.
pub const COMMANDS: &[&str] = &[
    "append",
    "audio",
    "bar",
    "bar-delete",
    "bar-end",
    "bar-insert-after",
    "bar-insert-before",
    "bar-next",
    "bar-prev",
    "bar-start",
    "beat-new",
    "beat-next",
    "beat-prev",
    "capo",
    "change",
    "chant-to-modern",
    "chant-view",
    "clef",
    "close",
    "close-pane",
    "command-mode",
    "count-in",
    "delete",
    "delete-note",
    "dot",
    "duration",
    "duration-longer",
    "duration-shorter",
    "e",
    "earlier",
    "edit",
    "enew",
    "focus-down",
    "focus-left",
    "focus-right",
    "focus-up",
    "follow-toggle",
    "gabc",
    "goto-bar",
    "goto-first",
    "goto-last",
    "help",
    "insert",
    "insert-next",
    "interpretation",
    "key",
    "later",
    "layout",
    "loop",
    "loop-toggle",
    "lyrics",
    "lyrics-clear",
    "metronome",
    "mix",
    "modern-to-chant",
    "mute",
    "new",
    "nop",
    "normal",
    "note",
    "note-next",
    "note-prev",
    "octave-down",
    "octave-up",
    "page-down",
    "page-up",
    "pause",
    "pitch",
    "pitch-down",
    "pitch-up",
    "play",
    "play-macro",
    "play-toggle",
    "put",
    "put-before",
    "q",
    "q!",
    "qa",
    "qa!",
    "quit",
    "quit!",
    "quitall",
    "quitall!",
    "record-macro",
    "redo",
    "refinger",
    "register",
    "repeat",
    "repeat-end",
    "repeat-start",
    "rest",
    "section",
    "solo",
    "source",
    "speed",
    "speed-down",
    "speed-up",
    "split",
    "stop",
    "string-down",
    "string-up",
    "sync",
    "sync-clear",
    "sync-detect",
    "sync-tap",
    "tab-next",
    "tab-prev",
    "tabclose",
    "tabedit",
    "tabnew",
    "tempo",
    "text",
    "time",
    "toggle",
    "track",
    "track-add",
    "track-delete",
    "track-next",
    "track-prev",
    "trainer",
    "transpose",
    "tuning",
    "tuplet",
    "undo",
    "undo-newer",
    "undo-older",
    "visual",
    "visual-bar",
    "voice",
    "volume",
    "vsplit",
    "w",
    "waveform",
    "wq",
    "write",
    "write-quit",
    "yank",
    "zoom-in",
    "zoom-out",
    "zoom-reset",
];

/// File extensions muse opens.
const SCORE_EXTENSIONS: &[&str] = &[
    "gp3", "gp4", "gp5", "gpx", "gp", "mid", "midi", "musicxml", "xml", "mxl", "gabc", "muse",
];
/// Extensions `:w` writes (scores, engravings, audio).
const WRITE_EXTENSIONS: &[&str] = &[
    "gp5", "gpx", "gp", "mid", "musicxml", "mxl", "gabc", "muse", "pdf", "svg", "png", "wav",
    "flac",
];

/// Fixed argument values per command.
fn values(command: &str) -> Option<&'static [&'static str]> {
    Some(match command {
        "layout" => &["horizontal", "page", "vertical"],
        "mix" => &["audio", "both", "midi"],
        "interpretation" => &["equalist", "solesmes"],
        "clef" => &[
            "c1", "c2", "c3", "c4", "cb2", "cb3", "cb4", "f1", "f2", "f3", "f4",
        ],
        "track-add" => &["bass", "drums", "guitar", "piano", "voice"],
        "volume" => &["audio", "metronome", "midi"],
        "duration" => &["1", "2", "4", "8", "16", "32", "64"],
        "tuning" => &[
            "7-string", "bass", "bass5", "d", "dadgad", "drop-d", "eb", "open-d", "open-e",
            "open-g", "standard", "ukulele",
        ],
        "toggle" => &[
            "accent",
            "bend",
            "dead",
            "fade-in",
            "ghost",
            "half-bend",
            "hammer",
            "harmonic",
            "heavy-accent",
            "let-ring",
            "palm-mute",
            "pinch-harmonic",
            "pop",
            "slap",
            "slide",
            "staccato",
            "stroke-down",
            "stroke-up",
            "tap",
            "tie",
            "tremolo",
            "vibrato",
            "vibrato-wide",
        ],
        _ => return None,
    })
}

/// What the completed part of the line is.
#[derive(Debug, Clone, PartialEq)]
pub struct Completions {
    /// Byte offset in the line where the completed word starts.
    pub start: usize,
    pub items: Vec<String>,
    /// The candidates are command names (a space follows a unique match).
    pub commands: bool,
}

/// Candidates for the word being typed at the end of `line`. `score_dir` is the folder of the
/// current score (for `:audio`).
pub fn complete(line: &str, cwd: &Path, score_dir: Option<&Path>) -> Completions {
    let lead = line.len() - line.trim_start_matches([':', ' ']).len();
    let body = &line[lead..];
    let Some(space) = body.find(char::is_whitespace) else {
        let mut items: Vec<String> = COMMANDS
            .iter()
            .filter(|c| c.starts_with(body))
            .map(|c| c.to_string())
            .collect();
        items.sort();
        return Completions {
            start: lead,
            items,
            commands: true,
        };
    };
    let command = &body[..space];
    let rest = &body[space..];
    let arg_start = lead + space + (rest.len() - rest.trim_start().len());
    let arg = &line[arg_start..];
    let items = match command {
        "e" | "edit" | "tabnew" | "tabe" | "tabedit" => paths(arg, cwd, Some(SCORE_EXTENSIONS)),
        "w" | "write" => paths(arg, cwd, Some(WRITE_EXTENSIONS)),
        "audio" => {
            let mut v = paths(
                arg,
                score_dir.unwrap_or(cwd),
                Some(crate::playalong::AUDIO_EXTENSIONS),
            );
            if "off".starts_with(arg) {
                v.insert(0, "off".into());
            }
            v
        }
        _ => {
            // Complete the last word of the arguments (e.g. `:volume mi`).
            let word_start = arg.rfind(' ').map_or(0, |i| i + 1);
            let word = &arg[word_start..];
            let list = if word_start == 0 {
                values(command)
            } else {
                None
            };
            let items = list
                .unwrap_or_default()
                .iter()
                .filter(|v| v.starts_with(word))
                .map(|v| v.to_string())
                .collect();
            return Completions {
                start: arg_start + word_start,
                items,
                commands: false,
            };
        }
    };
    Completions {
        start: arg_start,
        items,
        commands: false,
    }
}

/// Files and folders matching a typed path (relative to `base`); folders end with `/`.
fn paths(typed: &str, base: &Path, extensions: Option<&[&str]>) -> Vec<String> {
    let (dir_part, prefix) = match typed.rfind('/') {
        Some(i) => (&typed[..=i], &typed[i + 1..]),
        None => ("", typed),
    };
    let dir = if let Some(rest) = dir_part.strip_prefix("~/") {
        std::env::var_os("HOME")
            .map(|h| Path::new(&h).join(rest))
            .unwrap_or_default()
    } else if Path::new(dir_part).is_absolute() {
        dir_part.into()
    } else {
        base.join(dir_part)
    };
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return Vec::new();
    };
    let mut out: Vec<String> = entries
        .flatten()
        .filter_map(|e| {
            let name = e.file_name().to_string_lossy().to_string();
            if !name.starts_with(prefix) || (name.starts_with('.') && !prefix.starts_with('.')) {
                return None;
            }
            let is_dir = e.path().is_dir();
            if !is_dir {
                let ext = Path::new(&name)
                    .extension()
                    .and_then(|x| x.to_str())
                    .map(str::to_ascii_lowercase)
                    .unwrap_or_default();
                if extensions.is_some_and(|list| !list.contains(&ext.as_str())) {
                    return None;
                }
            }
            Some(format!("{dir_part}{name}{}", if is_dir { "/" } else { "" }))
        })
        .collect();
    // Folders first, then files, alphabetically.
    out.sort_by_key(|p| (!p.ends_with('/'), p.to_lowercase()));
    out
}

/// Longest common prefix of the candidates.
pub fn common_prefix(items: &[String]) -> String {
    let Some(first) = items.first() else {
        return String::new();
    };
    let mut end = first.len();
    for item in &items[1..] {
        end = end.min(
            first
                .char_indices()
                .zip(item.chars())
                .find(|((_, a), b)| a != b)
                .map_or(end.min(item.len()), |((i, _), _)| i),
        );
    }
    first[..end].to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Command;

    #[test]
    fn every_listed_command_parses() {
        for name in COMMANDS {
            let err = Command::parse(name).err().unwrap_or_default();
            assert!(!err.starts_with("unknown command"), "{name}: {err}");
        }
    }

    #[test]
    fn completes_names_values_and_paths() {
        let dir = std::env::temp_dir().join(format!("muse-complete-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("songs")).unwrap();
        for f in ["a.gp5", "b.txt", "songs/c.gabc"] {
            std::fs::write(dir.join(f), b"").unwrap();
        }
        let c = complete(":sync-", &dir, None);
        assert_eq!(c.items, ["sync-clear", "sync-detect", "sync-tap"]);
        assert_eq!((c.start, c.commands), (1, true));
        let c = complete(":layout p", &dir, None);
        assert_eq!(
            (c.start, c.items.as_slice()),
            (8, &["page".to_string()][..])
        );
        let c = complete(":e ", &dir, None);
        assert_eq!(c.items, ["songs/", "a.gp5"]);
        assert_eq!(complete(":e songs/", &dir, None).items, ["songs/c.gabc"]);
        assert_eq!(complete(":w a", &dir, None).items, ["a.gp5"]);
        assert_eq!(
            common_prefix(&["sync-clear".into(), "sync-detect".into()]),
            "sync-"
        );
        std::fs::remove_dir_all(dir).ok();
    }
}

#[cfg(test)]
mod key_tests {
    use crate::keys::{Key, parse_keys};
    use crate::{App, Config};

    fn keys(app: &mut App, s: &str) {
        for k in parse_keys(s, &[Key::char(' ')]).unwrap() {
            app.handle_key(k);
        }
    }

    #[test]
    fn tab_completes_and_cycles() {
        let mut app = App::new(Config::default(), None);
        keys(&mut app, ":lay<Tab>");
        assert_eq!(app.cmdline, "layout ");
        keys(&mut app, "<Tab>");
        assert_eq!(app.cmdline, "layout horizontal");
        let (items, sel) = app.wildmenu().unwrap();
        assert_eq!((items.len(), sel), (3, Some(0)));
        keys(&mut app, "<Tab><Tab><S-Tab>");
        assert_eq!(app.cmdline, "layout page");
        // Typing ends the completion.
        keys(&mut app, "x");
        assert!(app.wildmenu().is_none());
        assert_eq!(app.cmdline, "layout pagex");
        // Nothing longer in common: the first Tab picks the first candidate.
        keys(&mut app, "<C-u>sync-<Tab>");
        assert_eq!(app.cmdline, "sync-clear");
        keys(&mut app, "<Tab>");
        assert_eq!(app.cmdline, "sync-detect");
    }
}
