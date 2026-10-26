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
    /// The toolbar's buttons (area, index into `App::toolbar_items`), and the hovered one's
    /// tooltip.
    toolbar: Vec<(Rect, usize)>,
    tooltip: Option<String>,
    /// Clickable areas of the tracks panel, and its first row and bar shown.
    tracks: Vec<(Rect, TracksHit)>,
    tracks_scroll: (usize, usize),
    /// What the tracks panel last scrolled to follow (cursor track, bar).
    tracks_anchor: Option<(usize, usize)>,
    /// The tracks panel's area, and its header row (dragged to resize it).
    tracks_area: Option<Rect>,
    /// A mouse drag in progress.
    drag: Option<Drag>,
    hits: Vec<Hit>,
    tabline: Vec<(Rect, TablineHit)>,
    hover: Option<(u16, u16)>,
}

/// A mouse drag in progress.
#[derive(Debug, Clone, PartialEq)]
enum Drag {
    /// A track's volume (one undo step).
    Volume,
    /// A border between panes, grabbed this far from its position.
    Divider(muse_app::layout::Divider, i32),
    /// The tracks panel's header: the row grabbed and the rows shown then.
    Tracks(u16, usize),
    /// A buffer tab being dragged along the tabline (its position now).
    Tab(usize),
}

/// A pane border under a cell: the divider and how far the cell is from its position.
fn divider_at(app: &App, x: u16, y: u16) -> Option<(muse_app::layout::Divider, i32)> {
    use muse_app::layout::Axis;
    app.dividers().into_iter().find_map(|d| {
        let a = d.area;
        let (along, across, lo, hi) = match d.axis {
            Axis::SideBySide => (x, y, a.y, a.y + a.h),
            Axis::Stacked => (y, x, a.x, a.x + a.w),
        };
        let off = d.pos as i32 - along as i32;
        ((0..=1).contains(&off) && across >= lo && across < hi).then_some((d, off))
    })
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
                    // Dragging a pane border or the tracks panel's header resizes them; a
                    // tab moves along the tabline.
                    MouseEventKind::Drag(MouseButton::Left)
                        if matches!(
                            ui.drag,
                            Some(Drag::Divider(..) | Drag::Tracks(..) | Drag::Tab(_))
                        ) =>
                    {
                        match ui.drag.clone() {
                            Some(Drag::Tab(n)) => {
                                let target = ui.tabline.iter().find_map(|(r, hit)| match hit {
                                    TablineHit::Buffer(t)
                                        if m.column >= r.x && m.column < r.x + r.width =>
                                    {
                                        Some(*t)
                                    }
                                    _ => None,
                                });
                                if let Some(t) = target.filter(|&t| t != n) {
                                    app.move_tab(n, t);
                                    ui.drag = Some(Drag::Tab(t));
                                }
                            }
                            Some(Drag::Divider(d, off)) => {
                                let along = match d.axis {
                                    muse_app::layout::Axis::SideBySide => m.column,
                                    muse_app::layout::Axis::Stacked => m.row,
                                };
                                app.drag_divider(&d, (along as i32 + off).max(0) as u16);
                            }
                            Some(Drag::Tracks(y0, rows0)) => {
                                let rows = rows0 as i64 + y0 as i64 - m.row as i64;
                                app.run(muse_app::Command::TracksResize(
                                    muse_app::command::Amount::To(rows.max(1) as u32),
                                ));
                            }
                            _ => {}
                        }
                    }
                    MouseEventKind::Down(MouseButton::Left)
                        if ui
                            .tracks_area
                            .is_some_and(|r| m.row == r.y && contains(r, m.column, m.row)) =>
                    {
                        let n = app.buffer().score.tracks.len();
                        ui.drag = Some(Drag::Tracks(m.row, n.clamp(1, app.tracks_rows.max(1))));
                    }
                    MouseEventKind::Down(MouseButton::Left)
                        if divider_at(app, m.column, m.row).is_some() =>
                    {
                        let (d, off) = divider_at(app, m.column, m.row).unwrap();
                        ui.drag = Some(Drag::Divider(d, off));
                    }
                    MouseEventKind::Down(MouseButton::Left)
                    | MouseEventKind::Drag(MouseButton::Left)
                        if ui.drag == Some(Drag::Volume)
                            || ui.tracks.iter().any(|(r, _)| contains(*r, m.column, m.row)) =>
                    {
                        click_tracks(app, &mut ui, m.column, m.row, m.kind);
                    }
                    MouseEventKind::Up(MouseButton::Left) if ui.drag.is_some() => {
                        if ui.drag.take() == Some(Drag::Volume) {
                            app.group_edits(false);
                        }
                    }
                    // The wheel over the tracks panel scrolls its rows (sideways: its bars).
                    MouseEventKind::ScrollDown
                    | MouseEventKind::ScrollUp
                    | MouseEventKind::ScrollLeft
                    | MouseEventKind::ScrollRight
                        if ui.tracks_area.is_some_and(|r| contains(r, m.column, m.row)) =>
                    {
                        let shift = m.modifiers.contains(KeyModifiers::SHIFT);
                        let (rows, bars): (i64, i64) = match (m.kind, shift) {
                            (MouseEventKind::ScrollDown, false) => (1, 0),
                            (MouseEventKind::ScrollUp, false) => (-1, 0),
                            (MouseEventKind::ScrollDown, true)
                            | (MouseEventKind::ScrollRight, _) => (0, 4),
                            _ => (0, -4),
                        };
                        let n = app.buffer().score.tracks.len();
                        let visible = n.clamp(1, app.tracks_rows.max(1));
                        let (first, first_bar) = ui.tracks_scroll;
                        ui.tracks_scroll = (
                            (first as i64 + rows).clamp(0, n.saturating_sub(visible) as i64)
                                as usize,
                            (first_bar as i64 + bars).max(0) as usize,
                        );
                    }
                    MouseEventKind::Down(MouseButton::Left)
                        if ui
                            .toolbar
                            .iter()
                            .any(|(r, _)| contains(*r, m.column, m.row)) =>
                    {
                        let i = ui
                            .toolbar
                            .iter()
                            .find(|(r, _)| contains(*r, m.column, m.row))
                            .map(|(_, i)| *i)
                            .unwrap();
                        app.run_toolbar(i);
                    }
                    MouseEventKind::Down(button @ (MouseButton::Left | MouseButton::Middle)) => {
                        let tab_hit = ui
                            .tabline
                            .iter()
                            .rev()
                            .find(|(r, _)| contains(*r, m.column, m.row))
                            .map(|(_, h)| *h);
                        if let Some(hit) = tab_hit {
                            if let (TablineHit::Buffer(n), MouseButton::Left) = (hit, button) {
                                ui.drag = Some(Drag::Tab(n));
                            }
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

/// A theme colour for the terminal.
pub(crate) fn rc(c: muse_app::theme::Rgba) -> Color {
    Color::Rgb(c.r, c.g, c.b)
}

/// Text on a theme-coloured background.
fn on(app: &App, bg: muse_app::theme::Rgba) -> Style {
    Style::default()
        .fg(rc(app.config.theme.text_on(bg)))
        .bg(rc(bg))
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
    let tracks_h = if app.tracks_shown() {
        1 + app
            .buffer()
            .score
            .tracks
            .len()
            .clamp(1, app.tracks_rows.max(1)) as u16
    } else {
        0
    };
    let [toolbar, tabline, main, tracks, status, cmdline] = Layout::vertical([
        Constraint::Length(u16::from(app.toolbar)),
        Constraint::Length(1),
        Constraint::Min(3),
        Constraint::Length(tracks_h),
        Constraint::Length(1),
        Constraint::Length(1),
    ])
    .areas(f.area());
    ui.tracks.clear();
    if tracks_h > 0 {
        draw_tracks(f, app, tracks, play, ui);
    }

    app.viewport = muse_app::layout::Rect {
        x: main.x,
        y: main.y,
        w: main.width,
        h: main.height,
    };
    draw_tabline(f, app, tabline, ui);
    ui.toolbar.clear();
    ui.tooltip = None;
    if app.toolbar {
        draw_toolbar(f, app, toolbar, ui);
    }
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
        let border_style = if id == focused && app.mode != Mode::Tracks {
            Style::default().fg(rc(app.config.theme.accent))
        } else {
            Style::default().fg(rc(app.config.theme.dim))
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
    // The hovered toolbar button's tooltip, on the command line.
    if let Some(tip) = ui.tooltip.clone() {
        let t = &app.config.theme;
        f.render_widget(
            Paragraph::new(format!(" {tip}")).style(
                Style::default()
                    .fg(rc(t.text_on(t.popover)))
                    .bg(rc(t.popover)),
            ),
            cmdline,
        );
    }
    if !app.pending.is_empty() {
        draw_which_key(f, app, main);
    }
    if app.show_help {
        draw_help(f, app, main);
    }
    if let Some(p) = app.palette() {
        draw_palette(f, app, &p, f.area());
    }
}

/// The command palette: a rounded box near the top centre with the typed command and the
/// completions of the word being typed below it (descriptions right-aligned for commands).
fn draw_palette(f: &mut Frame, app: &App, p: &muse_app::Palette, screen: Rect) {
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
        .border_style(Style::default().fg(rc(app.config.theme.accent)))
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
            on(app, app.config.theme.accent)
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

/// A toolbar button's symbol in the `tabline.icons` style (`none`: a short word).
fn tool_icon(b: &muse_app::toolbar::ToolButton, style: &str) -> String {
    let name = b.name.as_str();
    let playing = b.active && matches!(name, "play-toggle" | "play");
    let (nerd, unicode, word) = match name {
        _ if playing => ("\u{f04c}", "⏸", "pause"),
        "new" => ("\u{f0fe}", "✚", "new"),
        "open" | "open-dialog" | "e" | "edit" => ("\u{f07c}", "▤", "open"),
        "write" | "w" | "wall" | "wa" => ("\u{f0c7}", "▣", "save"),
        "undo" | "u" => ("\u{f0e2}", "↶", "undo"),
        "redo" => ("\u{f01e}", "↷", "redo"),
        "play-toggle" | "play" => ("\u{f04b}", "▶", "play"),
        "stop" => ("\u{f04d}", "■", "stop"),
        "follow-toggle" => ("\u{f05b}", "◎", "follow"),
        "loop-toggle" => ("\u{f079}", "↻", "loop"),
        "metronome" => ("\u{f017}", "♩", "click"),
        "count-in" => ("12", "12", "count"),
        "tracks" | "tracks-focus" => ("\u{f0c9}", "≡", "tracks"),
        "split" | "sp" => ("\u{f2d0}", "⊟", "split"),
        "vsplit" | "vs" => ("\u{f0db}", "◫", "vsplit"),
        "maximize" | "max" => ("\u{f065}", "⤢", "max"),
        "command-mode" => ("\u{f120}", ":", ":"),
        "pick-keymap" | "keymaps" => ("\u{f11c}", "⌨", "keys"),
        other => ("", "", other),
    };
    match style {
        "none" => word.to_string(),
        "unicode" if !unicode.is_empty() => unicode.to_string(),
        "nerd" if !nerd.is_empty() => nerd.to_string(),
        _ => word.to_string(),
    }
}

/// The toolbar: a button per configured command, `│` between groups.
fn draw_toolbar(f: &mut Frame, app: &App, area: Rect, ui: &mut Ui) {
    let t = &app.config.theme;
    let base = Style::default().bg(rc(t.toolbar)).fg(rc(t.chrome_text));
    f.render_widget(Paragraph::new("").style(base), area);
    let style = app.config.file.tabline.icons.as_str();
    let buf = f.buffer_mut();
    let mut x = area.x + 1;
    for (i, item) in app.toolbar_items().into_iter().enumerate() {
        let b = match item {
            muse_app::toolbar::ToolItem::Separator => {
                if x + 1 < area.x + area.width {
                    buf.set_string(x, area.y, "│", base.fg(rc(t.dim)));
                }
                x += 2;
                continue;
            }
            muse_app::toolbar::ToolItem::Button(b) => b,
        };
        let label = format!(" {} ", tool_icon(&b, style));
        let w = label.chars().count() as u16;
        if x + w > area.x + area.width {
            break;
        }
        let r = Rect {
            x,
            y: area.y,
            width: w,
            height: 1,
        };
        let hovered = ui.hover.is_some_and(|(hx, hy)| contains(r, hx, hy));
        let st = if !b.enabled {
            base.fg(rc(t.dim))
        } else if b.active {
            on(app, t.accent)
        } else if hovered {
            on(app, t.button_hover)
        } else {
            base
        };
        buf.set_string(x, area.y, &label, st);
        if hovered {
            ui.tooltip = Some(b.tooltip.clone());
        }
        ui.toolbar.push((r, i));
        x += w + 1;
    }
}

/// The tabline, as in bufferline.nvim: the active view's buffers on the left (icon, name
/// shortened in the middle, × when hovered, ● when modified) and the views on the right.
fn draw_tabline(f: &mut Frame, app: &App, area: Rect, ui: &mut Ui) {
    ui.tabline.clear();
    let bg = Style::default().bg(rc(app.config.theme.chrome));
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
            on(app, app.config.theme.accent).add_modifier(Modifier::BOLD)
        } else {
            on(app, app.config.theme.tab)
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
        // The file playing audio shows ▶ (or ⏸) where its file-type icon goes.
        let play_mark = if tab.playing {
            Some("▶")
        } else if tab.paused {
            Some("⏸")
        } else {
            None
        };
        let icon = play_mark.unwrap_or_else(|| file_icon(tab.kind, icons));
        let head = if icon.is_empty() {
            format!(" {name} ")
        } else {
            format!(" {icon} {name} ")
        };
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
        let theme = &app.config.theme;
        let style = if tab.active {
            on(app, theme.tab_hover).add_modifier(Modifier::BOLD)
        } else if hovered {
            on(app, theme.tab_hover)
        } else {
            on(app, theme.tab)
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
                x: x + 1,
                y: area.y,
                width: 1,
                height: 1,
            };
            f.buffer_mut()
                .set_style(cell, Style::default().fg(rc(app.config.theme.playhead)));
        }
        ui.tabline.push((r, TablineHit::Buffer(n)));
        ui.tabline.push((close, TablineHit::Close(n)));
        x += w + 1;
    }
}

/// Width of the tracks panel's columns before the bar grid.
const TRACK_INFO_W: u16 = 48;

/// What a click on the tracks panel does.
#[derive(Debug, Clone, Copy, PartialEq)]
enum TracksHit {
    /// A volume slider (track, first column, width).
    Volume(usize, u16, u16),
    Solo(usize),
    Mute(usize),
    Row(usize),
    Bar(usize, usize),
}

fn click_tracks(app: &mut App, ui: &mut Ui, x: u16, y: u16, kind: MouseEventKind) {
    // While dragging a volume, its slider follows the mouse anywhere along the row.
    let hit = ui
        .tracks
        .iter()
        .find(|(r, h)| {
            if ui.drag == Some(Drag::Volume) {
                matches!(h, TracksHit::Volume(..)) && y >= r.y && y < r.y + r.height
            } else {
                contains(*r, x, y)
            }
        })
        .map(|(_, h)| *h);
    let drag = matches!(kind, MouseEventKind::Drag(_));
    match hit {
        Some(TracksHit::Volume(t, x0, w)) => {
            if ui.drag != Some(Drag::Volume) {
                app.select_track_bar(t, None);
                app.group_edits(true);
                ui.drag = Some(Drag::Volume);
            }
            let frac = (x.saturating_sub(x0) as f64 / (w.max(2) - 1) as f64).clamp(0.0, 1.0);
            app.set_track_volume(t, (frac * 127.0).round() as u8);
        }
        _ if drag => {}
        Some(TracksHit::Solo(t)) => app.run(muse_app::Command::Solo(Some(t + 1))),
        Some(TracksHit::Mute(t)) => app.run(muse_app::Command::Mute(Some(t + 1))),
        Some(TracksHit::Row(t)) => app.select_track_bar(t, None),
        Some(TracksHit::Bar(t, b)) => app.select_track_bar(t, Some(b)),
        None => {}
    }
}

/// The tracks panel: per track its number, volume, solo, mute, name and instrument, then a
/// cell per bar (coloured where it has notes) with the edit cursor and the playhead.
fn draw_tracks(
    f: &mut Frame,
    app: &App,
    area: Rect,
    play: Option<(usize, Location, f64, f64)>,
    ui: &mut Ui,
) {
    let t = app.config.theme.clone();
    let focused = app.mode == Mode::Tracks;
    let base = Style::default().bg(rc(t.panel)).fg(rc(t.chrome_text));
    f.render_widget(Paragraph::new("").style(base), area);
    let rows = app.track_rows();
    let buffer = app.pane().buffer;
    let cursor = app.pane().cursor;
    let visible = rows.len().clamp(1, app.tracks_rows.max(1));
    ui.tracks_area = Some(area);
    let (mut first, mut first_bar) = ui.tracks_scroll;
    let gx = area.x + TRACK_INFO_W;
    let gw = area.width.saturating_sub(TRACK_INFO_W + 1) as usize;
    let playing = play.filter(|p| p.0 == buffer && (app.playing || app.paused));
    let follow_bar = match playing {
        Some((_, loc, ..)) if app.follow && app.playing => loc.bar,
        _ => cursor.bar,
    };
    // Scroll to the cursor's track and bar when they move (the wheel scrolls freely between).
    if ui.tracks_anchor != Some((cursor.track, follow_bar)) {
        ui.tracks_anchor = Some((cursor.track, follow_bar));
        if cursor.track < first {
            first = cursor.track;
        } else if cursor.track >= first + visible {
            first = cursor.track + 1 - visible;
        }
        if follow_bar < first_bar {
            first_bar = follow_bar.saturating_sub(4);
        } else if gw > 0 && follow_bar >= first_bar + gw {
            first_bar = follow_bar + 1 + 4.min(gw / 2) - gw;
        }
    }
    first = first.min(rows.len().saturating_sub(visible));
    let bar_count = rows.first().map_or(0, |r| r.bars.len());
    first_bar = first_bar.min(bar_count.saturating_sub(gw.max(1)));
    ui.tracks_scroll = (first, first_bar);
    let buf = f.buffer_mut();
    let dim = Style::default().fg(rc(t.dim)).bg(rc(t.panel));
    // Header (accent when focused), with bar numbers every 4 bars.
    let head = if focused {
        on(app, t.mode_tracks)
    } else {
        Style::default().fg(rc(t.dim)).bg(rc(t.tab))
    };
    buf.set_style(Rect { height: 1, ..area }, head);
    buf.set_string(
        area.x,
        area.y,
        format!(
            "{:<width$}",
            "  # volume S M track          instrument",
            width = TRACK_INFO_W as usize
        ),
        head,
    );
    let bars = rows.first().map_or(0, |r| r.bars.len());
    let mut b = first_bar - first_bar % 4;
    while b < (first_bar + gw).min(bars) {
        if b >= first_bar {
            let label = (b + 1).to_string();
            if b - first_bar + label.len() <= gw {
                buf.set_string(gx + (b - first_bar) as u16, area.y, label, head);
            }
        }
        b += 4;
    }
    let play_bar = playing.map(|(_, loc, ..)| loc.bar);
    for (k, row) in rows.iter().enumerate().skip(first).take(visible) {
        let y = area.y + 1 + (k - first) as u16;
        if y >= area.y + area.height {
            break;
        }
        let selected = row.index == cursor.track;
        let row_style = if selected {
            Style::default().bg(rc(t.tab)).fg(rc(t.chrome_text))
        } else {
            base
        };
        let fg = if row.audible {
            row_style
        } else {
            row_style.fg(rc(t.dim))
        };
        buf.set_style(
            Rect {
                x: area.x,
                y,
                width: area.width,
                height: 1,
            },
            row_style,
        );
        buf.set_string(area.x, y, "▌", row_style.fg(rc(row.color)));
        buf.set_string(area.x + 1, y, format!("{:>2}", row.index + 1), fg);
        // Volume: 6 cells.
        let filled = ((row.volume as f64 / 127.0) * 6.0).round() as usize;
        for i in 0..6u16 {
            let (ch, st) = if (i as usize) < filled {
                ("━", row_style.fg(rc(row.color)))
            } else {
                ("─", row_style.fg(rc(t.dim)))
            };
            buf.set_string(area.x + 4 + i, y, ch, st);
        }
        ui.tracks.push((
            Rect {
                x: area.x + 4,
                y,
                width: 6,
                height: 1,
            },
            TracksHit::Volume(row.index, area.x + 4, 6),
        ));
        for (x, on_, label, color, hit) in [
            (
                11u16,
                row.solo,
                "S",
                t.mode_insert,
                TracksHit::Solo(row.index),
            ),
            (13, row.mute, "M", t.playhead, TracksHit::Mute(row.index)),
        ] {
            let st = if on_ {
                on(app, color)
            } else {
                row_style.fg(rc(t.dim))
            };
            buf.set_string(area.x + x, y, label, st);
            ui.tracks.push((
                Rect {
                    x: area.x + x,
                    y,
                    width: 1,
                    height: 1,
                },
                hit,
            ));
        }
        let name: String = row.name.chars().take(14).collect();
        let st = if selected {
            fg.add_modifier(Modifier::BOLD)
        } else {
            fg
        };
        buf.set_string(area.x + 15, y, format!("{name:<14}"), st);
        let instr: String = row.instrument.chars().take(16).collect();
        buf.set_string(area.x + 30, y, instr, row_style.fg(rc(t.dim)));
        ui.tracks.push((
            Rect {
                x: area.x,
                y,
                width: TRACK_INFO_W,
                height: 1,
            },
            TracksHit::Row(row.index),
        ));
        buf.set_string(gx - 1, y, "│", dim.bg(row_style.bg.unwrap_or(rc(t.panel))));
        for c in 0..gw.min(bars.saturating_sub(first_bar)) {
            let bar = first_bar + c;
            let x = gx + c as u16;
            let notes = row.bars.get(bar).copied().unwrap_or(false);
            let mut st = row_style;
            let ch = if notes {
                st = st.fg(rc(if row.audible {
                    row.color
                } else {
                    row.color.mix(t.panel, 0.6)
                }));
                "■"
            } else {
                st = st.fg(rc(t.empty_bar));
                "·"
            };
            if play_bar == Some(bar) {
                st = st.bg(rc(t.playhead));
            }
            // The edit cursor: a black dot on its bar's block.
            let ch = if selected && bar == cursor.bar && !app.cursor_hidden(buffer) {
                let block = if notes { row.color } else { t.empty_bar };
                st = Style::default()
                    .fg(Color::Black)
                    .bg(rc(if play_bar == Some(bar) {
                        t.playhead
                    } else {
                        block
                    }));
                "●"
            } else {
                ch
            };
            buf.set_string(x, y, ch, st);
            ui.tracks.push((
                Rect {
                    x,
                    y,
                    width: 1,
                    height: 1,
                },
                TracksHit::Bar(row.index, bar),
            ));
        }
    }
}

/// The status line, as lualine.nvim: mode, layout, workspace and file on the left; the rest of
/// the state, progress and `bar:beat` on the right, in coloured sections.
fn draw_status(f: &mut Frame, app: &App, play: Option<(usize, Location, f64, f64)>, area: Rect) {
    use muse_app::statusline::{SegKind, Segment, StatusIcon};
    let t = &app.config.theme;
    let line = app.statusline(play.map(|(_, _, now, total)| (now, total)), false);
    let icons = app.config.file.tabline.icons.as_str();
    // Powerline separators need a Nerd/Powerline font: only with Nerd Font icons.
    let seps = match (app.config.file.statusline.separators.as_str(), icons) {
        ("arrow", "nerd") => Some(("\u{e0b0}", "\u{e0b2}")),
        ("round", "nerd") => Some(("\u{e0b4}", "\u{e0b6}")),
        _ => None,
    };
    let mode = t.mode_color(app.mode);
    let bg_of = |k: SegKind| match k {
        SegKind::Mode => mode,
        SegKind::ModeText | SegKind::B => t.status_b,
        SegKind::Plain => t.chrome,
    };
    let style_of = |seg: &Segment| {
        let bg = bg_of(seg.kind);
        let fg = match seg.kind {
            SegKind::Mode => t.text_on(mode),
            SegKind::ModeText => mode,
            SegKind::B | SegKind::Plain => t.chrome_text,
        };
        let st = Style::default().fg(rc(fg)).bg(rc(bg));
        if seg.bold {
            st.add_modifier(Modifier::BOLD)
        } else {
            st
        }
    };
    let text_of = |seg: &Segment| {
        let icon = match (seg.icon, icons) {
            (Some(StatusIcon::Workspace), "nerd") => "\u{f0b1} ",
            (Some(StatusIcon::Workspace), "unicode") => "⌂ ",
            (Some(StatusIcon::Workspace), _) => "ws ",
            (None, _) => "",
        };
        format!(" {icon}{} ", seg.text)
    };
    let width = |s: &str| s.chars().count() as u16;
    let buf = f.buffer_mut();
    let base = Style::default().bg(rc(t.chrome)).fg(rc(t.chrome_text));
    buf.set_style(area, base);
    for x in area.x..area.x + area.width {
        buf[(x, area.y)].set_symbol(" ");
    }
    let end = area.x + area.width;

    // Right side first (it keeps its place; the left gives way when narrow).
    let mut right_x = end;
    let mut prev_bg = t.chrome;
    let right: Vec<&Segment> = line.right.iter().filter(|s| !s.text.is_empty()).collect();
    let mut placed = Vec::new();
    for seg in right.iter().rev() {
        let text = text_of(seg);
        let sep_w = u16::from(seps.is_some() && seg.kind != SegKind::Plain);
        let w = width(&text) + sep_w;
        if w + 20 > right_x.saturating_sub(area.x) {
            continue;
        }
        right_x -= w;
        placed.push((right_x, *seg, text));
    }
    for (x, seg, text) in placed.iter().rev() {
        let bg = bg_of(seg.kind);
        let mut x = *x;
        if let (Some((_, left_sep)), true) = (seps, seg.kind != SegKind::Plain) {
            buf.set_string(
                x,
                area.y,
                left_sep,
                Style::default().fg(rc(bg)).bg(rc(prev_bg)),
            );
            x += 1;
        }
        buf.set_string(x, area.y, text, style_of(seg));
        prev_bg = bg;
    }

    // Left side.
    let mut x = area.x;
    let segs: Vec<&Segment> = line.left.iter().collect();
    for (i, seg) in segs.iter().enumerate() {
        let text = text_of(seg);
        let room = right_x.saturating_sub(x + 1);
        if room == 0 {
            break;
        }
        let shown: String = text.chars().take(room as usize).collect();
        buf.set_string(x, area.y, &shown, style_of(seg));
        x += width(&shown);
        let bg = bg_of(seg.kind);
        let next = segs.get(i + 1).map_or(t.chrome, |n| bg_of(n.kind));
        if let Some((sep, _)) = seps {
            if bg != next && x < right_x {
                buf.set_string(x, area.y, sep, Style::default().fg(rc(bg)).bg(rc(next)));
                x += 1;
            }
        }
    }
}

fn draw_cmdline(f: &mut Frame, app: &App, area: Rect) {
    match app.mode {
        // Typed in the command palette.
        Mode::Command | Mode::Picker => {}
        Mode::Normal | Mode::Insert | Mode::Visual | Mode::Tracks => {
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

    #[test]
    fn the_toolbar_shows_its_buttons_and_runs_them() {
        let (config, _) = muse_app::Config::parse("[tabline]\nicons = \"none\"\n").unwrap();
        let mut app = App::new(config, None);
        app.run(muse_app::Command::New);
        let mut terminal = Terminal::new(TestBackend::new(120, 20)).unwrap();
        let mut ui = Ui::default();
        terminal.draw(|f| draw(f, &mut app, None, &mut ui)).unwrap();
        let buf = terminal.backend().buffer().clone();
        let row: String = (0..120).map(|x| buf[(x, 0)].symbol().to_string()).collect();
        assert!(
            row.contains(" new ") && row.contains(" save ") && row.contains(" tracks "),
            "{row}"
        );
        // Hovering shows the tooltip; clicking runs the command.
        let (r, i) = *ui
            .toolbar
            .iter()
            .find(|(r, _)| {
                (r.x..r.x + r.width)
                    .map(|x| buf[(x, 0)].symbol())
                    .collect::<String>()
                    == " tracks "
            })
            .unwrap();
        ui.hover = Some((r.x, r.y));
        terminal.draw(|f| draw(f, &mut app, None, &mut ui)).unwrap();
        assert!(ui.tooltip.as_deref().unwrap().contains("tracks panel"));
        app.run_toolbar(i);
        assert!(app.tracks_panel);
        // Hidden with :toolbar.
        app.run(muse_app::Command::Toolbar(Some(false)));
        let screen = render(&mut app, 120, 20);
        let first = screen.lines().next().unwrap();
        assert!(!first.contains(" save "), "{screen}");
    }

    #[test]
    fn the_status_line_has_lualine_sections() {
        let mut app = App::new(muse_app::Config::default(), None);
        app.run(muse_app::Command::New);
        let mut terminal = Terminal::new(TestBackend::new(100, 12)).unwrap();
        let mut ui = Ui::default();
        terminal.draw(|f| draw(f, &mut app, None, &mut ui)).unwrap();
        let buf = terminal.backend().buffer().clone();
        let y = 10;
        let row: String = (0..100).map(|x| buf[(x, y)].symbol().to_string()).collect();
        // The mode keeps its colour (it used to be painted over).
        let mode = rc(app.config.theme.mode_color(muse_app::Mode::Normal));
        assert!(row.starts_with(" NORMAL "), "{row}");
        assert_eq!(buf[(1, y)].bg, mode);
        assert!(row.contains("vertical"), "{row}");
        // Arrow separators with Nerd Font icons; the location on the mode colour at the end.
        assert_eq!(buf[(8, y)].symbol(), "\u{e0b0}");
        assert!(row.trim_end().ends_with("1:1"), "{row}");
        assert_eq!(buf[(98, y)].bg, mode);
        assert!(row.contains("Top"), "{row}");
    }

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
            .filter(|&(x, y)| buf[(x, y)].bg == rc(app.config.theme.playhead))
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
        let top: String = (0..100).map(|x| buf[(x, 1)].symbol().to_string()).collect();
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
                .map(|x| buf[(x, 1)].symbol().to_string())
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
        assert!(screen.contains(" 1:1 "), "{screen}");
        // The tab of the file playing audio is marked.
        assert!(!screen.contains("▶"), "{screen}");
        app.playing_buffer = Some(0);
        app.playing = true;
        let screen = render(&mut app, 80, 20);
        assert!(screen.contains("▶ t.gp5"), "{screen}");
        // The tracks panel: a row per track with its name, instrument and bars.
        app.run(muse_app::Command::TracksFocus);
        let screen = render(&mut app, 100, 20);
        assert!(screen.contains("volume S M track"), "{screen}");
        assert!(screen.contains("Lead"), "{screen}");
        assert!(screen.contains("TRACKS"), "{screen}");
        assert!(screen.contains('■') || screen.contains('·'), "{screen}");
    }
}
