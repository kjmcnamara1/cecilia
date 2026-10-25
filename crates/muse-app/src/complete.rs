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
    "earlier",
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
    "open",
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
    "waveform-global",
    "workspace-close",
    "workspace-open",
    "workspace-open!",
    "workspace-save",
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
        "completion-ignore-case" | "waveform-global" => &["off", "on", "toggle"],
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
        "open" | "tabnew" | "tabe" | "tabedit" => {
            paths(arg, cwd, Some(SCORE_EXTENSIONS), ignore_case)
        }
        "w" | "write" => paths(arg, cwd, Some(WRITE_EXTENSIONS), ignore_case),
        "workspace-open" | "workspace-open!" | "workspace-save" => {
            crate::workspace::workspace_names()
                .into_iter()
                .filter(|n| matches(n, arg, ignore_case))
                .collect()
        }
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
        let c = complete(":open ", &dir, None, false);
        assert_eq!(c.items, ["songs/", "a.gp5"]);
        assert_eq!(
            complete(":open songs/", &dir, None, false).items,
            ["songs/c.gabc"]
        );
        assert_eq!(complete(":w a", &dir, None, false).items, ["a.gp5"]);
        assert_eq!(
            common_prefix(&["sync-clear".into(), "sync-detect".into()], false),
            "sync-"
        );
        // Ignoring case.
        std::fs::write(dir.join("Hymn.gabc"), b"").unwrap();
        assert!(complete(":open h", &dir, None, false).items.is_empty());
        assert_eq!(complete(":open h", &dir, None, true).items, ["Hymn.gabc"]);
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
        let p = app.palette().unwrap();
        assert_eq!((p.items.len(), p.selected), (3, Some(0)));
        keys(&mut app, "<Tab><Down><Up>");
        assert_eq!(app.cmdline, "layout page");
        // Typing ends the completion.
        keys(&mut app, "x");
        assert!(app.palette().unwrap().selected.is_none());
        assert_eq!(app.cmdline, "layout pagex");
        // Nothing longer in common: the first Tab picks the first candidate.
        keys(&mut app, "<C-u>sync-<Tab>");
        assert_eq!(app.cmdline, "sync-clear");
        keys(&mut app, "<Tab>");
        assert_eq!(app.cmdline, "sync-detect");
    }
}

/// One-line description of a command, shown in the command palette.
pub fn describe(command: &str) -> Option<&'static str> {
    Some(match command {
        "append" => "Insert mode after the cursor beat",
        "audio" => "Attach an audio file to the score (off detaches)",
        "bar" | "goto-bar" => "Go to bar number",
        "bar-delete" => "Delete the cursor bar",
        "bar-end" => "Cursor to the last beat of the bar",
        "bar-insert-after" => "Insert an empty bar after the cursor",
        "bar-insert-before" => "Insert an empty bar before the cursor",
        "bar-next" => "Next bar",
        "bar-prev" => "Previous bar",
        "bar-start" => "Cursor to the first beat of the bar",
        "beat-new" => "New beat after the cursor",
        "beat-next" => "Next beat",
        "beat-prev" => "Previous beat",
        "capo" => "Set the capo fret",
        "change" => "Change operator (delete, then insert)",
        "chant-to-modern" => "Turn the chant track into a staff track",
        "chant-view" => "Square notation / modern view of chant",
        "clef" => "Chant clef (c1–c4, f1–f4, cb3)",
        "close" | "close-pane" => "Close the pane",
        "command-mode" => "Open the command line",
        "completion-ignore-case" => "Toggle case-insensitive completion",
        "count-in" => "Toggle the count-in before playing",
        "delete" => "Delete operator",
        "delete-note" => "Delete the note under the cursor",
        "dot" => "Toggle a dot on the beat",
        "duration" => "Set the note value (1, 2, 4 … 64)",
        "duration-longer" => "Double the beat's length",
        "duration-shorter" => "Halve the beat's length",
        "earlier" | "undo-older" => "Go to the older undo state (across branches)",
        "enew" | "new" => "New empty score",
        "focus-down" => "Focus the pane below",
        "focus-left" => "Focus the pane to the left",
        "focus-right" => "Focus the pane to the right",
        "focus-up" => "Focus the pane above",
        "follow-toggle" => "Toggle following playback",
        "gabc" => "Show or replace the chant syllable as GABC",
        "goto-first" => "Go to the first bar",
        "goto-last" => "Go to the last bar",
        "help" => "Show all key bindings",
        "insert" => "Insert mode at the cursor",
        "insert-next" => "Insert after the cursor beat",
        "interpretation" => "Chant rhythm: equalist or solesmes",
        "key" => "Key signature from the cursor bar",
        "later" | "undo-newer" => "Go to the newer undo state (across branches)",
        "layout" => "Score layout of this pane",
        "loop" => "Loop bars a–b",
        "loop-toggle" => "Loop the selection or cursor bar",
        "lyrics" => "Lyrics from the cursor on",
        "lyrics-clear" => "Remove a verse of lyrics",
        "metronome" => "Toggle the metronome",
        "mix" => "Mix: both, audio or midi",
        "modern-to-chant" => "Open the track's melody as chant",
        "mute" => "Toggle muting a track",
        "nop" => "Do nothing",
        "normal" => "Back to Normal mode",
        "note" => "Set the cursor note's pitch (e.g. C#4)",
        "note-next" => "Next note",
        "note-prev" => "Previous note",
        "octave-down" => "Note down an octave",
        "octave-up" => "Note up an octave",
        "open" => "Open a file",
        "page-down" => "Scroll forward a page of bars",
        "page-up" => "Scroll back a page of bars",
        "pause" => "Pause playback",
        "pitch" => "Shift the recording's pitch (semitones)",
        "pitch-down" => "Note down a semitone",
        "pitch-up" => "Note up a semitone",
        "play" => "Play from the cursor",
        "play-macro" => "Replay a recorded macro",
        "play-toggle" => "Play / pause",
        "put" => "Paste after the cursor",
        "put-before" => "Paste before the cursor",
        "q" | "quit" => "Close the pane (quit with the last one)",
        "q!" | "quit!" => "Close the pane, discarding changes",
        "qa" | "quitall" => "Quit muse",
        "qa!" | "quitall!" => "Quit muse, discarding changes",
        "record-macro" => "Record a macro into a register",
        "redo" => "Redo",
        "refinger" => "Re-finger the track to minimise hand movement",
        "register" => "Use a register for the next yank/put",
        "repeat" => "Repeat the last change",
        "repeat-end" => "Toggle a repeat end (times played)",
        "repeat-start" => "Toggle a repeat start",
        "rest" => "Make the beat a rest",
        "section" => "Set or clear the bar's section name",
        "solo" => "Toggle soloing a track",
        "source" => "Reload the configuration",
        "speed" => "Playback speed in percent",
        "speed-down" => "Slower playback",
        "speed-up" => "Faster playback",
        "split" => "Split the pane horizontally",
        "stop" => "Stop playback",
        "string-down" => "Cursor to the next lower string",
        "string-up" => "Cursor to the next higher string",
        "sync" => "Start or stop tapping sync points",
        "sync-clear" => "Remove all audio sync points",
        "sync-detect" => "Sync the audio by beat detection",
        "sync-tap" => "Sync point at the playing bar",
        "tab-next" => "Next tab",
        "tab-prev" => "Previous tab",
        "tabclose" => "Close the tab",
        "tabedit" | "tabnew" => "Open a file in a new tab",
        "tempo" => "Tempo from the cursor bar",
        "text" => "Text above the beat",
        "time" => "Time signature from the cursor bar",
        "toggle" => "Toggle a technique on the note",
        "track" => "Go to track number",
        "track-add" => "Add a track (guitar, bass, drums, piano, voice)",
        "track-delete" => "Delete the track",
        "track-next" => "Next track",
        "track-prev" => "Previous track",
        "trainer" => "Speed trainer: start, target, step",
        "transpose" => "Transpose the track (semitones)",
        "tuning" => "Retune the track, keeping pitches",
        "tuplet" => "Make the beat a tuplet",
        "undo" => "Undo",
        "visual" => "Visual mode (beats)",
        "visual-bar" => "Visual mode (bars)",
        "voice" => "Edit voice 1–4",
        "volume" => "Level of midi, audio or metronome",
        "vsplit" => "Split the pane vertically",
        "w" | "write" => "Save (or save as / export by extension)",
        "waveform" => "Toggle this pane's waveform strip",
        "waveform-global" => "Toggle the window waveform bar",
        "wq" | "write-quit" => "Save and close the pane",
        "workspace-save" => "Save open files, tabs and panes as a workspace",
        "workspace-open" => "Open a saved workspace",
        "workspace-open!" => "Open a workspace, discarding unsaved changes",
        "workspace-close" => "Stop saving to the active workspace",
        "yank" => "Yank operator",
        "zoom-in" => "Zoom in",
        "zoom-out" => "Zoom out",
        "zoom-reset" => "Reset zoom",
        _ => return None,
    })
}

#[cfg(test)]
mod describe_tests {
    #[test]
    fn every_command_has_a_description() {
        for name in super::COMMANDS {
            assert!(super::describe(name).is_some(), "{name}");
        }
    }
}
