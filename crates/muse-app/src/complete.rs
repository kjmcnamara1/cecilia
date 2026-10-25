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
    "completion-ignore-case",
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
        "completion-ignore-case" => &["off", "on", "toggle"],
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

/// Whether `candidate` starts with `typed`, optionally ignoring case.
fn matches(candidate: &str, typed: &str, ignore_case: bool) -> bool {
    if ignore_case {
        candidate.to_lowercase().starts_with(&typed.to_lowercase())
    } else {
        candidate.starts_with(typed)
    }
}

/// Candidates for the word being typed at the end of `line`. `score_dir` is the folder of the
/// current score (for `:audio`).
pub fn complete(
    line: &str,
    cwd: &Path,
    score_dir: Option<&Path>,
    ignore_case: bool,
) -> Completions {
    let lead = line.len() - line.trim_start_matches([':', ' ']).len();
    let body = &line[lead..];
    let Some(space) = body.find(char::is_whitespace) else {
        let mut items: Vec<String> = COMMANDS
            .iter()
            .filter(|c| matches(c, body, ignore_case))
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
        "e" | "edit" | "tabnew" | "tabe" | "tabedit" => {
            paths(arg, cwd, Some(SCORE_EXTENSIONS), ignore_case)
        }
        "w" | "write" => paths(arg, cwd, Some(WRITE_EXTENSIONS), ignore_case),
        "audio" => {
            let mut v = paths(
                arg,
                score_dir.unwrap_or(cwd),
                Some(crate::playalong::AUDIO_EXTENSIONS),
                ignore_case,
            );
            if matches("off", arg, ignore_case) {
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
                .filter(|v| matches(v, word, ignore_case))
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
fn paths(typed: &str, base: &Path, extensions: Option<&[&str]>, ignore_case: bool) -> Vec<String> {
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
            if !matches(&name, prefix, ignore_case)
                || (name.starts_with('.') && !prefix.starts_with('.'))
            {
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

/// Longest common prefix of the candidates (compared without case when `ignore_case`), in the
/// first candidate's spelling.
pub fn common_prefix(items: &[String], ignore_case: bool) -> String {
    let Some(first) = items.first() else {
        return String::new();
    };
    let same = |a: char, b: char| {
        if ignore_case {
            a.to_lowercase().eq(b.to_lowercase())
        } else {
            a == b
        }
    };
    let mut end = first.len();
    for item in &items[1..] {
        end = end.min(
            first
                .char_indices()
                .zip(item.chars())
                .find(|((_, a), b)| !same(*a, *b))
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
        let c = complete(":sync-", &dir, None, false);
        assert_eq!(c.items, ["sync-clear", "sync-detect", "sync-tap"]);
        assert_eq!((c.start, c.commands), (1, true));
        let c = complete(":layout p", &dir, None, false);
        assert_eq!(
            (c.start, c.items.as_slice()),
            (8, &["page".to_string()][..])
        );
        let c = complete(":e ", &dir, None, false);
        assert_eq!(c.items, ["songs/", "a.gp5"]);
        assert_eq!(
            complete(":e songs/", &dir, None, false).items,
            ["songs/c.gabc"]
        );
        assert_eq!(complete(":w a", &dir, None, false).items, ["a.gp5"]);
        assert_eq!(
            common_prefix(&["sync-clear".into(), "sync-detect".into()], false),
            "sync-"
        );
        // Ignoring case.
        std::fs::write(dir.join("Hymn.gabc"), b"").unwrap();
        assert!(complete(":e h", &dir, None, false).items.is_empty());
        assert_eq!(complete(":e h", &dir, None, true).items, ["Hymn.gabc"]);
        assert_eq!(complete(":LAY", &dir, None, true).items, ["layout"]);
        assert_eq!(complete(":layout PA", &dir, None, true).items, ["page"]);
        assert_eq!(
            common_prefix(&["Song1".into(), "song2".into()], true),
            "Song"
        );
        assert_eq!(common_prefix(&["Song1".into(), "song2".into()], false), "");
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
    fn case_setting_from_config_and_a_command_mode_binding() {
        let (config, errors) = Config::parse(
            "[ui]\ncompletion_ignore_case = false\n[keymap.command]\n\"<C-g>\" = \"completion-ignore-case\"\n",
        )
        .unwrap();
        assert!(errors.is_empty(), "{errors:?}");
        let mut app = App::new(config, None);
        assert!(!app.completion_ignore_case);
        keys(&mut app, ":LAY<Tab>");
        assert_eq!(app.cmdline, "LAY");
        // Toggling stays on the command line and the next Tab uses the new setting.
        keys(&mut app, "<C-g>");
        assert!(app.completion_ignore_case);
        assert_eq!(app.cmdline, "LAY");
        keys(&mut app, "<Tab>");
        assert_eq!(app.cmdline, "layout ");
        app.run(crate::Command::CompletionIgnoreCase(Some(false)));
        assert!(!app.completion_ignore_case);
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
