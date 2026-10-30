//! Commands: the single vocabulary shared by keymaps, the `:` command line and frontends.

use std::path::PathBuf;

/// A size given as `N` (to N) or `+N` / `-N` (by N).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Amount {
    To(u32),
    By(i32),
}

impl Amount {
    pub fn parse(s: &str) -> Option<Self> {
        let s = s.trim().trim_end_matches('%');
        if let Some(d) = s.strip_prefix('+') {
            return d.parse().ok().map(Amount::By);
        }
        if s.starts_with('-') {
            return s.parse().ok().map(Amount::By);
        }
        s.parse().ok().map(Amount::To)
    }

    /// The new size from the current one.
    pub fn apply(self, now: i64) -> i64 {
        match self {
            Amount::To(n) => n as i64,
            Amount::By(d) => now + d as i64,
        }
    }
}

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
    /// Move to the next / previous bar that starts a section (`]s`, `[s`).
    SectionNext,
    SectionPrev,
    Text(Option<String>),
    Tuning(String),
    Capo(u8),
    Refinger,
    Transpose(i32),
    TrackAdd(String),
    TrackDelete,
    /// Show the cursor's staff another way: `standard [treble|bass|grand [split]]`, `tab`,
    /// `tab+`, `slash` or `chant`. The notes stay the same.
    Staff(String),
    /// Add a staff at the bottom of the cursor's track (standard when not given).
    StaffAdd(Option<String>),
    /// Remove a staff of the cursor's track (1 = top; the cursor's when not given).
    StaffRemove(Option<usize>),
    /// Move the cursor's staff to a position (1 = top) or by `+N` / `-N`.
    StaffMove(String),
    /// On a standard staff: the note of this letter (`c` … `b`) nearest the cursor.
    NoteLetter(char),
    /// On a standard staff: the cursor note sharp (+1), flat (-1) or natural (0).
    Accidental(i32),
    /// On a slash staff: a slash at the cursor beat.
    Slash,
    /// On chant: the cursor note's GABC pitch letter (upper case: punctum inclinatum).
    ChantNote(char),
    /// On chant: toggles a GABC modifier of the cursor note (`v`, `s`, `~`, `.`, `_`, `'`, `-` …).
    ChantMod(String),
    /// On chant: a division (`,` `;` `:`; `:` again makes `::`) after the cursor syllable.
    ChantBar(String),
    /// On chant: a new note after the cursor note, in the same neume.
    ChantAppend,
    /// On chant: a new note after the cursor note, starting a new neume.
    ChantBreak,
    /// On chant: a new syllable after the cursor syllable.
    ChantSyllable,
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
    /// Show or hide the audio (waveform) pane.
    AudioPane(Option<bool>),
    /// Toggle whether a tool pane is global (`ui.global_tools`).
    GlobalTool(String),
    /// Show or hide the progress bar of the score panes.
    ProgressBar(Option<bool>),
    /// Level of "midi", "audio" or "metronome" in percent.
    Volume(String, u32),
    /// Mute / solo a track (the cursor's when not given), the audio track or the synth master
    /// (all synth tracks).
    Mute(Option<Strip>),
    Solo(Option<Strip>),
    /// Show or hide the tracks panel (no argument: toggle).
    Tracks(Option<bool>),
    /// Move the shown file's tab in the tabline: to position N, or by +N / -N.
    BufferMove(Amount),
    /// Show or hide the fretboard / piano panel (no argument: toggle).
    Fretboard(Option<bool>),
    Piano(Option<bool>),
    /// Show or hide the tuner (it listens to the default audio input while shown).
    Tuner(Option<bool>),
    /// Show or hide the toolbar (no argument: toggle).
    Toolbar(Option<bool>),
    /// Show or hide the properties pane (no argument: toggle).
    Properties(Option<bool>),
    /// Focus the properties pane, or give the focus back to the score.
    PropertiesFocus,
    /// In the properties pane: select the row below / above.
    PropertiesDown,
    PropertiesUp,
    /// Show a tab of the properties pane: `score`, `tracks` or `lyrics`; none for the next.
    PropertiesTab(Option<crate::properties::PropTab>),
    PropertiesTabPrev,
    /// Edit the selected property (a header: fold its section).
    PropertiesEdit,
    /// Fold or unfold the selected property's section.
    PropertiesFold,
    /// Clear the selected property (a verse's syllables, a track's own colour).
    PropertiesClear,
    /// Focus the tracks panel, or give the focus back to the score.
    TracksFocus,
    /// In the tracks panel: select the row below / above (tracks, the audio track, the master).
    TracksDown,
    TracksUp,
    /// In the tracks panel: show the selected track in the pane.
    TracksOpen,
    /// Rename a track: `NAME [n]` (the cursor's track without a number).
    TrackRename(String),
    /// Rename the tracks pane's selected track (or the cursor's) in place.
    TrackRenameStart,
    /// Set a track's instrument: `PROGRAM [n]` (General MIDI 1–128, or `drums`); without an
    /// argument, a picker of instruments.
    TrackInstrument(Option<String>),
    /// Volume of the cursor's track.
    TrackVolume(crate::tracks::VolumeChange),
    /// Colour of the cursor's track in the tracks panel: `#rrggbb`, `off`, or none for the
    /// next colour of the theme's palette.
    TrackColor(Option<String>),
    /// Lyrics from the cursor on (verse 1–5, default 1), in Guitar Pro syntax: spaces and
    /// hyphens separate syllables, one per note.
    Lyrics(usize, String),
    /// Remove a verse of lyrics from the cursor track (all verses without a number).
    LyricsClear(Option<usize>),
    /// Whether command-line completion ignores case: on, off, or toggle (None).
    CompletionIgnoreCase(Option<bool>),
    /// Whether command-line completion is fuzzy: on, off, or toggle (None).
    CompletionFuzzy(Option<bool>),
    /// Save the session as a workspace (by name or path; the active one without).
    WorkspaceSave(Option<String>),
    /// Replace the session with a saved workspace (`!` discards unsaved changes).
    WorkspaceOpen(String, bool),
    /// Stop saving the session to the active workspace.
    WorkspaceClose,
    /// Set a mark: await its letter (`m`).
    SetMark,
    /// Jump to a mark: await its letter (`'` to its bar, `` ` `` exactly).
    JumpMark {
        exact: bool,
    },
    /// Jump to a mark's exact position (`:goto-mark a`).
    GotoMark(char),
    /// Play the macro in a register (`:run-macro a`).
    RunMacro(char),
    /// Set mark `name` at the cursor (`:mark a`).
    Mark(char),
    /// Delete marks by letter, or all lowercase ones of the file (`:delmarks!`).
    DeleteMarks(Option<String>),
    /// Open a picker: buffers, views or workspaces.
    Pick(crate::picker::PickerKind),
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
    /// Height (stacked) or width (side by side) of the focused pane, in percent of its split;
    /// in the tracks panel, the rows it shows.
    Resize(crate::layout::Axis, Amount),
    /// Every pane of the view the same size.
    ResizeEqual,
    /// Show the focused pane alone, or the view's panes again.
    Maximize,
    /// Rows of the tracks panel.
    TracksResize(Amount),
    /// Height of the waveform strips and bar, in pixels.
    WaveformResize(Amount),
    VSplit,
    Focus(Direction),
    /// Move the focused pane to the far side of the view, spanning it (`<C-w>H J K L`).
    PaneMove(Direction),
    /// Swap the focused pane with the next one of its split (`<C-w>x`).
    PaneExchange,
    /// Rotate the panes of the focused pane's split on, or back (`<C-w>r`, `<C-w>R`).
    PaneRotate(bool),
    ClosePane,
    ViewNext,
    ViewPrev,
    /// Go to view number (1-based).
    ViewGoto(usize),
    /// A new view (`tabline.new_view` says what it starts with), optionally opening a file.
    ViewNew(Option<PathBuf>),
    /// Close the active view (true: discard unsaved changes of buffers only it had).
    ViewClose(bool),
    ViewRename(Option<String>),
    BufferNext,
    BufferPrev,
    /// Show the buffer of bufferline tab number (1-based).
    BufferGoto(usize),
    /// Close a bufferline tab (number, or the focused pane's buffer); true discards changes.
    BufferClose(Option<usize>, bool),
    Edit(PathBuf),
    /// Choose files to open in the system file picker.
    OpenDialog,
    /// Write the buffer, to its own file or (exporting) to another path.
    Write(Option<PathBuf>),
    /// Write every changed file (`:wall`).
    WriteAll,
    /// Write every changed file, then quit (`:write-quit`).
    WriteQuit,
    /// Quit muse; refused while files have unsaved changes unless forced.
    Quit {
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

/// A row of the mixer: a track (0-based), the audio track or the synth master.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Strip {
    Track(usize),
    Audio,
    Master,
}

impl Strip {
    /// `3` (a track number), `audio` or `master`.
    pub fn parse(s: &str) -> Result<Strip, String> {
        match s.trim() {
            "audio" => Ok(Strip::Audio),
            "master" => Ok(Strip::Master),
            n => n
                .parse::<usize>()
                .ok()
                .filter(|&n| n > 0)
                .map(|n| Strip::Track(n - 1))
                .ok_or_else(|| format!("'{n}' is not a track number, audio or master")),
        }
    }
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
            "resize" | "res" | "vertical-resize" | "vresize" | "tracks-resize"
            | "waveform-resize" => {
                let amount = arg
                    .and_then(Amount::parse)
                    .ok_or_else(|| format!("{name}: give a size: N, +N or -N"))?;
                match name {
                    "resize" | "res" => Resize(crate::layout::Axis::Stacked, amount),
                    "tracks-resize" => TracksResize(amount),
                    "waveform-resize" => WaveformResize(amount),
                    _ => Resize(crate::layout::Axis::SideBySide, amount),
                }
            }
            "resize-equal" => ResizeEqual,
            "buffer-move" | "tab-move" => BufferMove(
                arg.and_then(Amount::parse)
                    .ok_or("buffer-move: give a position: N, +N or -N")?,
            ),
            "buffer-move-next" => BufferMove(Amount::By(1)),
            "buffer-move-prev" => BufferMove(Amount::By(-1)),
            "maximize" | "max" => Maximize,
            "vsplit" | "vs" => VSplit,
            "focus-left" => Focus(Direction::Left),
            "focus-down" => Focus(Direction::Down),
            "focus-up" => Focus(Direction::Up),
            "focus-right" => Focus(Direction::Right),
            "pane-move-left" => PaneMove(Direction::Left),
            "pane-move-down" => PaneMove(Direction::Down),
            "pane-move-up" => PaneMove(Direction::Up),
            "pane-move-right" => PaneMove(Direction::Right),
            "pane-exchange" => PaneExchange,
            "pane-rotate" => PaneRotate(true),
            "pane-rotate-back" => PaneRotate(false),
            "pane-close" | "close" | "clo" => ClosePane,
            "view-next" => ViewNext,
            "view-prev" => ViewPrev,
            "view" => ViewGoto(number("view number")?),
            "view-new" => ViewNew(arg.map(expand_tilde)),
            "view-close" => ViewClose(false),
            "view-close!" => ViewClose(true),
            "view-rename" => ViewRename(arg.map(String::from)),
            "buffer-next" | "bnext" | "bn" => BufferNext,
            "buffer-prev" | "bprevious" | "bprev" | "bp" => BufferPrev,
            "buffer" | "b" => BufferGoto(number("buffer tab number")?),
            "buffer-close" | "bdelete" | "bd" => {
                BufferClose(arg.and_then(|a| a.parse().ok()), false)
            }
            "buffer-close!" | "bdelete!" | "bd!" => {
                BufferClose(arg.and_then(|a| a.parse().ok()), true)
            }
            "open" => match arg {
                Some(_) => Edit(path()?),
                None => OpenDialog,
            },
            "open-dialog" => OpenDialog,
            "write" | "w" => Write(arg.map(expand_tilde)),
            "wall" | "wa" => WriteAll,
            "write-quit" | "wq" | "x" | "wqall" | "wqa" | "xall" | "xa" => WriteQuit,
            "quit" | "q" | "quitall" | "qa" => Quit { force: false },
            "quit!" | "q!" | "quitall!" | "qa!" => Quit { force: true },
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
            "section-next" => SectionNext,
            "section-prev" => SectionPrev,
            "pick-section" | "sections" => Pick(crate::picker::PickerKind::Section),
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
            "staff" => Staff(
                arg.ok_or("staff: expected standard [treble|bass|grand [split]], tab, tab+, slash or chant")?
                    .to_string(),
            ),
            "staff-add" => StaffAdd(arg.map(String::from)),
            "staff-remove" => StaffRemove(match arg {
                None => None,
                Some(a) => Some(a.parse().map_err(|_| "staff-remove: expected a staff number")?),
            }),
            "staff-move" => StaffMove(
                arg.ok_or("staff-move: expected a position, +N or -N")?
                    .to_string(),
            ),
            "note-letter" => NoteLetter(
                arg.and_then(|a| a.chars().next())
                    .filter(|c| "abcdefgABCDEFG".contains(*c))
                    .ok_or("note-letter: expected a letter a-g")?
                    .to_ascii_lowercase(),
            ),
            "accidental" => Accidental(match arg {
                Some("sharp") | Some("#") => 1,
                Some("flat") | Some("b") => -1,
                Some("natural") | Some("=") => 0,
                _ => return Err("accidental: expected sharp, flat or natural".into()),
            }),
            "slash" => Slash,
            "chant-note" => ChantNote(
                arg.and_then(|a| a.chars().next())
                    .filter(|c| muse_core::chant::letter_position(*c).is_some_and(|p| p <= 12))
                    .ok_or("chant-note: expected a GABC pitch letter a-m")?,
            ),
            "chant-mod" => ChantMod(
                arg.ok_or("chant-mod: expected a GABC modifier (v s o q w ~ . _ ' -)")?
                    .to_string(),
            ),
            "chant-bar" => ChantBar(
                arg.filter(|a| [",", ";", ":", "::"].contains(a))
                    .ok_or("chant-bar: expected , ; : or ::")?
                    .to_string(),
            ),
            "chant-append" => ChantAppend,
            "chant-break" => ChantBreak,
            "chant-syllable" => ChantSyllable,
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
            "waveform-strip" => Waveform,
            "waveform" | "progress-bar" => {
                let on = match arg {
                    None | Some("toggle") => None,
                    Some("on" | "true" | "yes") => Some(true),
                    Some("off" | "false" | "no") => Some(false),
                    Some(other) => {
                        return Err(format!("{name}: expected on, off or toggle, not '{other}'"));
                    }
                };
                if name != "progress-bar" {
                    AudioPane(on)
                } else {
                    ProgressBar(on)
                }
            }
            "global-tool" => GlobalTool(
                arg.ok_or("global-tool: expected tracks, waveform, fretboard, piano or tuner")?
                    .to_string(),
            ),
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
            "completion-fuzzy" => CompletionFuzzy(match arg {
                None | Some("toggle") => None,
                Some("on" | "true" | "yes") => Some(true),
                Some("off" | "false" | "no") => Some(false),
                Some(other) => {
                    return Err(format!(
                        "completion-fuzzy: expected on, off or toggle, not '{other}'"
                    ));
                }
            }),
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
            "set-mark" => SetMark,
            "goto-mark" => GotoMark(
                arg.and_then(|a| a.chars().next())
                    .ok_or("goto-mark: expected a mark letter")?,
            ),
            "run-macro" => RunMacro(
                arg.and_then(|a| a.chars().next())
                    .ok_or("run-macro: expected a register")?,
            ),
            "jump-mark" => JumpMark { exact: true },
            "jump-mark-bar" => JumpMark { exact: false },
            "mark" | "k" => Mark(
                arg.and_then(|a| a.chars().next())
                    .filter(|c| c.is_ascii_alphabetic())
                    .ok_or("mark: expected a letter (a-z local, A-Z global)")?,
            ),
            "delmarks" | "delm" => DeleteMarks(Some(
                arg.ok_or("delmarks: expected letters (or :delmarks!)")?
                    .to_string(),
            )),
            "delmarks!" | "delm!" => DeleteMarks(None),
            "marks" => Pick(crate::picker::PickerKind::Mark),
            "macros" | "registers" | "reg" => Pick(crate::picker::PickerKind::Macro),
            "pick-buffer" => Pick(crate::picker::PickerKind::Buffer),
            "pick-keymap" | "keymaps" | "maps" => Pick(crate::picker::PickerKind::Keymap),
            "pick-view" => Pick(crate::picker::PickerKind::View),
            "pick-workspace" => Pick(crate::picker::PickerKind::Workspace),
            "pick" => Pick(
                arg.and_then(crate::picker::PickerKind::parse)
                    .ok_or("pick: expected buffer, view or workspace")?,
            ),
            "gabc" => Gabc(arg.map(String::from)),
            "interpretation" => Interpretation(arg.map(String::from)),
            "clef" => ChantClef(arg.ok_or("clef: expected c1–c4, f1–f4 or cb3")?.to_string()),
            "chant-to-modern" => ChantToModern,
            "modern-to-chant" => ModernToChant,
            "chant-view" => ChantView,
            "mute" => Mute(arg.map(Strip::parse).transpose()?),
            "tracks" => Tracks(match arg {
                None => None,
                Some("on") => Some(true),
                Some("off") => Some(false),
                Some(a) => return Err(format!("tracks: '{a}' is not on or off")),
            }),
            "properties" => Properties(match arg {
                None => None,
                Some("on") => Some(true),
                Some("off") => Some(false),
                Some(a) => return Err(format!("properties: '{a}' is not on or off")),
            }),
            "properties-focus" => PropertiesFocus,
            "properties-down" => PropertiesDown,
            "properties-up" => PropertiesUp,
            "properties-tab" => PropertiesTab(match arg {
                None => None,
                Some(a) => Some(
                    crate::properties::PropTab::parse(a)
                        .ok_or_else(|| format!("properties-tab: '{a}' is not score, tracks or lyrics"))?,
                ),
            }),
            "properties-tab-prev" => PropertiesTabPrev,
            "properties-edit" => PropertiesEdit,
            "properties-fold" => PropertiesFold,
            "properties-clear" => PropertiesClear,
            "tracks-focus" => TracksFocus,
            "tracks-down" => TracksDown,
            "tracks-up" => TracksUp,
            "tracks-open" => TracksOpen,
            "track-rename" => TrackRename(
                arg.ok_or("track-rename: expected NAME [track number]")?
                    .to_string(),
            ),
            "track-rename-start" => TrackRenameStart,
            "track-instrument" => TrackInstrument(arg.map(String::from)),
            "fretboard" | "piano" | "tuner" => {
                let on = match arg {
                    None => None,
                    Some("on") => Some(true),
                    Some("off") => Some(false),
                    Some(a) => return Err(format!("{name}: '{a}' is not on or off")),
                };
                match name {
                    "piano" => Piano(on),
                    "tuner" => Tuner(on),
                    _ => Fretboard(on),
                }
            }
            "toolbar" => Toolbar(match arg {
                None => None,
                Some("on") => Some(true),
                Some("off") => Some(false),
                Some(a) => return Err(format!("toolbar: '{a}' is not on or off")),
            }),
            "track-volume" => TrackVolume(
                arg.and_then(crate::tracks::VolumeChange::parse)
                    .ok_or("track-volume: give a percentage (80, +10 or -10)")?,
            ),
            "track-volume-up" => TrackVolume(crate::tracks::VolumeChange::Up),
            "track-volume-down" => TrackVolume(crate::tracks::VolumeChange::Down),
            "track-color" | "track-colour" => TrackColor(arg.map(String::from)),
            "solo" => Solo(arg.map(Strip::parse).transpose()?),
            _ => return Err(format!("unknown command: {name}")),
        })
    }

    /// Whether a numeric count repeats this command.
    pub fn repeatable(&self) -> bool {
        use Command::*;
        matches!(
            self,
            BeatNext
                | TracksDown
                | TracksUp
                | PropertiesDown
                | PropertiesUp
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
                | ViewNext
                | ViewPrev
                | BufferNext
                | BufferPrev
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
                | NoteLetter(_)
                | Accidental(_)
                | Slash
                | ChantNote(_)
                | ChantMod(_)
                | ChantBar(_)
                | ChantAppend
                | ChantBreak
                | ChantSyllable
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
        assert_eq!(Command::parse("view-new"), Ok(Command::ViewNew(None)));
        assert_eq!(
            Command::parse("bd! 2"),
            Ok(Command::BufferClose(Some(2), true))
        );
        assert_eq!(
            Command::parse("w out.gp"),
            Ok(Command::Write(Some("out.gp".into())))
        );
        assert_eq!(Command::parse("w"), Ok(Command::Write(None)));
        assert!(Command::parse("e").is_err());
        assert!(Command::parse("frobnicate").is_err());
    }
}
