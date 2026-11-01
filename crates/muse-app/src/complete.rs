//! Tab completion for the command line: command names, then each command's arguments (file
//! paths, layouts, mixer modes, tunings, techniques, ...).

use std::path::Path;

/// Every command name the parser accepts, long names first where there are aliases.
pub const COMMANDS: &[&str] = &[
    "set-mark",
    "run-macro",
    "marks",
    "mark",
    "macros",
    "jump-mark-bar",
    "jump-mark",
    "goto-mark",
    "delmarks!",
    "delmarks",
    "accidental",
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
    "bdelete",
    "bdelete!",
    "buffer",
    "buffer-close",
    "buffer-close!",
    "buffer-next",
    "buffer-prev",
    "capo",
    "change",
    "chant-append",
    "chant-bar",
    "chant-break",
    "chant-mod",
    "chant-note",
    "chant-syllable",
    "chant-to-modern",
    "chant-view",
    "clef",
    "close",
    "pane-close",
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
    "focus-last",
    "focus-left",
    "focus-next",
    "focus-prev",
    "focus-right",
    "focus-up",
    "completion-fuzzy",
    "completion-ignore-case",
    "follow-toggle",
    "gabc",
    "goto-bar",
    "global-tool",
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
    "note-letter",
    "note-next",
    "note-prev",
    "octave-down",
    "octave-up",
    "open",
    "open-dialog",
    "page-down",
    "page-up",
    "pause",
    "pick",
    "pick-buffer",
    "pick-keymap",
    "pick-section",
    "sections",
    "section-next",
    "section-prev",
    "keymaps",
    "tracks",
    "toolbar",
    "fretboard",
    "piano",
    "progress-bar",
    "scrollbar",
    "tuner",
    "buffer-move",
    "buffer-move-next",
    "buffer-move-prev",
    "resize",
    "vertical-resize",
    "resize-equal",
    "pane-move-left",
    "pane-move-down",
    "pane-move-up",
    "pane-move-right",
    "pane-exchange",
    "pane-rotate",
    "pane-rotate-back",
    "maximize",
    "tracks-resize",
    "waveform-resize",
    "tracks-focus",
    "properties",
    "properties-focus",
    "properties-tab",
    "palette",
    "palette-focus",
    "dynamic",
    "track-volume",
    "track-volume-up",
    "track-volume-down",
    "track-color",
    "pick-view",
    "pick-workspace",
    "pitch",
    "pitch-down",
    "pitch-up",
    "play",
    "play-macro",
    "play-toggle",
    "put",
    "recover",
    "recover!",
    "swap-discard",
    "put-before",
    "q",
    "q!",
    "qa",
    "qa!",
    "quit",
    "quit!",
    "quitall",
    "wall",
    "wqall",
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
    "slash",
    "split",
    "staff",
    "staff-add",
    "staff-move",
    "staff-remove",
    "stop",
    "staff-next",
    "staff-prev",
    "string-down",
    "string-up",
    "sync",
    "sync-clear",
    "sync-detect",
    "sync-tap",
    "tempo",
    "text",
    "time",
    "toggle",
    "track",
    "track-add",
    "track-delete",
    "track-instrument",
    "track-rename",
    "track-rename-start",
    "track-next",
    "track-prev",
    "trainer",
    "transpose",
    "tuning",
    "tuplet",
    "undo",
    "undo-newer",
    "undo-older",
    "view",
    "view-close",
    "view-close!",
    "view-new",
    "view-next",
    "view-prev",
    "view-rename",
    "visual",
    "visual-bar",
    "voice",
    "volume",
    "vsplit",
    "w",
    "waveform",
    "waveform-strip",
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
        "global-tool" => &["fretboard", "piano", "tracks", "tuner", "waveform"],
        "track-add" => &["bass", "drums", "guitar", "piano", "voice"],
        "staff" | "staff-add" => &["chant", "slash", "bass", "grand", "treble", "tab", "tab+"],
        "accidental" => &["flat", "natural", "sharp"],
        "chant-bar" => &[",", ":", "::", ";"],
        "volume" => &["audio", "metronome", "midi"],
        "pick" => &[
            "buffer",
            "keymap",
            "macro",
            "mark",
            "section",
            "view",
            "workspace",
        ],
        "tracks" | "toolbar" | "palette" | "fretboard" | "piano" | "tuner" => &["off", "on"],
        "track-color" => &["off"],
        "completion-ignore-case"
        | "completion-fuzzy"
        | "waveform"
        | "progress-bar"
        | "scrollbar" => &["off", "on", "toggle"],
        "duration" => &["1", "2", "4", "8", "16", "32", "64"],
        "tuning" => &[
            "7-string", "bass", "bass5", "d", "dadgad", "drop-d", "eb", "open-d", "open-e",
            "open-g", "standard", "ukulele",
        ],
        "dynamic" => &["ppp", "pp", "p", "mp", "mf", "f", "ff", "fff"],
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
    /// Character positions in each item that matched what was typed (for highlighting).
    pub positions: Vec<Vec<usize>>,
    /// The candidates are command names (a space follows a unique match).
    pub commands: bool,
}

/// How typed text matches candidates.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Matcher {
    pub ignore_case: bool,
    /// The typed characters may be spread through the candidate, in order ("sd" finds
    /// "sync-detect"); otherwise the candidate must start with them.
    pub fuzzy: bool,
}

impl Matcher {
    fn same(&self, a: char, b: char) -> bool {
        if self.ignore_case {
            a.to_lowercase().eq(b.to_lowercase())
        } else {
            a == b
        }
    }

    /// Whether `candidate` starts with `typed`.
    pub fn is_prefix(&self, candidate: &str, typed: &str) -> bool {
        let mut c = candidate.chars();
        typed
            .chars()
            .all(|t| c.next().is_some_and(|x| self.same(x, t)))
    }

    /// A score for `candidate` against `typed` (higher is better) and the matched character
    /// positions, or None when it does not match.
    pub fn score(&self, candidate: &str, typed: &str) -> Option<(i64, Vec<usize>)> {
        let chars: Vec<char> = candidate.chars().collect();
        let n = typed.chars().count();
        if self.is_prefix(candidate, typed) {
            // Prefix matches first (an exact match before the rest, which keep their order).
            let exact = chars.len() == n;
            return Some((1_000_000 + i64::from(exact), (0..n).collect()));
        }
        if !self.fuzzy {
            return None;
        }
        // In-order subsequence, preferring word starts and runs of consecutive characters.
        let boundary =
            |i: usize| i == 0 || matches!(chars[i - 1], '-' | '_' | ' ' | '/' | '.' | '(' | '[');
        let mut positions = Vec::with_capacity(n);
        let mut from = 0;
        for t in typed.chars() {
            let candidates: Vec<usize> = (from..chars.len())
                .filter(|&i| self.same(chars[i], t))
                .collect();
            let first = *candidates.first()?;
            // Take a later word-start occurrence over an earlier mid-word one, unless the
            // earlier one continues a run.
            let continues = positions.last().is_some_and(|&p: &usize| p + 1 == first);
            let pick = if continues {
                first
            } else {
                candidates
                    .iter()
                    .copied()
                    .find(|&i| boundary(i))
                    .unwrap_or(first)
            };
            positions.push(pick);
            from = pick + 1;
        }
        let mut score = 0i64;
        for (k, &p) in positions.iter().enumerate() {
            score += 16;
            if boundary(p) {
                score += 10;
            }
            if k > 0 && positions[k - 1] + 1 == p {
                score += 8;
            }
        }
        score -= positions.last().map_or(0, |&l| l as i64 - n as i64 + 1);
        score -= chars.len() as i64 / 4;
        Some((score, positions))
    }

    /// The candidates that match, best first (prefix matches in their given order).
    pub fn rank(
        &self,
        items: impl IntoIterator<Item = String>,
        typed: &str,
    ) -> Vec<(String, Vec<usize>)> {
        let mut scored: Vec<(i64, usize, String, Vec<usize>)> = items
            .into_iter()
            .enumerate()
            .filter_map(|(i, item)| {
                let (score, pos) = self.score(&item, typed)?;
                Some((score, i, item, pos))
            })
            .collect();
        // Equal scores keep the given order (alphabetical, folders first).
        scored.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
        scored
            .into_iter()
            .map(|(_, _, item, pos)| (item, pos))
            .collect()
    }
}

fn completions(start: usize, ranked: Vec<(String, Vec<usize>)>, commands: bool) -> Completions {
    let (items, positions) = ranked.into_iter().unzip();
    Completions {
        start,
        items,
        positions,
        commands,
    }
}

/// Candidates for the word being typed at the end of `line`. `score_dir` is the folder of the
/// current score (for `:audio`).
pub fn complete(line: &str, cwd: &Path, score_dir: Option<&Path>, m: Matcher) -> Completions {
    let lead = line.len() - line.trim_start_matches([':', ' ']).len();
    let body = &line[lead..];
    let Some(space) = body.find(char::is_whitespace) else {
        let mut names: Vec<String> = COMMANDS.iter().map(|c| c.to_string()).collect();
        names.sort();
        return completions(lead, m.rank(names, body), true);
    };
    let command = &body[..space];
    let rest = &body[space..];
    let arg_start = lead + space + (rest.len() - rest.trim_start().len());
    let arg = &line[arg_start..];
    let ranked = match command {
        "recover" | "recover!" => paths(arg, cwd, None, m),
        "open" | "view-new" => paths(arg, cwd, Some(SCORE_EXTENSIONS), m),
        "w" | "write" => paths(arg, cwd, Some(WRITE_EXTENSIONS), m),
        "workspace-open" | "workspace-open!" | "workspace-save" => {
            m.rank(crate::workspace::workspace_names(), arg)
        }
        "audio" => {
            let mut v = paths(
                arg,
                score_dir.unwrap_or(cwd),
                Some(crate::playalong::AUDIO_EXTENSIONS),
                m,
            );
            if let Some((_, pos)) = m.score("off", arg) {
                v.insert(0, ("off".into(), pos));
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
            let items = list.unwrap_or_default().iter().map(|v| v.to_string());
            return completions(arg_start + word_start, m.rank(items, word), false);
        }
    };
    completions(arg_start, ranked, false)
}

/// Files and folders matching a typed path (relative to `base`); folders end with `/`. Only the
/// last path component is matched.
fn paths(
    typed: &str,
    base: &Path,
    extensions: Option<&[&str]>,
    m: Matcher,
) -> Vec<(String, Vec<usize>)> {
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
    let mut names: Vec<(String, bool)> = entries
        .flatten()
        .filter_map(|e| {
            let name = e.file_name().to_string_lossy().to_string();
            if name.starts_with('.') && !prefix.starts_with('.') {
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
            Some((name, is_dir))
        })
        .collect();
    // Folders first, then files, alphabetically (the order among equal scores).
    names.sort_by_key(|(n, d)| (!d, n.to_lowercase()));
    let offset = dir_part.chars().count();
    m.rank(names.iter().map(|(n, _)| n.clone()), prefix)
        .into_iter()
        .map(|(name, pos)| {
            let is_dir = names.iter().any(|(n, d)| *n == name && *d);
            (
                format!("{dir_part}{name}{}", if is_dir { "/" } else { "" }),
                pos.into_iter().map(|p| p + offset).collect(),
            )
        })
        .collect()
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

    const PREFIX: Matcher = Matcher {
        ignore_case: false,
        fuzzy: false,
    };
    const NOCASE: Matcher = Matcher {
        ignore_case: true,
        fuzzy: false,
    };
    const FUZZY: Matcher = Matcher {
        ignore_case: true,
        fuzzy: true,
    };

    #[test]
    fn fuzzy_matching_ranks_and_marks_characters() {
        let dir = std::env::temp_dir();
        // Characters in order anywhere; word starts rank above mid-word matches.
        let c = complete(":sd", &dir, None, FUZZY);
        assert_eq!(&c.items[..2], ["sync-detect", "speed-down"]);
        assert_eq!(c.positions[0], [0, 5]);
        // Prefix matches come first, the closest first.
        let c = complete(":sync", &dir, None, FUZZY);
        assert_eq!(c.items[0], "sync");
        assert!(c.items.iter().take(5).all(|i| i.starts_with("sync")));
        // Out-of-order characters do not match; without fuzzy only prefixes do.
        assert!(complete(":zyx", &dir, None, FUZZY).items.is_empty());
        assert!(complete(":sd", &dir, None, NOCASE).items.is_empty());
        // Case follows the case setting.
        let strict = Matcher {
            ignore_case: false,
            fuzzy: true,
        };
        assert!(strict.score("sync-detect", "SD").is_none());
        assert!(FUZZY.score("sync-detect", "SD").is_some());
        // Argument values too.
        assert_eq!(
            complete(":layout hz", &dir, None, FUZZY).items,
            ["horizontal"]
        );
    }

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
        let c = complete(":sync-", &dir, None, PREFIX);
        assert_eq!(c.items, ["sync-clear", "sync-detect", "sync-tap"]);
        assert_eq!((c.start, c.commands), (1, true));
        let c = complete(":layout p", &dir, None, PREFIX);
        assert_eq!(
            (c.start, c.items.as_slice()),
            (8, &["page".to_string()][..])
        );
        let c = complete(":open ", &dir, None, PREFIX);
        assert_eq!(c.items, ["songs/", "a.gp5"]);
        assert_eq!(
            complete(":open songs/", &dir, None, PREFIX).items,
            ["songs/c.gabc"]
        );
        assert_eq!(complete(":w a", &dir, None, PREFIX).items, ["a.gp5"]);
        assert_eq!(
            common_prefix(&["sync-clear".into(), "sync-detect".into()], false),
            "sync-"
        );
        // Ignoring case.
        std::fs::write(dir.join("Hymn.gabc"), b"").unwrap();
        assert!(complete(":open h", &dir, None, PREFIX).items.is_empty());
        assert_eq!(complete(":open h", &dir, None, NOCASE).items, ["Hymn.gabc"]);
        assert_eq!(complete(":LAY", &dir, None, NOCASE).items, ["layout"]);
        assert_eq!(complete(":layout PA", &dir, None, NOCASE).items, ["page"]);
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
        "close" | "pane-close" => "Close the pane",
        "command-mode" => "Open the command line",
        "completion-ignore-case" => "Toggle case-insensitive completion",
        "completion-fuzzy" => "Toggle fuzzy completion (letters in order, anywhere)",
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
        "focus-last" => "Focus the previously focused pane",
        "focus-left" => "Focus the pane to the left",
        "focus-next" => "Focus the next pane",
        "focus-prev" => "Focus the previous pane",
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
        "open" => "Open a file (no file: the system file picker)",
        "open-dialog" => "Choose files to open in the system file picker",
        "page-down" => "Scroll forward a page of bars",
        "page-up" => "Scroll back a page of bars",
        "delmarks" => "Delete marks (letters), or :delmarks! for the file's",
        "delmarks!" => "Delete all lowercase marks of the file",
        "goto-mark" => "Jump to a mark's exact position",
        "jump-mark" => "Jump to a mark (then its letter)",
        "jump-mark-bar" => "Jump to a mark's bar (then its letter)",
        "macros" => "Pick a recorded macro to run",
        "mark" => "Set a mark at the cursor (a-z local, A-Z global)",
        "marks" => "Pick a mark to jump to",
        "run-macro" => "Play the macro in a register",
        "set-mark" => "Set a mark (then its letter)",
        "pause" => "Pause playback",
        "pick" => "Open a picker: buffer, view, workspace, mark, macro or keymap",
        "pick-keymap" | "keymaps" => "Find a key binding by keys, command or description",
        "pick-section" | "sections" => "Jump to a section of this file",
        "section-next" => "Next section",
        "section-prev" => "Previous section",
        "tracks" => "Show or hide the tracks panel",
        "toolbar" => "Show or hide the toolbar",
        "fretboard" => "Show or hide the fretboard (notes at the cursor or playhead)",
        "piano" => "Show or hide the piano keys (notes at the cursor or playhead)",
        "tuner" => "Show or hide the tuner (listens to the audio input)",
        "buffer-move" | "tab-move" => "Move this file's tab: to position N, or by +N / -N",
        "buffer-move-next" => "Move this file's tab one place right",
        "buffer-move-prev" => "Move this file's tab one place left",
        "resize" | "res" => "Pane height in percent of its split: N, +N or -N",
        "vertical-resize" | "vresize" => "Pane width in percent of its split: N, +N or -N",
        "resize-equal" => "Make all panes of the view the same size",
        "pane-move-left" => "Move the pane to the far left, full height",
        "pane-move-down" => "Move the pane to the bottom, full width",
        "pane-move-up" => "Move the pane to the top, full width",
        "pane-move-right" => "Move the pane to the far right, full height",
        "pane-exchange" => "Swap the pane with the next one",
        "pane-rotate" => "Rotate the panes of this split forwards",
        "pane-rotate-back" => "Rotate the panes of this split backwards",
        "maximize" => "Show the focused pane alone (again: all panes)",
        "tracks-resize" => "Tracks the tracks panel shows: N, +N or -N",
        "waveform-resize" => "Height of the waveforms in pixels: N, +N or -N",
        "tracks-focus" => "Focus the tracks panel (again: back to the score)",
        "palette" => "Show or hide the editing palette (durations, rests, effects)",
        "palette-focus" => "Focus the editing palette (again: back to the score)",
        "palette-down" => "Palette: select the button below",
        "palette-up" => "Palette: select the button above",
        "palette-left" => "Palette: select the button to the left",
        "palette-right" => "Palette: select the button to the right",
        "palette-activate" => "Palette: apply the selected item (a section: fold or unfold)",
        "palette-fold" => "Palette: fold (close), unfold (open) or toggle the selected section",
        "dynamic" => "Set the dynamic of the note: ppp, pp, p, mp, mf, f, ff or fff",
        "properties" => "Show or hide the properties pane (score, tracks, lyrics)",
        "properties-focus" => "Focus the properties pane (again: back to the score)",
        "properties-tab" => "Show a tab of the properties pane: score, tracks or lyrics",
        "properties-tab-prev" => "Show the previous tab of the properties pane",
        "properties-down" => "Properties pane: select the next property",
        "properties-up" => "Properties pane: select the previous property",
        "properties-edit" => "Properties pane: edit the selected property",
        "properties-fold" => {
            "Properties pane: fold (close), unfold (open) or toggle the selected section"
        }
        "properties-staff-move" => "Properties pane: move the selected staff down (+1) or up (-1)",
        "properties-change" => "Properties pane: clear the selected property and edit it",
        "properties-step" => {
            "Properties pane: switch off (-1) / on (+1), or the previous / next choice"
        }
        "properties-search" => "Properties pane: search the keys and values (or the lyrics text)",
        "properties-search-next" => "Properties pane: jump to the next match of the / register",
        "properties-search-prev" => "Properties pane: jump to the previous match of the / register",
        "properties-clear" => {
            "Properties pane: clear the selected text, lyrics or color, or remove the staff"
        }
        "track-volume" => "Set the track's volume: 80, +10 or -10 (percent)",
        "track-volume-up" => "Turn the track up",
        "track-volume-down" => "Turn the track down",
        "track-color" => "Track color in the tracks panel: #rrggbb, off, or the next one",
        "pick-buffer" => "Pick a buffer of this view to show",
        "pick-view" => "Pick a view to switch to",
        "pick-workspace" => "Pick a saved workspace to open",
        "pitch" => "Shift the recording's pitch (semitones)",
        "pitch-down" => "Note down a semitone",
        "pitch-up" => "Note up a semitone",
        "play" => "Play from the cursor",
        "play-macro" => "Replay a recorded macro",
        "play-toggle" => "Play / pause",
        "put" => "Paste after the cursor",
        "put-before" => "Paste before the cursor",
        "q" | "qa" | "quit" | "quitall" => "Quit muse (not while files have unsaved changes)",
        "q!" | "qa!" | "quit!" | "quitall!" => "Quit muse, discarding changes",
        "wa" | "wall" => "Write every changed file",
        "wqa" | "wqall" | "xa" | "xall" => "Write every changed file and quit",
        "recover" => "Restore the file from its swap file",
        "recover!" => "Restore from a swap file even if another muse owns it",
        "swap-discard" => "Delete the file's swap file",
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
        "staff-next" => "Cursor to the next staff of the track",
        "staff-prev" => "Cursor to the previous staff of the track",
        "string-down" => "Cursor to the next lower string",
        "string-up" => "Cursor to the next higher string",
        "sync" => "Start or stop tapping sync points",
        "sync-clear" => "Remove all audio sync points",
        "sync-detect" => "Sync the audio by beat detection",
        "sync-tap" => "Sync point at the playing bar",
        "bdelete" | "buffer-close" => "Close the buffer's tab in this view",
        "bdelete!" | "buffer-close!" => "Close the buffer's tab, discarding changes",
        "buffer" => "Show bufferline tab number",
        "buffer-next" => "Next buffer tab",
        "buffer-prev" => "Previous buffer tab",
        "view" => "Go to view number",
        "view-close" => "Close the view",
        "view-close!" => "Close the view, discarding unsaved changes",
        "view-new" => "New view (optionally opening a file)",
        "view-next" => "Next view",
        "view-prev" => "Previous view",
        "view-rename" => "Name the view (no name: back to its number)",
        "tempo" => "Tempo from the cursor bar",
        "text" => "Text above the beat",
        "time" => "Time signature from the cursor bar",
        "toggle" => "Toggle a technique on the note",
        "track" => "Go to track number",
        "track-add" => "Add a track (guitar, bass, drums, piano, voice)",
        "track-delete" => "Delete the track",
        "track-rename" => "Rename a track: NAME [track number] (the cursor's track by default)",
        "track-rename-start" => "Rename the selected track in place in the tracks pane",
        "track-instrument" => {
            "Set a track's instrument: PROGRAM (1-128) or drums [track number]; without one, a picker"
        }
        "staff" => {
            "Show the cursor's staff another way (treble, bass, grand [split], tab, tab+, slash, chant)"
        }
        "staff-add" => "Add a staff at the bottom of the track",
        "staff-remove" => "Remove a staff of the track (1 = top; the cursor's by default)",
        "staff-move" => "Move the cursor's staff to a position (1 = top), or by +N / -N",
        "note-letter" => "Enter the note of a letter (a-g) nearest the cursor on a standard staff",
        "accidental" => "Make the cursor note sharp, flat or natural",
        "slash" => "Put a slash at the cursor beat of a slash staff",
        "chant-note" => "Set the cursor chant note's GABC pitch (a-m; upper case: inclinatum)",
        "chant-mod" => "Toggle a GABC modifier of the cursor chant note",
        "chant-bar" => "Add a division after the cursor syllable",
        "chant-append" => "Add a chant note after the cursor note",
        "chant-break" => "Add a chant note after the cursor note, starting a new neume",
        "chant-syllable" => "Add a chant syllable after the cursor syllable",
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
        "waveform-strip" => "Toggle this pane's waveform strip",
        "waveform" => "Show or hide the waveform tool pane",
        "global-tool" => {
            "Toggle whether a tool pane (tracks, audio, fretboard, piano, tuner) is global"
        }
        "progress-bar" => "Show or hide the progress bar along the score panes",
        "scrollbar" => "Show or hide the scrollbar on the score panes",
        "wq" | "x" | "write-quit" => "Write every changed file and quit",
        "workspace-save" => "Save the views, panes and open files as a workspace",
        "workspace-open" => "Open a saved workspace",
        "workspace-open!" => "Open a workspace, discarding unsaved changes",
        "workspace-close" => "Save and leave the active workspace for default",
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
