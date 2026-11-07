//! Tab completion for the command line: command names, then each command's arguments (file
//! paths, layouts, mixer modes, tunings, techniques, ...).

use std::path::Path;

/// The kind of score a command is for. A chant score offers `Both` and `Chant` commands in
/// completion, help and the palettes; a modern score `Both` and `Modern`. Typing a command of
/// the other kind in full still parses and says which kind it is not for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scope {
    Both,
    Modern,
    Chant,
}

impl Scope {
    /// Whether a score of this kind offers a command of scope `self`.
    pub fn applies(self, chant_score: bool) -> bool {
        match self {
            Scope::Both => true,
            Scope::Modern => !chant_score,
            Scope::Chant => chant_score,
        }
    }
}

use Scope::{Both as B, Chant as C, Modern as M};

/// Every command name the parser accepts with its scope, long names first where there are
/// aliases. A command added here must say which kind of score it is for.
pub const COMMAND_SCOPES: &[(&str, Scope)] = &[
    ("set-mark", B),
    ("run-macro", B),
    ("marks", B),
    ("mark", B),
    ("macros", B),
    ("jump-mark-bar", B),
    ("jump-mark", B),
    ("goto-mark", B),
    ("delmarks!", B),
    ("delmarks", B),
    ("accidental", M),
    ("append", B),
    ("audio", B),
    ("bar", B),
    ("bar-repeat", M),
    ("beams", M),
    ("direction", M),
    ("double-bar", M),
    ("dynamic-step", M),
    ("double-dot", M),
    ("ending", M),
    ("free-time", M),
    ("stems", M),
    ("bar-delete", M),
    ("bar-end", B),
    ("bar-first-note", B),
    ("bar-last-note", B),
    ("bar-open-after", M),
    ("bar-open-before", M),
    ("bar-insert-after", M),
    ("bar-insert-before", M),
    ("bar-next", B),
    ("bar-prev", B),
    ("bar-start", B),
    ("beat-new", B),
    ("beat-next", B),
    ("beat-prev", B),
    ("bdelete", B),
    ("bdelete!", B),
    ("buffer", B),
    ("buffer-close", B),
    ("buffer-close!", B),
    ("buffer-next", B),
    ("buffer-prev", B),
    ("capo", M),
    ("change", B),
    ("chant-append", C),
    ("chant-bar", C),
    ("chant-break", C),
    ("chant-mod", C),
    ("chant-syllable", C),
    ("chant-to-modern", C),
    ("clef", B),
    ("close", B),
    ("pane-close", B),
    ("command-mode", B),
    ("count-in", B),
    ("delete", B),
    ("delete-note", B),
    ("delete-beat", B),
    ("reflow", M),
    ("dot", M),
    ("duration", M),
    ("duration-longer", M),
    ("duration-shorter", M),
    ("earlier", B),
    ("enew", B),
    ("focus-down", B),
    ("focus-last", B),
    ("focus-left", B),
    ("focus-next", B),
    ("focus-prev", B),
    ("focus-right", B),
    ("focus-up", B),
    ("bar-numbers", B),
    ("incomplete-bars", B),
    ("completion-fuzzy", B),
    ("completion-ignore-case", B),
    ("follow-toggle", B),
    ("gabc", C),
    ("goto-bar", B),
    ("global-tool", B),
    ("goto-first", B),
    ("goto-last", B),
    ("help", B),
    ("insert", B),
    ("insert-bar-start", B),
    ("insert-enter", B),
    ("insert-here", B),
    ("insert-last", B),
    ("append-bar-end", B),
    ("insert-next", B),
    ("interpretation", C),
    ("key", M),
    ("later", B),
    ("layout", B),
    ("loop", B),
    ("loop-toggle", B),
    ("lyrics", M),
    ("lyrics-attach", M),
    ("lyrics-clear", M),
    ("notation", M),
    ("metronome", B),
    ("mix", B),
    ("modern-to-chant", M),
    ("mute", B),
    ("new", B),
    ("nop", B),
    ("normal", B),
    ("note", M),
    ("note-letter", M),
    ("note-next", B),
    ("note-prev", B),
    ("note-row-next", B),
    ("note-row-end", B),
    ("note-row-prev", B),
    ("octave-down", B),
    ("octave-up", B),
    ("open", B),
    ("open-dialog", B),
    ("page-down", B),
    ("page-up", B),
    ("pause", B),
    ("pick", B),
    ("pick-buffer", B),
    ("pick-keymap", B),
    ("pick-section", B),
    ("save-as", B),
    ("sections", B),
    ("section-next", B),
    ("section-prev", B),
    ("keymaps", B),
    ("tracks", B),
    ("toolbar", B),
    ("equalize-on-close", B),
    ("focus-ring-width", B),
    ("focus-ring-color", B),
    ("tabline-wrap", B),
    ("fretboard", B),
    ("piano", B),
    ("progress-bar", B),
    ("scrollbar", B),
    ("design", B),
    ("bar-width", B),
    ("bars-per-line", B),
    ("line-break", B),
    ("line-join", B),
    ("beat-nudge", B),
    ("design-reset", B),
    ("tuner", B),
    ("buffer-move", B),
    ("buffer-move-next", B),
    ("buffer-move-prev", B),
    ("resize", B),
    ("vertical-resize", B),
    ("resize-equal", B),
    ("pane-move-left", B),
    ("pane-move-down", B),
    ("pane-move-up", B),
    ("pane-move-right", B),
    ("pane-exchange", B),
    ("pane-rotate", B),
    ("pane-rotate-back", B),
    ("maximize", B),
    ("tracks-resize", B),
    ("waveform-resize", B),
    ("tracks-focus", B),
    ("properties", B),
    ("properties-focus", B),
    ("properties-tab", B),
    ("palette", B),
    ("palette-focus", B),
    ("palette-search", B),
    ("palette-search-next", B),
    ("palette-search-prev", B),
    ("dynamic", M),
    ("track-volume", B),
    ("track-volume-up", B),
    ("track-volume-down", B),
    ("track-color", B),
    ("pick-view", B),
    ("pick-recent", B),
    ("pick-workspace", B),
    ("pitch", B),
    ("pitch-down", B),
    ("pitch-up", B),
    ("play", B),
    ("play-macro", B),
    ("play-toggle", B),
    ("put", B),
    ("recover", B),
    ("recover!", B),
    ("swap-discard", B),
    ("put-before", B),
    ("q", B),
    ("q!", B),
    ("qa", B),
    ("qa!", B),
    ("quit", B),
    ("quit!", B),
    ("quitall", B),
    ("wall", B),
    ("wqall", B),
    ("quitall!", B),
    ("record-macro", B),
    ("redo", B),
    ("refinger", M),
    ("register", B),
    ("repeat", B),
    ("repeat-end", M),
    ("repeat-start", M),
    ("rest", M),
    ("section", M),
    ("solo", B),
    ("source", B),
    ("speed", B),
    ("speed-down", B),
    ("speed-up", B),
    ("slash", M),
    ("split", B),
    ("staff-add", M),
    ("staff-move", M),
    ("staff-remove", M),
    ("stop", B),
    ("staff-next", M),
    ("staff-prev", M),
    ("string-down", M),
    ("string-up", M),
    ("sync", B),
    ("sync-clear", B),
    ("sync-detect", B),
    ("sync-tap", B),
    ("sync-unit", C),
    ("sync-undo", C),
    ("sync-here", C),
    ("sync-nudge", C),
    ("tempo", B),
    ("text", M),
    ("time", M),
    ("toggle", M),
    ("track", M),
    ("track-add", M),
    ("track-delete", M),
    ("track-instrument", B),
    ("track-rename", B),
    ("track-rename-start", B),
    ("track-next", M),
    ("track-prev", M),
    ("trainer", B),
    ("transpose", M),
    ("tuning", M),
    ("tuplet", M),
    ("undo", B),
    ("undo-newer", B),
    ("undo-older", B),
    ("view", B),
    ("view-close", B),
    ("view-close!", B),
    ("view-new", B),
    ("view-next", B),
    ("view-prev", B),
    ("view-rename", B),
    ("visual", B),
    ("visual-reselect", B),
    ("visual-bar", B),
    ("visual-insert", B),
    ("visual-append", B),
    ("voice", M),
    ("voice-next", M),
    ("voice-prev", M),
    ("volume", B),
    ("vsplit", B),
    ("w", B),
    ("waveform", B),
    ("waveform-strip", B),
    ("workspace-delete", B),
    ("workspace-delete!", B),
    ("workspace-open", B),
    ("workspace-open!", B),
    ("workspace-save", B),
    ("workspace-save!", B),
    ("wq", B),
    ("write", B),
    ("write-quit", B),
    ("yank", B),
    ("scroll-bottom", B),
    ("scroll-center", B),
    ("scroll-top", B),
    ("zoom-in", B),
    ("zoom-out", B),
    ("zoom-reset", B),
    ("chant-shape", C),
    ("chant-division", C),
    ("chant-clef", C),
    ("chant-mora", C),
    ("chant-episema", C),
    ("chant-ictus", C),
    ("chant-alter", C),
    ("lyrics-select", C),
    ("chant-liquescence", C),
    ("chant-symbol", C),
    ("score-kind", B),
];

/// Every command name (see [`COMMAND_SCOPES`]).
pub fn command_names() -> impl Iterator<Item = &'static str> {
    COMMAND_SCOPES.iter().map(|(n, _)| *n)
}

/// The scope of a command by name (aliases and names the table doesn't list are `Both`).
pub fn scope(name: &str) -> Scope {
    COMMAND_SCOPES
        .iter()
        .find(|(n, _)| *n == name)
        .map_or(Scope::Both, |(_, s)| *s)
}

/// Whether a command line's command may run in a score of the given kind; the error otherwise
/// names the kind it is not for (`capo: not in a chant score`).
pub fn check_scope(name: &str, chant_score: bool) -> Result<(), String> {
    if scope(name).applies(chant_score) {
        return Ok(());
    }
    Err(format!(
        "{name}: not in a {} score",
        if chant_score { "chant" } else { "modern" }
    ))
}

/// File extensions cecilia opens.
const SCORE_EXTENSIONS: &[&str] = &[
    "gp3", "gp4", "gp5", "gpx", "gp", "mid", "midi", "musicxml", "xml", "mxl", "gabc", "cc",
];
/// Extensions `:w` writes (scores, engravings, audio).
const WRITE_EXTENSIONS: &[&str] = &[
    "gp5", "gpx", "gp", "mid", "musicxml", "mxl", "gabc", "cc", "pdf", "svg", "png", "wav", "flac",
];

/// Fixed argument values per command.
fn values(command: &str) -> Option<&'static [&'static str]> {
    Some(match command {
        "layout" => &["horizontal", "page", "vertical"],
        "mix" => &["audio", "both", "midi"],
        "interpretation" => &["equalist", "solesmes"],
        "new" | "enew" => &["modern", "chant"],
        "clef" => &["treble", "bass", "grand", "neutral"],
        "global-tool" => &["fretboard", "piano", "tracks", "tuner", "waveform"],
        "track-add" => &["bass", "drums", "guitar", "piano", "voice"],
        "notation" => &["standard", "tab", "tab+", "slash", "chant"],
        "lyrics-attach" => &["above", "below", "none"],
        "staff-add" => &[
            "chant", "slash", "bass", "grand", "neutral", "treble", "tab", "tab+",
        ],
        "accidental" => &["double-flat", "double-sharp", "flat", "natural", "sharp"],
        "bar-repeat" => &["1", "2", "off"],
        "dynamic-step" => &["+1", "-1"],
        "beams" => &["auto", "break", "break-secondary", "force"],
        "stems" => &["auto", "down", "up"],
        "ending" => &["1", "2", "3", "1 2", "none"],
        "key" => &[
            "C", "G", "D", "A", "E", "B", "F#", "C#", "F", "Bb", "Eb", "Ab", "Db", "Gb", "Cb",
            "Am", "Em", "Bm", "F#m", "C#m", "G#m", "D#m", "A#m", "Dm", "Gm", "Cm", "Fm", "Bbm",
            "Ebm",
        ],
        "time" => &[
            "2/2", "2/4", "3/4", "4/4", "5/4", "6/4", "3/8", "5/8", "6/8", "7/8", "9/8", "12/8",
        ],
        "direction" => &[
            "fine",
            "segno",
            "double-segno",
            "coda",
            "double-coda",
            "da-capo",
            "da-capo-al-coda",
            "da-capo-al-double-coda",
            "da-capo-al-fine",
            "dal-segno",
            "dal-segno-al-coda",
            "dal-segno-al-double-coda",
            "dal-segno-al-fine",
            "dal-double-segno",
            "dal-double-segno-al-coda",
            "dal-double-segno-al-double-coda",
            "dal-double-segno-al-fine",
            "da-coda",
            "da-double-coda",
        ],
        "chant-bar" => &[",", ":", "::", ";"],
        "chant-division" => &[
            ",", ";", ":", "::", ",?", ";1", ";2", ";3", ";4", ";5", ";6", "`", "``",
        ],
        "chant-clef" => &["c1", "c2", "c3", "c4", "cb3", "f1", "f2", "f3", "f4"],
        "chant-shape" => &[
            "punctum",
            "inclinatum",
            "virga",
            "virga-reversa",
            "bivirga",
            "trivirga",
            "stropha",
            "bistropha",
            "tristropha",
            "quilisma",
            "oriscus",
            "oriscus-stem",
            "quadratum",
        ],
        "sync-unit" => &["syllable", "note"],
        "chant-alter" => &["flat", "natural"],
        "chant-liquescence" => &["deminutus", "ascendens", "descendens"],
        "chant-symbol" => &["*", "+", "†", "℣", "℟"],
        "volume" => &["audio", "metronome", "midi"],
        "pick" => &[
            "buffer",
            "keymap",
            "macro",
            "mark",
            "recent",
            "section",
            "view",
            "workspace",
        ],
        "tracks" | "toolbar" | "equalize-on-close" | "tabline-wrap" | "palette" | "fretboard"
        | "piano" | "tuner" => &["off", "on"],
        "focus-ring-color" => &["default"],
        "track-color" => &["off"],
        "design" => &["off", "on", "toggle"],
        "bar-width" | "bars-per-line" => &["auto"],
        "beat-nudge" => &["reset"],
        "bar-numbers" => &["every", "system", "toggle"],
        "completion-ignore-case"
        | "completion-fuzzy"
        | "incomplete-bars"
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
            "arpeggio-down",
            "arpeggio-up",
            "artificial-harmonic",
            "bend",
            "crescendo",
            "decrescendo",
            "fade-out",
            "fermata",
            "grace-before",
            "grace-on",
            "left-hand-tap",
            "pick-down",
            "pick-up",
            "rasgueado",
            "slide-in-above",
            "slide-in-below",
            "slide-legato",
            "slide-out-down",
            "slide-out-up",
            "volume-swell",
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

/// The clefs `:clef` takes on a chant score.
const CHANT_CLEFS: &[&str] = &[
    "c1", "c2", "c3", "c4", "cb2", "cb3", "cb4", "f1", "f2", "f3", "f4",
];

/// Candidates for the word being typed at the end of `line`. `score_dir` is the folder of the
/// current score (for `:audio`).
pub fn complete(line: &str, cwd: &Path, score_dir: Option<&Path>, m: Matcher) -> Completions {
    complete_in(line, cwd, score_dir, m, None)
}

/// [`complete`] for a kind of score: with `Some(chant_score)` only the commands (and the
/// arguments) of that kind of score and of both are offered.
pub fn complete_in(
    line: &str,
    cwd: &Path,
    score_dir: Option<&Path>,
    m: Matcher,
    chant_score: Option<bool>,
) -> Completions {
    let offered = |name: &str| chant_score.is_none_or(|k| scope(name).applies(k));
    let lead = line.len() - line.trim_start_matches([':', ' ']).len();
    let body = &line[lead..];
    let Some(space) = body.find(char::is_whitespace) else {
        let mut names: Vec<String> = command_names()
            .filter(|c| offered(c))
            .map(String::from)
            .collect();
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
        "w" | "write" | "save-as" | "saveas" => paths(arg, cwd, Some(WRITE_EXTENSIONS), m),
        "workspace-open" | "workspace-open!" | "workspace-save" | "workspace-save!"
        | "workspace-delete" | "workspace-delete!" => {
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
            let list = if word_start == 0 && offered(command) {
                match (command, chant_score) {
                    // A chant score's clefs are the do and fa clefs.
                    ("clef", Some(true)) => Some(CHANT_CLEFS),
                    ("clef", None) => values(command),
                    _ => values(command),
                }
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
    fn every_command_has_one_scope() {
        let mut seen = std::collections::HashSet::new();
        for (name, _) in COMMAND_SCOPES {
            assert!(seen.insert(*name), "{name} is listed twice");
        }
        // Every name the parser matches is in the table (or is the alias of one that is).
        let src = include_str!("command.rs");
        let parse = &src[src.find("pub fn parse(").expect("Command::parse")..];
        let mut missing = Vec::new();
        for line in parse.lines() {
            let line = line.trim_start();
            if !line.starts_with('"') {
                continue;
            }
            let Some(arm) = line.split(" =>").next().filter(|_| line.contains(" =>")) else {
                continue;
            };
            let names: Vec<&str> = arm.split('|').map(|n| n.trim().trim_matches('"')).collect();
            // Commands for keymaps only (the pane commands) are not listed and apply to both.
            let internal = |n: &str| {
                n.len() <= 1
                    || n == "master"
                    || ["palette-", "properties-", "tracks-"]
                        .iter()
                        .any(|p| n.starts_with(p))
            };
            if !names.iter().any(|n| internal(n))
                && !names
                    .iter()
                    .any(|n| COMMAND_SCOPES.iter().any(|(c, _)| c == n))
            {
                missing.push(names[0].to_string());
            }
        }
        assert!(missing.is_empty(), "commands with no scope: {missing:?}");
    }

    #[test]
    fn commands_of_the_other_kind_error_naming_it_and_are_not_offered() {
        let cwd = std::env::temp_dir();
        let names =
            |chant| -> Vec<String> { complete_in("", &cwd, None, PREFIX, Some(chant)).items };
        let (modern, chant) = (names(false), names(true));
        assert!(modern.iter().any(|c| c == "capo") && !modern.iter().any(|c| c == "chant-mora"));
        assert!(chant.iter().any(|c| c == "chant-mora") && !chant.iter().any(|c| c == "capo"));
        assert!(modern.iter().any(|c| c == "write") && chant.iter().any(|c| c == "write"));
        assert_eq!(
            check_scope("capo", true).unwrap_err(),
            "capo: not in a chant score"
        );
        assert_eq!(
            check_scope("chant-mora", false).unwrap_err(),
            "chant-mora: not in a modern score"
        );
        assert!(check_scope("capo", false).is_ok() && check_scope("write", true).is_ok());
        // Arguments follow the kind: a chant score is offered do and fa clefs.
        let clefs = |chant| complete_in("clef ", &cwd, None, PREFIX, Some(chant)).items;
        assert!(clefs(true).contains(&"c3".to_string()) && !clefs(true).contains(&"treble".into()));
        assert!(
            clefs(false).contains(&"treble".to_string()) && !clefs(false).contains(&"c3".into())
        );
    }

    #[test]
    fn every_listed_command_parses() {
        for name in command_names() {
            let err = Command::parse(name).err().unwrap_or_default();
            assert!(!err.starts_with("unknown command"), "{name}: {err}");
        }
    }

    #[test]
    fn completes_names_values_and_paths() {
        let dir = std::env::temp_dir().join(format!("cecilia-complete-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("songs")).unwrap();
        for f in ["a.gp5", "b.txt", "songs/c.gabc"] {
            std::fs::write(dir.join(f), b"").unwrap();
        }
        let c = complete(":sync-", &dir, None, PREFIX);
        assert_eq!(
            c.items,
            [
                "sync-clear",
                "sync-detect",
                "sync-here",
                "sync-nudge",
                "sync-tap",
                "sync-undo",
                "sync-unit"
            ]
        );
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
        "append" => "New beat after the cursor, then insert mode",
        "append-bar-end" => "New beat at the end of the bar, then insert mode",
        "audio" => "Attach an audio file to the score (off detaches)",
        "bar" | "goto-bar" => "Go to bar number",
        "bar-delete" => "Delete the cursor bar",
        "bar-end" => "Cursor to the last beat of the bar",
        "bar-first-note" => "Cursor to the first beat of the bar with a note",
        "bar-last-note" => "Cursor to the last beat of the bar with a note",
        "bar-open-after" => "New bar after the cursor, then insert mode",
        "bar-open-before" => "New bar before the cursor, then insert mode",
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
        "chant-to-modern" => "Open the chant as a modern score in a new buffer",
        "clef" => "Clef of the staff: treble, bass, grand, neutral (standard); do, fa (chant)",
        "close" | "pane-close" => "Close the pane",
        "command-mode" => "Open the command line",
        "completion-ignore-case" => "Toggle case-insensitive completion",
        "bar-numbers" => "Number every bar, or only the first bar of each line",
        "incomplete-bars" => "Toggle red staff lines on bars that lack their full beat count",
        "completion-fuzzy" => "Toggle fuzzy completion (letters in order, anywhere)",
        "count-in" => "Toggle the count-in before playing",
        "delete" => "Delete operator",
        "delete-note" => "Delete the note under the cursor",
        "delete-beat" => "Delete the whole beat under the cursor",
        "reflow" => {
            "Redraw the barlines of the voice to fit the time signatures: shift notes between bars (the Visual selection, else every bar)"
        }
        "dot" => "Toggle a dot on the beat",
        "duration" => "Set the note value (1, 2, 4 … 64)",
        "duration-longer" => "Double the beat's length",
        "duration-shorter" => "Halve the beat's length",
        "earlier" | "undo-older" => "Go to the older undo state (across branches)",
        "enew" | "new" => "New empty score: modern (the default) or chant",
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
        "insert" => "New beat before the cursor, then insert mode",
        "insert-bar-start" => "New beat at the start of the bar, then insert mode",
        "insert-enter" => "Insert mode: put a note at the cursor, next beat",
        "insert-here" => "Insert mode on the cursor beat, adding nothing",
        "insert-last" => "Insert mode where it was last left",
        "insert-next" => "Insert after the cursor beat",
        "interpretation" => "Chant rhythm: equalist or solesmes",
        "key" => "Key signature from the cursor bar: sharps / flats (2, -3) or a name (G, Bb, Am)",
        "free-time" => "Toggle free time on the bar",
        "double-bar" => "Toggle a double bar line at the end of the bar",
        "dynamic-step" => {
            "Louder (+1) or softer (-1) by a dynamic: the note, or the Visual selection"
        }
        "double-dot" => "Toggle a double dot on the beat",
        "bar-repeat" => {
            "Repeat the previous bar (1) or two bars (2) with a repeat sign; off removes it"
        }
        "ending" => "Alternate endings of the bar: numbers (1, 1 2, 2-3) or none",
        "direction" => "Toggle a direction over the bar: da-capo, dal-segno-al-coda, fine, coda …",
        "beams" => "Beaming of the beat or selection: auto, force, break or break-secondary",
        "stems" => "Stem direction of the beat or selection: auto, up or down",
        "later" | "undo-newer" => "Go to the newer undo state (across branches)",
        "layout" => "Score layout of this pane",
        "loop" => "Loop bars a–b",
        "loop-toggle" => "Loop the selection or cursor bar",
        "lyrics" => "Lyrics from the cursor on (lyrics; \\n a line break)",
        "lyrics-attach" => "Attach lyrics to the cursor staff: above|below [track], or none",
        "notation" => "Notation of the cursor's staff (standard, tab, tab+, slash, chant)",
        "lyrics-clear" => "Remove the lyrics, or one verse row of them",
        "metronome" => "Toggle the metronome",
        "mix" => "Mix: both, audio or midi",
        "modern-to-chant" => "Open the track's melody as chant",
        "mute" => "Toggle muting a track",
        "nop" => "Do nothing",
        "normal" => "Back to Normal mode",
        "note" => "Set the cursor note's pitch (e.g. C#4)",
        "note-next" => "Next note",
        "note-prev" => "Previous note",
        "note-row-next" => "Next note on the cursor's string or line (not tied)",
        "note-row-end" => "End of the tie, or the next note on the string or line",
        "note-row-prev" => "Start of the tie, or the previous note on the string or line",
        "octave-down" => "Note down an octave",
        "octave-up" => "Note up an octave",
        "open" => "Open a file (no file: the system file picker)",
        "save-as" => {
            "Write to a new file and make it the buffer's file (no file: the system dialog)"
        }
        "open-dialog" => "Choose files to open in the system file picker",
        "page-down" => "Move the cursor half a screen forward",
        "page-up" => "Move the cursor half a screen back",
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
        "equalize-on-close" => {
            "Equalize the remaining panes of a split when one closes (on, off or toggle)"
        }
        "focus-ring-width" => "Thickness of the focus ring in pixels (0 to 8)",
        "focus-ring-color" => "Color of the focus ring: #rrggbb or default (the mode's color)",
        "tabline-wrap" => "Wrap buffer next/prev around the tabline ends (on, off or toggle)",
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
        "palette-first" => "Palette: select the first button (top)",
        "palette-last" => "Palette: select the last button (bottom)",
        "palette-down" => "Palette: select the button below",
        "palette-up" => "Palette: select the button above",
        "palette-left" => "Palette: select the button to the left",
        "palette-right" => "Palette: select the button to the right",
        "palette-activate" => "Palette: apply the selected item (a section: fold or unfold)",
        "palette-search" => "Palette: search the tools by name, tooltip or section",
        "palette-search-next" => "Palette: jump to the next match of the / register",
        "palette-search-prev" => "Palette: jump to the previous match of the / register",
        "palette-fold" => "Palette: fold (close), unfold (open) or toggle the selected section",
        "dynamic" => "Set the dynamic of the note: ppp, pp, p, mp, mf, f, ff or fff",
        "properties" => "Show or hide the properties pane (score, tracks, lyrics)",
        "properties-focus" => "Focus the properties pane (again: back to the score)",
        "properties-tab" => "Show a tab of the properties pane: score, tracks or lyrics",
        "properties-tab-prev" => "Show the previous tab of the properties pane",
        "properties-down" => "Properties pane: select the next property",
        "properties-up" => "Properties pane: select the previous property",
        "properties-center" => "Properties pane: scroll so the selected property is in the middle",
        "properties-first" => "Properties pane: select the first property (top)",
        "properties-last" => "Properties pane: select the last property (bottom)",
        "properties-edit" => "Properties pane: edit the selected property",
        "properties-fold" => {
            "Properties pane: fold (close), unfold (open) or toggle the selected section"
        }
        "properties-staff-move" => "Properties pane: move the selected staff down (+1) or up (-1)",
        "properties-box" => {
            "Properties pane: on a staff, select the notation box (-1) or the clef box (+1)"
        }
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
        "pick-recent" => "Pick a recently closed file to open",
        "pick-workspace" => "Pick a saved workspace to open",
        "pitch" => "Shift the recording's pitch (semitones)",
        "pitch-down" => "Note down a semitone",
        "pitch-up" => "Note up a semitone",
        "play" => "Play from the cursor",
        "play-macro" => "Replay a recorded macro",
        "play-toggle" => "Play / pause",
        "put" => "Paste after the cursor",
        "put-before" => "Paste before the cursor",
        "q" | "qa" | "quit" | "quitall" => "Quit cecilia (not while files have unsaved changes)",
        "q!" | "qa!" | "quit!" | "quitall!" => "Quit cecilia, discarding changes",
        "wa" | "wall" => "Write every changed file",
        "wqa" | "wqall" | "xa" | "xall" => "Write every changed file and quit",
        "recover" => "Restore the file from its swap file",
        "recover!" => "Restore from a swap file even if another cecilia owns it",
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
        "sync-tap" => "Sync point at the playing bar (chant: at the next syllable or note)",
        "sync-unit" => "Chant: tap one syllable or one note at a time",
        "sync-undo" => "Chant: take back the last sync tap",
        "sync-here" => "Chant: sync the cursor note at the playhead",
        "sync-nudge" => "Chant: move the sync point under the cursor by some milliseconds",
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
        "staff-add" => "Add a staff at the bottom of the track",
        "staff-remove" => "Remove a staff of the track (1 = top; the cursor's by default)",
        "staff-move" => "Move the cursor's staff to a position (1 = top), or by +N / -N",
        "note-letter" => "Enter the note of a letter (a-g) nearest the cursor on a standard staff",
        "accidental" => "Make the cursor note sharp, flat or natural",
        "slash" => "Put a slash at the cursor beat of a slash staff",
        "chant-mod" => "Toggle a GABC modifier of the cursor chant note",
        "chant-bar" => "Add a division after the cursor syllable",
        "chant-append" => "Add a chant note after the cursor note",
        "chant-break" => "Add a chant note after the cursor note, starting a new neume",
        "chant-syllable" => "Chant: a new syllable after the cursor syllable",
        "chant-shape" => "Give the cursor chant note a head (punctum, virga, quilisma, oriscus …)",
        "chant-division" => "Toggle a division after the cursor syllable (, ; : :: and variants)",
        "chant-clef" => "Set the chant's first clef (c1–c4, f1–f4, cb3)",
        "chant-mora" => "Toggle the mora (lengthening dot) of the cursor chant note",
        "chant-episema" => "Toggle the horizontal episema of the cursor chant note",
        "chant-ictus" => "Toggle the ictus of the cursor chant note",
        "chant-alter" => "Toggle a flat or natural before the cursor chant note",
        "lyrics-select" => "Select the cursor note's GABC in the lyrics text area (Visual mode)",
        "chant-liquescence" => "Toggle a liquescence of the cursor chant note",
        "chant-symbol" => "Add a symbol (* + † ℣ ℟) to the cursor syllable's text",
        "score-kind" => "Say whether this is a modern or a chant score, and how to convert it",
        "track-next" => "Next track",
        "track-prev" => "Previous track",
        "trainer" => "Speed trainer: start, target, step",
        "transpose" => "Transpose the track (semitones)",
        "tuning" => "Retune the track, keeping pitches",
        "tuplet" => "Make the beat a tuplet",
        "undo" => "Undo",
        "visual" => "Visual mode (beats)",
        "visual-reselect" => "Reselect the last visual selection",
        "visual-bar" => "Visual mode (bars)",
        "visual-insert" => "Visual mode: new beat before the selection, then insert mode",
        "visual-append" => "Visual mode: new beat after the selection, then insert mode",
        "voice" => "Edit voice 1–4 of the bar (created empty when new)",
        "voice-next" => "Edit the next voice of the bar",
        "voice-prev" => "Edit the previous voice of the bar",
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
        "design" => "Design mode: adjust the page layout by hand (on, off or toggle)",
        "bar-width" => "Width of the cursor bar in the page layout (N, +N, -N percent, or auto)",
        "bars-per-line" => "Bars on the cursor's line in the page layout (N, +N, -N, or auto)",
        "line-break" => "Start a new line at the cursor bar in the page layout",
        "line-join" => "Bring the cursor bar back onto the previous line in the page layout",
        "beat-nudge" => {
            "Move the cursor beat sideways in the page layout (tenths of a space, or reset)"
        }
        "design-reset" => "Discard every manual page-layout adjustment",
        "wq" | "x" | "write-quit" => "Write every changed file and quit",
        "workspace-save" => "Save the session as a workspace (a new name duplicates it)",
        "workspace-save!" => "Save the session as a workspace, overwriting one of that name",
        "workspace-open" => "Save this workspace, then switch to another (created if new)",
        "workspace-open!" => "Switch to another workspace without saving this one",
        "workspace-delete" => "Delete a workspace (the active one without a name), after asking",
        "workspace-delete!" => "Delete a workspace without asking",
        "yank" => "Yank operator",
        "scroll-bottom" => "Scroll so the cursor is at the bottom of the view",
        "scroll-center" => "Scroll so the cursor is in the middle of the view",
        "scroll-top" => "Scroll so the cursor is at the top of the view",
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
        for name in super::command_names() {
            assert!(super::describe(name).is_some(), "{name}");
        }
    }
}
