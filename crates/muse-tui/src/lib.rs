//! Terminal frontend for muse.

mod chantview;
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

/// Mouse state between frames: what was drawn where, and where the mouse is.
#[derive(Default)]
struct Ui {
    hits: Vec<Hit>,
    tabline: Vec<(Rect, TablineHit)>,
    hover: Option<(u16, u16)>,
}

/// What a click on the tabline does.
#[derive(Debug, Clone, Copy, PartialEq)]
enum TablineHit {
    Buffer(usize),
    Close(usize),
    View(usize),
}

fn event_loop(terminal: &mut DefaultTerminal, app: &mut App) -> Result<()> {
    let mut playback = Playback::default();
    let mut ui = Ui::default();
    let mut last_config_check = Instant::now();
    while !app.quit {
        playback.sync(app);
        let play_loc = playback.location(app);
        if app.playing {
            if let Some((buffer, loc, _, _)) = play_loc {
                app.follow_playback(buffer, loc.bar, loc.tick);
            }
        }
        terminal.draw(|f| draw(f, app, play_loc, &mut ui))?;

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
                    MouseEventKind::Moved => ui.hover = Some((m.column, m.row)),
                    MouseEventKind::Down(button @ (MouseButton::Left | MouseButton::Middle)) => {
                        let tab_hit = ui
                            .tabline
                            .iter()
                            .rev()
                            .find(|(r, _)| contains(*r, m.column, m.row))
                            .map(|(_, h)| *h);
                        if let Some(hit) = tab_hit {
                            app.run(match (hit, button) {
                                (
                                    TablineHit::Buffer(n) | TablineHit::Close(n),
                                    MouseButton::Middle,
                                )
                                | (TablineHit::Close(n), _) => {
                                    muse_app::Command::BufferClose(Some(n + 1), false)
                                }
                                (TablineHit::Buffer(n), _) => muse_app::Command::BufferGoto(n + 1),
                                (TablineHit::View(n), _) => muse_app::Command::ViewGoto(n + 1),
                            });
                        } else if button == MouseButton::Middle {
                        } else if let Some(hit) =
                            ui.hits.iter().find(|h| contains(h.area, m.column, m.row))
                        {
                            app.views[app.active_view].focus = hit.pane;
                            if let Some(p) = app.panes.get_mut(&hit.pane) {
                                p.cursor = hit.cursor;
                            }
                        } else if let Some((id, _)) = app
                            .layout()
                            .into_iter()
                            .find(|(_, r)| contains(to_rect(*r), m.column, m.row))
                        {
                            app.views[app.active_view].focus = id;
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
            if let Some(effect) = muse_app::dialog::apply(app, effect) {
                playback.apply(app, effect);
            }
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

fn draw(f: &mut Frame, app: &mut App, play: Option<(usize, Location, f64, f64)>, ui: &mut Ui) {
    ui.hits.clear();
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
    draw_tabline(f, app, tabline, ui);
    let hits = &mut ui.hits;

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
        let chant = app.chant_view && track.is_some_and(|t| t.chant.is_some());
        if chant {
            f.render_widget(
                chantview::ChantView {
                    app,
                    pane: id,
                    focused: id == focused,
                    play: play_here,
                    hits,
                    scroll: &mut scroll,
                },
                inner,
            );
        } else {
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
        }
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
    if let Some(p) = app.palette() {
        draw_palette(f, &p, f.area());
    }
}

/// The command palette: a rounded box near the top centre with the typed command and the
/// completions of the word being typed below it (descriptions right-aligned for commands).
fn draw_palette(f: &mut Frame, p: &muse_app::Palette, screen: Rect) {
    const ROWS: usize = 12;
    let w = screen.width.saturating_sub(4).clamp(20, 76);
    let room = screen.height.saturating_sub(8) as usize;
    let rows = p.items.len().min(ROWS).min(room);
    let h = 3 + rows as u16 + u16::from(rows > 0);
    let area = Rect {
        x: screen.x + (screen.width.saturating_sub(w)) / 2,
        y: screen.y + 2,
        width: w,
        height: h.min(screen.height),
    };
    f.render_widget(Clear, area);
    let more = match p.selected {
        Some(i) if p.items.len() > 1 => format!(" {} / {} ", i + 1, p.items.len()),
        None if p.items.len() > rows => format!(" {} matches ", p.items.len()),
        _ => String::new(),
    };
    let block = Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(Color::Yellow))
        .title(format!(" {} ", p.title))
        .title_bottom(Line::from(more).right_aligned());
    let inner = block.inner(area);
    f.render_widget(block, area);
    let input = if p.prompt == ":" {
        format!(":{}", p.input)
    } else {
        format!("{} {}", p.prompt, p.input)
    };
    f.render_widget(Paragraph::new(input.clone()), Rect { height: 1, ..inner });
    f.set_cursor_position((inner.x + input.chars().count() as u16, inner.y));
    if rows == 0 {
        return;
    }
    let sep = "─".repeat(inner.width as usize);
    f.render_widget(
        Paragraph::new(sep).style(Style::default().fg(Color::DarkGray)),
        Rect {
            y: inner.y + 1,
            height: 1,
            ..inner
        },
    );
    let first = match p.selected {
        Some(sel) if sel >= rows => sel + 1 - rows,
        _ => 0,
    };
    for (k, item) in p.items.iter().enumerate().skip(first).take(rows) {
        let row = Rect {
            y: inner.y + 2 + (k - first) as u16,
            height: 1,
            ..inner
        };
        let selected = p.selected == Some(k);
        let style = if selected {
            Style::default().fg(Color::Black).bg(Color::Yellow)
        } else {
            Style::default()
        };
        let width = row.width as usize;
        let name: String = item.text.chars().take(width.saturating_sub(1)).collect();
        let mut line = format!(" {name}");
        if let Some(d) = &item.description {
            let room = width.saturating_sub(line.chars().count() + 3);
            let d: String = d.chars().take(room).collect();
            let pad = width.saturating_sub(line.chars().count() + d.chars().count() + 1);
            line.push_str(&" ".repeat(pad));
            line.push_str(&d);
            line.push(' ');
        }
        f.render_widget(Paragraph::new(line).style(style), row);
        // Highlight the characters that matched what was typed.
        let hl = if selected {
            Style::default()
                .fg(Color::Black)
                .add_modifier(Modifier::BOLD | Modifier::UNDERLINED)
        } else {
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD)
        };
        for &i in &item.matched {
            let x = row.x + 1 + i as u16;
            if x < row.x + row.width {
                f.buffer_mut().set_style(Rect { x, width: 1, ..row }, hl);
            }
        }
        if let Some(d) = item.description.as_deref().filter(|_| !selected) {
            // Dim the description part.
            let len = d.chars().take(width).count() as u16;
            let x = row.x + row.width.saturating_sub(len + 1);
            f.buffer_mut().set_style(
                Rect {
                    x,
                    width: len.min(row.width),
                    ..row
                },
                Style::default().fg(Color::DarkGray),
            );
        }
    }
}

fn page_bars(area: Rect, track: &muse_core::Track) -> usize {
    let height = if track.chant.is_some() {
        chantview::system_height()
    } else {
        tabview::system_height(track)
    };
    let rows = (area.height / height).max(1) as usize;
    rows * 2
}

/// File-type icon for a tab, by `tabline.icons`: Nerd Font glyphs, plain Unicode or none.
fn file_icon(kind: &str, style: &str) -> &'static str {
    match style {
        "none" => "",
        "unicode" => match kind {
            "guitar-pro" => "♪",
            "musicxml" => "♫",
            "midi" => "♬",
            "chant" => "✠",
            "muse" => "♩",
            _ => "·",
        },
        // Nerd Font (Font Awesome range): music, file-code, file-audio, book, headphones, file.
        _ => match kind {
            "guitar-pro" => "\u{f001}",
            "musicxml" => "\u{f1c9}",
            "midi" => "\u{f1c7}",
            "chant" => "\u{f02d}",
            "muse" => "\u{f025}",
            _ => "\u{f016}",
        },
    }
}

/// The tabline, as in bufferline.nvim: the active view's buffers on the left (icon, name
/// shortened in the middle, × when hovered, ● when modified) and the views on the right.
fn draw_tabline(f: &mut Frame, app: &App, area: Rect, ui: &mut Ui) {
    ui.tabline.clear();
    let bg = Style::default().bg(Color::Rgb(24, 24, 28));
    f.render_widget(Paragraph::new("").style(bg), area);
    let width = |s: &str| s.chars().count() as u16;

    // Views, right-aligned.
    let labels = app.tabline_views();
    let mut right = area.x + area.width;
    for (i, label) in labels.iter().enumerate().rev() {
        let text = format!(" {label} ");
        let w = width(&text);
        if right < area.x + w {
            break;
        }
        right -= w;
        let style = if i == app.active_view {
            Style::default()
                .fg(Color::Black)
                .bg(Color::Yellow)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::Gray).bg(Color::Rgb(45, 45, 52))
        };
        let r = Rect {
            x: right,
            y: area.y,
            width: w,
            height: 1,
        };
        f.render_widget(Paragraph::new(text).style(style), r);
        ui.tabline.push((r, TablineHit::View(i)));
        right = right.saturating_sub(1);
    }

    // Buffer tabs from the left, up to the views.
    let t = &app.config.file.tabline.truncation;
    let icons = app.config.file.tabline.icons.as_str();
    let mut x = area.x;
    for (n, tab) in app.bufferline().into_iter().enumerate() {
        let name =
            muse_app::truncate_middle(&tab.name, t.max_width, t.prefix_width, t.suffix_width);
        let icon = file_icon(tab.kind, icons);
        let mut head = if icon.is_empty() {
            format!(" {name} ")
        } else {
            format!(" {icon} {name} ")
        };
        // The file playing audio.
        let play_mark = if tab.playing {
            Some("▶")
        } else if tab.paused {
            Some("⏸")
        } else {
            None
        };
        if let Some(m) = play_mark {
            head.push_str(m);
            head.push(' ');
        }
        let w = width(&head) + 2;
        if x + w > right {
            break;
        }
        let r = Rect {
            x,
            y: area.y,
            width: w,
            height: 1,
        };
        let hovered = ui.hover.is_some_and(|(hx, hy)| contains(r, hx, hy));
        let style = if tab.active {
            Style::default()
                .fg(Color::White)
                .bg(Color::Rgb(70, 70, 85))
                .add_modifier(Modifier::BOLD)
        } else if hovered {
            Style::default().fg(Color::White).bg(Color::Rgb(50, 50, 60))
        } else {
            Style::default().fg(Color::Gray).bg(Color::Rgb(35, 35, 42))
        };
        let close = Rect {
            x: x + w - 2,
            y: area.y,
            width: 1,
            height: 1,
        };
        let mark = if hovered {
            "×"
        } else if tab.modified {
            "●"
        } else {
            " "
        };
        f.render_widget(Paragraph::new(format!("{head}{mark} ")).style(style), r);
        if hovered && ui.hover.is_some_and(|(hx, hy)| contains(close, hx, hy)) {
            f.buffer_mut()
                .set_style(close, Style::default().fg(Color::White).bg(Color::Red));
        } else if !hovered && tab.modified {
            f.buffer_mut()
                .set_style(close, Style::default().fg(Color::Yellow));
        }
        if play_mark.is_some() {
            let cell = Rect {
                x: x + width(&head) - 2,
                y: area.y,
                width: 1,
                height: 1,
            };
            f.buffer_mut()
                .set_style(cell, Style::default().fg(Color::Rgb(235, 70, 60)));
        }
        ui.tabline.push((r, TablineHit::Buffer(n)));
        ui.tabline.push((close, TablineHit::Close(n)));
        x += w + 1;
    }
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
        Mode::Picker => (" PICK ", Style::default().fg(Color::Black).bg(Color::Cyan)),
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
    if let Some(ws) = app.workspace_name() {
        right.push_str(&format!("  ws:{ws}"));
    }
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
        // Typed in the command palette.
        Mode::Command | Mode::Picker => {}
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
        let mut ui = Ui::default();
        terminal.draw(|f| draw(f, app, None, &mut ui)).unwrap();
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
    fn playhead_is_a_red_column_apart_from_the_cursor() {
        use muse_core::*;
        let beat = |fret| Beat {
            duration: Duration::new(DurationValue::Quarter),
            notes: vec![Note::new(1, fret)],
            ..Beat::default()
        };
        let score = Score {
            tempo: 90,
            master_bars: vec![MasterBar::default(); 2],
            tracks: vec![Track {
                strings: vec![64, 59, 55, 50, 45, 40],
                bars: vec![
                    Bar {
                        voices: vec![Voice {
                            beats: vec![beat(5), beat(7), beat(9), beat(0)]
                        }]
                    };
                    2
                ],
                ..Track::default()
            }],
            ..Score::default()
        };
        let mut app = App::new(muse_app::Config::default(), None);
        app.buffers[0].score = score;
        app.playing = true;
        let play = Some((0, muse_audio::Location { bar: 1, tick: 1440 }, 3.0, 6.0));
        let mut terminal = Terminal::new(TestBackend::new(80, 20)).unwrap();
        let mut ui = Ui::default();
        terminal.draw(|f| draw(f, &mut app, play, &mut ui)).unwrap();
        let buf = terminal.backend().buffer().clone();
        let red: Vec<(u16, u16)> = (0..20)
            .flat_map(|y| (0..80).map(move |x| (x, y)))
            .filter(|&(x, y)| buf[(x, y)].bg == ratatui::style::Color::Rgb(200, 30, 30))
            .collect();
        // One column, the six string rows, in bar 2 halfway through its second beat.
        let cols: std::collections::BTreeSet<u16> = red.iter().map(|p| p.0).collect();
        assert_eq!(cols.len(), 1, "{red:?}");
        assert_eq!(red.len(), 6);
        // The editing cursor has not moved.
        assert_eq!((app.pane().cursor.bar, app.pane().cursor.beat), (0, 0));
    }

    #[test]
    fn bufferline_shows_buffers_left_and_views_right() {
        let dir = std::env::temp_dir().join(format!("muse-bufferline-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let mut app = App::new(muse_app::Config::default(), None);
        app.run(muse_app::Command::New);
        let long = dir.join("A very long song title that does not fit.muse");
        let short = dir.join("short.gp5");
        app.run(muse_app::Command::Write(Some(long.clone())));
        app.run(muse_app::Command::Write(Some(short.clone())));
        let mut app = App::new(muse_app::Config::default(), None);
        app.run(muse_app::Command::Edit(long));
        app.run(muse_app::Command::Edit(short));
        app.run(muse_app::Command::ViewNew(None));
        let mut terminal = Terminal::new(TestBackend::new(100, 12)).unwrap();
        let mut ui = Ui::default();
        terminal.draw(|f| draw(f, &mut app, None, &mut ui)).unwrap();
        let buf = terminal.backend().buffer().clone();
        let top: String = (0..100).map(|x| buf[(x, 0)].symbol().to_string()).collect();
        // Long names lose their middle; icons come first; views sit at the right.
        assert!(top.contains("\u{f025} A very long so…t fit.muse"), "{top}");
        assert!(top.contains("\u{f001} short.gp5"), "{top}");
        assert!(top.trim_end().ends_with("1   2"), "{top}");
        // A lone view isn't shown, unless tabline.hide_single_view is off.
        let top_line = |app: &mut App| {
            let mut terminal = Terminal::new(TestBackend::new(100, 12)).unwrap();
            terminal
                .draw(|f| draw(f, app, None, &mut Ui::default()))
                .unwrap();
            let buf = terminal.backend().buffer().clone();
            (0..100)
                .map(|x| buf[(x, 0)].symbol().to_string())
                .collect::<String>()
        };
        app.run(muse_app::Command::ViewClose(false));
        assert!(top_line(&mut app).trim_end().ends_with("short.gp5"));
        app.config.file.tabline.hide_single_view = false;
        assert!(top_line(&mut app).trim_end().ends_with(" 1"));
        // Clicking a tab's × closes it (only this view had it… but view 1 still does).
        let close = ui
            .tabline
            .iter()
            .find(|(_, h)| *h == TablineHit::Close(1))
            .map(|(r, _)| *r)
            .unwrap();
        assert_eq!(close.width, 1);
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn renders_chant_staff_and_syllables() {
        let chant =
            muse_formats::gabc::parse("name: t;\n%%\n(c4) Al(fg)le(h.) lu(ixi)ia.(f) (::)\n")
                .unwrap();
        let mut app = App::new(muse_app::Config::default(), None);
        app.buffers[0].score = muse_formats::gabc::chant_score(chant);
        let screen = render(&mut app, 60, 24);
        assert!(screen.contains("Al- le lu- ia."), "{screen}");
        assert!(
            screen.contains('■') && screen.contains('♭') && screen.contains('‖'),
            "{screen}"
        );
        assert!(screen.contains("C─"), "{screen}");
        // Square notation off: the transcription shows as a staff track.
        app.run(muse_app::Command::ChantView);
        let screen = render(&mut app, 60, 24);
        assert!(screen.contains("F4"), "{screen}");
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
        // The tab of the file playing audio is marked.
        assert!(!screen.contains("▶"), "{screen}");
        app.playing_buffer = Some(0);
        app.playing = true;
        let screen = render(&mut app, 80, 20);
        assert!(screen.contains("t.gp5 ▶"), "{screen}");
    }
}
