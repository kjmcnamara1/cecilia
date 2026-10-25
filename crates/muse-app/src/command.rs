//! Commands: the single vocabulary shared by keymaps, the `:` command line and frontends.

use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    Left,
    Down,
    Up,
    Right,
}

/// Operators that act on a motion or selection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Operator {
    Delete,
    Yank,
    Change,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Command {
    // Editing
    Insert,
    Append,
    DeleteNote,
    Operator(Operator),
    Put {
        before: bool,
    },
    Undo,
    Redo,
    UndoOlder,
    UndoNewer,
    Repeat,
    RecordMacro,
    PlayMacro,
    Register,
    Visual {
        bars: bool,
    },
    BarInsert {
        after: bool,
    },
    BarDelete,
    DurationLonger,
    DurationShorter,
    /// Note value by denominator (1 = whole ... 64).
    Duration(u32),
    Dot,
    Tuplet(u8),
    PitchUp,
    PitchDown,
    OctaveUp,
    OctaveDown,
    Rest,
    Toggle(String),
    /// Insert mode: next beat, appending one at the end of the bar.
    InsertNext,
    /// Insert a new beat after the cursor.
    BeatNew,
    /// Set the cursor note's pitch ("C#4" or a MIDI number).
    Note(String),
    Time(u8, u8),
    Key(i8),
    Tempo(u32),
    RepeatStart,
    RepeatEnd(Option<u8>),
    Section(Option<String>),
    Text(Option<String>),
    Tuning(String),
    Capo(u8),
    Refinger,
    Transpose(i32),
    TrackAdd(String),
    TrackDelete,
    New,
    Voice(usize),
    /// Playback speed in percent.
    Speed(u32),
    // Play-along
    /// Attach an audio file (None = best match next to the score; "off" detaches).
    Audio(Option<String>),
    /// Mixer source: "both", "audio" or "midi" (None cycles).
    MixMode(Option<String>),
    Metronome,
    CountIn,
    /// Change playback speed by percentage points.
    SpeedStep(i32),
    /// Loop the visual selection or current bar (toggles off when looping).
    LoopToggle,
    /// Loop bars (1-based, inclusive), or None to stop looping.
    Loop(Option<(usize, usize)>),
    /// Speed trainer: start %, target %, step %.
    Trainer(Option<(u32, u32, u32)>),
    /// Pitch-shift the audio track (semitones).
    AudioPitch(f32),
    /// Start (or stop) tapping sync points from the cursor bar.
    Sync,
    SyncTap,
    SyncClear,
    SyncDetect,
    /// Toggle the waveform strip of the focused pane.
    Waveform,
    /// The window-wide waveform bar: on, off, or toggle (None).
    WaveformGlobal(Option<bool>),
    /// Level of "midi", "audio" or "metronome" in percent.
    Volume(String, u32),
    Mute(Option<usize>),
    Solo(Option<usize>),
    /// Lyrics from the cursor on (verse 1–5, default 1), in Guitar Pro syntax: spaces and
    /// hyphens separate syllables, one per note.
    Lyrics(usize, String),
    /// Remove a verse of lyrics from the cursor track (all verses without a number).
    LyricsClear(Option<usize>),
    /// Whether command-line completion ignores case: on, off, or toggle (None).
    CompletionIgnoreCase(Option<bool>),
    /// Save the session as a workspace (by name or path; the active one without).
    WorkspaceSave(Option<String>),
    /// Replace the session with a saved workspace (`!` discards unsaved changes).
    WorkspaceOpen(String, bool),
    /// Stop saving the session to the active workspace.
    WorkspaceClose,
    /// Show the cursor syllable as GABC, or replace it (`Glo(fg)`, or just notes `fgh`).
    Gabc(Option<String>),
    /// Chant rhythm: "equalist" or "solesmes" (None shows it).
    Interpretation(Option<String>),
    /// Initial clef of a chant track (`c4`, `cb3`, `f3`); notes keep their staff positions.
    ChantClef(String),
    /// Turn a chant track into an ordinary staff track (its transcription).
    ChantToModern,
    /// Open the cursor track's melody as chant in a new buffer.
    ModernToChant,
    /// Toggle square notation / modern transcription for chant tracks.
    ChantView,
    BeatNext,
    BeatPrev,
    StringDown,
    StringUp,
    NoteNext,
    NotePrev,
    BarNext,
    BarPrev,
    BarStart,
    BarEnd,
    /// Go to the first bar, or to bar `count` when a count is given.
    GotoFirst,
    /// Go to the last bar, or to bar `count` when a count is given.
    GotoLast,
    GotoBar(usize),
    TrackNext,
    TrackPrev,
    Track(usize),
    PageDown,
    PageUp,
    PlayToggle,
    Play,
    Pause,
    Stop,
    FollowToggle,
    Split,
    VSplit,
    Focus(Direction),
    ClosePane,
    TabNext,
    TabPrev,
    TabNew(Option<PathBuf>),
    TabClose,
    Edit(PathBuf),
    /// Write the buffer, to its own file or (exporting) to another path.
    Write(Option<PathBuf>),
    WriteQuit,
    Quit {
        force: bool,
    },
    QuitAll {
        force: bool,
    },
    Source,
    CommandMode,
    Normal,
    Help,
    /// Score view layout: "page", "vertical" or "horizontal"; None cycles.
    Layout(Option<String>),
    ZoomIn,
    ZoomOut,
    ZoomReset,
    Nop,
}

impl Command {
    /// Parses a command as written in keymaps (`bar-next`, `goto-bar 3`) or on the command line
    /// (`e file.gp5`, `q!`, `12`).
    pub fn parse(input: &str) -> Result<Command, String> {
        let input = input.trim().trim_start_matches(':').trim();
        if input.is_empty() {
            return Ok(Command::Nop);
        }
        if let Ok(n) = input.parse::<usize>() {
            return Ok(Command::GotoBar(n));
        }
        let (name, arg) = match input.split_once(char::is_whitespace) {
            Some((n, a)) => (n, Some(a.trim()).filter(|a| !a.is_empty())),
            None => (input, None),
        };
        let number = |what: &str| -> Result<usize, String> {
            arg.ok_or_else(|| format!("{name}: missing {what}"))?
                .parse::<usize>()
                .map_err(|_| format!("{name}: invalid {what}"))
        };
        let path = || -> Result<PathBuf, String> {
            arg.map(expand_tilde)
                .ok_or_else(|| format!("{name}: missing file name"))
        };
        use Command::*;
        Ok(match name {
            "beat-next" => BeatNext,
            "beat-prev" => BeatPrev,
            "string-down" => StringDown,
            "string-up" => StringUp,
            "note-next" => NoteNext,
            "note-prev" => NotePrev,
            "bar-next" => BarNext,
            "bar-prev" => BarPrev,
            "bar-start" => BarStart,
            "bar-end" => BarEnd,
            "goto-first" => GotoFirst,
            "goto-last" => GotoLast,
            "goto-bar" | "bar" => GotoBar(number("bar number")?),
            "track-next" => TrackNext,
            "track-prev" => TrackPrev,
            "track" => Track(number("track number")?),
            "page-down" => PageDown,
            "page-up" => PageUp,
            "play-toggle" => PlayToggle,
            "play" => Play,
            "pause" => Pause,
            "stop" => Stop,
            "follow-toggle" => FollowToggle,
            "split" | "sp" => Split,
            "vsplit" | "vs" => VSplit,
            "focus-left" => Focus(Direction::Left),
            "focus-down" => Focus(Direction::Down),
            "focus-up" => Focus(Direction::Up),
            "focus-right" => Focus(Direction::Right),
            "close-pane" | "close" | "clo" => ClosePane,
            "tab-next" | "tabnext" | "tabn" => TabNext,
            "tab-prev" | "tabprevious" | "tabp" => TabPrev,
            "tabnew" | "tabe" | "tabedit" => TabNew(arg.map(expand_tilde)),
            "tabclose" | "tabc" => TabClose,
            "open" => Edit(path()?),
            "write" | "w" => Write(arg.map(expand_tilde)),
            "wq" | "x" | "write-quit" => WriteQuit,
            "quit" | "q" => Quit { force: false },
            "quit!" | "q!" => Quit { force: true },
            "quitall" | "qa" => QuitAll { force: false },
            "quitall!" | "qa!" => QuitAll { force: true },
            "source" | "so" => Source,
            "command-mode" => CommandMode,
            "normal" => Normal,
            "help" | "h" => Help,
            "layout" => match arg {
                None => Layout(None),
                Some(a @ ("page" | "vertical" | "horizontal")) => Layout(Some(a.to_string())),
                Some(a) => {
                    return Err(format!(
                        "layout: unknown layout '{a}' (page, vertical, horizontal)"
                    ));
                }
            },
            "zoom-in" => ZoomIn,
            "zoom-out" => ZoomOut,
            "zoom-reset" => ZoomReset,
            "nop" => Nop,
            "insert" => Insert,
            "append" => Append,
            "delete-note" => DeleteNote,
            "delete" => Operator(self::Operator::Delete),
            "yank" => Operator(self::Operator::Yank),
            "change" => Operator(self::Operator::Change),
            "put" => Put { before: false },
            "put-before" => Put { before: true },
            "undo" | "u" => Undo,
            "redo" | "red" => Redo,
            "undo-older" | "earlier" => UndoOlder,
            "undo-newer" | "later" => UndoNewer,
            "repeat" => Repeat,
            "record-macro" => RecordMacro,
            "play-macro" => PlayMacro,
            "register" => Register,
            "visual" => Visual { bars: false },
            "visual-bar" => Visual { bars: true },
            "bar-insert-after" => BarInsert { after: true },
            "bar-insert-before" => BarInsert { after: false },
            "bar-delete" => BarDelete,
            "duration-longer" => DurationLonger,
            "duration-shorter" => DurationShorter,
            "duration" => {
                let n = number("note value")?;
                if ![1, 2, 4, 8, 16, 32, 64, 128].contains(&n) {
                    return Err(format!("duration: {n} is not 1, 2, 4 ... 128"));
                }
                Duration(n as u32)
            }
            "dot" => Dot,
            "tuplet" => Tuplet(arg.and_then(|a| a.parse().ok()).unwrap_or(3)),
            "pitch-up" => PitchUp,
            "pitch-down" => PitchDown,
            "octave-up" => OctaveUp,
            "octave-down" => OctaveDown,
            "rest" => Rest,
            "toggle" => Toggle(arg.ok_or("toggle: missing technique")?.to_string()),
            "insert-next" => InsertNext,
            "beat-new" => BeatNew,
            "note" => Note(arg.ok_or("note: missing pitch (e.g. C#4)")?.to_string()),
            "time" => {
                let a = arg.ok_or("time: missing signature (e.g. 3/4)")?;
                let (n, d) = a.split_once('/').ok_or("time: expected N/D")?;
                let (n, d) = (
                    n.trim().parse::<u8>().map_err(|_| "time: bad numerator")?,
                    d.trim()
                        .parse::<u8>()
                        .map_err(|_| "time: bad denominator")?,
                );
                if n == 0 || ![1, 2, 4, 8, 16, 32].contains(&d) {
                    return Err("time: invalid signature".into());
                }
                Time(n, d)
            }
            "key" => Key(arg
                .and_then(|a| a.parse().ok())
                .filter(|k: &i8| (-7..=7).contains(k))
                .ok_or("key: expected sharps (1..7) or flats (-1..-7)")?),
            "tempo" => Tempo(number("tempo")? as u32),
            "repeat-start" => RepeatStart,
            "repeat-end" => RepeatEnd(arg.and_then(|a| a.parse().ok())),
            "section" => Section(arg.map(String::from)),
            "text" => Text(arg.map(String::from)),
            "tuning" => Tuning(
                arg.ok_or("tuning: missing tuning (e.g. drop-d or 'D A D G B E')")?
                    .to_string(),
            ),
            "capo" => Capo(number("fret")? as u8),
            "refinger" => Refinger,
            "transpose" => Transpose(
                arg.and_then(|a| a.parse().ok())
                    .ok_or("transpose: expected semitones")?,
            ),
            "track-add" => TrackAdd(arg.unwrap_or("guitar").to_string()),
            "track-delete" => TrackDelete,
            "new" | "enew" => New,
            "voice" => Voice(number("voice")?),
            "speed" => Speed(number("percent")? as u32),
            "audio" => Audio(arg.map(String::from)),
            "mix" => MixMode(arg.map(String::from)),
            "metronome" => Metronome,
            "count-in" | "countin" => CountIn,
            "speed-up" => SpeedStep(5),
            "speed-down" => SpeedStep(-5),
            "loop-toggle" => LoopToggle,
            "loop" => match arg {
                None => LoopToggle,
                Some("off") => Loop(None),
                Some(a) => {
                    let parts: Vec<usize> = a
                        .split_whitespace()
                        .filter_map(|x| x.parse().ok())
                        .collect();
                    match parts[..] {
                        [x] => Loop(Some((x, x))),
                        [x, y] => Loop(Some((x.min(y), x.max(y)))),
                        _ => return Err("loop: expected bars, e.g. :loop 5 8".into()),
                    }
                }
            },
            "trainer" => match arg {
                None | Some("off") => Trainer(None),
                Some(a) => {
                    let p: Vec<u32> = a
                        .split_whitespace()
                        .filter_map(|x| x.parse().ok())
                        .collect();
                    match p[..] {
                        [s, t, step] => Trainer(Some((s, t, step))),
                        [s, t] => Trainer(Some((s, t, 5))),
                        _ => return Err("trainer: expected START TARGET [STEP] in percent".into()),
                    }
                }
            },
            "pitch" => AudioPitch(
                arg.and_then(|a| a.parse().ok())
                    .ok_or("pitch: expected semitones")?,
            ),
            "sync" => Sync,
            "sync-tap" => SyncTap,
            "sync-clear" => SyncClear,
            "sync-detect" => SyncDetect,
            "waveform" => Waveform,
            "waveform-global" => WaveformGlobal(match arg {
                None | Some("toggle") => None,
                Some("on" | "true" | "yes") => Some(true),
                Some("off" | "false" | "no") => Some(false),
                Some(other) => {
                    return Err(format!(
                        "waveform-global: expected on, off or toggle, not '{other}'"
                    ));
                }
            }),
            "volume" => {
                let a = arg.ok_or("volume: expected SOURCE PERCENT (midi, audio, metronome)")?;
                let (src, pct) = a.split_once(' ').ok_or("volume: expected SOURCE PERCENT")?;
                Volume(
                    src.to_string(),
                    pct.trim().parse().map_err(|_| "volume: bad percent")?,
                )
            }
            "lyrics" => {
                let a = arg.ok_or("lyrics: expected [verse] TEXT")?;
                match a.split_once(' ') {
                    Some((n, rest)) if n.parse::<usize>().is_ok_and(|n| (1..=5).contains(&n)) => {
                        Lyrics(n.parse::<usize>().unwrap() - 1, rest.to_string())
                    }
                    _ => Lyrics(0, a.to_string()),
                }
            }
            "lyrics-clear" => LyricsClear(
                arg.map(|a| {
                    a.trim()
                        .parse::<usize>()
                        .map_err(|_| "lyrics-clear: expected a verse number")
                })
                .transpose()?
                .map(|n| n.saturating_sub(1)),
            ),
            "completion-ignore-case" => CompletionIgnoreCase(match arg {
                None | Some("toggle") => None,
                Some("on" | "true" | "yes") => Some(true),
                Some("off" | "false" | "no") => Some(false),
                Some(other) => {
                    return Err(format!(
                        "completion-ignore-case: expected on, off or toggle, not '{other}'"
                    ));
                }
            }),
            "workspace-save" => WorkspaceSave(arg.map(String::from)),
            "workspace-open" | "workspace-open!" => WorkspaceOpen(
                arg.ok_or("workspace-open: expected a workspace name or path")?
                    .to_string(),
                name.ends_with('!'),
            ),
            "workspace-close" => WorkspaceClose,
            "gabc" => Gabc(arg.map(String::from)),
            "interpretation" => Interpretation(arg.map(String::from)),
            "clef" => ChantClef(arg.ok_or("clef: expected c1–c4, f1–f4 or cb3")?.to_string()),
            "chant-to-modern" => ChantToModern,
            "modern-to-chant" => ModernToChant,
            "chant-view" => ChantView,
            "mute" => Mute(arg.and_then(|a| a.parse().ok())),
            "solo" => Solo(arg.and_then(|a| a.parse().ok())),
            _ => return Err(format!("unknown command: {name}")),
        })
    }

    /// Whether a numeric count repeats this command.
    pub fn repeatable(&self) -> bool {
        use Command::*;
        matches!(
            self,
            BeatNext
                | BeatPrev
                | StringDown
                | StringUp
                | NoteNext
                | NotePrev
                | BarNext
                | BarPrev
                | TrackNext
                | TrackPrev
                | PageDown
                | PageUp
                | TabNext
                | TabPrev
                | PitchUp
                | PitchDown
                | DurationLonger
                | DurationShorter
                | InsertNext
                | DeleteNote
        )
    }

    /// Whether the command changes the score (and can be repeated with `.`).
    pub fn is_change(&self) -> bool {
        use Command::*;
        matches!(
            self,
            Insert
                | Append
                | DeleteNote
                | Operator(_)
                | Put { .. }
                | BarInsert { .. }
                | BarDelete
                | DurationLonger
                | DurationShorter
                | Duration(_)
                | Dot
                | Tuplet(_)
                | PitchUp
                | PitchDown
                | OctaveUp
                | OctaveDown
                | Rest
                | Toggle(_)
                | Note(_)
        )
    }
}

fn expand_tilde(p: &str) -> PathBuf {
    match p.strip_prefix("~/") {
        Some(rest) => std::env::var_os("HOME")
            .map(PathBuf::from)
            .unwrap_or_default()
            .join(rest),
        None => PathBuf::from(p),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_keymap_and_ex_forms() {
        assert_eq!(Command::parse("bar-next"), Ok(Command::BarNext));
        assert_eq!(Command::parse(":12"), Ok(Command::GotoBar(12)));
        assert_eq!(Command::parse("q!"), Ok(Command::Quit { force: true }));
        assert_eq!(
            Command::parse("open song.gp5"),
            Ok(Command::Edit("song.gp5".into()))
        );
        assert_eq!(Command::parse("tabnew"), Ok(Command::TabNew(None)));
        assert_eq!(
            Command::parse("w out.gp"),
            Ok(Command::Write(Some("out.gp".into())))
        );
        assert_eq!(Command::parse("w"), Ok(Command::Write(None)));
        assert!(Command::parse("e").is_err());
        assert!(Command::parse("frobnicate").is_err());
    }
}
