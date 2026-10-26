//! Editing behaviour of [`App`]: insert and visual modes, operators with motions, registers,
//! macros, `.` repeat, undo, and the score-editing commands.

use std::collections::HashMap;

use muse_core::*;

use crate::app::{beats, clamp_cursor};
use crate::command::{Command, Operator};
use crate::config::{Lookup, Mode};
use crate::edit::{self, BarClip, EditCx, Pos};
use crate::keys::{Key, KeyCode};
use crate::{App, Buffer, Cursor, Effect};

/// What a register holds.
#[derive(Debug, Clone)]
pub enum Clip {
    Beats(Vec<Beat>),
    Bars(BarClip),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Awaiting {
    Register,
    RecordMacro,
    PlayMacro,
    /// `m`: the mark's letter.
    Mark,
    /// `'` / `` ` ``: the mark to jump to.
    Jump {
        exact: bool,
    },
}

#[derive(Debug, Default)]
pub(crate) struct EditorState {
    /// Note value for new beats.
    pub insert_duration: Option<Duration>,
    operator: Option<(Operator, Option<usize>)>,
    pub visual_anchor: Option<Cursor>,
    pub visual_bars: bool,
    registers: HashMap<char, Clip>,
    register: Option<char>,
    awaiting: Option<Awaiting>,
    pub recording: Option<(char, Vec<Key>)>,
    pub(crate) macros: HashMap<char, Vec<Key>>,
    last_macro: Option<char>,
    /// Keys of the last change, for `.`.
    last_change: Vec<Key>,
    /// Keys of a change still being typed (an insert session).
    change_keys: Option<Vec<Key>>,
    /// Keys of the normal-mode command being typed.
    cmd_keys: Vec<Key>,
    replaying: u32,
    /// First digit of a possibly two-digit fret typed in insert mode.
    digit: Option<(Cursor, u8)>,
    /// Undo grouping of an insert session: Some(true) once its first change is recorded.
    pub(crate) group: Option<bool>,
}

fn is_motion(cmd: &Command) -> bool {
    use Command::*;
    matches!(
        cmd,
        BeatNext
            | BeatPrev
            | NoteNext
            | NotePrev
            | BarNext
            | BarPrev
            | BarStart
            | BarEnd
            | GotoFirst
            | GotoLast
            | GotoBar(_)
    )
}

/// The beat before a position, crossing to the previous bar's last beat.
fn previous(score: &Score, c: &Cursor) -> Option<Pos> {
    if c.beat > 0 {
        return Some((c.bar, c.beat - 1));
    }
    let bar = c.bar.checked_sub(1)?;
    let prev = Cursor { bar, ..*c };
    Some((bar, beats(score, &prev).len().saturating_sub(1)))
}

impl App {
    pub fn insert_duration(&self) -> Duration {
        self.ed.insert_duration.unwrap_or_default()
    }

    /// Visual selection: (anchor, cursor, whole bars).
    pub fn selection(&self) -> Option<(Cursor, Cursor, bool)> {
        (self.mode == Mode::Visual)
            .then(|| {
                self.ed
                    .visual_anchor
                    .map(|a| (a, self.pane().cursor, self.ed.visual_bars))
            })
            .flatten()
    }

    /// Register being recorded into, for display.
    pub fn recording_macro(&self) -> Option<char> {
        self.ed.recording.as_ref().map(|r| r.0)
    }

    pub(crate) fn dispatch_key(&mut self, key: Key) {
        if let Some((_, keys)) = self.ed.recording.as_mut() {
            keys.push(key);
        }
        let in_insert = self.mode == Mode::Insert;
        if in_insert && self.ed.replaying == 0 {
            if let Some(k) = self.ed.change_keys.as_mut() {
                k.push(key);
            }
        }
        let before = (self.focused(), self.pane().buffer, self.pane().cursor);
        match self.mode {
            Mode::Command => self.handle_command_key(key),
            Mode::Picker => self.handle_picker_key(key),
            Mode::Normal => self.normal_key(key),
            Mode::Insert => self.insert_key(key),
            Mode::Visual => self.visual_key(key),
            Mode::Tracks => self.tracks_key(key),
        }
        self.seek_to_cursor(before);
        self.peek_tracks(before);
        if in_insert && self.mode != Mode::Insert {
            self.ed.group = None;
            if let Some(k) = self.ed.change_keys.take() {
                self.ed.last_change = k;
            }
        }
    }

    fn normal_key(&mut self, key: Key) {
        if let Some(a) = self.ed.awaiting.take() {
            self.ed.cmd_keys.clear();
            if let Some(c) = key.as_char() {
                self.awaited(a, c);
            }
            return;
        }
        let close_help = self.show_help && self.pending.is_empty() && key.as_char() == Some('q');
        if key.code == KeyCode::Esc || close_help {
            self.show_help = false;
            self.pending.clear();
            self.count = None;
            self.ed.operator = None;
            self.ed.register = None;
            self.ed.cmd_keys.clear();
            return;
        }
        self.ed.cmd_keys.push(key);
        if self.pending.is_empty() {
            if let Some(d) = key.as_char().and_then(|c| c.to_digit(10)) {
                if d != 0 || self.count.is_some() {
                    self.count = Some(
                        self.count
                            .unwrap_or(0)
                            .saturating_mul(10)
                            .saturating_add(d as usize),
                    );
                    return;
                }
            }
        }
        self.pending.push(key);
        let cmds = match self.config.keymap(Mode::Normal).lookup(&self.pending) {
            Lookup::Exact(cmds) => cmds.to_vec(),
            Lookup::Prefix => return,
            Lookup::None => {
                self.pending.clear();
                self.count = None;
                self.ed.operator = None;
                self.ed.cmd_keys.clear();
                return;
            }
        };
        self.pending.clear();
        let count = self.count.take();
        self.message = None;
        let keys = std::mem::take(&mut self.ed.cmd_keys);
        let mut changed = false;
        for cmd in cmds {
            // An operator waiting for its motion.
            if let Some((op, op_count)) = self.ed.operator.take() {
                if cmd == Command::Operator(op) {
                    let n = op_count.unwrap_or(1) * count.unwrap_or(1);
                    self.operate_bars(op, n);
                    changed = true;
                    continue;
                }
                if is_motion(&cmd) {
                    let n = match (op_count, count) {
                        (None, None) => None,
                        (a, b) => Some(a.unwrap_or(1) * b.unwrap_or(1)),
                    };
                    self.operate_motion(op, cmd, n);
                    changed = true;
                    continue;
                }
            }
            if let Command::Operator(op) = cmd {
                self.ed.operator = Some((op, count));
                self.ed.cmd_keys = keys.clone();
                return;
            }
            changed |= cmd.is_change();
            // Commands that use the count themselves (p, o, u, @...) read it from self.count.
            let own_count =
                !cmd.repeatable() && !matches!(cmd, Command::GotoFirst | Command::GotoLast);
            if own_count {
                self.count = count;
            }
            self.run_counted(cmd, count);
            if self.ed.awaiting.is_none() {
                self.count = None;
            }
        }
        if changed && self.ed.replaying == 0 {
            if self.mode == Mode::Insert {
                self.ed.change_keys = Some(keys);
            } else {
                self.ed.last_change = keys;
            }
        }
    }

    /// Play mode: counts and the Play keymap (playback, navigation and view; no editing).
    fn awaited(&mut self, a: Awaiting, c: char) {
        match a {
            Awaiting::Mark => self.set_mark(c),
            Awaiting::Jump { exact } => self.jump_to_mark(c, exact),
            Awaiting::Register => {
                self.ed.register = Some(c);
                self.ed.cmd_keys = vec![Key::char('"'), Key::char(c)];
            }
            Awaiting::RecordMacro => {
                if c.is_ascii_uppercase() {
                    // An uppercase register appends to the lowercase one, as in Vim.
                    let reg = c.to_ascii_lowercase();
                    let keys = self.ed.macros.get(&reg).cloned().unwrap_or_default();
                    self.ed.recording = Some((reg, keys));
                    self.info(format!("recording @{reg} (appending)"));
                } else if c.is_ascii_alphanumeric() {
                    self.ed.recording = Some((c, Vec::new()));
                    self.info(format!("recording @{c}"));
                }
            }
            Awaiting::PlayMacro => {
                let reg = if c == '@' {
                    self.ed.last_macro
                } else {
                    Some(c.to_ascii_lowercase())
                };
                let Some(reg) = reg else {
                    return self.error("no previous macro");
                };
                let Some(keys) = self.ed.macros.get(&reg).cloned() else {
                    return self.error(format!("register {reg} is empty"));
                };
                self.ed.last_macro = Some(reg);
                let times = self.count.take().unwrap_or(1).min(1000);
                if self.ed.replaying > 20 {
                    return self.error("macro recursion too deep");
                }
                self.ed.replaying += 1;
                for _ in 0..times {
                    for &k in &keys {
                        self.dispatch_key(k);
                    }
                }
                self.ed.replaying -= 1;
            }
        }
    }

    fn insert_key(&mut self, key: Key) {
        // Digits type frets; two quick digits make a two-digit fret.
        if let Some(d) = key.as_char().and_then(|c| c.to_digit(10)) {
            let cursor = self.pane().cursor;
            let max: u32 = self
                .buffer()
                .score
                .tracks
                .get(cursor.track)
                .map_or(24, |t| {
                    if t.kind == TrackKind::Fretted {
                        t.fret_count.max(24) as u32
                    } else {
                        127
                    }
                });
            let value = match self.ed.digit {
                Some((c, first)) if c == cursor && first as u32 * 10 + d <= max => {
                    first as u32 * 10 + d
                }
                _ => d,
            };
            let duration = self.insert_duration();
            self.edit(|cx| edit::set_fret(cx, value as u8, duration));
            self.ed.digit = (value < 10).then_some((self.pane().cursor, value as u8));
            return;
        }
        self.ed.digit = None;
        // On drum tracks, letters enter instruments (k kick, s snare, h hi-hat, …).
        let cursor = self.pane().cursor;
        let drums = self
            .buffer()
            .score
            .tracks
            .get(cursor.track)
            .is_some_and(|t| t.kind == TrackKind::Drums);
        if drums && self.pending.is_empty() {
            if let Some(k) = key.as_char().and_then(muse_core::drums::key_for_letter) {
                let duration = self.insert_duration();
                self.edit(|cx| edit::toggle_drum(cx, k, duration));
                return;
            }
        }
        self.pending.push(key);
        let cmds = match self.config.keymap(Mode::Insert).lookup(&self.pending) {
            Lookup::Exact(cmds) => cmds.to_vec(),
            Lookup::Prefix => return,
            Lookup::None => {
                self.pending.clear();
                return;
            }
        };
        self.pending.clear();
        for cmd in cmds {
            self.run(cmd);
        }
    }

    fn visual_key(&mut self, key: Key) {
        if self.ed.awaiting.is_some() {
            return self.normal_key(key);
        }
        self.pending.push(key);
        let lookup = match self.config.keymap(Mode::Visual).lookup(&self.pending) {
            Lookup::Exact(c) => Some(c.to_vec()),
            Lookup::Prefix => return,
            Lookup::None => None,
        };
        let cmds = match lookup {
            Some(c) => c,
            None => match self.config.keymap(Mode::Normal).lookup(&self.pending) {
                Lookup::Exact(c) => c.to_vec(),
                Lookup::Prefix => return,
                Lookup::None => {
                    self.pending.clear();
                    return;
                }
            },
        };
        self.pending.clear();
        let count = self.count.take();
        for cmd in cmds {
            match cmd {
                Command::Operator(op) => self.operate_selection(op),
                Command::Visual { bars } if bars == self.ed.visual_bars => {
                    self.run(Command::Normal)
                }
                Command::Visual { bars } => self.ed.visual_bars = bars,
                c if is_motion(&c) || !c.is_change() => self.run_counted(c, count),
                _ => self.error("not available in visual mode"),
            }
        }
    }

    /// Runs an edit on the focused pane's buffer, recording it for undo.
    pub(crate) fn edit<R>(
        &mut self,
        f: impl FnOnce(&mut EditCx) -> Result<R, String>,
    ) -> Option<R> {
        let id = self.focused();
        let pane = self.panes[&id].clone();
        let buffer_index = pane.buffer;
        let buffer = &mut self.buffers[buffer_index];
        if buffer.score.tracks.is_empty() {
            self.error("no score to edit (use :new)");
            return None;
        }
        let mut cx = EditCx::new(&mut buffer.score, pane.cursor);
        let result = f(&mut cx);
        cx.sync_chant();
        let mut cursor = cx.cursor;
        if let Some(change) = cx.finish() {
            match self.ed.group {
                Some(true) => buffer.history.amend(change, cursor),
                _ => buffer.history.push(change, pane.cursor, cursor),
            }
            if self.ed.group.is_some() {
                self.ed.group = Some(true);
            }
            buffer.modified = true;
            buffer.version += 1;
            self.effects.push(Effect::ScoreChanged {
                buffer: buffer_index,
            });
        }
        clamp_cursor(&buffer.score, &mut cursor);
        self.panes.get_mut(&id).unwrap().cursor = cursor;
        match result {
            Ok(r) => Some(r),
            Err(e) => {
                self.error(e);
                None
            }
        }
    }

    fn store(&mut self, clip: Clip) {
        if let Some(r) = self.ed.register.take() {
            self.ed.registers.insert(r, clip.clone());
        }
        self.ed.registers.insert('"', clip);
    }

    fn operate_bars(&mut self, op: Operator, n: usize) {
        let c = self.pane().cursor;
        let (from, to) = (c.bar, c.bar + n.max(1) - 1);
        let clip = edit::yank_bars(&self.buffer().score, from, to);
        self.store(Clip::Bars(clip));
        match op {
            Operator::Yank => self.info(format!("{n} bar{} yanked", if n == 1 { "" } else { "s" })),
            Operator::Delete => {
                self.edit(|cx| edit::delete_bars(cx, from, to));
            }
            Operator::Change => {
                self.ed.group = Some(false);
                self.edit(|cx| {
                    let to = to.min(cx.score.master_bars.len() - 1);
                    for t in 0..cx.score.tracks.len() {
                        for b in from..=to {
                            cx.bar(t, b).voices = vec![Voice {
                                beats: vec![Beat {
                                    empty: true,
                                    ..Beat::default()
                                }],
                            }];
                        }
                    }
                    cx.cursor.bar = from;
                    cx.cursor.beat = 0;
                    Ok(())
                });
                self.enter_insert();
            }
        }
    }

    fn operate_motion(&mut self, op: Operator, motion: Command, count: Option<usize>) {
        let start = self.pane().cursor;
        self.run_counted(motion.clone(), count);
        let target = self.pane().cursor;
        self.panes.get_mut(&self.focused()).unwrap().cursor = start;
        let score = &self.buffer().score;
        let inclusive = matches!(motion, Command::BarEnd);
        let (s, t) = ((start.bar, start.beat), (target.bar, target.beat));
        let range = if t > s {
            let end = if inclusive {
                Some(t)
            } else {
                previous(score, &target)
            };
            end.map(|e| (s, e))
        } else if t < s {
            previous(score, &start).map(|e| (t, e))
        } else if inclusive {
            Some((s, s))
        } else {
            None
        };
        if let Some((from, to)) = range {
            self.operate_range(op, from, to);
        }
    }

    fn operate_selection(&mut self, op: Operator) {
        let Some((anchor, cursor, bars)) = self.selection() else {
            return;
        };
        self.run(Command::Normal);
        if bars {
            let (from, to) = (anchor.bar.min(cursor.bar), anchor.bar.max(cursor.bar));
            self.panes.get_mut(&self.focused()).unwrap().cursor.bar = from;
            return self.operate_bars(op, to - from + 1);
        }
        let (a, b) = ((anchor.bar, anchor.beat), (cursor.bar, cursor.beat));
        let (from, to) = if a <= b { (a, b) } else { (b, a) };
        self.operate_range(op, from, to);
    }

    fn operate_range(&mut self, op: Operator, from: Pos, to: Pos) {
        let c = self.pane().cursor;
        let beats = edit::collect(&self.buffer().score, c.track, c.voice, from, to);
        let n = beats.len();
        self.store(Clip::Beats(beats));
        match op {
            Operator::Yank => {
                self.info(format!("{n} beat{} yanked", if n == 1 { "" } else { "s" }));
                let p = self.panes.get_mut(&self.focused()).unwrap();
                p.cursor.bar = from.0;
                p.cursor.beat = from.1;
            }
            Operator::Delete => {
                self.edit(|cx| {
                    edit::delete_range(cx, from, to);
                    Ok(())
                });
            }
            Operator::Change => {
                let duration = self.insert_duration();
                self.ed.group = Some(false);
                self.edit(|cx| {
                    edit::delete_range(cx, from, to);
                    edit::insert_beat(cx, false, duration);
                    Ok(())
                });
                self.enter_insert();
            }
        }
    }

    fn put(&mut self, before: bool, count: usize) {
        let reg = self.ed.register.take().unwrap_or('"');
        let Some(clip) = self.ed.registers.get(&reg).cloned() else {
            return self.error(format!("register {reg} is empty"));
        };
        match clip {
            Clip::Beats(beats) => {
                let all: Vec<Beat> = (0..count.max(1)).flat_map(|_| beats.clone()).collect();
                self.edit(|cx| {
                    edit::put_beats(cx, &all, !before);
                    Ok(())
                });
            }
            Clip::Bars(bars) => {
                let at = self.pane().cursor.bar + usize::from(!before);
                self.edit(|cx| {
                    for _ in 0..count.max(1) {
                        edit::put_bars(cx, &bars, at);
                    }
                    Ok(())
                });
            }
        }
    }

    /// Runs editing commands; returns false for commands it does not handle.
    pub(crate) fn run_edit(&mut self, cmd: &Command) -> bool {
        use Command::*;
        let duration = self.insert_duration();
        match cmd {
            Insert => self.enter_insert(),
            Append => {
                self.ed.group = Some(false);
                self.edit(|cx| {
                    edit::insert_beat(cx, true, duration);
                    Ok(())
                });
                self.enter_insert();
            }
            InsertNext => {
                self.edit(|cx| {
                    edit::advance(cx, duration);
                    Ok(())
                });
            }
            BeatNew => {
                self.edit(|cx| {
                    edit::insert_beat(cx, true, duration);
                    Ok(())
                });
            }
            DeleteNote => {
                self.edit(|cx| {
                    edit::delete_note(cx);
                    Ok(())
                });
            }
            Operator(_) => {}
            Put { before } => {
                let n = self.count.take().unwrap_or(1);
                self.put(*before, n);
            }
            Undo | Redo | UndoOlder | UndoNewer => self.undo(cmd),
            Repeat => {
                let keys = self.ed.last_change.clone();
                if keys.is_empty() {
                    return true;
                }
                self.ed.replaying += 1;
                for k in keys {
                    self.dispatch_key(k);
                }
                if self.mode == Mode::Insert {
                    self.dispatch_key(crate::keys::Key::new(KeyCode::Esc));
                }
                self.ed.replaying -= 1;
            }
            RecordMacro => match self.ed.recording.take() {
                Some((reg, mut keys)) => {
                    keys.pop(); // the `q` that stopped recording
                    self.ed.macros.insert(reg, keys);
                    self.info(format!("recorded @{reg}"));
                }
                None => self.ed.awaiting = Some(Awaiting::RecordMacro),
            },
            PlayMacro => self.ed.awaiting = Some(Awaiting::PlayMacro),
            SetMark => self.ed.awaiting = Some(Awaiting::Mark),
            GotoMark(c) => self.jump_to_mark(*c, true),
            RunMacro(c) => self.awaited(Awaiting::PlayMacro, *c),
            JumpMark { exact } => self.ed.awaiting = Some(Awaiting::Jump { exact: *exact }),
            Register => self.ed.awaiting = Some(Awaiting::Register),
            Visual { bars } => {
                self.ed.visual_anchor = Some(self.pane().cursor);
                self.ed.visual_bars = *bars;
                self.mode = Mode::Visual;
            }
            BarInsert { after } => {
                let at = self.pane().cursor.bar + usize::from(*after);
                let n = self.count.take().unwrap_or(1);
                self.edit(|cx| {
                    edit::insert_bars(cx, at, n);
                    cx.cursor.bar = at;
                    cx.cursor.beat = 0;
                    Ok(())
                });
            }
            BarDelete => self.operate_bars(self::Operator::Delete, 1),
            DurationLonger | DurationShorter | Duration(_) | Dot | Tuplet(_) => {
                let cmd = cmd.clone();
                let new = self.edit(move |cx| {
                    Ok(edit::set_duration(cx, |mut d| {
                        match &cmd {
                            DurationLonger => d.value = d.value.longer(),
                            DurationShorter => d.value = d.value.shorter(),
                            Duration(n) => {
                                d.value = DurationValue::from_denominator(*n).unwrap_or(d.value)
                            }
                            Dot => d.dots = if d.dots == 0 { 1 } else { 0 },
                            Tuplet(n) => {
                                let times = match n {
                                    3 => 2,
                                    5..=7 => 4,
                                    _ => 8,
                                };
                                d.tuplet = if d.tuplet.is_some() {
                                    None
                                } else {
                                    Some(muse_core::Tuplet { enters: *n, times })
                                };
                            }
                            _ => {}
                        }
                        d
                    }))
                });
                if let Some(d) = new {
                    self.ed.insert_duration = Some(d);
                }
            }
            PitchUp | PitchDown | OctaveUp | OctaveDown => {
                let n = match cmd {
                    PitchUp => 1,
                    PitchDown => -1,
                    OctaveUp => 12,
                    _ => -12,
                };
                self.edit(|cx| edit::shift_pitch(cx, n));
            }
            Rest => {
                self.edit(|cx| {
                    edit::make_rest(cx, duration);
                    Ok(())
                });
            }
            Toggle(t) => {
                let t = t.clone();
                self.edit(|cx| edit::toggle(cx, &t, duration));
            }
            Note(p) => match edit::parse_pitch(p) {
                Some(pitch) => {
                    self.edit(|cx| edit::set_pitch(cx, pitch));
                }
                None => self.error(format!("note: cannot read pitch '{p}' (e.g. C#4, Bb2, 60)")),
            },
            Time(n, d) => {
                let ts = TimeSignature {
                    numerator: *n,
                    denominator: *d,
                };
                self.edit_master(move |bars, i| {
                    let old = bars[i].time_signature;
                    for mb in bars[i..].iter_mut().take_while(|m| m.time_signature == old) {
                        mb.time_signature = ts;
                    }
                });
            }
            Key(k) => {
                let k = *k;
                self.edit_master(move |bars, i| {
                    let old = bars[i].key_signature;
                    for mb in bars[i..].iter_mut().take_while(|m| m.key_signature == old) {
                        mb.key_signature.accidentals = k;
                    }
                });
            }
            Tempo(bpm) => {
                let bpm = (*bpm).clamp(10, 999);
                let bar = self.pane().cursor.bar;
                self.edit(|cx| {
                    let s = cx.whole();
                    if bar == 0 {
                        s.tempo = bpm;
                    } else {
                        let changes = &mut s.master_bars[bar].tempo_changes;
                        changes.retain(|c| c.tick != 0);
                        changes.insert(
                            0,
                            TempoChange {
                                tick: 0,
                                bpm: bpm as f64,
                            },
                        );
                    }
                    Ok(())
                });
            }
            RepeatStart => self.edit_master(|bars, i| bars[i].repeat_start = !bars[i].repeat_start),
            RepeatEnd(n) => {
                let n = *n;
                self.edit_master(move |bars, i| {
                    bars[i].repeat_end = if bars[i].repeat_end.is_some() && n.is_none() {
                        None
                    } else {
                        Some(n.unwrap_or(2).max(2))
                    }
                });
            }
            Section(name) => {
                let name = name.clone();
                self.edit_master(move |bars, i| {
                    bars[i].marker = name.map(|title| Marker {
                        title,
                        color: Color { r: 255, g: 0, b: 0 },
                    })
                });
            }
            Text(text) => {
                let text = text.clone();
                self.edit(|cx| {
                    let beat = cx.beat();
                    beat.text = text;
                    Ok(())
                });
            }
            Tuning(spec) => self.retune(spec),
            Capo(fret) => {
                let fret = *fret;
                self.edit(|cx| {
                    let t = cx.cursor.track;
                    let track = &mut cx.whole().tracks[t];
                    if track.kind != TrackKind::Fretted {
                        return Err("capo: not a fretted track".into());
                    }
                    let strings = track.strings.clone();
                    let (shifted, dropped) = muse_convert::retune(track, strings, fret);
                    report(shifted, dropped)
                });
            }
            Refinger => {
                self.edit(|cx| {
                    let t = cx.cursor.track;
                    let track = &mut cx.whole().tracks[t];
                    if track.kind != TrackKind::Fretted {
                        return Err("refinger: not a fretted track".into());
                    }
                    let n = track.bars.len();
                    let dropped = muse_convert::refinger(track, 0..n);
                    report(0, dropped)
                });
            }
            Transpose(n) => {
                let n = *n;
                self.edit(|cx| {
                    let t = cx.cursor.track;
                    let track = &mut cx.whole().tracks[t];
                    if track.kind == TrackKind::Fretted {
                        // Raise the open strings, then retune back: pitches move by n.
                        let strings = track.strings.clone();
                        track.strings = strings
                            .iter()
                            .map(|&p| (p as i32 + n).clamp(0, 127) as u8)
                            .collect();
                        let (shifted, dropped) = muse_convert::retune(track, strings, track.capo);
                        return report(shifted, dropped);
                    }
                    if track.kind == TrackKind::Pitched {
                        for note in track
                            .bars
                            .iter_mut()
                            .flat_map(|b| &mut b.voices)
                            .flat_map(|v| &mut v.beats)
                            .flat_map(|b| &mut b.notes)
                        {
                            note.fret = (note.fret as i32 + n).clamp(0, 127) as u8;
                        }
                    }
                    Ok(())
                });
            }
            Lyrics(verse, text) => {
                let (verse, syllables) = (*verse, muse_core::lyrics::syllables(text));
                let placed = self.edit(|cx| {
                    let c = cx.cursor;
                    let score = cx.whole();
                    let n = muse_core::lyrics::place(
                        &mut score.tracks[c.track],
                        verse,
                        c.bar,
                        c.beat,
                        &syllables,
                    );
                    refresh_lyric_lines(score, c.track);
                    Ok(n)
                });
                if let Some(n) = placed {
                    if n < syllables.len() {
                        self.error(format!(
                            "lyrics: {} syllables did not fit",
                            syllables.len() - n
                        ));
                    } else {
                        self.info(format!("{n} syllables on verse {}", verse + 1));
                    }
                }
            }
            LyricsClear(verse) => {
                let verse = *verse;
                self.edit(|cx| {
                    let t = cx.cursor.track;
                    let score = cx.whole();
                    for b in score.tracks[t]
                        .bars
                        .iter_mut()
                        .flat_map(|b| &mut b.voices)
                        .flat_map(|v| &mut v.beats)
                    {
                        match verse {
                            Some(v) => {
                                if let Some(s) = b.lyrics.get_mut(v) {
                                    s.clear();
                                }
                                while b.lyrics.last().is_some_and(String::is_empty) {
                                    b.lyrics.pop();
                                }
                            }
                            None => b.lyrics.clear(),
                        }
                    }
                    refresh_lyric_lines(score, t);
                    Ok(())
                });
            }
            TrackAdd(kind) => self.add_track(kind),
            TrackDelete => {
                self.edit(|cx| {
                    if cx.score.tracks.len() <= 1 {
                        return Err("cannot delete the only track".into());
                    }
                    let t = cx.cursor.track;
                    let tracks = &mut cx.whole().tracks;
                    let removed = tracks.remove(t);
                    // Staves of a deleted part's first track become their own part.
                    if !removed.extra_staff {
                        if let Some(next) = tracks.get_mut(t) {
                            next.extra_staff = false;
                        }
                    }
                    cx.cursor.track = t.saturating_sub(1);
                    Ok(())
                });
            }
            New => self.new_score(),
            Voice(n) => {
                let v = n.clamp(&1, &4) - 1;
                let p = self.panes.get_mut(&self.focused()).unwrap();
                p.cursor.voice = v;
                p.cursor.beat = 0;
                self.info(format!("voice {}", v + 1));
            }
            Speed(pct) => {
                self.config.file.audio.speed = (*pct as f64 / 100.0).clamp(0.1, 4.0);
                self.effects.push(crate::app::Effect::PlaybackSettings);
                self.info(format!("playback speed {pct}%"));
            }
            _ => return false,
        }
        true
    }

    fn enter_insert(&mut self) {
        if self.buffer().score.tracks.is_empty() {
            return self.error("no score to edit (use :new)");
        }
        self.mode = Mode::Insert;
        self.ed.digit = None;
        self.ed.group.get_or_insert(false);
    }

    fn edit_master(&mut self, f: impl FnOnce(&mut Vec<MasterBar>, usize)) {
        let bar = self.pane().cursor.bar;
        self.edit(|cx| {
            f(&mut cx.whole().master_bars, bar);
            Ok(())
        });
    }

    fn undo(&mut self, cmd: &Command) {
        let id = self.focused();
        let b = self.panes[&id].buffer;
        let n = if matches!(cmd, Command::Undo | Command::Redo) {
            self.count.take().unwrap_or(1)
        } else {
            1
        };
        let buffer = &mut self.buffers[b];
        let mut cursor = None;
        for _ in 0..n {
            let c = match cmd {
                Command::Undo => buffer.history.undo(&mut buffer.score),
                Command::Redo => buffer.history.redo(&mut buffer.score),
                Command::UndoOlder => buffer.history.older(&mut buffer.score),
                _ => buffer.history.newer(&mut buffer.score),
            };
            match c {
                Some(c) => cursor = Some(c),
                None => break,
            }
        }
        let Some(mut c) = cursor else {
            return self.info(if matches!(cmd, Command::Undo | Command::UndoOlder) {
                "already at oldest change"
            } else {
                "already at newest change"
            });
        };
        buffer.version += 1;
        buffer.modified = true;
        clamp_cursor(&buffer.score, &mut c);
        let seq = buffer.history.current_seq();
        self.panes.get_mut(&id).unwrap().cursor = c;
        self.effects.push(Effect::ScoreChanged { buffer: b });
        self.info(format!("change {seq}"));
    }

    fn retune(&mut self, spec: &str) {
        let strings = muse_convert::tuning_preset(spec).or_else(|| {
            // Note names from the lowest string up, e.g. "D2 A2 D3 G3 B3 E4".
            let pitches: Option<Vec<u8>> = spec.split_whitespace().map(edit::parse_pitch).collect();
            pitches
                .filter(|p| (1..=10).contains(&p.len()))
                .map(|p| p.into_iter().rev().collect())
        });
        let Some(strings) = strings else {
            return self.error(format!("tuning: unknown tuning '{spec}' (a preset like drop-d, or notes like 'D2 A2 D3 G3 B3 E4')"));
        };
        self.edit(|cx| {
            let t = cx.cursor.track;
            let track = &mut cx.whole().tracks[t];
            if track.kind == TrackKind::Drums {
                return Err("tuning: drum tracks have no tuning".into());
            }
            let capo = track.capo;
            let (shifted, dropped) = muse_convert::retune(track, strings, capo);
            report(shifted, dropped)
        });
    }

    fn add_track(&mut self, kind: &str) {
        let (name, kind, strings, program, channel) = match kind {
            "bass" => ("Bass", TrackKind::Fretted, vec![43, 38, 33, 28], 33, 2),
            "drums" => ("Drums", TrackKind::Drums, vec![0; 6], 0, 9),
            "piano" => ("Piano", TrackKind::Pitched, vec![], 0, 4),
            "voice" => ("Voice", TrackKind::Pitched, vec![], 52, 6),
            "guitar" => (
                "Guitar",
                TrackKind::Fretted,
                vec![64, 59, 55, 50, 45, 40],
                25,
                0,
            ),
            other => {
                return self.error(format!(
                    "track-add: unknown kind '{other}' (guitar, bass, drums, piano, voice)"
                ));
            }
        };
        let grand_staff = kind == TrackKind::Pitched && name == "Piano";
        self.edit(|cx| {
            let s = cx.whole();
            let bars = s.master_bars.len();
            let first = s.tracks.len();
            s.tracks.push(Track {
                name: name.into(),
                kind,
                strings,
                fret_count: 24,
                midi: MidiSettings {
                    program,
                    channel,
                    effect_channel: channel,
                    ..MidiSettings::default()
                },
                bars: vec![
                    Bar {
                        voices: vec![Voice {
                            beats: vec![Beat {
                                empty: true,
                                ..Beat::default()
                            }]
                        }]
                    };
                    bars
                ],
                clef: grand_staff.then_some(StaffClef::Treble),
                ..Track::default()
            });
            if grand_staff {
                // A piano gets a grand staff: a second, bass-clef staff in the same part.
                let mut lower = s.tracks[first].clone();
                lower.name = "Piano (2)".into();
                lower.extra_staff = true;
                lower.clef = Some(StaffClef::Bass);
                s.tracks.push(lower);
            }
            cx.cursor.track = first;
            cx.cursor.bar = 0;
            cx.cursor.beat = 0;
            Ok(())
        });
    }

    fn new_score(&mut self) {
        let empty = Bar {
            voices: vec![Voice {
                beats: vec![Beat {
                    empty: true,
                    ..Beat::default()
                }],
            }],
        };
        let score = Score {
            tempo: 120,
            meta: Metadata {
                title: "Untitled".into(),
                ..Metadata::default()
            },
            master_bars: vec![MasterBar::default(); 8],
            tracks: vec![Track {
                name: "Guitar".into(),
                strings: vec![64, 59, 55, 50, 45, 40],
                fret_count: 24,
                bars: vec![empty; 8],
                ..Track::default()
            }],
            ..Score::default()
        };
        self.buffers.push(Buffer {
            path: None,
            name: "[New]".into(),
            score,
            modified: true,
            history: Default::default(),
            version: 0,
        });
        let b = self.buffers.len() - 1;
        self.show_buffer(b);
        self.info("new score: i to insert, :w name.gp to save");
    }
}

/// Keeps a score's stored lyric lines (Guitar Pro 3–5 form) in step with the syllables of the
/// track they belong to.
fn refresh_lyric_lines(score: &mut Score, track: usize) {
    let lines = muse_core::lyrics::lines_from_beats(&score.tracks[track]);
    match score.lyrics.as_mut() {
        Some(l) if l.track == Some(track) || l.lines.is_empty() => {
            l.track = Some(track);
            l.lines = lines;
        }
        None if !lines.is_empty() => {
            score.lyrics = Some(Lyrics {
                track: Some(track),
                lines,
            });
        }
        _ => {}
    }
}

fn report(shifted: usize, dropped: usize) -> Result<(), String> {
    match (shifted, dropped) {
        (0, 0) => Ok(()),
        (s, 0) => Err(format!("{s} notes moved by an octave to fit")),
        (s, d) => Err(format!(
            "{s} notes moved by an octave, {d} notes did not fit and were removed"
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Config;

    fn app() -> App {
        let mut app = App::new(Config::default(), None);
        app.run(Command::New);
        app
    }

    fn keys(app: &mut App, s: &str) {
        for k in crate::keys::parse_keys(s, &[Key::char(' ')]).unwrap() {
            app.handle_key(k);
        }
    }

    fn frets(app: &App, bar: usize) -> Vec<Option<u8>> {
        app.buffer().score.tracks[0].bars[bar].voices[0]
            .beats
            .iter()
            .filter(|b| !b.empty)
            .map(|b| b.notes.first().map(|n| n.fret))
            .collect()
    }

    #[test]
    fn drum_letters_enter_instruments() {
        let mut a = app();
        a.run(Command::TrackAdd("drums".into()));
        keys(&mut a, "ikh<Space>h<Space>sh<Space>hh<Esc>");
        let keys_at = |a: &App, i: usize| -> Vec<u8> {
            let mut k: Vec<u8> = a.buffer().score.tracks[1].bars[0].voices[0].beats[i]
                .notes
                .iter()
                .map(|n| n.fret)
                .collect();
            k.sort_unstable();
            k
        };
        assert_eq!(keys_at(&a, 0), [36, 42]);
        assert_eq!(keys_at(&a, 1), [42]);
        assert_eq!(keys_at(&a, 2), [38, 42]);
        // Typing an instrument twice removes it again.
        assert_eq!(keys_at(&a, 3), Vec::<u8>::new());
    }

    #[test]
    fn lyrics_commands_and_grand_staff() {
        let mut a = app();
        keys(&mut a, "i3<Space>12<Space>5<Space>7<Esc>0");
        a.run(Command::Lyrics(0, "Hel-lo  world".into()));
        let lyr = |a: &App| -> Vec<String> {
            a.buffer().score.tracks[0].bars[0].voices[0]
                .beats
                .iter()
                .map(|b| b.lyrics.first().cloned().unwrap_or_default())
                .collect()
        };
        assert_eq!(lyr(&a), ["Hel-", "lo", "", "world"]);
        let lines = &a.buffer().score.lyrics.as_ref().unwrap().lines;
        assert_eq!(lines[0].text, "Hel-lo  world");
        keys(&mut a, "l");
        a.run(Command::Lyrics(1, "two".into()));
        assert_eq!(
            a.buffer().score.tracks[0].bars[0].voices[0].beats[1].lyrics,
            ["lo", "two"]
        );
        a.run(Command::LyricsClear(Some(0)));
        assert_eq!(lyr(&a), ["", "", "", ""]);
        keys(&mut a, "u");
        assert_eq!(lyr(&a)[0], "Hel-");

        // A piano is two staves of one part.
        a.run(Command::TrackAdd("piano".into()));
        let s = &a.buffer().score;
        assert_eq!(s.tracks.len(), 3);
        assert_eq!(s.part_tracks(2), 1..3);
        assert_eq!(s.tracks[2].clef, Some(StaffClef::Bass));
    }

    #[test]
    fn insert_typing_and_one_undo_step() {
        let mut a = app();
        keys(&mut a, "i3<Space>12<Space>5<Esc>");
        assert_eq!(frets(&a, 0), vec![Some(3), Some(12), Some(5)]);
        assert_eq!(a.mode, Mode::Normal);
        keys(&mut a, "u");
        assert_eq!(frets(&a, 0), Vec::<Option<u8>>::new());
        keys(&mut a, "U");
        assert_eq!(frets(&a, 0), vec![Some(3), Some(12), Some(5)]);
    }

    #[test]
    fn operators_motions_and_put() {
        let mut a = app();
        keys(&mut a, "i1<Space>2<Space>3<Space>4<Esc>0");
        keys(&mut a, "dl");
        assert_eq!(frets(&a, 0), vec![Some(2), Some(3), Some(4)]);
        keys(&mut a, "y$");
        keys(&mut a, "}P");
        assert_eq!(frets(&a, 1), vec![Some(2), Some(3), Some(4)]);
        keys(&mut a, "gg");
        keys(&mut a, "x");
        assert_eq!(frets(&a, 0)[0], None);
        // Repeat the deletion on the next beat.
        keys(&mut a, "l.");
        assert_eq!(frets(&a, 0)[1], None);
    }

    #[test]
    fn bars_visual_registers_and_macros() {
        let mut a = app();
        keys(&mut a, "i7<Esc>");
        keys(&mut a, "\"ayy");
        keys(&mut a, "3}\"aP");
        assert_eq!(frets(&a, 3), vec![Some(7)]);
        assert_eq!(a.buffer().score.master_bars.len(), 9);
        keys(&mut a, "dd");
        assert_eq!(a.buffer().score.master_bars.len(), 8);
        keys(&mut a, "ggVjd");
        assert_eq!(a.buffer().score.master_bars.len(), 7);
        // A macro that types a fret and moves on.
        keys(&mut a, "gg");
        keys(&mut a, "qqi5<Esc>}q");
        keys(&mut a, "2@q");
        assert_eq!(frets(&a, 1), vec![Some(5)]);
        assert_eq!(frets(&a, 2), vec![Some(5)]);
    }

    #[test]
    fn techniques_durations_and_score_commands() {
        let mut a = app();
        keys(&mut a, "i5h<A-4>b<Esc>");
        let beat = a.buffer().score.tracks[0].bars[0].voices[0].beats[0].clone();
        assert!(beat.notes[0].effects.hammer_pull);
        assert!(beat.notes[0].effects.bend.is_some());
        assert_eq!(beat.duration.value, DurationValue::Eighth);
        keys(&mut a, ":time 3/4<CR>:tempo 90<CR>:key -2<CR>");
        let s = &a.buffer().score;
        assert_eq!(
            s.master_bars[3].time_signature,
            TimeSignature {
                numerator: 3,
                denominator: 4
            }
        );
        assert_eq!(s.tempo, 90);
        assert_eq!(s.master_bars[7].key_signature.accidentals, -2);
        keys(&mut a, ":tuning drop-d<CR>");
        let t = &a.buffer().score.tracks[0];
        assert_eq!(t.strings[5], 38);
        assert_eq!(t.pitch_of(&t.bars[0].voices[0].beats[0].notes[0]), 69);
        keys(&mut a, "<C-a>");
        let t = &a.buffer().score.tracks[0];
        let n = &t.bars[0].voices[0].beats[0].notes[0];
        assert_eq!(t.pitch_of(n), 70);
        keys(&mut a, "g-g-g-");
        assert_eq!(a.buffer().score.tracks[0].strings[5], 40);
    }
}
