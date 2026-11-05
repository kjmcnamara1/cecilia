//! Editing behaviour of [`App`]: insert and visual modes, operators with motions, registers,
//! macros, `.` repeat, undo, and the score-editing commands.

use std::collections::HashMap;

use cecilia_core::chant::ClefKind;
use cecilia_core::*;

use crate::app::{beats, clamp_cursor};
use crate::command::{BeatAt, Command, Operator};
use crate::config::{InsertKind, Lookup, Mode};
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
    /// Per buffer: the last Visual selection (anchor, cursor, whole bars), for `gv` and `` `< ``.
    last_visual: HashMap<usize, (Cursor, Cursor, bool)>,
    /// Per buffer: where Insert mode was last left, for `gi` and `` `^ ``.
    last_insert: HashMap<usize, Cursor>,
    registers: HashMap<char, Clip>,
    register: Option<char>,
    pub(crate) awaiting: Option<Awaiting>,
    pub recording: Option<(char, Vec<Key>)>,
    pub(crate) macros: HashMap<char, Vec<Key>>,
    last_macro: Option<char>,
    /// Keys of the last change, for `.`.
    last_change: Vec<Key>,
    /// The last palette tool applied (or a visual range command), for `.`: the command and the
    /// size of the selection it was applied to (beats or bars).
    last_tool: Option<(Command, Option<(bool, usize)>)>,
    /// The tool, not `last_change`, is what `.` repeats.
    tool_last: bool,
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
            | RowNext
            | RowEnd
            | RowPrev
            | BarFirstNote
            | BarLastNote
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

    /// Records where Visual mode (`visual`, the selection before the key) or Insert mode
    /// (`insert`, the cursor before the key) was left by the key just handled, in `buffer`.
    fn remember_leaves(
        &mut self,
        buffer: usize,
        visual: Option<(Cursor, Cursor, bool)>,
        insert: Option<Cursor>,
    ) {
        if let Some(v) = visual.filter(|_| self.mode != Mode::Visual) {
            self.ed.last_visual.insert(buffer, v);
        }
        if let Some(c) = insert.filter(|_| self.mode != Mode::Insert) {
            self.ed.last_insert.insert(buffer, c);
        }
    }

    /// The buffer's last selection / insert position, clamped to the score as it is now.
    fn clamped_mark(&self, mut c: Cursor) -> Cursor {
        crate::app::clamp_cursor(&self.buffer().score, &mut c);
        c
    }

    /// `gv`: reselect the last Visual selection (in Visual mode: swap with the current one).
    fn visual_reselect(&mut self) {
        let buffer = self.pane().buffer;
        let Some((a, c, bars)) = self.ed.last_visual.get(&buffer).copied() else {
            return self.error("no previous visual selection");
        };
        if let Some(now) = self.selection() {
            self.ed.last_visual.insert(buffer, now);
        }
        self.ed.visual_anchor = Some(self.clamped_mark(a));
        self.ed.visual_bars = bars;
        let c = self.clamped_mark(c);
        self.pane_mut().cursor = c;
        self.mode = Mode::Visual;
    }

    /// `gi`: Insert mode where it was last left (on the cursor if it never was).
    fn insert_last(&mut self) {
        let buffer = self.pane().buffer;
        if let Some(c) = self.ed.last_insert.get(&buffer).copied() {
            let c = self.clamped_mark(c);
            self.pane_mut().cursor = c;
        }
        self.enter_insert();
    }

    /// The `<` `>` `^` marks (selection start / end, last insert position) as jump targets.
    pub(crate) fn special_mark(&self, name: char) -> Option<(usize, Cursor)> {
        let b = self.pane().buffer;
        let c = match name {
            '^' => *self.ed.last_insert.get(&b)?,
            '<' | '>' => {
                let (a, c, _) = *self.ed.last_visual.get(&b)?;
                let (lo, hi) = if (a.bar, a.beat) <= (c.bar, c.beat) {
                    (a, c)
                } else {
                    (c, a)
                };
                if name == '<' { lo } else { hi }
            }
            _ => return None,
        };
        Some((b, self.clamped_mark(c)))
    }

    /// Register being recorded into, for display.
    pub fn recording_macro(&self) -> Option<char> {
        self.ed.recording.as_ref().map(|r| r.0)
    }

    pub(crate) fn dispatch_key(&mut self, key: Key) {
        // <F1> shows or hides the key help in every mode (not recorded in macros); Esc also
        // hides it. A Normal-mode binding of <F1> to something else wins in the Normal modes.
        let plain = !key.ctrl && !key.alt && !key.sup;
        if plain && key.code == KeyCode::F(1) && !self.f1_rebound() {
            self.show_help = !self.show_help;
            return;
        }
        if self.show_help && key.code == KeyCode::Esc {
            self.show_help = false;
            return;
        }
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
        let visual = self.selection();
        match self.mode {
            Mode::Command => self.handle_command_key(key),
            Mode::Picker => self.handle_picker_key(key),
            // The pane's search field takes every key.
            Mode::Normal | Mode::Tracks | Mode::Properties
                if self.props.search.is_some()
                    && self.focused_kind() == crate::app::PaneKind::Properties =>
            {
                self.props_search_key(key)
            }
            // The palette's search field takes every key.
            Mode::Normal | Mode::Tracks | Mode::Properties | Mode::Palette | Mode::Visual
                if self.palette.search.is_some()
                    && self.focused_kind() == crate::app::PaneKind::Palette =>
            {
                self.palette_search_key(key)
            }
            // A property being typed takes every key.
            Mode::Normal | Mode::Tracks | Mode::Properties
                if self.props.edit.is_some()
                    && self.focused_kind() == crate::app::PaneKind::Properties =>
            {
                self.props_edit_key(key)
            }
            // Escape in a tool pane goes back to the score pane worked on last (the Visual
            // selection stays).
            Mode::Normal | Mode::Tracks | Mode::Visual
                if key.code == KeyCode::Esc
                    && self.pending.is_empty()
                    && self.focused_kind() != crate::app::PaneKind::Score =>
            {
                self.count = None;
                self.set_focus(self.focused());
            }
            Mode::Normal | Mode::Tracks | Mode::Properties
                if self.focused_kind() == crate::app::PaneKind::Properties =>
            {
                self.properties_key(key)
            }
            // The palette's keys, in Visual mode too: the selection stays for its tools.
            Mode::Normal | Mode::Tracks | Mode::Properties | Mode::Visual
                if self.focused_kind() == crate::app::PaneKind::Palette =>
            {
                self.palette_key(key)
            }
            // A focused tracks pane has its own keys first.
            Mode::Normal | Mode::Tracks if self.focused_kind() == crate::app::PaneKind::Tracks => {
                self.tracks_key(key)
            }
            Mode::Normal | Mode::Tracks | Mode::Properties | Mode::Palette => self.normal_key(key),
            Mode::Insert => self.insert_key(key),
            Mode::Visual => self.visual_key(key),
        }
        self.remember_leaves(before.1, visual, in_insert.then_some(before.2));
        self.seek_to_cursor(before);
        self.peek_tracks(before);
        if in_insert && self.mode != Mode::Insert {
            self.ed.group = None;
            if let Some(k) = self.ed.change_keys.take() {
                self.ed.last_change = k;
                self.ed.tool_last = false;
            }
        }
    }

    /// Whether the user bound `<F1>` in Normal mode to something other than `help`.
    fn f1_rebound(&self) -> bool {
        let normalish = matches!(
            self.mode,
            Mode::Normal | Mode::Tracks | Mode::Properties | Mode::Palette
        );
        normalish
            && self
                .config
                .keymap(Mode::Normal)
                .bindings
                .get(&[Key::new(KeyCode::F(1))][..])
                .is_some_and(|c| c.as_slice() != [Command::Help])
    }

    /// The key help for the current mode: a title and (keys, what) rows, sorted by command.
    /// Built from the effective keymaps, so user bindings show and removed ones do not.
    pub fn help_view(&self) -> (String, Vec<(String, String)>) {
        use crate::app::PaneKind;
        use crate::config::Keymap;
        fn rows(map: &Keymap, skip: &[&Keymap]) -> Vec<(String, String)> {
            map.bindings
                .iter()
                .filter(|(k, _)| !skip.iter().any(|m| m.bindings.contains_key(*k)))
                .map(|(k, c)| {
                    let what = match map.names.get(k) {
                        Some(n) => n.join(", "),
                        None => format!("{c:?}"),
                    };
                    (crate::keys::format_keys(k), what)
                })
                .collect()
        }
        fn fixed(list: &[(&str, &str)]) -> Vec<(String, String)> {
            list.iter()
                .map(|(k, c)| (k.to_string(), c.to_string()))
                .collect()
        }
        let c = &self.config;
        let (name, mut items) = match self.mode {
            Mode::Command => {
                let mut v = rows(c.keymap(Mode::Command), &[]);
                v.extend(fixed(&[
                    ("<CR>", "run the command"),
                    ("<Esc>", "cancel"),
                    ("<C-c>", "cancel"),
                    ("<Tab>", "complete / next candidate"),
                    ("<S-Tab>", "previous candidate"),
                    ("<C-p>", "older history"),
                    ("<C-n>", "newer history"),
                    ("<C-u>", "clear the line"),
                    ("<BS>", "delete a character"),
                ]));
                ("Command line", v)
            }
            Mode::Picker => (
                "Picker",
                fixed(&[
                    ("<CR>", "choose the entry"),
                    ("<Esc>", "close"),
                    ("<C-c>", "close"),
                    ("<Down>", "next entry"),
                    ("<Tab>", "next entry"),
                    ("<C-n>", "next entry"),
                    ("<A-j>", "next entry"),
                    ("<Up>", "previous entry"),
                    ("<S-Tab>", "previous entry"),
                    ("<C-p>", "previous entry"),
                    ("<A-k>", "previous entry"),
                    ("<C-u>", "clear the query"),
                    ("<BS>", "delete a character"),
                ]),
            ),
            Mode::Insert => {
                let kind = self.insert_kind();
                let specific = c.insert_keymap(kind);
                let mut v = rows(specific, &[]);
                v.extend(rows(c.keymap(Mode::Insert), &[specific]));
                ("Insert", v)
            }
            Mode::Visual => ("Visual", rows(c.keymap(Mode::Visual), &[])),
            Mode::Normal | Mode::Tracks | Mode::Properties | Mode::Palette => {
                let normal = c.keymap(Mode::Normal);
                let notation = c.normal_keymap(self.insert_kind());
                let (name, pane) = match self.focused_kind() {
                    PaneKind::Tracks => ("Tracks pane", Some(Mode::Tracks)),
                    PaneKind::Properties => ("Properties pane", Some(Mode::Properties)),
                    PaneKind::Palette => ("Palette pane", Some(Mode::Palette)),
                    _ => ("Normal", None),
                };
                let mut v = Vec::new();
                let mut skip = Vec::new();
                if let Some(m) = pane {
                    v = rows(c.keymap(m), &[]);
                    skip.push(c.keymap(m));
                }
                if pane.is_none() {
                    v.extend(rows(notation, &[]));
                    skip.push(notation);
                }
                v.extend(rows(normal, &skip));
                (name, v)
            }
        };
        items.sort_by(|a, b| a.1.cmp(&b.1).then(a.0.cmp(&b.0)));
        (format!("{name} keys"), items)
    }

    pub(crate) fn normal_key(&mut self, key: Key) {
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
        let kind = self.insert_kind();
        // Design mode's keys come first.
        let design = if self.design_active() && self.mode == Mode::Normal {
            match self.config.design_keymap.lookup(&self.pending) {
                Lookup::Exact(cmds) => Some(cmds.to_vec()),
                _ => None,
            }
        } else {
            None
        };
        let cmds = match design {
            Some(cmds) => cmds,
            None => match self.config.normal_lookup(kind, &self.pending) {
                Lookup::Exact(cmds) => cmds.to_vec(),
                Lookup::Prefix => return,
                Lookup::None => {
                    self.pending.clear();
                    self.count = None;
                    self.ed.operator = None;
                    self.ed.cmd_keys.clear();
                    return;
                }
            },
        };
        self.pending.clear();
        let count = self.count.take();
        self.message = None;
        let keys = std::mem::take(&mut self.ed.cmd_keys);
        let mut changed = false;
        for cmd in cmds {
            // Nothing to edit behind the dashboard.
            if self.dashboard_visible() && (cmd.is_change() || matches!(cmd, Command::Operator(_)))
            {
                self.ed.operator = None;
                continue;
            }
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
                self.ed.tool_last = false;
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

    /// The insert keymap for the cursor's staff.
    pub fn insert_kind(&self) -> InsertKind {
        let c = self.pane().cursor;
        let Some(t) = self.buffer().score.tracks.get(c.track) else {
            return InsertKind::Tab;
        };
        match t.staves().get(crate::app::staff_index(t, c.staff)).copied() {
            Some(s) if crate::app::uses_strings(t, s) => InsertKind::Tab,
            Some(Staff::Chant | Staff::ChantFa) => InsertKind::Chant,
            Some(Staff::Slash) => InsertKind::Slash,
            _ => InsertKind::Standard,
        }
    }

    pub(crate) fn on_staff_kind(&self, kind: InsertKind) -> bool {
        self.insert_kind() == kind
    }

    fn insert_key(&mut self, key: Key) {
        // Renaming a track in the tracks pane takes the keys.
        if self.renaming.is_some() {
            return self.rename_key(key);
        }
        let kind = self.insert_kind();
        // On a tab, digits type frets; two quick digits make a two-digit fret.
        if let Some(d) = key
            .as_char()
            .and_then(|c| c.to_digit(10))
            .filter(|_| kind == InsertKind::Tab)
        {
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
            if let Some(k) = key.as_char().and_then(cecilia_core::drums::key_for_letter) {
                let duration = self.insert_duration();
                self.edit(|cx| edit::toggle_drum(cx, k, duration));
                return;
            }
        }
        self.pending.push(key);
        // The notation's own keys first, then the shared insert keys.
        let cmds = match self.config.insert_keymap(kind).lookup(&self.pending) {
            Lookup::Exact(cmds) => cmds.to_vec(),
            Lookup::Prefix => return,
            Lookup::None => match self.config.keymap(Mode::Insert).lookup(&self.pending) {
                Lookup::Exact(cmds) => cmds.to_vec(),
                Lookup::Prefix => return,
                Lookup::None => {
                    self.pending.clear();
                    return;
                }
            },
        };
        self.pending.clear();
        for cmd in cmds {
            self.run(cmd);
        }
    }

    pub(crate) fn visual_key(&mut self, key: Key) {
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
            None => match self.config.normal_lookup(self.insert_kind(), &self.pending) {
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
                c if c.is_range() => self.run_tool(c),
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
                    let t = cx.cursor.track;
                    for b in from..=to {
                        cx.bar(t, b).voices = vec![Voice {
                            beats: vec![Beat {
                                empty: true,
                                ..Beat::default()
                            }],
                        }];
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
        let inclusive = matches!(
            motion,
            Command::BarEnd | Command::BarLastNote | Command::RowEnd
        );
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
        } else if inclusive || motion == Command::BeatNext {
            // `l` on the last beat of the score still covers the beat under the cursor.
            Some((s, s))
        } else {
            None
        };
        if let Some((from, to)) = range {
            self.operate_range(op, from, to);
        } else if op == Operator::Change {
            // Nothing to clear, but `c` still ends in insert mode.
            self.ed.group = Some(false);
            self.enter_insert();
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
                self.ed.group = Some(false);
                self.edit(|cx| {
                    edit::clear_range(cx, from, to);
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
        // A bar selection: bar commands go to its first or last bar, or to every bar of it.
        if matches!(self.selection(), Some((_, _, true)))
            && matches!(
                cmd,
                RepeatStart
                    | RepeatEnd(_)
                    | DoubleBar
                    | Section(_)
                    | DirectionMark(_)
                    | Key(..)
                    | Time(..)
                    | FreeTime
                    | Ending(_)
            )
        {
            self.run_over_bars(cmd);
            return true;
        }
        let duration = self.insert_duration();
        match cmd {
            Insert => self.enter_insert(),
            InsertLast => self.insert_last(),
            VisualReselect => self.visual_reselect(),
            InsertBeat(at) => {
                let at = *at;
                self.ed.group = Some(false);
                self.edit(|cx| {
                    match at {
                        BeatAt::Before | BeatAt::After => {}
                        BeatAt::BarStart => cx.cursor.beat = 0,
                        BeatAt::BarEnd => cx.cursor.beat = cx.voice().len() - 1,
                    }
                    edit::insert_beat(cx, matches!(at, BeatAt::After | BeatAt::BarEnd), duration);
                    Ok(())
                });
                self.enter_insert();
            }
            VisualInsert { after } => self.visual_insert(*after),
            InsertEnter => {
                self.ed.digit = None;
                let kind = self.insert_kind();
                self.edit(|cx| {
                    edit::enter(cx, kind, duration)?;
                    edit::advance(cx, duration);
                    Ok(())
                });
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
            Repeat if self.ed.tool_last && self.ed.last_tool.is_some() => {
                let (cmd, span) = self.ed.last_tool.clone().unwrap();
                self.repeat_tool(cmd, span);
            }
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
            BarOpen { after } => {
                let at = self.pane().cursor.bar + usize::from(*after);
                let n = self.count.take().unwrap_or(1);
                self.ed.group = Some(false);
                self.edit(|cx| {
                    edit::insert_bars(cx, at, n);
                    cx.cursor.bar = at;
                    cx.cursor.beat = 0;
                    Ok(())
                });
                self.enter_insert();
            }
            BarDelete => self.operate_bars(self::Operator::Delete, 1),
            DurationLonger | DurationShorter | Duration(_) | Dot | DoubleDot | Tuplet(_)
                if self.selection().is_some() =>
            {
                let (from, to) = self.beat_range();
                let cmd = cmd.clone();
                let n = self.edit(|cx| {
                    Ok(match &cmd {
                        Dot => {
                            edit::dot_range(cx, from, to, 1);
                            0
                        }
                        DoubleDot => {
                            edit::dot_range(cx, from, to, 2);
                            0
                        }
                        Tuplet(n) => {
                            edit::tuplet_range(cx, from, to, *n);
                            0
                        }
                        _ => edit::change_durations(cx, from, to, |mut d| {
                            match &cmd {
                                DurationLonger => d.value = d.value.longer(),
                                DurationShorter => d.value = d.value.shorter(),
                                Duration(n) => {
                                    d.value = DurationValue::from_denominator(*n).unwrap_or(d.value)
                                }
                                _ => {}
                            }
                            d
                        }),
                    })
                });
                if n == Some(0) && !matches!(cmd, Dot | DoubleDot | Tuplet(_)) {
                    self.error("no beats in the selection");
                }
                self.report_overflow(from, to);
            }
            DurationLonger | DurationShorter | Duration(_) | Dot | DoubleDot | Tuplet(_) => {
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
                            DoubleDot => d.dots = if d.dots == 2 { 0 } else { 2 },
                            Tuplet(n) => {
                                let times = match n {
                                    3 => 2,
                                    5..=7 => 4,
                                    _ => 8,
                                };
                                d.tuplet = if d.tuplet.is_some() {
                                    None
                                } else {
                                    Some(cecilia_core::Tuplet { enters: *n, times })
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
            PitchUp | PitchDown | OctaveUp | OctaveDown if self.selection().is_some() => {
                let n = match cmd {
                    PitchUp => 1,
                    PitchDown => -1,
                    OctaveUp => 12,
                    _ => -12,
                };
                let (from, to) = self.beat_range();
                if let Some(stuck) = self.edit(|cx| Ok(edit::shift_range(cx, from, to, n)))
                    && stuck > 0
                {
                    self.info(format!("{stuck} notes could not move"));
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
            Rest if self.selection().is_some() => {
                let (from, to) = self.beat_range();
                self.edit(|cx| {
                    edit::clear_range(cx, from, to);
                    Ok(())
                });
            }
            Rest => {
                self.edit(|cx| {
                    edit::make_rest(cx, duration);
                    Ok(())
                });
            }
            Toggle(t) if self.selection().is_some() => {
                let (from, to) = self.beat_range();
                let t = t.clone();
                self.edit(|cx| edit::toggle_range(cx, from, to, &t));
            }
            DynamicStep(by) => {
                let (from, to) = self.beat_range();
                let by = *by;
                self.edit(|cx| edit::step_dynamic_range(cx, from, to, by));
            }
            Dynamic(d) if self.selection().is_some() => {
                let (from, to) = self.beat_range();
                let d = d.clone();
                self.edit(|cx| edit::set_dynamic_range(cx, from, to, &d));
            }
            Accidental(alter) if self.selection().is_some() => {
                let (from, to) = self.beat_range();
                let alter = *alter;
                if let Some(stuck) = self.edit(|cx| edit::accidental_range(cx, from, to, alter))
                    && stuck > 0
                {
                    self.info(format!("{stuck} notes could not move"));
                }
            }
            Toggle(t) => {
                let t = t.clone();
                self.edit(|cx| edit::toggle(cx, &t, duration));
            }
            Dynamic(d) => {
                let d = d.clone();
                self.edit(|cx| edit::set_dynamic(cx, &d));
            }
            NoteLetter(c) => {
                let step = cecilia_core::staff::letter_step(*c).unwrap_or(0);
                if self.on_staff_kind(InsertKind::Standard) {
                    let duration = self.insert_duration();
                    self.edit(|cx| edit::put_letter(cx, step, duration));
                } else {
                    self.error("note-letter: the cursor is not on a standard staff");
                }
            }
            Accidental(alter) => {
                self.edit(|cx| edit::set_accidental(cx, *alter));
            }
            Slash => {
                let duration = self.insert_duration();
                self.edit(|cx| edit::put_slash(cx, duration));
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
            Key(k, minor) => {
                let (k, minor) = (*k, *minor);
                self.edit_master(move |bars, i| {
                    let old = bars[i].key_signature;
                    for mb in bars[i..].iter_mut().take_while(|m| m.key_signature == old) {
                        mb.key_signature.accidentals = k;
                        if let Some(minor) = minor {
                            mb.key_signature.minor = minor;
                        }
                    }
                });
            }
            FreeTime => self.edit_master(|bars, i| bars[i].free_time = !bars[i].free_time),
            DoubleBar => self.edit_master(|bars, i| bars[i].double_bar = !bars[i].double_bar),
            Ending(mask) => {
                let mask = *mask;
                self.edit_master(move |bars, i| {
                    bars[i].alternate_endings = if bars[i].alternate_endings == mask {
                        0
                    } else {
                        mask
                    }
                });
            }
            DirectionMark(d) => {
                let d = *d;
                self.edit_master(move |bars, i| {
                    let list = &mut bars[i].directions;
                    match list.iter().position(|x| *x == d) {
                        Some(k) => {
                            list.remove(k);
                        }
                        None => list.push(d),
                    }
                });
            }
            BarRepeat(n) => self.bar_repeat(*n),
            Beams(mode) => {
                let mode = *mode;
                let (from, to) = self.beat_range();
                self.edit(|cx| {
                    edit::set_beams(cx, from, to, mode);
                    Ok(())
                });
            }
            Stems(dir) => {
                let dir = *dir;
                let (from, to) = self.beat_range();
                self.edit(|cx| {
                    edit::set_stems(cx, from, to, dir);
                    Ok(())
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
                        letter: String::new(),
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
                    let (shifted, dropped) = cecilia_convert::retune(track, strings, fret);
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
                    let dropped = cecilia_convert::refinger(track, 0..n);
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
                        let (shifted, dropped) =
                            cecilia_convert::retune(track, strings, track.capo);
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
                let (verse, syllables) = (*verse, cecilia_core::lyrics::syllables(text));
                let placed = self.edit(|cx| {
                    let c = cx.cursor;
                    let score = cx.whole();
                    let n = cecilia_core::lyrics::place(
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
            Notation(arg) => self.set_notation(arg),
            Clef(arg) => self.set_clef(arg),
            ClefLine(arg) => self.set_clef_line(arg),
            LyricsAttach(arg) => self.attach_lyrics(arg),
            StaffAdd(arg) => self.add_staff(arg.as_deref()),
            StaffRemove(n) => self.remove_staff(*n),
            StaffMove(to) => self.move_staff(to),
            TrackDelete => {
                self.edit(|cx| {
                    if cx.score.tracks.len() <= 1 {
                        return Err("cannot delete the only track".into());
                    }
                    let t = cx.cursor.track;
                    cx.whole().design.delete_track(t);
                    let tracks = &mut cx.whole().tracks;
                    tracks.remove(t);
                    // Lyrics attached from the deleted track go; later tracks shift down.
                    for track in tracks.iter_mut() {
                        for l in track.staff_lyrics.iter_mut() {
                            match l.as_mut().and_then(|l| l.track) {
                                Some(src) if src == t => *l = None,
                                Some(src) if src > t => {
                                    if let Some(l) = l.as_mut() {
                                        l.track = Some(src - 1);
                                    }
                                }
                                _ => {}
                            }
                        }
                        while track.staff_lyrics.last() == Some(&None) {
                            track.staff_lyrics.pop();
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

    /// Visual `i` / `a`: leaves the selection and starts typing in a new beat before / after
    /// it (a bar selection: at the start of its first bar / the end of its last).
    fn visual_insert(&mut self, after: bool) {
        let Some((anchor, cursor, bars)) = self.selection() else {
            return;
        };
        let (a, b) = ((anchor.bar, anchor.beat), (cursor.bar, cursor.beat));
        let (lo, hi) = if a <= b { (a, b) } else { (b, a) };
        self.run(Command::Normal);
        let at = match (after, bars) {
            (false, false) => BeatAt::Before,
            (true, false) => BeatAt::After,
            (false, true) => BeatAt::BarStart,
            (true, true) => BeatAt::BarEnd,
        };
        let (bar, beat) = if after { hi } else { lo };
        let p = self.pane_mut();
        p.cursor.bar = bar;
        p.cursor.beat = beat;
        self.run(Command::InsertBeat(at));
    }

    fn enter_insert(&mut self) {
        if self.buffer().score.tracks.is_empty() {
            return self.error("no score to edit (use :new)");
        }
        self.mode = Mode::Insert;
        self.ed.digit = None;
        self.ed.group.get_or_insert(false);
    }

    /// The beats a beam or stem command covers: the visual selection, else the cursor beat.
    fn beat_range(&self) -> (Pos, Pos) {
        let c = self.pane().cursor;
        let here = (c.bar, c.beat);
        let Some((anchor, cursor, bars)) = self.selection() else {
            return (here, here);
        };
        if bars {
            let (a, b) = (anchor.bar.min(cursor.bar), anchor.bar.max(cursor.bar));
            return ((a, 0), (b, usize::MAX));
        }
        let (a, b) = ((anchor.bar, anchor.beat), (cursor.bar, cursor.beat));
        if a <= b { (a, b) } else { (b, a) }
    }

    /// Runs a tool (a palette button, a command applied to a Visual selection) and, if it
    /// changed the score, keeps it with the size of its selection for `.`.
    pub(crate) fn run_tool(&mut self, cmd: Command) {
        let span = self.selection().map(|(a, b, bars)| {
            if bars {
                (true, a.bar.abs_diff(b.bar) + 1)
            } else {
                let c = self.pane().cursor;
                let (lo, hi) = (
                    (a.bar, a.beat).min((b.bar, b.beat)),
                    (a.bar, a.beat).max((b.bar, b.beat)),
                );
                let n = edit::collect(&self.buffer().score, c.track, c.voice, lo, hi).len();
                (false, n.max(1))
            }
        });
        let before = self.buffer().version;
        self.run(cmd.clone());
        if self.buffer().version != before {
            self.ed.last_tool = Some((cmd, span));
            self.ed.tool_last = true;
        }
    }

    /// `.` after a tool: applies it again at the cursor, over a selection of the same size
    /// if it was applied to one.
    fn repeat_tool(&mut self, cmd: Command, span: Option<(bool, usize)>) {
        self.ed.replaying += 1;
        match span {
            None => self.run(cmd),
            Some((bars, n)) => {
                self.run(Command::Visual { bars });
                for _ in 1..n {
                    self.run(if bars {
                        Command::BarNext
                    } else {
                        Command::BeatNext
                    });
                }
                self.run(cmd);
                if self.mode == Mode::Visual {
                    self.run(Command::Normal);
                }
            }
        }
        self.ed.replaying -= 1;
    }

    /// Says so when the beats of the bars `from` and `to` (positions) now run over their bars.
    fn report_overflow(&mut self, from: Pos, to: Pos) {
        let c = self.pane().cursor;
        let over = edit::overflowing_bars(&self.buffer().score, c.track, c.voice, from.0, to.0);
        if let Some(b) = over.first() {
            let more = if over.len() > 1 {
                format!(" (and {} more)", over.len() - 1)
            } else {
                String::new()
            };
            self.info(format!("bar {} is over full{more}", b + 1));
        }
    }

    /// Runs a bar command on the bars of the Visual bar selection it is meant for: repeat
    /// start, section, key and time signature at the first, repeat end, double bar and
    /// direction at the last, free time and endings on all (taken off when all have them).
    fn run_over_bars(&mut self, cmd: &Command) {
        use Command::*;
        let (a, b) = self.bar_range();
        let marks = &self.buffer().score.master_bars;
        let bars: Vec<usize> = match cmd {
            RepeatStart | Section(_) | Key(..) | Time(..) => vec![a],
            RepeatEnd(_) | DoubleBar | DirectionMark(_) => vec![b],
            FreeTime => {
                let all = (a..=b).all(|i| marks[i].free_time);
                (a..=b).filter(|&i| marks[i].free_time == all).collect()
            }
            Ending(mask) => {
                let all = (a..=b).all(|i| marks[i].alternate_endings == *mask);
                (a..=b)
                    .filter(|&i| (marks[i].alternate_endings == *mask) == all)
                    .collect()
            }
            _ => vec![],
        };
        let (anchor, cursor) = (self.ed.visual_anchor.take(), self.pane().cursor);
        // One undo step for all the bars.
        self.ed.group = Some(false);
        for bar in bars {
            self.pane_mut().cursor.bar = bar;
            self.run(cmd.clone());
        }
        self.ed.group = None;
        self.pane_mut().cursor.bar = cursor.bar;
        self.ed.visual_anchor = anchor;
    }

    /// The bars a bar command applies to: the Visual bar selection, else the cursor bar.
    fn bar_range(&self) -> (usize, usize) {
        let here = self.pane().cursor.bar;
        match self.selection() {
            Some((a, b, true)) => (a.bar.min(b.bar), a.bar.max(b.bar)),
            _ => (here, here),
        }
    }

    /// Makes the cursor bar (or the two from it) repeat the bar(s) before, shown as a repeat
    /// sign; the notes are copied so playback and files without the sign still have them.
    fn bar_repeat(&mut self, n: Option<u8>) {
        let at = self.pane().cursor.bar;
        let track = self.pane().cursor.track;
        let Some(n) = n else {
            self.edit(|cx| {
                let bars = &mut cx.whole().tracks[track].bars;
                let partner = match bars.get(at).and_then(|b| b.simile) {
                    Some(Simile::TwoBarsFirst) => Some(at + 1),
                    Some(Simile::TwoBarsSecond) => at.checked_sub(1),
                    _ => None,
                };
                for b in std::iter::once(at).chain(partner) {
                    if let Some(bar) = bars.get_mut(b) {
                        bar.simile = None;
                    }
                }
                Ok(())
            });
            return;
        };
        let n = n as usize;
        if at < n {
            return self.error(format!(
                "bar-repeat: there are not {n} bars before this one"
            ));
        }
        if at + n > self.buffer().score.master_bars.len() {
            return self.error(format!("bar-repeat: {n} bars do not fit from here"));
        }
        // Pressing it again on a repeat takes the repeat away.
        let already = self.buffer().score.tracks[track].bars[at].simile.is_some();
        if already {
            return self.run(Command::BarRepeat(None));
        }
        self.edit(|cx| {
            let bars = &mut cx.whole().tracks[track].bars;
            for k in 0..n {
                let mut copy = bars[at - n + k].clone();
                copy.simile = Some(match (n, k) {
                    (1, _) => Simile::OneBar,
                    (_, 0) => Simile::TwoBarsFirst,
                    _ => Simile::TwoBarsSecond,
                });
                bars[at + k] = copy;
            }
            Ok(())
        });
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
        // Back at the saved state, the file is unchanged (a new score: back to empty).
        buffer.modified = !buffer.history.at_saved();
        clamp_cursor(&buffer.score, &mut c);
        let seq = buffer.history.current_seq();
        self.panes.get_mut(&id).unwrap().cursor = c;
        self.effects.push(Effect::ScoreChanged { buffer: b });
        self.info(format!("change {seq}"));
    }

    fn retune(&mut self, spec: &str) {
        let strings = cecilia_convert::tuning_preset(spec).or_else(|| {
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
            let (shifted, dropped) = cecilia_convert::retune(track, strings, capo);
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
                        }],
                        ..Default::default()
                    };
                    bars
                ],
                // A piano gets a grand staff.
                staves: if grand_staff {
                    vec![Staff::Standard(Clef::Grand { split: GRAND_SPLIT })]
                } else {
                    Vec::new()
                },
                ..Track::default()
            });
            cx.cursor.track = first;
            cx.cursor.bar = 0;
            cx.cursor.beat = 0;
            Ok(())
        });
    }

    /// The staff `arg` names (`standard grand C4`, `tab+` …), the clef of a plain `standard`
    /// being the cursor staff's, or the config's.
    fn parse_staff(&mut self, arg: &str) -> Option<Staff> {
        let (track, here) = self.cursor_staff();
        let clef = match here {
            Some(Staff::Standard(c)) => c,
            _ if track.is_some_and(|t| t.kind == TrackKind::Drums) => Clef::Neutral,
            _ => self.config.default_clef(),
        };
        let arg = arg.trim();
        match Staff::parse(arg, clef) {
            // A plain `chant` keeps a fa clef.
            Ok(Staff::Chant) if arg == "chant" && here == Some(Staff::ChantFa) => {
                Some(Staff::ChantFa)
            }
            Ok(s) => self.fit_clef(track.map(|t| t.kind), s),
            Err(e) => {
                self.error(format!("notation: {e}"));
                None
            }
        }
    }

    /// The cursor's track and staff.
    fn cursor_staff(&self) -> (Option<&Track>, Option<Staff>) {
        let c = self.pane().cursor;
        let track = self.buffer().score.tracks.get(c.track);
        let staff =
            track.and_then(|t| t.staves().get(crate::app::staff_index(t, c.staff)).copied());
        (track, staff)
    }

    /// A drum track reads the neutral clef only, and no other track does.
    fn fit_clef(&mut self, kind: Option<TrackKind>, staff: Staff) -> Option<Staff> {
        let drums = kind == Some(TrackKind::Drums);
        match staff {
            Staff::Standard(Clef::Neutral) if !drums => {
                self.error("clef: the neutral clef is for drum tracks");
                None
            }
            Staff::Standard(Clef::Neutral) => Some(staff),
            Staff::Standard(_) if drums => Some(Staff::Standard(Clef::Neutral)),
            s => Some(s),
        }
    }

    /// Shows the cursor's staff in another notation. The notes stay as they are; a tab on a
    /// track without strings first puts them on strings (refused if any would not fit).
    fn set_notation(&mut self, arg: &str) {
        let Some(staff) = self.parse_staff(arg) else {
            return;
        };
        self.put_staff(staff);
    }

    fn put_staff(&mut self, staff: Staff) {
        let done = self.edit(|cx| {
            let (i, t) = (cx.cursor.staff, cx.cursor.track);
            let track = &mut cx.whole().tracks[t];
            fret_for_tab(track, staff)?;
            let staves = track.staves_mut();
            let i = i.min(staves.len() - 1);
            staves[i] = staff;
            Ok(i)
        });
        if let Some(i) = done {
            self.info(format!("staff {}: {}", i + 1, staff.name()));
        }
    }

    /// Puts a chant staff of an ordinary track in the cursor's place, with its clef on a staff
    /// line (the track's `chant_line`; `None` lets the engraver choose).
    fn put_chant_staff(&mut self, staff: Staff, line: Option<u8>) {
        let done = self.edit(|cx| {
            let (i, t) = (cx.cursor.staff, cx.cursor.track);
            let track = &mut cx.whole().tracks[t];
            fret_for_tab(track, staff)?;
            track.chant_line = line;
            let staves = track.staves_mut();
            let i = i.min(staves.len() - 1);
            staves[i] = staff;
            Ok(i)
        });
        if let Some(i) = done {
            self.info(format!("staff {}: {}", i + 1, staff.name()));
        }
    }

    /// Moves the clef of the cursor's chant staff to another line: `1`–`4`, or `up` / `down`
    /// from where it is now.
    fn set_clef_line(&mut self, arg: &str) {
        let arg = arg.trim();
        let (track, here) = self.cursor_staff();
        let (Some(track), Some(here)) = (track, here) else {
            return;
        };
        if !here.is_chant() {
            return self.error("clef-line: the cursor is not on a chant staff");
        }
        let now = match track.chant.as_deref() {
            Some(c) => c.initial_clef(),
            None => crate::edit::chant_clef_of(&self.buffer().score, track, self.chord_policy()).0,
        };
        let line = match arg {
            "up" => now.line.saturating_add(1),
            "down" => now.line.saturating_sub(1),
            n => match n.parse::<u8>() {
                Ok(l) => l,
                Err(_) => return self.error("clef-line: expected 1–4, up or down"),
            },
        };
        if !(1..=4).contains(&line) {
            return self.error(format!("clef-line: line {line} is off the staff (1–4)"));
        }
        let k = if now.kind == ClefKind::Do { 'c' } else { 'f' };
        let flat = if now.flat { "b" } else { "" };
        self.set_clef(&format!("{k}{flat}{line}"));
    }

    /// Sets the clef of the cursor's staff, among those its notation has: treble, bass, grand
    /// or neutral for standard notation; do or fa for chant. Tab and slash have none.
    fn set_clef(&mut self, arg: &str) {
        let arg = arg.trim();
        let (track, here) = self.cursor_staff();
        let (Some(track), Some(here)) = (track, here) else {
            return;
        };
        let chant_track = track.chant.is_some();
        let kind = track.kind;
        match here.notation() {
            Notation::Tab | Notation::Slash => {
                self.error(format!("clef: a {} staff has no clef", here.name()));
            }
            Notation::Standard => match Staff::parse(arg, Clef::Treble) {
                Ok(s @ Staff::Standard(_)) => {
                    if let Some(s) = self.fit_clef(Some(kind), s) {
                        self.put_staff(s);
                    }
                }
                _ => self.error(format!(
                    "clef: '{arg}' is not a standard clef (treble, bass, grand [split], neutral)"
                )),
            },
            Notation::Chant => {
                let family = match arg.chars().next() {
                    _ if arg == "do" => Some(ClefKind::Do),
                    _ if arg == "fa" => Some(ClefKind::Fa),
                    Some('c') => Some(ClefKind::Do),
                    Some('f') => Some(ClefKind::Fa),
                    _ => None,
                };
                let Some(family) = family else {
                    self.error(format!(
                        "clef: '{arg}' is not a chant clef (do, fa{})",
                        if chant_track {
                            ", c1–c4, f1–f4, cb3"
                        } else {
                            ""
                        }
                    ));
                    return;
                };
                if chant_track {
                    // The clef of the chant itself; do / fa keep its line.
                    let spec = match arg {
                        "do" | "fa" => {
                            let line = track.chant.as_ref().map_or(3, |c| c.initial_clef().line);
                            format!("{}{line}", if arg == "do" { 'c' } else { 'f' })
                        }
                        _ => arg.to_string(),
                    };
                    self.run(Command::ChantClef(spec));
                } else if arg == "do" || arg == "fa" {
                    self.put_staff(if family == ClefKind::Do {
                        Staff::Chant
                    } else {
                        Staff::ChantFa
                    });
                } else {
                    // `c3`, `f2` …: the notes keep their pitch, so the engraver re-places them.
                    match cecilia_core::chant::ChantClef::parse(arg) {
                        Some(c) if !c.flat && c.line <= 4 => {
                            self.put_chant_staff(
                                if family == ClefKind::Do {
                                    Staff::Chant
                                } else {
                                    Staff::ChantFa
                                },
                                Some(c.line),
                            );
                        }
                        _ => self.error(format!(
                            "clef: '{arg}' is not a clef for this staff (do, fa, c1–c4, f1–f4)"
                        )),
                    }
                }
            }
        }
    }

    /// Attaches lyrics to the cursor's staff: `above` or `below` it, from the staff's own
    /// track or from track N; `none` detaches them. A staff has them above or below, not both.
    fn attach_lyrics(&mut self, arg: &str) {
        let words: Vec<&str> = arg.split_whitespace().collect();
        let (lyrics, from) = match words.as_slice() {
            ["none" | "off"] => (None, None),
            [side @ ("above" | "below"), rest @ ..] if rest.len() <= 1 => {
                let from = match rest {
                    [n] => match n.parse::<usize>() {
                        Ok(n) if (1..=self.buffer().score.tracks.len()).contains(&n) => Some(n - 1),
                        _ => {
                            return self.error(format!("lyrics-attach: there is no track '{n}'"));
                        }
                    },
                    _ => None,
                };
                (Some(*side == "above"), from)
            }
            _ => {
                return self.error("lyrics-attach: expected above|below [TRACK] or none");
            }
        };
        let done = self.edit(|cx| {
            let (i, t) = (cx.cursor.staff, cx.cursor.track);
            let track = &mut cx.whole().tracks[t];
            let i = i.min(track.staves_mut().len() - 1);
            track.set_staff_lyrics(
                i,
                lyrics.map(|above| StaffLyrics {
                    track: from.filter(|&f| f != t),
                    above,
                }),
            );
            Ok(i)
        });
        if let Some(i) = done {
            self.info(match lyrics {
                None => format!("staff {}: no lyrics attached", i + 1),
                Some(above) => format!(
                    "staff {}: lyrics {} the staff",
                    i + 1,
                    if above { "above" } else { "below" }
                ),
            });
        }
    }

    /// Adds one or more staves at the bottom of the cursor's track, in the order given.
    fn add_staff(&mut self, arg: Option<&str>) {
        let default = self.config.default_clef();
        let parsed = match arg.filter(|a| !a.trim().is_empty()) {
            Some(a) => Staff::parse_list(a, default),
            None => Ok(vec![Staff::Standard(default)]),
        };
        let new = match parsed {
            Ok(new) => new,
            Err(e) => return self.error(format!("staff: {e}")),
        };
        let kind = self.cursor_staff().0.map(|t| t.kind);
        let Some(new) = new
            .into_iter()
            .map(|s| self.fit_clef(kind, s))
            .collect::<Option<Vec<_>>>()
        else {
            return;
        };
        let done = self.edit(|cx| {
            let t = cx.cursor.track;
            let track = &mut cx.whole().tracks[t];
            for &staff in &new {
                fret_for_tab(track, staff)?;
            }
            let staves = track.staves_mut();
            let first = staves.len() + 1;
            staves.extend(new.iter().copied());
            let n = staves.len();
            cx.cursor.staff = n - 1;
            Ok((first, n))
        });
        if let Some((first, n)) = done {
            let names: Vec<String> = new.iter().map(|s| s.name()).collect();
            if first == n {
                self.info(format!("staff {n}: {}", names[0]));
            } else {
                self.info(format!("staves {first}-{n}: {}", names.join(", ")));
            }
        }
    }

    /// Removes a staff of the cursor's track (1 = top; the cursor's without a number). The
    /// last one stays.
    fn remove_staff(&mut self, n: Option<usize>) {
        self.edit(|cx| {
            let (t, here) = (cx.cursor.track, cx.cursor.staff);
            let i = n.map_or(here, |n| n.saturating_sub(1));
            let staves = cx.whole().tracks[t].staves_mut();
            if staves.len() <= 1 {
                return Err("staff-remove: a track keeps at least one staff".into());
            }
            if i >= staves.len() {
                return Err(format!("staff-remove: there is no staff {}", i + 1));
            }
            let left = staves.len() - 1;
            cx.whole().tracks[t].remove_staff(i);
            cx.cursor.staff = here.min(left - 1);
            Ok(())
        });
    }

    /// Moves the cursor's staff to position `to` (1 = top), or up / down by `+N` / `-N`
    /// (`+1` moves it down one place).
    fn move_staff(&mut self, to: &str) {
        let to = to.trim();
        self.edit(|cx| {
            let (t, from) = (cx.cursor.track, cx.cursor.staff);
            let staves = cx.whole().tracks[t].staves_mut();
            let n = staves.len() as i64;
            let target = match to
                .strip_prefix('+')
                .or_else(|| to.starts_with('-').then_some(to))
            {
                Some(d) => {
                    from as i64
                        + d.parse::<i64>()
                            .map_err(|_| "staff-move: expected N, +N or -N")?
                }
                None => {
                    to.parse::<i64>()
                        .map_err(|_| "staff-move: expected N, +N or -N")?
                        - 1
                }
            };
            let target = target.clamp(0, n - 1) as usize;
            let from = from.min(staves.len() - 1);
            cx.whole().tracks[t].move_staff(from, target);
            cx.cursor.staff = target;
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
            ..Default::default()
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
            modified: false,
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
pub(crate) fn refresh_lyric_lines(score: &mut Score, track: usize) {
    let lines = cecilia_core::lyrics::lines_from_beats(&score.tracks[track]);
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

/// A tab needs strings: a track without them gets a tuning for its range and its notes go
/// on strings, keeping every pitch (refused if one would not fit).
fn fret_for_tab(track: &mut Track, staff: Staff) -> Result<(), String> {
    if !staff.is_tab() || track.kind == TrackKind::Fretted {
        return Ok(());
    }
    if track.kind != TrackKind::Pitched {
        return Err("staff: only pitched and fretted tracks have a tab".into());
    }
    let mut fretted = track.clone();
    let dropped = cecilia_convert::pitched_to_fretted(&mut fretted);
    if dropped > 0 {
        return Err(format!("staff: {dropped} notes would not fit on a tab"));
    }
    // The other staves keep showing the same notes.
    let staves = track.staves();
    *track = fretted;
    track.staves = staves;
    Ok(())
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

    #[test]
    fn f1_shows_the_help_of_every_mode_and_esc_hides_it() {
        let mut app = app();
        for (enter, title, sample) in [
            ("", "Normal keys", "<F1>"),
            ("i", "Insert keys", "<Esc>"),
            ("<Esc>v", "Visual keys", "<Esc>"),
            (":", "Command line keys", "<CR>"),
            ("<Esc><leader>b", "Picker keys", "<CR>"),
        ] {
            keys(&mut app, "<Esc><Esc>");
            if !enter.is_empty() {
                keys(&mut app, enter);
            }
            let mode = app.mode;
            keys(&mut app, "<F1>");
            assert!(app.show_help, "{title}");
            assert_eq!(app.mode, mode, "F1 keeps the mode");
            let (t, rows) = app.help_view();
            assert_eq!(t, title);
            assert!(rows.iter().any(|(k, _)| k == sample), "{title}: {sample}");
            keys(&mut app, "<F1>");
            assert!(!app.show_help);
            keys(&mut app, "<F1><Esc>");
            assert!(!app.show_help);
            assert_eq!(app.mode, mode, "Esc only hides the help");
        }
    }

    #[test]
    fn help_lists_user_bindings_and_not_removed_ones() {
        let (config, _) =
            Config::parse("[keymap.normal]\n\"gz\" = \"bar-next\"\n\"o\" = \"nop\"\n").unwrap();
        let mut app = App::new(config, None);
        app.run(Command::New);
        let rows = app.help_view().1;
        assert!(rows.contains(&("gz".to_string(), "bar-next".to_string())));
        assert!(!rows.iter().any(|(k, _)| k == "o"));
    }

    #[test]
    fn f1_is_not_recorded_in_macros_and_honors_a_rebinding() {
        let mut app = app();
        keys(&mut app, "qa<F1><F1>q");
        assert_eq!(app.recording_macro(), None);
        let (config, _) = Config::parse("[keymap.normal]\n\"<F1>\" = \"bar-next\"\n").unwrap();
        let mut app = App::new(config, None);
        app.run(Command::New);
        keys(&mut app, "<F1>");
        assert!(!app.show_help);
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

        // A piano is one track on a grand staff.
        a.run(Command::TrackAdd("piano".into()));
        let s = &a.buffer().score;
        assert_eq!(s.tracks.len(), 2);
        assert_eq!(
            s.tracks[1].staves(),
            vec![Staff::Standard(Clef::Grand { split: GRAND_SPLIT })]
        );
    }

    #[test]
    fn notation_and_clef_commands() {
        use cecilia_core::Clef::{Bass, Grand, Treble};
        let mut a = app();
        let staves = |a: &App| a.buffer().score.tracks[0].staves();
        a.run(Command::StaffPrev);
        // `:clef` works on the cursor staff by its notation.
        a.run(Command::Clef("bass".into()));
        assert_eq!(staves(&a), vec![Staff::Standard(Bass), Staff::Tab]);
        a.run(Command::Clef("grand G3".into()));
        assert_eq!(staves(&a)[0], Staff::Standard(Grand { split: 55 }));
        // Neutral is for drum tracks.
        a.run(Command::Clef("neutral".into()));
        assert_eq!(staves(&a)[0], Staff::Standard(Grand { split: 55 }));
        // `:notation standard` keeps the clef the staff had.
        a.run(Command::Notation("slash".into()));
        a.run(Command::Notation("standard".into()));
        assert_eq!(staves(&a)[0], Staff::Standard(Treble));
        // Tab and slash have no clef.
        a.run(Command::Notation("tab".into()));
        a.run(Command::Clef("treble".into()));
        assert_eq!(staves(&a)[0], Staff::Tab);
        a.run(Command::Notation("slash".into()));
        a.run(Command::Clef("treble".into()));
        assert_eq!(staves(&a)[0], Staff::Slash);
        // Chant staves take do or fa, and keep fa when asked for `chant` again.
        a.run(Command::Notation("chant".into()));
        assert_eq!(staves(&a)[0], Staff::Chant);
        a.run(Command::Clef("fa".into()));
        assert_eq!(staves(&a)[0], Staff::ChantFa);
        a.run(Command::Clef("treble".into()));
        assert_eq!(staves(&a)[0], Staff::ChantFa);
        a.run(Command::Notation("chant".into()));
        assert_eq!(staves(&a)[0], Staff::ChantFa);
        a.run(Command::Clef("do".into()));
        assert_eq!(staves(&a)[0], Staff::Chant);
        // Drums read the neutral clef, whatever is asked.
        a.run(Command::TrackAdd("drums".into()));
        let t = a.pane().cursor.track;
        assert_eq!(
            a.buffer().score.tracks[t].staves(),
            vec![Staff::Standard(cecilia_core::Clef::Neutral)]
        );
        a.run(Command::Clef("treble".into()));
        assert_eq!(
            a.buffer().score.tracks[t].staves(),
            vec![Staff::Standard(cecilia_core::Clef::Neutral)]
        );
    }

    #[test]
    fn lyrics_attach_to_staves_and_follow_them() {
        let mut a = app();
        a.run(Command::TrackAdd("guitar".into()));
        a.run(Command::Track(1));
        let lyrics = |a: &App| a.buffer().score.tracks[0].staff_lyrics.clone();
        let l = |track, above| Some(StaffLyrics { track, above });
        a.run(Command::StaffPrev);
        a.run(Command::LyricsAttach("above 2".into()));
        assert_eq!(lyrics(&a), vec![l(Some(1), true)]);
        a.run(Command::LyricsAttach("below".into()));
        assert_eq!(lyrics(&a), vec![l(None, false)]);
        a.run(Command::LyricsAttach("above 9".into()));
        assert_eq!(lyrics(&a), vec![l(None, false)]);
        // The attachment goes with its staff when it moves, and is dropped with it.
        a.run(Command::StaffMove("+1".into()));
        assert_eq!(lyrics(&a), vec![None, l(None, false)]);
        a.run(Command::StaffRemove(None));
        assert!(lyrics(&a).is_empty());
        a.run(Command::LyricsAttach("above 2".into()));
        a.run(Command::LyricsAttach("none".into()));
        assert!(lyrics(&a).is_empty());
        // Deleting the source track detaches lyrics taken from it.
        a.run(Command::LyricsAttach("below 2".into()));
        a.run(Command::Track(2));
        a.run(Command::TrackDelete);
        assert!(lyrics(&a).is_empty());
    }

    #[test]
    fn staves_of_a_track() {
        let mut a = app();
        keys(&mut a, "i3<Space>5<Esc>");
        let bars = a.buffer().score.tracks[0].bars.clone();
        let staves = |a: &App| a.buffer().score.tracks[0].staves();
        use cecilia_core::Clef::{Grand, Treble};
        assert_eq!(staves(&a), vec![Staff::Standard(Treble), Staff::Tab]);
        // Added at the bottom; the cursor goes to the new staff, which can be changed.
        a.run(Command::StaffAdd(Some("slash".into())));
        a.run(Command::StaffAdd(Some("grand G3".into())));
        assert_eq!(staves(&a)[3], Staff::Standard(Grand { split: 55 }));
        assert_eq!(a.pane().cursor.staff, 3);
        a.run(Command::Notation("tab+".into()));
        // Reordered: to the top, then two places down.
        a.run(Command::StaffMove("1".into()));
        assert_eq!(
            staves(&a),
            vec![
                Staff::TabRhythm,
                Staff::Standard(Treble),
                Staff::Tab,
                Staff::Slash
            ]
        );
        a.run(Command::StaffMove("+2".into()));
        assert_eq!(staves(&a)[2], Staff::TabRhythm);
        assert_eq!(a.pane().cursor.staff, 2);
        // Removed down to one, which stays.
        a.run(Command::StaffRemove(Some(1)));
        assert_eq!(staves(&a), vec![Staff::Tab, Staff::TabRhythm, Staff::Slash]);
        a.run(Command::StaffRemove(None));
        a.run(Command::StaffRemove(None));
        a.run(Command::StaffRemove(None));
        assert_eq!(staves(&a).len(), 1);
        a.run(Command::Notation("bogus".into()));
        assert_eq!(staves(&a).len(), 1);
        // The notes never changed; undo brings the staves back.
        assert_eq!(a.buffer().score.tracks[0].bars, bars);
        keys(&mut a, "uuuuuuuu");
        assert_eq!(staves(&a), vec![Staff::Standard(Treble), Staff::Tab]);

        // A piano is one track on a grand staff; a tab for it puts the same pitches on strings.
        a.run(Command::TrackAdd("piano".into()));
        let t = a.pane().cursor.track;
        assert_eq!(a.buffer().score.tracks.len(), 2);
        assert_eq!(
            a.buffer().score.tracks[t].staves(),
            vec![Staff::Standard(Grand { split: 48 })]
        );
        keys(&mut a, "ic<Esc>");
        a.run(Command::StaffAdd(Some("tab".into())));
        let piano = &a.buffer().score.tracks[t];
        assert_eq!(piano.kind, TrackKind::Fretted);
        assert_eq!(piano.staves().len(), 2);
        let note = &piano.bars[0].voices[0].beats[0].notes[0];
        assert_eq!(piano.pitch_of(note), 72);
    }

    #[test]
    fn chant_staff_on_an_ordinary_track() {
        let mut a = app();
        a.run(Command::StaffAdd(Some("chant".into())));
        assert_eq!(a.pane().cursor.staff, 2);
        // h under the c4 clef is A4 (la, two steps below do = C5); the guitar plays it.
        keys(&mut a, "ihv");
        let t = a.buffer().score.tracks[0].clone();
        let beat =
            |a: &App, i: usize| a.buffer().score.tracks[0].bars[0].voices[0].beats[i].clone();
        let n = &beat(&a, 0).notes[0];
        assert_eq!(t.pitch_of(n), 69);
        assert_eq!(n.chant.as_ref().unwrap().mods, vec!["v".to_string()]);
        assert_eq!(a.pane().cursor.line, 7);
        // A note after it, then one starting a new neume, a division after that.
        keys(&mut a, "<Space>g/;<Esc>");
        assert_eq!(t.pitch_of(&beat(&a, 1).notes[0]), 67);
        let third = beat(&a, 2);
        assert!(third.notes[0].chant.as_ref().unwrap().neume_start);
        assert_eq!(third.chant_division.as_deref(), Some(";"));
        // The other staves show the same notes.
        assert_eq!(a.buffer().score.tracks[0].staves().len(), 3);
    }

    #[test]
    fn default_clef_from_config() {
        let (cfg, errors) = Config::parse("[score]\nclef = \"bass\"\n").unwrap();
        assert!(errors.is_empty());
        let mut a = App::new(cfg, None);
        a.run(Command::New);
        a.run(Command::StaffAdd(None));
        assert_eq!(
            a.buffer().score.tracks[0].staves()[2],
            Staff::Standard(cecilia_core::Clef::Bass)
        );
    }

    /// Sounding pitches of a beat of the cursor track, high first.
    fn pitches(a: &App, beat: usize) -> Vec<u8> {
        let c = a.pane().cursor;
        let t = &a.buffer().score.tracks[c.track];
        let mut p: Vec<u8> = t.bars[c.bar].voices[0].beats[beat]
            .notes
            .iter()
            .map(|n| t.pitch_of(n))
            .collect();
        p.sort_unstable_by(|a, b| b.cmp(a));
        p
    }

    #[test]
    fn the_cursor_stays_on_the_line_of_a_slash_staff() {
        let mut a = app();
        a.run(Command::StaffAdd(Some("slash".into())));
        let slash = a.buffer().score.tracks[0]
            .staves()
            .iter()
            .position(|s| *s == Staff::Slash)
            .unwrap();
        // Landing on it from another staff with a stray line, as a click does.
        let id = a.focused();
        a.pane_mut().cursor.staff = slash;
        a.pane_mut().cursor.line = 41;
        a.normalize_cursor(id);
        assert_eq!(a.pane().cursor.line, 34);
        // Moving between staves and within the slash staff keeps it on the line.
        a.pane_mut().cursor.staff = 0;
        a.pane_mut().cursor.line = 29;
        keys(&mut a, "<A-j>");
        keys(&mut a, "<A-j>");
        assert_eq!(a.pane().cursor.staff, slash);
        assert_eq!(a.pane().cursor.line, 34);
        keys(&mut a, "jkjk");
        assert_eq!(a.pane().cursor.line, 34);
        // Another kind of staff keeps what a click set.
        a.pane_mut().cursor.staff = 0;
        a.pane_mut().cursor.line = 31;
        a.normalize_cursor(id);
        assert_eq!(a.pane().cursor.line, 31);
    }

    #[test]
    fn cursor_moves_across_staves() {
        let mut a = app();
        a.config.file.ui.jk_moves_between_staves = true;
        let at = |a: &App| {
            let c = a.pane().cursor;
            (c.track, crate::app::cursor_row(&a.buffer().score, &c))
        };
        use crate::app::Row::{Line, String};
        // A guitar starts on its tab, under a treble-8vb staff (bottom line E4 written,
        // diatonic 30): up from the tab is the space below that staff.
        assert_eq!(at(&a), (0, String(1, 0)));
        keys(&mut a, "k");
        assert_eq!(at(&a), (0, Line(0, 29)));
        keys(&mut a, "kkk");
        assert_eq!(at(&a), (0, Line(0, 32)));
        keys(&mut a, "jjjj");
        assert_eq!(at(&a), (0, String(1, 0)));
        // Past the last string: the track's next staff (a slash staff), and back.
        a.run(Command::StaffAdd(Some("slash".into())));
        a.pane_mut().cursor.staff = 1;
        a.pane_mut().cursor.string = 0;
        keys(&mut a, "jjjjjj");
        assert_eq!(at(&a), (0, Line(2, 34)));
        keys(&mut a, "k");
        assert_eq!(at(&a), (0, String(1, 5)));
        // Past the last staff the cursor stays put; j and k never change track.
        a.run(Command::TrackAdd("piano".into()));
        a.pane_mut().cursor.track = 0;
        a.pane_mut().cursor.staff = 2;
        let last = at(&a);
        keys(&mut a, "j");
        assert_eq!(at(&a), last);
        a.pane_mut().cursor.track = 1;
        a.pane_mut().cursor.staff = 0;
        keys(&mut a, "kkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkk");
        let top = at(&a);
        assert_eq!(top.0, 1);
        keys(&mut a, "k");
        assert_eq!(at(&a), top);
    }

    #[test]
    fn staff_add_takes_several_staves_in_order() {
        let mut a = app();
        let before = a.buffer().score.tracks[0].staves().len();
        a.run(Command::parse("staff-add slash grand G3 tab+ chant").unwrap());
        let staves = a.buffer().score.tracks[0].staves().to_vec();
        assert_eq!(staves.len(), before + 4);
        assert_eq!(
            staves[before..]
                .iter()
                .map(|s| s.name())
                .collect::<Vec<_>>(),
            ["slash", "grand G3", "tab+", "chant"]
        );
        // The cursor goes to the last one; the whole command is one undo step.
        assert_eq!(a.pane().cursor.staff, before + 3);
        a.run(Command::Undo);
        assert_eq!(a.buffer().score.tracks[0].staves().len(), before);
        // A bad word adds none of them.
        a.run(Command::parse("staff-add tab bogus").unwrap());
        assert_eq!(a.buffer().score.tracks[0].staves().len(), before);
        assert!(a.message.as_ref().unwrap().error);
    }

    #[test]
    fn standard_staff_insert() {
        let mut a = app();
        a.config.file.ui.jk_moves_between_staves = true;
        keys(&mut a, "kkkk");
        assert_eq!((a.pane().cursor.staff, a.pane().cursor.line), (0, 32));
        // Written G4 under the cursor: a is A4 written (A3 sounding on guitar), c then the
        // nearest C (C5 written), replacing it; # sharpens it.
        keys(&mut a, "ia");
        assert_eq!(pitches(&a, 0), vec![57]);
        assert_eq!(a.pane().cursor.line, 33);
        keys(&mut a, "c");
        assert_eq!(pitches(&a, 0), vec![60]);
        keys(&mut a, "#");
        assert_eq!(pitches(&a, 0), vec![61]);
        // Off the note, a letter adds to the chord.
        keys(&mut a, "<Up><Up>e");
        assert_eq!(pitches(&a, 0), vec![64, 61]);
        keys(&mut a, "<Space>g<Esc>");
        assert_eq!(pitches(&a, 1), vec![67]);
        keys(&mut a, "u");
        assert_eq!(
            beats(&a.buffer().score, &a.pane().cursor)
                .iter()
                .filter(|b| !b.empty)
                .count(),
            0
        );

        // A piano chord: notes on distinct strings, highest first.
        a.run(Command::TrackAdd("piano".into()));
        keys(&mut a, "ic<Up><Up>e<Up><Up>g<Esc>");
        assert_eq!(pitches(&a, 0), vec![79, 76, 72]);
        let strings: Vec<u8> = a.buffer().score.tracks[a.pane().cursor.track].bars[0].voices[0]
            .beats[0]
            .notes
            .iter()
            .map(|n| n.string)
            .collect();
        assert_eq!(strings, vec![0, 1, 2]);
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
    fn gv_reselects_the_last_selection() {
        let mut a = app();
        keys(&mut a, "a1<Space>2<Space>3<Space>4<Esc>gg");
        // Nothing selected yet: a no-op with a message.
        keys(&mut a, "gv");
        assert_eq!(a.mode, Mode::Normal);
        assert!(a.message.as_ref().unwrap().error);
        keys(&mut a, "vll<Esc>gg");
        assert_eq!(a.mode, Mode::Normal);
        assert!(a.selection().is_none());
        keys(&mut a, "gv");
        let (anchor, cursor, bars) = a.selection().unwrap();
        assert_eq!((anchor.beat, cursor.beat, bars), (0, 2, false));
        // A bar selection, left by an operator; the kind is kept and the other pane/mode
        // state does not matter.
        keys(&mut a, "<Esc>}V}<Esc>gg");
        keys(&mut a, "gv");
        let (anchor, cursor, bars) = a.selection().unwrap();
        assert_eq!((anchor.bar, cursor.bar, bars), (1, 2, true));
        // After an operator.
        keys(&mut a, "<Esc>ggvlld");
        assert_eq!(a.mode, Mode::Normal);
        keys(&mut a, "gv");
        // (The beats are gone, so the end is clamped to the bar's remaining beat.)
        let (anchor, cursor, _) = a.selection().unwrap();
        assert_eq!((anchor.bar, anchor.beat, cursor.bar), (0, 0, 0));
        assert!(cursor.beat < beats(&a.buffer().score, &cursor).len());
        // gv in Visual mode swaps with the current selection.
        keys(&mut a, "<Esc>}vgv");
        let (anchor, cursor, _) = a.selection().unwrap();
        assert_eq!((anchor.bar, cursor.bar), (0, 0));
        keys(&mut a, "gv");
        let (anchor, _, _) = a.selection().unwrap();
        assert_eq!(anchor.bar, 1);
    }

    #[test]
    fn gv_clamps_to_a_shrunk_score() {
        let mut a = app();
        keys(&mut a, "7}Vk<Esc>gg");
        keys(&mut a, "5dd");
        let bars = a.buffer().score.master_bars.len();
        keys(&mut a, "gv");
        let (anchor, cursor, _) = a.selection().unwrap();
        assert!(anchor.bar < bars && cursor.bar < bars);
        // The marks jump to the ends too.
        keys(&mut a, "<Esc>gg`>");
        assert_eq!(a.pane().cursor.bar, anchor.bar.max(cursor.bar));
    }

    #[test]
    fn gi_goes_back_to_where_insert_mode_was_left() {
        let mut a = app();
        // Never inserted: Insert mode on the cursor.
        keys(&mut a, "}gi");
        assert_eq!(a.mode, Mode::Insert);
        assert_eq!(a.pane().cursor.bar, 1);
        keys(&mut a, "<Esc>");
        keys(&mut a, "a3<Space>4<Esc>");
        let left = a.pane().cursor;
        keys(&mut a, "gg}}");
        assert_ne!(a.pane().cursor, left);
        keys(&mut a, "gi");
        assert_eq!(a.mode, Mode::Insert);
        assert_eq!(a.pane().cursor, left);
        keys(&mut a, "<Esc>gg`^");
        assert_eq!(a.pane().cursor, left);
        // The old behaviour is on <leader>i, and adds nothing.
        keys(&mut a, "gg");
        let beats = a.buffer().score.tracks[0].bars[0].voices[0].beats.len();
        keys(&mut a, "<Space>i");
        assert_eq!(a.mode, Mode::Insert);
        assert_eq!(a.pane().cursor.bar, 0);
        assert_eq!(
            a.buffer().score.tracks[0].bars[0].voices[0].beats.len(),
            beats
        );
        keys(&mut a, "<Esc>");
        // Edits that shrink the score clamp the position.
        keys(&mut a, "7}i5<Esc>gg7dd");
        keys(&mut a, "gi");
        assert!(a.pane().cursor.bar < a.buffer().score.master_bars.len());
    }

    #[test]
    fn gv_and_gi_are_listed_in_the_keymap_picker() {
        let mut a = app();
        keys(&mut a, "<Space>?");
        keys(&mut a, "visual-reselect");
        assert_eq!(a.palette().unwrap().items[0].text, "gv");
        keys(&mut a, "<C-u>insert-last");
        assert_eq!(a.palette().unwrap().items[0].text, "gi");
        keys(&mut a, "<C-u>insert-here");
        assert_eq!(a.palette().unwrap().items[0].text, "<Space>i");
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
        keys(&mut a, "i5H<A-4>b<Esc>");
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
