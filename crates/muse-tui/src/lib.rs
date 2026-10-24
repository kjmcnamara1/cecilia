//! Terminal frontend for muse.

mod tabview;

use std::time::{Duration, Instant};

use anyhow::Result;
use muse_app::playback::Playback;
use muse_app::{App, Key, KeyCode, Mode};
use muse_audio::Location;
use ratatui::crossterm::event::{
    self, DisableMouseCapture, EnableMouseCapture, Event, KeyEvent, KeyEventKind, KeyModifiers,
    MouseButton, MouseEventKind,
};
use ratatui::crossterm::execute;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Clear, Paragraph};
use ratatui::{DefaultTerminal, Frame};

use tabview::{Hit, TabView};

/// Runs the TUI until the user quits.
pub fn run(mut app: App) -> Result<()> {
    let mut terminal = ratatui::try_init()?;
    execute!(std::io::stdout(), EnableMouseCapture)?;
    let result = event_loop(&mut terminal, &mut app);
    let _ = execute!(std::io::stdout(), DisableMouseCapture);
    ratatui::restore();
    result
}

fn event_loop(terminal: &mut DefaultTerminal, app: &mut App) -> Result<()> {
    let mut playback = Playback::default();
    let mut hits: Vec<Hit> = Vec::new();
    let mut last_config_check = Instant::now();
    while !app.quit {
        playback.sync(app);
        let play_loc = playback.location(app);
        if app.playing {
            if let Some((buffer, loc, _, _)) = play_loc {
                app.follow_playback(buffer, loc.bar, loc.tick);
            }
        }
        terminal.draw(|f| draw(f, app, play_loc, &mut hits))?;

        let timeout = if app.playing {
            Duration::from_millis(16)
        } else {
            Duration::from_millis(250)
        };
        if event::poll(timeout)? {
            match event::read()? {
                Event::Key(k) if k.kind != KeyEventKind::Release => {
                    if let Some(key) = convert_key(k) {
                        app.handle_key(key);
                    }
                }
                Event::Mouse(m) => match m.kind {
                    MouseEventKind::Down(MouseButton::Left) => {
                        if let Some(hit) = hits.iter().find(|h| contains(h.area, m.column, m.row)) {
                            app.tabs[app.active_tab].focus = hit.pane;
                            if let Some(p) = app.panes.get_mut(&hit.pane) {
                                p.cursor = hit.cursor;
                            }
                        } else if let Some((id, _)) = app
                            .layout()
                            .into_iter()
                            .find(|(_, r)| contains(to_rect(*r), m.column, m.row))
                        {
                            app.tabs[app.active_tab].focus = id;
                        }
                    }
                    MouseEventKind::ScrollDown => app.run(muse_app::Command::BarNext),
                    MouseEventKind::ScrollUp => app.run(muse_app::Command::BarPrev),
                    _ => {}
                },
                _ => {}
            }
        }
        for effect in app.take_effects() {
            playback.apply(app, effect);
        }
        if last_config_check.elapsed() > Duration::from_secs(1) {
            last_config_check = Instant::now();
            app.poll_config();
        }
    }
    Ok(())
}

fn contains(r: Rect, x: u16, y: u16) -> bool {
    x >= r.x && x < r.x + r.width && y >= r.y && y < r.y + r.height
}

fn to_rect(r: muse_app::layout::Rect) -> Rect {
    Rect {
        x: r.x,
        y: r.y,
        width: r.w,
        height: r.h,
    }
}

fn convert_key(k: KeyEvent) -> Option<Key> {
    use ratatui::crossterm::event::KeyCode as C;
    let code = match k.code {
        C::Char(c) => KeyCode::Char(c),
        C::Esc => KeyCode::Esc,
        C::Enter => KeyCode::Enter,
        C::Tab => KeyCode::Tab,
        C::BackTab => KeyCode::BackTab,
        C::Backspace => KeyCode::Backspace,
        C::Delete => KeyCode::Delete,
        C::Up => KeyCode::Up,
        C::Down => KeyCode::Down,
        C::Left => KeyCode::Left,
        C::Right => KeyCode::Right,
        C::Home => KeyCode::Home,
        C::End => KeyCode::End,
        C::PageUp => KeyCode::PageUp,
        C::PageDown => KeyCode::PageDown,
        C::F(n) => KeyCode::F(n),
        _ => return None,
    };
    let ctrl = k.modifiers.contains(KeyModifiers::CONTROL);
    let code = match code {
        KeyCode::Char(c) if ctrl => KeyCode::Char(c.to_ascii_lowercase()),
        other => other,
    };
    Some(Key {
        code,
        ctrl,
        alt: k.modifiers.contains(KeyModifiers::ALT),
    })
}

fn draw(
    f: &mut Frame,
    app: &mut App,
    play: Option<(usize, Location, f64, f64)>,
    hits: &mut Vec<Hit>,
) {
    hits.clear();
    let [tabline, main, status, cmdline] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Min(3),
        Constraint::Length(1),
        Constraint::Length(1),
    ])
    .areas(f.area());

    app.viewport = muse_app::layout::Rect {
        x: main.x,
        y: main.y,
        w: main.width,
        h: main.height,
    };
    draw_tabline(f, app, tabline);

    let focused = app.focused();
    for (id, r) in app.layout() {
        let area = to_rect(r);
        let pane = app.panes[&id].clone();
        let buffer = &app.buffers[pane.buffer];
        let track = buffer.score.tracks.get(pane.cursor.track);
        let title = match track {
            Some(t) => format!(
                " {} — {}/{} {} ",
                buffer.name,
                pane.cursor.track + 1,
                buffer.score.tracks.len(),
                if t.name.is_empty() {
                    "Track"
                } else {
                    t.name.trim()
                }
            ),
            None => format!(" {} ", buffer.name),
        };
        let border_style = if id == focused {
            Style::default().fg(Color::Yellow)
        } else {
            Style::default().fg(Color::DarkGray)
        };
        let block = Block::bordered()
            .border_type(BorderType::Rounded)
            .border_style(border_style)
            .title(title);
        let inner = block.inner(area);
        f.render_widget(block, area);
        if let Some(track) = track {
            app.page_bars = page_bars(inner, track);
        }
        let play_here = play
            .filter(|(b, ..)| *b == pane.buffer && app.playing)
            .map(|(_, l, ..)| (l.bar, l.tick));
        let mut scroll = pane.scroll;
        f.render_widget(
            TabView {
                app,
                pane: id,
                focused: id == focused,
                play: play_here,
                hits,
                scroll: &mut scroll,
            },
            inner,
        );
        if let Some(p) = app.panes.get_mut(&id) {
            p.scroll = scroll;
        }
    }

    draw_status(f, app, play, status);
    draw_cmdline(f, app, cmdline);
    if !app.pending.is_empty() {
        draw_which_key(f, app, main);
    }
    if app.show_help {
        draw_help(f, app, main);
    }
}

fn page_bars(area: Rect, track: &muse_core::Track) -> usize {
    let rows = (area.height / tabview::system_height(track)).max(1) as usize;
    rows * 2
}

fn draw_tabline(f: &mut Frame, app: &App, area: Rect) {
    let mut spans = vec![];
    for (i, tab) in app.tabs.iter().enumerate() {
        let name = app
            .panes
            .get(&tab.focus)
            .map_or("?", |p| app.buffers[p.buffer].name.as_str());
        let style = if i == app.active_tab {
            Style::default().fg(Color::Black).bg(Color::Yellow)
        } else {
            Style::default().fg(Color::Gray).bg(Color::Rgb(40, 40, 40))
        };
        spans.push(Span::styled(format!(" {} {} ", i + 1, name), style));
        spans.push(Span::raw(" "));
    }
    f.render_widget(Paragraph::new(Line::from(spans)), area);
}

fn fmt_time(t: f64) -> String {
    let t = t.max(0.0) as u64;
    format!("{}:{:02}", t / 60, t % 60)
}

fn draw_status(f: &mut Frame, app: &App, play: Option<(usize, Location, f64, f64)>, area: Rect) {
    let (mode, mode_style) = match app.mode {
        Mode::Normal => (
            " NORMAL ",
            Style::default().fg(Color::Black).bg(Color::Blue),
        ),
        Mode::Insert => (
            " INSERT ",
            Style::default().fg(Color::Black).bg(Color::Yellow),
        ),
        Mode::Visual => (
            " VISUAL ",
            Style::default().fg(Color::Black).bg(Color::Magenta),
        ),
        Mode::Command => (
            " COMMAND ",
            Style::default().fg(Color::Black).bg(Color::Green),
        ),
    };
    let pane = app.pane();
    let score = &app.buffer().score;
    let mut right = String::new();
    if let Some(t) = score.tracks.get(pane.cursor.track) {
        let tuning: Vec<&str> = t
            .strings
            .iter()
            .rev()
            .map(|&p| {
                [
                    "C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B",
                ][p as usize % 12]
            })
            .collect();
        if t.kind != muse_core::TrackKind::Drums {
            right.push_str(&tuning.join(" "));
            if t.capo > 0 {
                right.push_str(&format!(" capo {}", t.capo));
            }
            right.push_str("  ");
        }
        right.push_str(&format!(
            "bar {}/{}  beat {}  str {}  ♩={}",
            pane.cursor.bar + 1,
            score.master_bars.len(),
            pane.cursor.beat + 1,
            pane.cursor.string + 1,
            score.tempo
        ));
    }
    if let Some((_, _, t, total)) = play {
        let icon = if app.playing { "▶" } else { "⏸" };
        right.push_str(&format!("  {icon} {}/{}", fmt_time(t), fmt_time(total)));
    }
    right.push_str("  ");
    right.push_str(
        &app.pa
            .status(app.config.file.audio.speed, score.audio.is_some()),
    );
    if app.follow {
        right.push_str("  follow");
    }
    let line = Line::from(vec![
        Span::styled(mode, mode_style.add_modifier(Modifier::BOLD)),
        Span::raw(" "),
        Span::styled(score.meta.title.clone(), Style::default().fg(Color::White)),
        Span::raw(if score.meta.artist.is_empty() {
            String::new()
        } else {
            format!(" — {}", score.meta.artist)
        }),
    ]);
    let bg = Style::default().bg(Color::Rgb(30, 30, 30));
    f.render_widget(Paragraph::new(line).style(bg), area);
    f.render_widget(Paragraph::new(right + " ").right_aligned().style(bg), area);
}

fn draw_cmdline(f: &mut Frame, app: &App, area: Rect) {
    match app.mode {
        Mode::Command => {
            f.render_widget(Paragraph::new(format!(":{}", app.cmdline)), area);
            f.set_cursor_position((area.x + 1 + app.cmdline.chars().count() as u16, area.y));
        }
        Mode::Normal | Mode::Insert | Mode::Visual => {
            if let Some(m) = &app.message {
                let style = if m.error {
                    Style::default().fg(Color::Red)
                } else {
                    Style::default()
                };
                f.render_widget(Paragraph::new(m.text.clone()).style(style), area);
            }
            let mut pending = app.count.map(|c| c.to_string()).unwrap_or_default();
            pending.push_str(&muse_app::keys::format_keys(&app.pending));
            if !pending.is_empty() {
                f.render_widget(Paragraph::new(pending).right_aligned(), area);
            }
        }
    }
}

/// Converts a command to its kebab-case name for display.
fn command_name(cmd: &muse_app::Command) -> String {
    let dbg = format!("{cmd:?}");
    let head = dbg.split(['(', ' ', '{']).next().unwrap_or(&dbg);
    let mut out = String::new();
    for (i, c) in head.chars().enumerate() {
        if c.is_uppercase() && i > 0 {
            out.push('-');
        }
        out.push(c.to_ascii_lowercase());
    }
    out
}

fn draw_which_key(f: &mut Frame, app: &App, main: Rect) {
    let items = app.config.keymap(Mode::Normal).continuations(&app.pending);
    if items.is_empty() {
        return;
    }
    let lines: Vec<Line> = items
        .iter()
        .map(|(keys, cmds)| {
            let names: Vec<String> = cmds.iter().map(command_name).collect();
            Line::from(vec![
                Span::styled(
                    format!("{:>8} ", muse_app::keys::format_keys(keys)),
                    Style::default().fg(Color::Yellow),
                ),
                Span::raw(names.join(", ")),
            ])
        })
        .collect();
    let h = (lines.len() as u16 + 2).min(main.height);
    let w = 40.min(main.width);
    let area = Rect {
        x: main.x + main.width - w,
        y: main.y + main.height - h,
        width: w,
        height: h,
    };
    f.render_widget(Clear, area);
    let title = format!(" {} ", muse_app::keys::format_keys(&app.pending));
    f.render_widget(
        Paragraph::new(lines).block(Block::bordered().title(title)),
        area,
    );
}

fn draw_help(f: &mut Frame, app: &App, main: Rect) {
    let mut items: Vec<(String, String)> = app
        .config
        .keymap(Mode::Normal)
        .bindings
        .iter()
        .map(|(k, c)| {
            (
                muse_app::keys::format_keys(k),
                c.iter().map(command_name).collect::<Vec<_>>().join(", "),
            )
        })
        .collect();
    items.sort_by(|a, b| a.1.cmp(&b.1).then(a.0.cmp(&b.0)));
    let w = main.width.saturating_sub(4).min(100);
    let h = main.height.saturating_sub(2);
    let area = Rect {
        x: main.x + (main.width - w) / 2,
        y: main.y + 1,
        width: w,
        height: h,
    };
    let col_w = 40usize;
    let cols = ((w as usize - 2) / col_w).max(1);
    let rows = (h as usize).saturating_sub(2).max(1);
    let mut lines = vec![String::new(); rows];
    for (i, (k, c)) in items.iter().enumerate().take(rows * cols) {
        let cell = format!("{k:>16}  {c:<20}");
        lines[i % rows].push_str(&format!("{cell:<col_w$}"));
    }
    f.render_widget(Clear, area);
    f.render_widget(
        Paragraph::new(lines.join("\n"))
            .block(Block::bordered().title(" Key bindings — Esc to close ")),
        area,
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_names_are_kebab_case() {
        assert_eq!(command_name(&muse_app::Command::BarNext), "bar-next");
        assert_eq!(command_name(&muse_app::Command::GotoBar(3)), "goto-bar");
        assert_eq!(
            command_name(&muse_app::Command::Quit { force: true }),
            "quit"
        );
    }
}

#[cfg(test)]
mod render_tests {
    use super::*;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    fn render(app: &mut App, w: u16, h: u16) -> String {
        let mut terminal = Terminal::new(TestBackend::new(w, h)).unwrap();
        let mut hits = vec![];
        terminal.draw(|f| draw(f, app, None, &mut hits)).unwrap();
        let buf = terminal.backend().buffer().clone();
        (0..h)
            .map(|y| {
                (0..w)
                    .map(|x| buf[(x, y)].symbol().to_string())
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// Renders a real file: `MUSE_RENDER_FILE=path cargo test -p muse-tui render_file -- --nocapture --ignored`
    #[test]
    #[ignore]
    fn render_file() {
        let Ok(path) = std::env::var("MUSE_RENDER_FILE") else {
            return;
        };
        let mut app = App::new(muse_app::Config::default(), None);
        app.run(muse_app::Command::Edit(path.into()));
        if let Ok(keys) = std::env::var("MUSE_RENDER_KEYS")
            .map(|k| k.trim().to_string())
            .and_then(|k| {
                if k.is_empty() {
                    Err(std::env::VarError::NotPresent)
                } else {
                    Ok(k)
                }
            })
        {
            for k in muse_app::keys::parse_keys(&keys, &[Key::char(' ')]).unwrap() {
                app.handle_key(k);
            }
        }
        println!("{}", render(&mut app, 120, 40));
    }

    #[test]
    fn renders_frets_and_status() {
        use muse_core::*;
        let beat = |fret| Beat {
            notes: vec![Note::new(1, fret)],
            ..Beat::default()
        };
        let bar = Bar {
            voices: vec![Voice {
                beats: vec![beat(5), beat(7), beat(12), beat(0)],
            }],
        };
        let track = Track {
            name: "Lead".into(),
            strings: vec![64, 59, 55, 50, 45, 40],
            bars: vec![bar; 6],
            ..Track::default()
        };
        let score = Score {
            tempo: 90,
            master_bars: vec![MasterBar::default(); 6],
            tracks: vec![track],
            ..Score::default()
        };
        let mut app = App::new(muse_app::Config::default(), None);
        app.buffers[0] = muse_app::Buffer {
            path: None,
            name: "t.gp5".into(),
            score,
            modified: false,
            history: Default::default(),
            version: 0,
        };
        let screen = render(&mut app, 80, 20);
        assert!(screen.contains("Lead"), "{screen}");
        assert!(screen.contains("12"), "{screen}");
        assert!(screen.contains("NORMAL"), "{screen}");
        assert!(screen.contains("bar 1/6"), "{screen}");
    }
}
