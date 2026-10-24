//! Application state machine: buffers, panes, tabs, modes and key handling.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use muse_core::{Beat, Score};

use crate::command::Command;
use crate::config::{Config, Lookup, Mode};
use crate::keys::{Key, KeyCode};
use crate::layout::{Axis, Node, PaneId, Rect, neighbour};

/// An open score.
#[derive(Debug, Clone)]
pub struct Buffer {
    pub path: Option<PathBuf>,
    pub name: String,
    pub score: Score,
}

/// A position in a score: track, bar, beat index (voice 1) and string.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Cursor {
    pub track: usize,
    pub bar: usize,
    pub beat: usize,
    pub string: usize,
}

#[derive(Debug, Clone)]
pub struct Pane {
    pub buffer: usize,
    pub cursor: Cursor,
    /// First visible row, maintained by the frontend.
    pub scroll: usize,
}

#[derive(Debug, Clone)]
pub struct Tab {
    pub root: Node,
    pub focus: PaneId,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Message {
    pub text: String,
    pub error: bool,
}

/// Requests for the frontend, drained with [`App::take_effects`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Effect {
    /// Start playback of a buffer from a bar/tick position.
    Play {
        buffer: usize,
        bar: usize,
        tick: u32,
    },
    Pause,
    /// Pause and rewind to where playback started.
    Stop {
        buffer: usize,
        bar: usize,
        tick: u32,
    },
    /// The configuration changed (e.g. a new SoundFont).
    ConfigReloaded,
}

pub struct App {
    pub config: Config,
    pub config_path: Option<PathBuf>,
    config_mtime: Option<SystemTime>,
    pub buffers: Vec<Buffer>,
    pub panes: HashMap<PaneId, Pane>,
    next_pane: PaneId,
    pub tabs: Vec<Tab>,
    pub active_tab: usize,
    pub mode: Mode,
    /// Keys typed so far of a multi-key binding.
    pub pending: Vec<Key>,
    pub count: Option<usize>,
    pub cmdline: String,
    history: Vec<String>,
    history_pos: Option<usize>,
    pub message: Option<Message>,
    pub show_help: bool,
    /// Set by the frontend to reflect the player state.
    pub playing: bool,
    /// Buffer being played, set when playback starts.
    pub playing_buffer: Option<usize>,
    play_start: Option<(PaneId, Cursor)>,
    pub follow: bool,
    /// Screen area for panes, set by the frontend.
    pub viewport: Rect,
    /// Bars moved by page-down/up, set by the frontend.
    pub page_bars: usize,
    effects: Vec<Effect>,
    pub quit: bool,
}

impl App {
    pub fn new(config: Config, config_path: Option<PathBuf>) -> Self {
        let follow = config.file.ui.follow_playback;
        let config_mtime = config_path.as_deref().and_then(mtime);
        let mut panes = HashMap::new();
        panes.insert(
            1,
            Pane {
                buffer: 0,
                cursor: Cursor::default(),
                scroll: 0,
            },
        );
        Self {
            config,
            config_path,
            config_mtime,
            buffers: vec![Buffer {
                path: None,
                name: "[No file]".into(),
                score: Score::default(),
            }],
            panes,
            next_pane: 2,
            tabs: vec![Tab {
                root: Node::Leaf(1),
                focus: 1,
            }],
            active_tab: 0,
            mode: Mode::Normal,
            pending: Vec::new(),
            count: None,
            cmdline: String::new(),
            history: Vec::new(),
            history_pos: None,
            message: None,
            show_help: false,
            playing: false,
            playing_buffer: None,
            play_start: None,
            follow,
            viewport: Rect {
                x: 0,
                y: 0,
                w: 80,
                h: 24,
            },
            page_bars: 4,
            effects: Vec::new(),
            quit: false,
        }
    }

    pub fn take_effects(&mut self) -> Vec<Effect> {
        std::mem::take(&mut self.effects)
    }

    pub fn tab(&self) -> &Tab {
        &self.tabs[self.active_tab]
    }

    pub fn focused(&self) -> PaneId {
        self.tab().focus
    }

    pub fn pane(&self) -> &Pane {
        &self.panes[&self.focused()]
    }

    fn pane_mut(&mut self) -> &mut Pane {
        let id = self.focused();
        self.panes.get_mut(&id).unwrap()
    }

    pub fn buffer(&self) -> &Buffer {
        &self.buffers[self.pane().buffer]
    }

    pub fn layout(&self) -> Vec<(PaneId, Rect)> {
        let mut out = Vec::new();
        self.tab().root.layout(self.viewport, &mut out);
        out
    }

    pub fn info(&mut self, text: impl Into<String>) {
        self.message = Some(Message {
            text: text.into(),
            error: false,
        });
    }

    pub fn error(&mut self, text: impl Into<String>) {
        self.message = Some(Message {
            text: text.into(),
            error: true,
        });
    }

    // ---- files ----

    /// Opens a file into a buffer (reusing one already open) and returns its index.
    pub fn open(&mut self, path: &Path) -> Result<usize, String> {
        let canonical = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
        if let Some(i) = self
            .buffers
            .iter()
            .position(|b| b.path.as_deref() == Some(&canonical))
        {
            return Ok(i);
        }
        let score =
            muse_formats::read_file(path).map_err(|e| format!("{}: {e}", path.display()))?;
        let name = path.file_name().map_or_else(
            || path.display().to_string(),
            |n| n.to_string_lossy().into(),
        );
        // Replace the initial empty buffer if nothing refers to anything else yet.
        let buffer = Buffer {
            path: Some(canonical),
            name,
            score,
        };
        if self.buffers.len() == 1 && self.buffers[0].path.is_none() {
            self.buffers[0] = buffer;
            return Ok(0);
        }
        self.buffers.push(buffer);
        Ok(self.buffers.len() - 1)
    }

    fn show_buffer(&mut self, buffer: usize) {
        let pane = self.pane_mut();
        pane.buffer = buffer;
        pane.cursor = Cursor::default();
        pane.scroll = 0;
        let b = &self.buffers[buffer];
        let s = &b.score;
        let msg = format!(
            "\"{}\" {} — {} tracks, {} bars, ♩={}",
            b.name,
            if s.meta.title.is_empty() {
                ""
            } else {
                &s.meta.title
            },
            s.tracks.len(),
            s.master_bars.len(),
            s.tempo
        );
        self.info(msg);
    }

    // ---- config ----

    /// Reloads the config file if it changed on disk.
    pub fn poll_config(&mut self) {
        let Some(path) = self.config_path.clone() else {
            return;
        };
        let m = mtime(&path);
        if m != self.config_mtime {
            self.config_mtime = m;
            self.reload_config();
        }
    }

    pub fn reload_config(&mut self) {
        let Some(path) = self.config_path.clone() else {
            self.error("no config file");
            return;
        };
        match Config::load(&path) {
            Ok((config, errors)) => {
                self.config = config;
                self.follow = self.config.file.ui.follow_playback;
                self.effects.push(Effect::ConfigReloaded);
                if errors.is_empty() {
                    self.info(format!("config reloaded: {}", path.display()));
                } else {
                    self.error(errors.join("; "));
                }
            }
            Err(e) => self.error(format!("config not reloaded: {e}")),
        }
    }

    // ---- keys ----

    pub fn handle_key(&mut self, key: Key) {
        match self.mode {
            Mode::Command => self.handle_command_key(key),
            Mode::Normal => self.handle_normal_key(key),
        }
    }

    fn handle_normal_key(&mut self, key: Key) {
        let close_help = self.show_help && self.pending.is_empty() && key.as_char() == Some('q');
        if key.code == KeyCode::Esc || close_help {
            self.show_help = false;
            self.pending.clear();
            self.count = None;
            return;
        }
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
        let lookup = match self.config.keymap(Mode::Normal).lookup(&self.pending) {
            Lookup::Exact(cmds) => Some(cmds.to_vec()),
            Lookup::Prefix => return,
            Lookup::None => None,
        };
        self.pending.clear();
        let count = self.count.take();
        if let Some(cmds) = lookup {
            self.message = None;
            for cmd in cmds {
                self.run_counted(cmd, count);
            }
        }
    }

    fn handle_command_key(&mut self, key: Key) {
        let bound = match self.config.keymap(Mode::Command).lookup(&[key]) {
            Lookup::Exact(cmds) => Some(cmds.to_vec()),
            _ => None,
        };
        if let Some(cmds) = bound {
            for cmd in cmds {
                self.run(cmd);
            }
            return;
        }
        match key.code {
            KeyCode::Esc => self.leave_command_mode(),
            KeyCode::Char('c') if key.ctrl => self.leave_command_mode(),
            KeyCode::Char('u') if key.ctrl => self.cmdline.clear(),
            KeyCode::Enter => {
                let line = std::mem::take(&mut self.cmdline);
                self.leave_command_mode();
                if !line.trim().is_empty() {
                    self.history.push(line.clone());
                }
                match Command::parse(&line) {
                    Ok(cmd) => self.run(cmd),
                    Err(e) => self.error(e),
                }
            }
            KeyCode::Backspace => {
                if self.cmdline.pop().is_none() {
                    self.leave_command_mode();
                }
            }
            KeyCode::Up | KeyCode::Down if !self.history.is_empty() => {
                let last = self.history.len() - 1;
                let pos = match (key.code, self.history_pos) {
                    (KeyCode::Up, None) => Some(last),
                    (KeyCode::Up, Some(p)) => Some(p.saturating_sub(1)),
                    (_, Some(p)) if p < last => Some(p + 1),
                    _ => None,
                };
                self.history_pos = pos;
                self.cmdline = pos.map(|p| self.history[p].clone()).unwrap_or_default();
            }
            KeyCode::Char(c) if !key.ctrl && !key.alt => self.cmdline.push(c),
            _ => {}
        }
    }

    fn leave_command_mode(&mut self) {
        self.mode = Mode::Normal;
        self.cmdline.clear();
        self.history_pos = None;
    }

    fn run_counted(&mut self, cmd: Command, count: Option<usize>) {
        match (&cmd, count) {
            (Command::GotoFirst | Command::GotoLast, Some(n)) => self.run(Command::GotoBar(n)),
            (c, Some(n)) if c.repeatable() => {
                for _ in 0..n.min(10_000) {
                    self.run(cmd.clone());
                }
            }
            _ => self.run(cmd),
        }
    }

    /// Executes a command.
    pub fn run(&mut self, cmd: Command) {
        use Command::*;
        match cmd {
            BeatNext => self.move_cursor(|s, c| {
                step_beat(s, c, true);
            }),
            BeatPrev => self.move_cursor(|s, c| {
                step_beat(s, c, false);
            }),
            NoteNext => self.move_cursor(|s, c| step_note(s, c, true)),
            NotePrev => self.move_cursor(|s, c| step_note(s, c, false)),
            StringDown => self.move_cursor(|s, c| {
                let n = s.tracks.get(c.track).map_or(1, |t| t.strings.len().max(1));
                c.string = (c.string + 1).min(n - 1);
            }),
            StringUp => self.move_cursor(|_, c| c.string = c.string.saturating_sub(1)),
            BarNext => self.move_cursor(|s, c| set_bar(s, c, c.bar + 1)),
            BarPrev => self.move_cursor(|s, c| set_bar(s, c, c.bar.saturating_sub(1))),
            BarStart => self.move_cursor(|_, c| c.beat = 0),
            BarEnd => self.move_cursor(|s, c| c.beat = beats(s, c).len().saturating_sub(1)),
            GotoFirst => self.move_cursor(|s, c| set_bar(s, c, 0)),
            GotoLast => self.move_cursor(|s, c| set_bar(s, c, usize::MAX)),
            GotoBar(n) => self.move_cursor(|s, c| set_bar(s, c, n.saturating_sub(1))),
            TrackNext => self.move_cursor(|s, c| set_track(s, c, c.track + 1)),
            TrackPrev => self.move_cursor(|s, c| set_track(s, c, c.track.saturating_sub(1))),
            Track(n) => self.move_cursor(|s, c| set_track(s, c, n.saturating_sub(1))),
            PageDown => {
                let n = self.page_bars.max(1);
                self.move_cursor(|s, c| set_bar(s, c, c.bar + n))
            }
            PageUp => {
                let n = self.page_bars.max(1);
                self.move_cursor(|s, c| set_bar(s, c, c.bar.saturating_sub(n)))
            }
            PlayToggle => {
                if self.playing {
                    self.run(Pause)
                } else {
                    self.run(Play)
                }
            }
            Play => {
                let pane = self.pane().clone();
                let score = &self.buffers[pane.buffer].score;
                if score.tracks.is_empty() {
                    self.error("nothing to play");
                    return;
                }
                let tick = cursor_tick(score, &pane.cursor);
                self.play_start = Some((self.focused(), pane.cursor));
                self.playing_buffer = Some(pane.buffer);
                self.effects.push(Effect::Play {
                    buffer: pane.buffer,
                    bar: pane.cursor.bar,
                    tick,
                });
            }
            Pause => self.effects.push(Effect::Pause),
            Stop => {
                if let Some((pane_id, cursor)) = self.play_start.take() {
                    if let Some(p) = self.panes.get_mut(&pane_id) {
                        p.cursor = cursor;
                        let tick = cursor_tick(&self.buffers[p.buffer].score, &cursor);
                        self.effects.push(Effect::Stop {
                            buffer: p.buffer,
                            bar: cursor.bar,
                            tick,
                        });
                        return;
                    }
                }
                self.effects.push(Effect::Pause);
            }
            FollowToggle => {
                self.follow = !self.follow;
                self.info(if self.follow {
                    "follow playback on"
                } else {
                    "follow playback off"
                });
            }
            Split => self.split(Axis::Stacked),
            VSplit => self.split(Axis::SideBySide),
            Focus(dir) => {
                if let Some(id) = neighbour(&self.layout(), self.focused(), dir) {
                    self.tabs[self.active_tab].focus = id;
                }
            }
            ClosePane | Quit { .. } => self.close_pane(),
            QuitAll { .. } => self.quit = true,
            TabNext => self.active_tab = (self.active_tab + 1) % self.tabs.len(),
            TabPrev => self.active_tab = (self.active_tab + self.tabs.len() - 1) % self.tabs.len(),
            TabNew(path) => {
                let buffer = match path {
                    Some(p) => match self.open(&p) {
                        Ok(b) => b,
                        Err(e) => return self.error(e),
                    },
                    None => self.pane().buffer,
                };
                let id = self.new_pane(buffer);
                self.tabs.insert(
                    self.active_tab + 1,
                    Tab {
                        root: Node::Leaf(id),
                        focus: id,
                    },
                );
                self.active_tab += 1;
                self.show_buffer(buffer);
            }
            TabClose => {
                if self.tabs.len() == 1 {
                    self.error("cannot close the last tab");
                } else {
                    let tab = self.tabs.remove(self.active_tab);
                    for id in tab.root.panes() {
                        self.panes.remove(&id);
                    }
                    self.active_tab = self.active_tab.min(self.tabs.len() - 1);
                }
            }
            Edit(path) => match self.open(&path) {
                Ok(b) => self.show_buffer(b),
                Err(e) => self.error(e),
            },
            Source => self.reload_config(),
            CommandMode => {
                self.mode = Mode::Command;
                self.cmdline.clear();
            }
            Normal => self.leave_command_mode(),
            Help => self.show_help = !self.show_help,
            Nop => {}
        }
    }

    fn move_cursor(&mut self, f: impl FnOnce(&Score, &mut Cursor)) {
        let id = self.focused();
        let pane = self.panes.get_mut(&id).unwrap();
        let score = &self.buffers[pane.buffer].score;
        if score.tracks.is_empty() {
            return;
        }
        f(score, &mut pane.cursor);
        clamp_cursor(score, &mut pane.cursor);
    }

    fn new_pane(&mut self, buffer: usize) -> PaneId {
        let id = self.next_pane;
        self.next_pane += 1;
        self.panes.insert(
            id,
            Pane {
                buffer,
                cursor: Cursor::default(),
                scroll: 0,
            },
        );
        id
    }

    fn split(&mut self, axis: Axis) {
        let current = self.pane().clone();
        let id = self.new_pane(current.buffer);
        self.panes.get_mut(&id).unwrap().cursor = current.cursor;
        let focus = self.focused();
        let tab = &mut self.tabs[self.active_tab];
        tab.root.split(focus, id, axis);
        tab.focus = id;
    }

    fn close_pane(&mut self) {
        let focus = self.focused();
        let rects = self.layout();
        let tab = &mut self.tabs[self.active_tab];
        if tab.root.remove(focus) {
            self.panes.remove(&focus);
            let remaining = tab.root.panes();
            // Focus the nearest remaining pane, as vim does.
            tab.focus = [
                crate::command::Direction::Left,
                crate::command::Direction::Up,
            ]
            .into_iter()
            .chain([
                crate::command::Direction::Right,
                crate::command::Direction::Down,
            ])
            .find_map(|d| neighbour(&rects, focus, d).filter(|id| remaining.contains(id)))
            .unwrap_or(remaining[0]);
        } else if self.tabs.len() > 1 {
            self.run(Command::TabClose);
        } else {
            self.quit = true;
        }
    }

    // ---- playback feedback ----

    /// Moves cursors to the playback position (called by the frontend while playing).
    pub fn follow_playback(&mut self, buffer: usize, bar: usize, tick: u32) {
        if !self.follow {
            return;
        }
        let score = &self.buffers[buffer].score;
        let focused = self.focused();
        if let Some(p) = self.panes.get_mut(&focused).filter(|p| p.buffer == buffer) {
            if p.cursor.bar != bar
                || cursor_tick(score, &p.cursor) > tick
                || beat_end(score, &p.cursor) <= tick
            {
                p.cursor.bar = bar;
                p.cursor.beat = beat_at_tick(score, p.cursor.track, bar, tick);
            }
        }
    }
}

fn mtime(path: &Path) -> Option<SystemTime> {
    std::fs::metadata(path).and_then(|m| m.modified()).ok()
}

// ---- cursor helpers ----

/// Beats of voice 1 at the cursor's track and bar.
pub fn beats<'a>(score: &'a Score, c: &Cursor) -> &'a [Beat] {
    score
        .tracks
        .get(c.track)
        .and_then(|t| t.bars.get(c.bar))
        .and_then(|b| b.voices.first())
        .map_or(&[], |v| v.beats.as_slice())
}

/// Tick within the bar at which the cursor's beat starts.
pub fn cursor_tick(score: &Score, c: &Cursor) -> u32 {
    beats(score, c).iter().take(c.beat).map(Beat::ticks).sum()
}

fn beat_end(score: &Score, c: &Cursor) -> u32 {
    cursor_tick(score, c) + beats(score, c).get(c.beat).map_or(0, |b| b.ticks().max(1))
}

/// Index of the beat sounding at `tick` in a bar.
pub fn beat_at_tick(score: &Score, track: usize, bar: usize, tick: u32) -> usize {
    let c = Cursor {
        track,
        bar,
        beat: 0,
        string: 0,
    };
    let mut t = 0;
    for (i, b) in beats(score, &c).iter().enumerate() {
        t += b.ticks();
        if t > tick {
            return i;
        }
    }
    beats(score, &c).len().saturating_sub(1)
}

fn clamp_cursor(score: &Score, c: &mut Cursor) {
    c.track = c.track.min(score.tracks.len().saturating_sub(1));
    c.bar = c.bar.min(score.master_bars.len().saturating_sub(1));
    c.beat = c.beat.min(beats(score, c).len().saturating_sub(1));
    let strings = score
        .tracks
        .get(c.track)
        .map_or(1, |t| t.strings.len().max(1));
    c.string = c.string.min(strings - 1);
}

fn set_bar(score: &Score, c: &mut Cursor, bar: usize) {
    c.bar = bar.min(score.master_bars.len().saturating_sub(1));
    c.beat = 0;
}

fn set_track(score: &Score, c: &mut Cursor, track: usize) {
    let tick = cursor_tick(score, c);
    c.track = track.min(score.tracks.len().saturating_sub(1));
    c.beat = beat_at_tick(score, c.track, c.bar, tick);
}

fn step_beat(score: &Score, c: &mut Cursor, forward: bool) -> bool {
    if forward {
        if c.beat + 1 < beats(score, c).len() {
            c.beat += 1;
        } else if c.bar + 1 < score.master_bars.len() {
            c.bar += 1;
            c.beat = 0;
        } else {
            return false;
        }
    } else if c.beat > 0 {
        c.beat -= 1;
    } else if c.bar > 0 {
        c.bar -= 1;
        c.beat = beats(score, c).len().saturating_sub(1);
    } else {
        return false;
    }
    true
}

fn step_note(score: &Score, c: &mut Cursor, forward: bool) {
    let start = *c;
    while step_beat(score, c, forward) {
        if beats(score, c)
            .get(c.beat)
            .is_some_and(|b| !b.notes.is_empty())
        {
            return;
        }
    }
    *c = start;
}

#[cfg(test)]
mod tests {
    use super::*;
    use muse_core::*;

    fn test_app() -> App {
        let beat = |fret| Beat {
            duration: Duration::new(DurationValue::Quarter),
            notes: vec![Note::new(0, fret)],
            ..Beat::default()
        };
        let rest = Beat::rest(Duration::new(DurationValue::Quarter));
        let bar = Bar {
            voices: vec![Voice {
                beats: vec![beat(1), rest.clone(), rest, beat(3)],
            }],
        };
        let track = Track {
            strings: vec![64, 59, 55, 50, 45, 40],
            bars: vec![bar; 8],
            ..Track::default()
        };
        let score = Score {
            tempo: 120,
            master_bars: vec![MasterBar::default(); 8],
            tracks: vec![track.clone(), track],
            ..Score::default()
        };
        let mut app = App::new(Config::default(), None);
        app.buffers[0] = Buffer {
            path: None,
            name: "test".into(),
            score,
        };
        app
    }

    fn keys(app: &mut App, s: &str) {
        for k in crate::keys::parse_keys(s, &[Key::char(' ')]).unwrap() {
            app.handle_key(k);
        }
    }

    #[test]
    fn motions_and_counts() {
        let mut app = test_app();
        keys(&mut app, "l");
        assert_eq!(app.pane().cursor.beat, 1);
        keys(&mut app, "w");
        assert_eq!((app.pane().cursor.bar, app.pane().cursor.beat), (0, 3));
        keys(&mut app, "w");
        assert_eq!((app.pane().cursor.bar, app.pane().cursor.beat), (1, 0));
        keys(&mut app, "3}");
        assert_eq!(app.pane().cursor.bar, 4);
        keys(&mut app, "G");
        assert_eq!(app.pane().cursor.bar, 7);
        keys(&mut app, "2G");
        assert_eq!(app.pane().cursor.bar, 1);
        keys(&mut app, "gg$");
        assert_eq!((app.pane().cursor.bar, app.pane().cursor.beat), (0, 3));
        keys(&mut app, "0jjjjjjjj");
        assert_eq!(app.pane().cursor.string, 5);
        keys(&mut app, "J");
        assert_eq!(app.pane().cursor.track, 1);
    }

    #[test]
    fn command_line_and_panes() {
        let mut app = test_app();
        keys(&mut app, ":5<CR>");
        assert_eq!(app.pane().cursor.bar, 4);
        assert_eq!(app.mode, Mode::Normal);
        keys(&mut app, "<C-w>v");
        assert_eq!(app.tab().root.panes().len(), 2);
        let right = app.focused();
        keys(&mut app, "<C-w>h");
        assert_ne!(app.focused(), right);
        keys(&mut app, ":q<CR>");
        assert_eq!(app.tab().root.panes(), vec![right]);
        assert!(!app.quit);
        keys(&mut app, ":tabnew<CR>gt");
        assert_eq!(app.active_tab, 0);
        keys(&mut app, ":bogus<CR>");
        assert!(app.message.as_ref().is_some_and(|m| m.error));
        keys(&mut app, ":qa<CR>");
        assert!(app.quit);
    }

    #[test]
    fn play_and_stop_effects() {
        let mut app = test_app();
        keys(&mut app, "}ll<Space><Space>");
        assert_eq!(
            app.take_effects(),
            vec![Effect::Play {
                buffer: 0,
                bar: 1,
                tick: 1920
            }]
        );
        app.playing = true;
        app.follow_playback(0, 3, 1000);
        assert_eq!((app.pane().cursor.bar, app.pane().cursor.beat), (3, 1));
        keys(&mut app, "<Space>s");
        assert_eq!(
            app.take_effects(),
            vec![Effect::Stop {
                buffer: 0,
                bar: 1,
                tick: 1920
            }]
        );
        assert_eq!(app.pane().cursor.bar, 1);
    }
}
