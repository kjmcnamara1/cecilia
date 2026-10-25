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
    /// Changed since it was opened or last written.
    pub modified: bool,
    pub history: crate::history::History,
    /// Incremented on every change, so views know to re-layout.
    pub version: u64,
}

/// A position in a score: track, bar, beat index (voice 1) and string.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Cursor {
    pub track: usize,
    pub bar: usize,
    pub beat: usize,
    pub string: usize,
    /// Voice being edited (0 = voice 1).
    pub voice: usize,
}

#[derive(Debug, Clone)]
pub struct Pane {
    pub buffer: usize,
    pub cursor: Cursor,
    /// First visible row, maintained by the frontend.
    pub scroll: usize,
    /// GUI score layout of this pane: "page", "vertical" or "horizontal".
    pub layout: String,
    /// GUI: show the audio waveform strip at the bottom of this pane.
    pub waveform: bool,
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
    /// Continue paused playback from where it stopped.
    Resume,
    /// Pause and rewind to where playback started.
    Stop {
        buffer: usize,
        bar: usize,
        tick: u32,
    },
    /// The configuration changed (e.g. a new SoundFont).
    ConfigReloaded,
    /// A buffer's score was edited.
    ScoreChanged {
        buffer: usize,
    },
    /// A buffer's audio track was attached, detached or changed.
    AudioChanged {
        buffer: usize,
    },
    /// Mixer, loop, metronome, speed or pitch settings changed.
    PlaybackSettings,
    /// Record a sync point for the next bar at the current audio position.
    SyncTap,
    /// Generate sync points from beat detection.
    SyncDetect,
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
    /// Command-line completion in progress: candidates, where they start and which is shown.
    completion: Option<(crate::complete::Completions, Option<usize>)>,
    pub message: Option<Message>,
    pub show_help: bool,
    /// Set by the frontend to reflect the player state.
    pub playing: bool,
    /// Buffer being played, set when playback starts.
    pub playing_buffer: Option<usize>,
    play_start: Option<(PaneId, Cursor)>,
    /// Playback is paused (Play mode stays on until stopped).
    pub paused: bool,
    pub follow: bool,
    /// Command-line completion ignores case (`ui.completion_ignore_case`).
    pub completion_ignore_case: bool,
    /// GUI: a waveform bar across the whole window above the status line, for the focused
    /// pane's file (`ui.global_waveform`).
    pub global_waveform: bool,
    /// Screen area for panes, set by the frontend.
    pub viewport: Rect,
    /// Bars moved by page-down/up, set by the frontend.
    pub page_bars: usize,
    pub(crate) effects: Vec<Effect>,
    pub quit: bool,
    /// Layout for panes opened without another to copy from (`ui.layout`); each pane keeps
    /// its own (`Pane::layout`).
    pub score_layout: String,
    /// Chant tracks show square notation (false: their modern transcription).
    pub chant_view: bool,
    pub zoom: f64,
    pub(crate) ed: crate::editor::EditorState,
    /// Play-along settings.
    pub pa: crate::playalong::PlayAlong,
}

impl App {
    pub fn new(config: Config, config_path: Option<PathBuf>) -> Self {
        let follow = config.file.ui.follow_playback;
        let completion_ignore_case = config.file.ui.completion_ignore_case;
        let global_waveform = config.file.ui.global_waveform;
        let (score_layout, zoom) = (config.file.ui.layout.clone(), config.file.ui.zoom);
        let config_mtime = config_path.as_deref().and_then(mtime);
        let mut panes = HashMap::new();
        panes.insert(
            1,
            Pane {
                buffer: 0,
                cursor: Cursor::default(),
                scroll: 0,
                layout: score_layout.clone(),
                waveform: true,
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
                modified: false,
                history: Default::default(),
                version: 0,
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
            completion: None,
            message: None,
            show_help: false,
            playing: false,
            playing_buffer: None,
            play_start: None,
            paused: false,
            follow,
            completion_ignore_case,
            global_waveform,
            viewport: Rect {
                x: 0,
                y: 0,
                w: 80,
                h: 24,
            },
            page_bars: 4,
            effects: Vec::new(),
            quit: false,
            score_layout,
            chant_view: true,
            zoom,
            ed: Default::default(),
            pa: Default::default(),
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

    pub(crate) fn pane_mut(&mut self) -> &mut Pane {
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
            modified: false,
            history: Default::default(),
            version: 0,
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
            "\"{}\" {} — {} tracks, {} bars, tempo {}",
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
        let hint = (s.audio.is_none())
            .then(|| b.path.as_deref().and_then(crate::playalong::find_audio_for))
            .flatten()
            .map(|a| {
                format!(
                    " — matching audio {}, :audio to attach",
                    a.file_name().unwrap_or_default().to_string_lossy()
                )
            });
        self.info(msg + &hint.unwrap_or_default());
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
                self.completion_ignore_case = self.config.file.ui.completion_ignore_case;
                self.global_waveform = self.config.file.ui.global_waveform;
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
        self.dispatch_key(key);
    }

    pub(crate) fn handle_command_key(&mut self, key: Key) {
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
        if !matches!(key.code, KeyCode::Tab | KeyCode::BackTab) {
            self.completion = None;
        }
        match key.code {
            KeyCode::Tab => self.complete_step(true),
            KeyCode::BackTab => self.complete_step(false),
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
        // Back to Play mode while a song is playing or paused.
        self.mode = if self.playing || self.paused {
            Mode::Play
        } else {
            Mode::Normal
        };
        self.cmdline.clear();
        self.history_pos = None;
        self.completion = None;
    }

    /// Tab (or Shift-Tab backwards): extend the word to the candidates' common prefix, then
    /// cycle through them.
    fn complete_step(&mut self, forward: bool) {
        use crate::complete::{common_prefix, complete};
        if let Some((c, index)) = self.completion.as_mut() {
            if c.items.len() > 1 {
                let n = c.items.len();
                let i = match (*index, forward) {
                    (None, true) => 0,
                    (None, false) => n - 1,
                    (Some(i), true) => (i + 1) % n,
                    (Some(i), false) => (i + n - 1) % n,
                };
                *index = Some(i);
                self.cmdline.truncate(c.start);
                self.cmdline.push_str(&c.items[i]);
            }
            return;
        }
        let cwd = std::env::current_dir().unwrap_or_default();
        let score_dir = self
            .buffer()
            .path
            .as_deref()
            .and_then(Path::parent)
            .map(Path::to_path_buf);
        let c = complete(
            &self.cmdline,
            &cwd,
            score_dir.as_deref(),
            self.completion_ignore_case,
        );
        let typed = self.cmdline[c.start..].to_string();
        match c.items.len() {
            0 => self.error(format!("no completions for '{typed}'")),
            1 => {
                self.cmdline.truncate(c.start);
                self.cmdline.push_str(&c.items[0]);
                if c.commands || !c.items[0].ends_with('/') {
                    self.cmdline.push(' ');
                }
            }
            _ => {
                let prefix = common_prefix(&c.items, self.completion_ignore_case);
                if prefix.chars().count() > typed.chars().count() {
                    self.cmdline.truncate(c.start);
                    self.cmdline.push_str(&prefix);
                    self.completion = Some((c, None));
                } else {
                    self.completion = Some((c, None));
                    self.complete_step(forward);
                }
            }
        }
    }

    /// Completion candidates to show while completing, with the selected one.
    pub fn wildmenu(&self) -> Option<(&[String], Option<usize>)> {
        self.completion
            .as_ref()
            .filter(|(c, _)| c.items.len() > 1)
            .map(|(c, i)| (c.items.as_slice(), *i))
    }

    pub(crate) fn run_counted(&mut self, cmd: Command, count: Option<usize>) {
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
        if self.run_edit(&cmd) || self.run_playalong(&cmd) || self.run_chant(&cmd) {
            return;
        }
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
                } else if self.paused && self.mode == Mode::Play {
                    // Carry on from where it was paused.
                    self.paused = false;
                    self.effects.push(Effect::Resume);
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
                self.paused = false;
                self.ed.visual_anchor = None;
                self.mode = Mode::Play;
                self.effects.push(Effect::Play {
                    buffer: pane.buffer,
                    bar: pane.cursor.bar,
                    tick,
                });
            }
            Pause => {
                if self.mode == Mode::Play {
                    self.paused = true;
                }
                self.effects.push(Effect::Pause);
            }
            Stop => {
                self.paused = false;
                if self.mode == Mode::Play {
                    self.mode = Mode::Normal;
                }
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
            CompletionIgnoreCase(on) => {
                self.completion_ignore_case = on.unwrap_or(!self.completion_ignore_case);
                // Candidates found under the old setting no longer apply.
                self.completion = None;
                self.info(if self.completion_ignore_case {
                    "completion ignores case"
                } else {
                    "completion matches case"
                });
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
            Write(path) => self.write(path),
            WriteQuit => {
                if self.buffer().modified {
                    self.write(None);
                    if self.message.as_ref().is_some_and(|m| m.error) {
                        return;
                    }
                }
                self.close_pane();
            }
            Source => self.reload_config(),
            CommandMode => {
                self.mode = Mode::Command;
                self.cmdline.clear();
            }
            Normal => {
                self.leave_command_mode();
                self.ed.visual_anchor = None;
            }
            Help => self.show_help = !self.show_help,
            Layout(name) => {
                // Only the focused pane changes.
                let pane = self.pane_mut();
                pane.layout = name.unwrap_or_else(|| {
                    match pane.layout.as_str() {
                        "vertical" => "page",
                        "page" => "horizontal",
                        _ => "vertical",
                    }
                    .to_string()
                });
                let msg = format!("layout: {}", pane.layout);
                self.info(msg);
            }
            ZoomIn => self.zoom = (self.zoom * 1.15).min(6.0),
            ZoomOut => self.zoom = (self.zoom / 1.15).max(0.2),
            ZoomReset => self.zoom = self.config.file.ui.zoom,
            Nop => {}
            _ => {}
        }
    }

    /// Writes the focused buffer. Without a path it writes back to the buffer's file, but only
    /// when there are changes, so opening and saving never rewrites an untouched original.
    fn write(&mut self, path: Option<PathBuf>) {
        let index = self.pane().buffer;
        let buffer = &self.buffers[index];
        let target = match path {
            Some(p) => p,
            None => {
                let Some(p) = buffer.path.clone() else {
                    return self.error("no file name (use :w <file>)");
                };
                if !buffer.modified {
                    return self.info("no changes to write");
                }
                p
            }
        };
        // Guitar Pro keeps the backing track inside the file: embed an attached recording.
        let mut embedded;
        let mut score = &buffer.score;
        let is_gp = target
            .extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("gp"));
        if let Some(audio) = score
            .audio
            .as_ref()
            .filter(|a| is_gp && a.embedded.is_none())
            && let Some(file) = audio.file.as_deref()
        {
            let file = buffer
                .path
                .as_deref()
                .and_then(Path::parent)
                .map_or_else(|| PathBuf::from(file), |d| d.join(file));
            match std::fs::read(&file) {
                Ok(bytes) => {
                    embedded = score.clone();
                    embedded.audio.as_mut().unwrap().embedded = Some(std::sync::Arc::new(bytes));
                    score = &embedded;
                }
                Err(e) => return self.error(format!("{}: {e}", file.display())),
            }
        }
        match muse_formats::write_file(score, &target) {
            Ok(()) => {
                let size = std::fs::metadata(&target).map(|m| m.len()).unwrap_or(0);
                if buffer.path.as_deref() == Some(target.as_path()) {
                    self.buffers[index].modified = false;
                }
                self.info(format!("\"{}\" written, {size} bytes", target.display()));
            }
            Err(e) => self.error(format!("{}: {e}", target.display())),
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

    /// A new pane on `buffer`, with the focused pane's layout.
    fn new_pane(&mut self, buffer: usize) -> PaneId {
        let id = self.next_pane;
        self.next_pane += 1;
        let (layout, waveform) = self.panes.get(&self.focused()).map_or_else(
            || (self.score_layout.clone(), true),
            |p| (p.layout.clone(), p.waveform),
        );
        self.panes.insert(
            id,
            Pane {
                buffer,
                cursor: Cursor::default(),
                scroll: 0,
                layout,
                waveform,
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

    /// Records whether the player is running (called by the frontend). When a song ends by
    /// itself, Play mode ends with it.
    pub fn set_playing(&mut self, playing: bool) {
        if self.playing && !playing && !self.paused && self.mode == Mode::Play {
            self.mode = Mode::Normal;
            self.play_start = None;
        }
        self.playing = playing;
    }

    /// While following playback, keeps the focused pane's cursor on the playing beat (the
    /// frontend calls this each frame while playing). With following off the cursor stays put,
    /// so turning it off mid-playback leaves the cursor where the music was at that moment.
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
        .and_then(|b| b.voices.get(c.voice))
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
        voice: 0,
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

pub(crate) fn clamp_cursor(score: &Score, c: &mut Cursor) {
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
            modified: false,
            history: Default::default(),
            version: 0,
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
    fn write_exports_and_protects_untouched_originals() {
        let dir = std::env::temp_dir().join(format!("muse-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let mut app = test_app();
        let out = dir.join("out.gp5");
        app.run(Command::Write(Some(out.clone())));
        assert!(out.exists(), "{:?}", app.message);
        app.buffers[0].path = Some(out.clone());
        app.run(Command::Write(None));
        assert_eq!(app.message.as_ref().unwrap().text, "no changes to write");
        app.run(Command::Write(Some(dir.join("out.unknown"))));
        assert!(app.message.as_ref().unwrap().error);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn waveform_strip_is_per_pane_and_window_bar_toggles() {
        let mut app = App::new(Config::default(), None);
        let first = app.focused();
        app.run(Command::Split);
        let second = app.focused();
        app.run(Command::Waveform);
        assert!(!app.panes[&second].waveform);
        assert!(app.panes[&first].waveform);
        assert!(!app.global_waveform);
        app.run(Command::WaveformGlobal(None));
        assert!(app.global_waveform);
        app.run(Command::WaveformGlobal(Some(false)));
        assert!(!app.global_waveform);
        let (config, _) = Config::parse("[ui]\nglobal_waveform = true\n").unwrap();
        assert!(App::new(config, None).global_waveform);
    }

    #[test]
    fn play_mode_is_separate_from_normal() {
        let mut app = App::new(Config::default(), None);
        app.run(Command::New);
        keys(&mut app, "i3<Space>5<Esc>0");
        keys(&mut app, "<Space><Space>");
        assert_eq!(app.mode, Mode::Play);
        app.set_playing(true);
        // Editing keys do nothing while playing; navigation still works.
        let before = app.buffer().score.clone();
        keys(&mut app, "xdd");
        assert!(app.buffer().score == before);
        keys(&mut app, "l");
        assert_eq!(app.pane().cursor.beat, 1);
        // Pausing stays in Play mode; resuming continues.
        keys(&mut app, "<Space><Space>");
        app.set_playing(false);
        assert!(app.paused && app.mode == Mode::Play);
        app.take_effects();
        keys(&mut app, "<Space><Space>");
        assert_eq!(app.take_effects(), vec![Effect::Resume]);
        app.set_playing(true);
        // The command line returns to Play mode.
        keys(&mut app, ":<Esc>");
        assert_eq!(app.mode, Mode::Play);
        // Esc stops and returns to Normal.
        keys(&mut app, "<Esc>");
        assert_eq!(app.mode, Mode::Normal);
        // A song that ends by itself ends Play mode too.
        keys(&mut app, "<Space><Space>");
        app.set_playing(true);
        app.set_playing(false);
        assert_eq!(app.mode, Mode::Normal);
    }

    #[test]
    fn layout_belongs_to_the_focused_pane() {
        let mut app = App::new(Config::default(), None);
        let first = app.focused();
        assert_eq!(app.pane().layout, "vertical");
        // A split starts with the layout of the pane it came from.
        app.run(Command::Layout(Some("page".into())));
        app.run(Command::Split);
        let second = app.focused();
        assert_ne!(first, second);
        assert_eq!(app.pane().layout, "page");
        // Changing it changes only the focused pane.
        app.run(Command::Layout(None));
        assert_eq!(app.panes[&second].layout, "horizontal");
        assert_eq!(app.panes[&first].layout, "page");
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
        // Following playback moves the cursor with the playhead…
        app.playing = true;
        app.follow_playback(0, 3, 1000);
        assert_eq!((app.pane().cursor.bar, app.pane().cursor.beat), (3, 1));
        // …until following is turned off: then it stays where it was.
        keys(&mut app, "<Space>f");
        assert!(!app.follow);
        app.follow_playback(0, 5, 0);
        assert_eq!((app.pane().cursor.bar, app.pane().cursor.beat), (3, 1));
        keys(&mut app, "<Space>f");
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
