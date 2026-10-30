//! Terminal frontend for muse.

mod chantview;
mod tabview;

use std::time::{Duration, Instant};

use anyhow::Result;
use muse_app::playback::Playback;
use muse_app::{App, Key, KeyCode, Mode, PaneKind, Strip};
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
    /// The last click in the tracks pane (when and where), to tell a double click.
    last_click: Option<(std::time::Instant, u16, u16)>,
    /// Loudness of the focused file's audio under each bar, for the tracks panel.
    audio_levels: Option<Vec<f32>>,
    /// Fretboard cells (string, frets from the nut) and piano keys (pitch): clicks enter notes.
    frets: Vec<(Rect, usize, u8)>,
    keys: Vec<(Rect, u8)>,
    /// The status line's clickable sections, and what they run.
    status: Vec<(Rect, muse_app::Command)>,
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
    /// Clickable areas of the properties pane.
    props: Vec<(Rect, PropHit)>,
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
        ui.audio_levels = if app.has_tool(PaneKind::Tracks) || app.has_tool(PaneKind::Audio) {
            playback.bar_levels(app, app.pane().buffer)
        } else {
            None
        };
        terminal.draw(|f| draw(f, app, play_loc, &mut ui))?;

        let timeout = if app.playing || app.has_tool(PaneKind::Tuner) {
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
                        if matches!(ui.drag, Some(Drag::Divider(..) | Drag::Tab(_))) =>
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
                            _ => {}
                        }
                    }
                    MouseEventKind::Down(MouseButton::Left)
                        if divider_at(app, m.column, m.row).is_some() =>
                    {
                        let (d, off) = divider_at(app, m.column, m.row).unwrap();
                        ui.drag = Some(Drag::Divider(d, off));
                    }
                    MouseEventKind::Down(MouseButton::Left)
                        if ui.props.iter().any(|(r, _)| contains(*r, m.column, m.row)) =>
                    {
                        let hit = ui
                            .props
                            .iter()
                            .find(|(r, _)| contains(*r, m.column, m.row))
                            .map(|(_, h)| *h)
                            .unwrap();
                        click_properties(app, hit);
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
                        let n = app.track_rows().len();
                        let fit = ui
                            .tracks_area
                            .map_or(1, |r| r.height.saturating_sub(1).max(1));
                        let visible = n.clamp(1, fit as usize);
                        let (first, first_bar) = ui.tracks_scroll;
                        ui.tracks_scroll = (
                            (first as i64 + rows).clamp(0, n.saturating_sub(visible) as i64)
                                as usize,
                            (first_bar as i64 + bars).max(0) as usize,
                        );
                    }
                    // A fret or a piano key enters (or removes) that note at the cursor.
                    MouseEventKind::Down(MouseButton::Left)
                        if ui.frets.iter().any(|(r, ..)| contains(*r, m.column, m.row))
                            || ui.keys.iter().any(|(r, _)| contains(*r, m.column, m.row)) =>
                    {
                        if let Some(&(_, string, fret)) = ui
                            .frets
                            .iter()
                            .find(|(r, ..)| contains(*r, m.column, m.row))
                        {
                            app.click_fret(string, fret);
                        } else if let Some(&(_, pitch)) =
                            ui.keys.iter().find(|(r, _)| contains(*r, m.column, m.row))
                        {
                            app.click_key(pitch);
                        }
                    }
                    // Clickable status sections: layout, workspace, mix, speed.
                    MouseEventKind::Down(MouseButton::Left)
                        if ui.status.iter().any(|(r, _)| contains(*r, m.column, m.row)) =>
                    {
                        let cmd = ui
                            .status
                            .iter()
                            .find(|(r, _)| contains(*r, m.column, m.row))
                            .map(|(_, c)| c.clone())
                            .unwrap();
                        app.run(cmd);
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
                            app.set_focus(hit.pane);
                            if let Some(p) = app.panes.get_mut(&hit.pane) {
                                p.cursor = hit.cursor;
                            }
                        } else if let Some((id, _)) = app
                            .layout()
                            .into_iter()
                            .find(|(_, r)| contains(to_rect(*r), m.column, m.row))
                        {
                            app.set_focus(id);
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
    // The tracks peeking after a change of track (no tracks pane shown) take the bottom.
    let peek_h = if app.tracks_peeking() {
        app.peek_height().saturating_sub(2)
    } else {
        0
    };
    let [toolbar, tabline, main, peek, status, cmdline] = Layout::vertical([
        Constraint::Length(u16::from(app.toolbar)),
        Constraint::Length(1),
        Constraint::Min(3),
        Constraint::Length(peek_h),
        Constraint::Length(1),
        Constraint::Length(1),
    ])
    .areas(f.area());
    ui.tracks.clear();
    ui.tracks_area = None;
    ui.props.clear();
    if peek_h > 0 {
        draw_tracks(f, app, peek, play, ui);
    }
    ui.frets.clear();
    ui.keys.clear();

    app.set_viewport(muse_app::layout::Rect {
        x: main.x,
        y: main.y,
        w: main.width,
        h: main.height,
    });
    draw_tabline(f, app, tabline, ui);
    ui.toolbar.clear();
    ui.tooltip = None;
    if app.toolbar {
        draw_toolbar(f, app, toolbar, ui);
    }
    // Tool panes: a frame named after the tool around it.
    let focused = app.window_focus();
    for (id, r) in app.layout() {
        let kind = app.panes.get(&id).map_or(PaneKind::Score, |p| p.kind);
        if kind == PaneKind::Score {
            continue;
        }
        let area = to_rect(r);
        let style = if id == focused {
            Style::default().fg(rc(app.config.theme.mode_color(app.mode)))
        } else {
            Style::default().fg(rc(app.config.theme.dim))
        };
        let block = Block::bordered()
            .border_type(BorderType::Rounded)
            .border_style(style)
            .title(format!(" {} ", kind.name()));
        let inner = block.inner(area);
        f.render_widget(block, area);
        // Drawn for its score pane (its own for a local tool), as if that were the view's.
        let view_score = app.view().score;
        let active = app.active_view;
        app.views[active].score = app.tool_score(id);
        let instrument = app.instrument_view(play.map(|(b, loc, ..)| (b, loc.bar, loc.tick)));
        match kind {
            PaneKind::Tracks => draw_tracks(f, app, inner, play, ui),
            PaneKind::Properties => draw_properties(f, app, inner, ui),
            PaneKind::Tuner => draw_tuner(f, app, inner),
            PaneKind::Fretboard | PaneKind::Piano => match &instrument {
                Some(v) if kind == PaneKind::Fretboard => draw_fretboard(f, app, v, inner, ui),
                Some(v) => draw_piano(f, app, v, inner, ui),
                None => f.render_widget(Paragraph::new(" no instrument on this track"), inner),
            },
            PaneKind::Audio => draw_audio(f, app, inner, ui),
            PaneKind::Score => {}
        }
        app.views[active].score = view_score;
    }
    let hits = &mut ui.hits;

    for (id, r) in app.layout() {
        if app
            .panes
            .get(&id)
            .is_some_and(|p| p.kind != PaneKind::Score)
        {
            continue;
        }
        let area = to_rect(r);
        let pane = app.panes[&id].clone();
        let buffer = &app.buffers[pane.buffer];
        let track = buffer.score.tracks.get(pane.cursor.track);
        // The track, and its tuning and capo (at the top of the score).
        let title = match track {
            Some(t) => {
                let tuning = t
                    .tuning_text()
                    .map(|s| format!(" · {}", s.trim_start_matches("Tuning: ")))
                    .unwrap_or_default();
                format!(
                    " {} — {}/{} {}{tuning} ",
                    buffer.name,
                    pane.cursor.track + 1,
                    buffer.score.tracks.len(),
                    if t.name.is_empty() {
                        "Track"
                    } else {
                        t.name.trim()
                    }
                )
            }
            None => format!(" {} ", buffer.name),
        };
        let border_style = if id == focused {
            Style::default().fg(rc(app.config.theme.accent))
        } else {
            Style::default().fg(rc(app.config.theme.dim))
        };
        let block = Block::bordered()
            .border_type(BorderType::Rounded)
            .border_style(border_style)
            .title(title);
        let zoom_indicator = app.is_maximized(id);
        let block = if zoom_indicator {
            block.title(
                Line::from("  ")
                    .right_aligned()
                    .style(Style::default().fg(rc(app.config.theme.accent)).bold()),
            )
        } else {
            block
        };
        let mut inner = block.inner(area);
        f.render_widget(block, area);
        // The progress bar on the pane's last row.
        if app.progress_bar && inner.height > 4 {
            let row = Rect {
                y: inner.y + inner.height - 1,
                height: 1,
                ..inner
            };
            inner.height -= 1;
            draw_progress(f, app, id, row, play);
        }
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

    draw_status(f, app, play, status, ui);
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
    // Keys that can follow the ones typed, or the marks while ' ` m wait for one.
    if let Some((title, rows)) = app.which_key() {
        draw_which_key(f, app, &title, &rows, main);
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

/// The tuner: the note, cents, frequency and nearest string on one row; a cents scale from
/// −50 to +50 with the needle (green when in tune) on the next.
fn draw_tuner(f: &mut Frame, app: &App, area: Rect) {
    let t = &app.config.theme;
    let base = Style::default().bg(rc(t.panel)).fg(rc(t.chrome_text));
    let dim = base.fg(rc(t.dim));
    let buf = f.buffer_mut();
    buf.set_style(area, base);
    let view = app.tuner_view();
    let needle_color = |cents: f32| {
        if cents.abs() < 3.0 {
            t.mode_command
        } else if cents.abs() < 15.0 {
            t.accent
        } else {
            t.playhead
        }
    };
    match &view {
        Some(v) => {
            let c = needle_color(v.cents);
            buf.set_string(
                area.x + 1,
                area.y,
                format!(" {}{} ", v.note, v.octave),
                Style::default()
                    .fg(rc(t.text_on(c)))
                    .bg(rc(c))
                    .add_modifier(Modifier::BOLD),
            );
            let mut line = format!("  {:+.0}¢   {:.1} Hz", v.cents, v.freq);
            if let Some((s, name, octave, cents)) = v.string {
                line.push_str(&format!(
                    "   string {} ({name}{octave}) {cents:+.0}¢",
                    s + 1
                ));
            }
            buf.set_string(area.x + 6, area.y, line, base);
        }
        None => {
            let msg = match &app.tuner_error {
                Some(e) => format!(" tuner: {e}"),
                None => " tuner: play a note…".into(),
            };
            buf.set_string(area.x, area.y, msg, dim);
        }
    }
    let a4 = format!("A4={:.0} ", app.config.file.audio.tuner_a4);
    let ax = (area.x + area.width).saturating_sub(a4.chars().count() as u16);
    buf.set_string(ax, area.y, a4, dim);
    // The scale.
    let y = area.y + 1;
    if y >= area.y + area.height || area.width < 20 {
        return;
    }
    let x0 = area.x + 5;
    let w = area.width.saturating_sub(10).max(10);
    buf.set_string(area.x + 1, y, "-50", dim);
    buf.set_string(x0 + w + 1, y, "+50", dim);
    for i in 0..w {
        let c = -50.0 + 100.0 * i as f32 / (w - 1) as f32;
        let ch = if i == w / 2 {
            "┼"
        } else if i % (w / 10).max(1) == 0 {
            "┴"
        } else {
            "─"
        };
        let st = if c.abs() < 3.0 {
            dim.fg(rc(t.mode_command))
        } else {
            dim
        };
        buf.set_string(x0 + i, y, ch, st);
    }
    if let Some(v) = &view {
        let i = (((v.cents.clamp(-50.0, 50.0) + 50.0) / 100.0) * (w - 1) as f32).round() as u16;
        buf.set_string(x0 + i, y, "█", base.fg(rc(needle_color(v.cents))));
    }
}

/// The fretboard: a row per string (open note, nut, a cell per fret) and the fret numbers
/// under it; the notes at the cursor or playhead are named in the track's colour.
fn draw_fretboard(
    f: &mut Frame,
    app: &App,
    v: &muse_app::instrument::InstrumentView,
    area: Rect,
    ui: &mut Ui,
) {
    use muse_app::instrument::NOTE_NAMES;
    let t = &app.config.theme;
    let base = Style::default().bg(rc(t.panel)).fg(rc(t.dim));
    let buf = f.buffer_mut();
    buf.set_style(area, base);
    if !v.fretted {
        buf.set_string(area.x + 1, area.y, "fretboard: not a fretted track", base);
        return;
    }
    // Label (2), open note (2), nut (1), then 4 columns a fret.
    let nut = area.x + 4;
    let frets = (v.frets as u16).min(area.width.saturating_sub(6) / 4);
    let note_style = Style::default()
        .fg(rc(t.text_on(v.color)))
        .bg(rc(v.color))
        .add_modifier(Modifier::BOLD);
    for (s, &open) in v.strings.iter().enumerate() {
        let y = area.y + s as u16;
        let cursor = v.cursor_string == Some(s);
        // The cursor's string in the mode's colour (thin, like the others).
        let line = if cursor {
            base.fg(rc(t.mode_color(app.mode)))
        } else {
            base.fg(rc(t.chrome_text.mix(t.panel, 0.4)))
        };
        buf.set_string(area.x, y, NOTE_NAMES[open as usize % 12], line);
        buf.set_string(nut, y, "║", base.fg(rc(t.chrome_text)));
        // Clickable: the open note, then each fret.
        let cell = |x, width| Rect {
            x,
            y,
            width,
            height: 1,
        };
        ui.frets.push((cell(area.x + 2, 2), s, 0));
        for k in 1..=frets {
            ui.frets.push((cell(nut + 1 + (k - 1) * 4, 3), s, k as u8));
        }
        for k in 1..=frets {
            let x = nut + 1 + (k - 1) * 4;
            buf.set_string(x, y, "───", line);
            buf.set_string(x + 3, y, "│", base);
        }
        for n in v.notes.iter().filter(|n| n.string == s) {
            let name = NOTE_NAMES[n.pitch as usize % 12];
            let fret = n.fret as u16 + v.capo as u16;
            if fret == 0 {
                buf.set_string(area.x + 2, y, format!("{name:<2}"), note_style);
            } else if fret <= frets {
                let x = nut + 1 + (fret - 1) * 4;
                buf.set_string(x, y, format!("{name:^3}"), note_style);
            }
        }
        // A capo: a solid bar (in the theme's capo colour) across its fret, over the strings.
        if v.capo > 0 && (v.capo as u16) <= frets {
            let x = nut + v.capo as u16 * 4;
            buf.set_string(x, y, "▌", base.fg(rc(t.capo)));
        }
    }
    // Fret numbers at the inlays.
    let y = area.y + v.strings.len() as u16;
    if y < area.y + area.height {
        for k in [3u16, 5, 7, 9, 12, 15, 17, 19, 21, 24] {
            if k <= frets {
                buf.set_string(nut + 1 + (k - 1) * 4 + 1, y, k.to_string(), base);
            }
        }
    }
}

/// The piano: a column per key around the notes (C4 in the middle when there are none); the
/// notes at the cursor or playhead are in the track's colour.
fn draw_piano(
    f: &mut Frame,
    app: &App,
    v: &muse_app::instrument::InstrumentView,
    area: Rect,
    ui: &mut Ui,
) {
    let t = &app.config.theme;
    let buf = f.buffer_mut();
    buf.set_style(area, Style::default().bg(rc(t.panel)));
    let is_black = |p: u8| matches!(p % 12, 1 | 3 | 6 | 8 | 10);
    let keys = area.width.min(88) as i32;
    // Centre the range on the notes (or middle C), within A0..C8.
    let centre = if v.notes.is_empty() {
        60
    } else {
        let (lo, hi) = v
            .notes
            .iter()
            .fold((127u8, 0u8), |(a, b), n| (a.min(n.pitch), b.max(n.pitch)));
        (lo as i32 + hi as i32) / 2
    };
    let start = (centre - keys / 2).clamp(21, 109 - keys) as u8;
    let x0 = area.x + (area.width - keys as u16) / 2;
    for i in 0..keys as u16 {
        let p = start + i as u8;
        let x = x0 + i;
        let pressed = v.notes.iter().any(|n| n.pitch == p);
        ui.keys.push((
            Rect {
                x,
                y: area.y,
                width: 1,
                height: 2,
            },
            p,
        ));
        let key = if pressed {
            v.color
        } else if is_black(p) {
            t.ink.mix(t.page, 0.15)
        } else {
            t.page
        };
        buf.set_string(x, area.y, "█", Style::default().fg(rc(key)).bg(rc(t.panel)));
        // The lower row: white keys only (black keys are short); C octaves numbered.
        let low = if is_black(p) { t.page } else { key };
        let label = if p % 12 == 0 {
            ((p / 12) as i32 - 1).to_string()
        } else {
            " ".into()
        };
        buf.set_string(
            x,
            area.y + 1,
            label,
            Style::default().fg(rc(t.text_on(low))).bg(rc(low)),
        );
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
    /// A volume slider (row, first column, width).
    Volume(Strip, u16, u16),
    Solo(Strip),
    Mute(Strip),
    Name(usize),
    Instrument(usize),
    Row(usize),
    Bar(usize, usize),
}

fn click_tracks(app: &mut App, ui: &mut Ui, x: u16, y: u16, kind: MouseEventKind) {
    // A click in the tracks pane focuses it; a second one soon after on the same cell is a
    // double click.
    let mut double = false;
    if matches!(kind, MouseEventKind::Down(_)) {
        if let Some(id) = app.tool_pane(PaneKind::Tracks) {
            app.set_focus(id);
        }
        let now = std::time::Instant::now();
        double = ui.last_click.is_some_and(|(t, x0, y0)| {
            now.duration_since(t) < std::time::Duration::from_millis(400) && (x0, y0) == (x, y)
        });
        ui.last_click = Some((now, x, y));
    }
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
        Some(TracksHit::Volume(strip, x0, w)) => {
            if ui.drag != Some(Drag::Volume) {
                if let Strip::Track(t) = strip {
                    app.select_track_bar(t, None);
                }
                app.group_edits(true);
                ui.drag = Some(Drag::Volume);
            }
            let frac = (x.saturating_sub(x0) as f64 / (w.max(2) - 1) as f64).clamp(0.0, 1.0);
            app.set_strip_volume(strip, frac);
        }
        _ if drag => {}
        Some(TracksHit::Solo(strip)) => app.run(muse_app::Command::Solo(Some(strip))),
        Some(TracksHit::Mute(strip)) => app.run(muse_app::Command::Mute(Some(strip))),
        Some(TracksHit::Name(t) | TracksHit::Instrument(t)) if double => {
            app.tracks_selected = t;
            app.run(if matches!(hit, Some(TracksHit::Name(_))) {
                muse_app::Command::TrackRenameStart
            } else {
                muse_app::Command::TrackInstrument(None)
            });
        }
        Some(TracksHit::Row(t) | TracksHit::Name(t) | TracksHit::Instrument(t)) => {
            app.tracks_selected = t;
            app.select_track_bar(t, None)
        }
        Some(TracksHit::Bar(t, b)) => app.select_track_bar(t, Some(b)),
        None => {}
    }
}

/// A score pane's progress bar: its sections as `[A] Title` cells, the play position (or the
/// cursor's) shaded red, and the time on the right.
fn draw_progress(
    f: &mut Frame,
    app: &App,
    id: muse_app::layout::PaneId,
    area: Rect,
    play: Option<(usize, Location, f64, f64)>,
) {
    let play = play.map(|(b, loc, t, total)| (b, loc.bar, loc.tick, t, total));
    let Some(p) = app.progress(id, play) else {
        return;
    };
    let t = &app.config.theme;
    // Match the score background (the page colour, since TUI is always vertical/horizontal).
    let ground = t.page;
    let label_ink = ground.mix(t.ink, 0.8);
    let segment = ground.mix(t.ink, 0.12);
    let base = Style::default().bg(rc(ground)).fg(rc(label_ink));
    let time = format!(
        " {} / {} ",
        muse_app::progress::clock(p.now),
        muse_app::progress::clock(p.total)
    );
    let buf = f.buffer_mut();
    buf.set_style(area, base);
    let tw = time.chars().count() as u16;
    if area.width <= tw + 4 {
        return;
    }
    buf.set_string(area.x + area.width - tw, area.y, &time, base);
    let w = (area.width - tw - 1) as f64;
    let col = |f: f64| area.x + (w * f.clamp(0.0, 1.0)).round() as u16;
    let seg = Style::default().bg(rc(segment)).fg(rc(label_ink));
    for sec in &p.sections {
        let (x0, x1) = (col(sec.start), col(sec.end));
        if x1 <= x0 + 1 {
            continue;
        }
        let width = (x1 - x0 - 1) as usize;
        let label = match (sec.letter.is_empty(), sec.title.is_empty()) {
            (false, false) => format!("[{}] {}", sec.letter, sec.title),
            (false, true) => format!("[{}]", sec.letter),
            (true, _) => sec.title.clone(),
        };
        let label: String = label.chars().take(width).collect();
        buf.set_string(x0, area.y, format!("{label:<width$}"), seg);
    }
    // The position: a red shade under whatever character is there.
    let x = col(p.at).min(area.x + w as u16);
    let cell = &mut buf[(x, area.y)];
    let under = cell.bg;
    let shade = match under {
        Color::Rgb(r, g, b) => {
            let red = t.playhead;
            Color::Rgb(
                ((r as u16 + red.r as u16) / 2) as u8,
                ((g as u16 + red.g as u16) / 2) as u8,
                ((b as u16 + red.b as u16) / 2) as u8,
            )
        }
        _ => rc(t.playhead),
    };
    cell.set_bg(shade);
}

/// The audio pane: the audio's loudness under each bar, from the cursor's bar on.
fn draw_audio(f: &mut Frame, app: &App, area: Rect, ui: &Ui) {
    const LEVELS: [&str; 8] = ["▁", "▂", "▃", "▄", "▅", "▆", "▇", "█"];
    let style = Style::default().fg(rc(app.config.theme.accent));
    let Some(levels) = ui.audio_levels.as_ref() else {
        let name = &app.buffer().name;
        let text = if app.buffer().score.audio.is_some() {
            format!(" {name}: audio loading…")
        } else {
            format!(" {name}: no audio attached (:audio FILE)")
        };
        f.render_widget(
            Paragraph::new(text).style(Style::default().fg(rc(app.config.theme.dim))),
            area,
        );
        return;
    };
    let first = app.pane().cursor.bar;
    let buf = f.buffer_mut();
    for (c, l) in levels
        .iter()
        .skip(first)
        .take(area.width as usize)
        .enumerate()
    {
        let ch = LEVELS[((l.clamp(0.0, 1.0) * 7.0).round() as usize).min(7)];
        for y in 0..area.height {
            buf.set_string(area.x + c as u16, area.y + y, ch, style);
        }
    }
}

#[derive(Clone, Copy)]
enum PropHit {
    Tab(muse_app::properties::PropTab),
    Row(usize),
}

fn click_properties(app: &mut App, hit: PropHit) {
    if let Some(id) = app.tool_pane(PaneKind::Properties) {
        app.set_focus(id);
    }
    match hit {
        PropHit::Tab(t) => app.set_property_tab(t),
        // A click selects a property; another on the selected one edits it.
        PropHit::Row(i) => {
            if i == app.property_selected() && app.props.edit.is_none() {
                app.edit_property();
            } else {
                app.select_property(i);
            }
        }
    }
}

/// `text` cut to `width` cells, ending in an ellipsis when it is longer.
fn clip(text: &str, width: usize) -> String {
    if text.chars().count() <= width {
        return text.to_string();
    }
    let mut out: String = text.chars().take(width.saturating_sub(1)).collect();
    out.push('…');
    out
}

/// The properties pane: its tabs, then the shown tab's rows (sections, then label and value).
fn draw_properties(f: &mut Frame, app: &App, area: Rect, ui: &mut Ui) {
    use muse_app::properties::{PropTab, RowKind};
    let t = app.config.theme.clone();
    let focused = app.focused_kind() == PaneKind::Properties;
    let base = Style::default().bg(rc(t.panel)).fg(rc(t.chrome_text));
    let dim = Style::default().fg(rc(t.dim)).bg(rc(t.panel));
    f.render_widget(Paragraph::new("").style(base), area);
    if area.height < 2 || area.width < 8 {
        return;
    }
    let buf = f.buffer_mut();
    // Tabs.
    let mut x = area.x;
    for tab in PropTab::ALL {
        let label = format!(" {} ", tab.name());
        let w = label.chars().count() as u16;
        let style = if tab == app.props.tab {
            on(app, if focused { t.mode_tracks } else { t.accent })
        } else {
            Style::default().fg(rc(t.dim)).bg(rc(t.tab))
        };
        buf.set_string(x, area.y, &label, style);
        ui.props.push((
            Rect {
                x,
                y: area.y,
                width: w,
                height: 1,
            },
            PropHit::Tab(tab),
        ));
        x += w + 1;
    }
    let rows = app.property_rows();
    let visible = (area.height - 1) as usize;
    let first = app.property_window(visible);
    let selected = app.property_selected();
    let label_w = 14usize.min(area.width as usize / 2);
    let value_w = (area.width as usize).saturating_sub(label_w + 1);
    for (k, row) in rows.iter().enumerate().skip(first).take(visible) {
        let y = area.y + 1 + (k - first) as u16;
        let sel = k == selected;
        let style = match (sel, focused) {
            (true, true) => on(app, t.mode_tracks),
            (true, false) => Style::default().bg(rc(t.tab)).fg(rc(t.chrome_text)),
            _ => base,
        };
        buf.set_style(
            Rect {
                x: area.x,
                y,
                width: area.width,
                height: 1,
            },
            style,
        );
        match &row.kind {
            RowKind::Header { folded, .. } => {
                let mark = if *folded { '▸' } else { '▾' };
                buf.set_string(
                    area.x,
                    y,
                    clip(&format!("{mark} {}", row.label), area.width as usize),
                    style.add_modifier(Modifier::BOLD),
                );
            }
            RowKind::Note => buf.set_string(
                area.x,
                y,
                clip(&row.label, area.width as usize),
                if sel { style } else { dim },
            ),
            RowKind::Field { field, editable } => {
                buf.set_string(
                    area.x + 1,
                    y,
                    clip(&row.label, label_w.saturating_sub(1)),
                    if sel || *editable { style } else { style.fg(rc(t.dim)) },
                );
                let typing = app.props.edit.as_ref().filter(|e| e.field == *field);
                let value = match typing {
                    Some(e) => {
                        // The tail of the text stays in view, with the caret after it.
                        let chars: Vec<char> = e.text.chars().collect();
                        let keep = value_w.saturating_sub(1);
                        let tail: String = chars[chars.len().saturating_sub(keep)..].iter().collect();
                        format!("{tail}▏")
                    }
                    None => clip(&row.value, value_w),
                };
                let vstyle = if typing.is_some() {
                    Style::default().fg(rc(t.chrome_text)).bg(rc(t.tab))
                } else if *editable {
                    style
                } else {
                    style.fg(rc(t.dim))
                };
                buf.set_string(area.x + label_w as u16 + 1, y, value, vstyle);
            }
        }
        ui.props.push((
            Rect {
                x: area.x,
                y,
                width: area.width,
                height: 1,
            },
            PropHit::Row(k),
        ));
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
    let focused = app.focused_kind() == PaneKind::Tracks;
    let base = Style::default().bg(rc(t.panel)).fg(rc(t.chrome_text));
    f.render_widget(Paragraph::new("").style(base), area);
    let rows = app.track_rows();
    let buffer = app.pane().buffer;
    let cursor = app.pane().cursor;
    let visible = rows
        .len()
        .clamp(1, area.height.saturating_sub(1).max(1) as usize);
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
    // The row to keep in view: the panel's selection while it has the focus, else the
    // pane's track.
    let sel_row = if focused {
        app.tracks_selected
    } else {
        cursor.track
    };
    if ui.tracks_anchor != Some((sel_row, follow_bar)) {
        ui.tracks_anchor = Some((sel_row, follow_bar));
        if sel_row < first {
            first = sel_row;
        } else if sel_row >= first + visible {
            first = sel_row + 1 - visible;
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
        let track = matches!(row.strip, Strip::Track(_));
        let shown = track && row.index == cursor.track;
        let selected = if focused { k == sel_row } else { shown };
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
        if track {
            buf.set_string(area.x + 1, y, format!("{:>2}", row.index + 1), fg);
        }
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
            TracksHit::Volume(row.strip, area.x + 4, 6),
        ));
        for (x, on_, label, color, hit) in [
            (
                11u16,
                row.solo,
                "S",
                t.mode_insert,
                TracksHit::Solo(row.strip),
            ),
            (13, row.mute, "M", t.playhead, TracksHit::Mute(row.strip)),
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
        // The name, or the name being typed (with a caret) while it is renamed.
        let renaming = app
            .renaming
            .as_ref()
            .filter(|(t, _)| track && *t == row.index);
        let (name, st) = match renaming {
            Some((_, typed)) => {
                let shown: String = typed
                    .chars()
                    .rev()
                    .take(13)
                    .collect::<Vec<_>>()
                    .into_iter()
                    .rev()
                    .collect();
                (
                    format!("{shown}▏"),
                    Style::default().fg(rc(t.ink)).bg(rc(t.page)),
                )
            }
            None => (
                row.name.chars().take(14).collect(),
                if selected {
                    fg.add_modifier(Modifier::BOLD)
                } else {
                    fg
                },
            ),
        };
        buf.set_string(area.x + 15, y, format!("{name:<14}"), st);
        let instr: String = row.instrument.chars().take(16).collect();
        buf.set_string(area.x + 30, y, instr, row_style.fg(rc(t.dim)));
        buf.set_string(gx - 1, y, "│", dim.bg(row_style.bg.unwrap_or(rc(t.panel))));
        let cells = gw.min(bars.saturating_sub(first_bar));
        match row.strip {
            // The audio: its loudness under each bar.
            Strip::Audio => {
                const LEVELS: [&str; 8] = ["▁", "▂", "▃", "▄", "▅", "▆", "▇", "█"];
                let st = row_style.fg(rc(if row.audible {
                    row.color
                } else {
                    row.color.mix(t.panel, 0.6)
                }));
                for c in 0..cells {
                    let level = ui
                        .audio_levels
                        .as_ref()
                        .and_then(|l| l.get(first_bar + c))
                        .copied();
                    let ch = level.map_or("·", |l| {
                        LEVELS[((l.clamp(0.0, 1.0) * 7.0).round() as usize).min(7)]
                    });
                    buf.set_string(gx + c as u16, y, ch, st);
                }
                continue;
            }
            // The master: each section's letter (or a flag) and title from its bar on.
            Strip::Master => {
                let mut until = 0;
                for (b, letter, title) in &row.markers {
                    if *b < first_bar || *b >= first_bar + cells {
                        continue;
                    }
                    let start = b - first_bar;
                    if start < until {
                        continue;
                    }
                    let head = if letter.is_empty() {
                        "⚑".to_string()
                    } else {
                        format!("[{letter}]")
                    };
                    let label = format!("{head} {title}");
                    let room = cells - start;
                    let label: String = label.chars().take(room).collect();
                    until = start + label.chars().count() + 1;
                    buf.set_string(gx + start as u16, y, label, row_style);
                }
                continue;
            }
            Strip::Track(_) => {}
        }
        // The name and instrument cells: a double click renames / picks an instrument.
        for (x, width, hit) in [
            (15, 14, TracksHit::Name(row.index)),
            (30, 16, TracksHit::Instrument(row.index)),
        ] {
            ui.tracks.push((
                Rect {
                    x: area.x + x,
                    y,
                    width,
                    height: 1,
                },
                hit,
            ));
        }
        ui.tracks.push((
            Rect {
                x: area.x,
                y,
                width: TRACK_INFO_W,
                height: 1,
            },
            TracksHit::Row(row.index),
        ));
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
            let ch = if shown && bar == cursor.bar && !app.cursor_hidden(buffer) {
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
fn draw_status(
    f: &mut Frame,
    app: &App,
    play: Option<(usize, Location, f64, f64)>,
    area: Rect,
    ui: &mut Ui,
) {
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
        SegKind::ModeText => t.status_b,
        SegKind::Plain => t.chrome,
    };
    let style_of = |seg: &Segment| {
        let bg = bg_of(seg.kind);
        let fg = match seg.kind {
            SegKind::Mode => t.text_on(mode),
            SegKind::ModeText => mode,
            SegKind::Plain => t.chrome_text,
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
            (Some(StatusIcon::Tempo), "nerd") => "\u{f038b}",
            (Some(StatusIcon::Tempo), _) => "♩",
            (Some(StatusIcon::Zoom), "nerd") => "\u{f002} ",
            (Some(StatusIcon::Zoom), _) => "zoom ",
            (Some(StatusIcon::Speed), "nerd") => "\u{f04c5} ",
            (Some(StatusIcon::Speed), _) => "speed ",
            (Some(StatusIcon::Synth), "nerd") => "\u{f08f1}",
            (Some(StatusIcon::Audio), "nerd") => "\u{f075a}",
            (Some(StatusIcon::SynthAndAudio), "nerd") => "\u{f08f1} +\u{f075a}",
            (Some(StatusIcon::Synth), _) => "synth",
            (Some(StatusIcon::Audio), _) => "audio",
            (Some(StatusIcon::SynthAndAudio), _) => "synth+audio",
            (None, _) => "",
        };
        format!(" {icon}{} ", seg.text)
    };
    let width = |s: &str| s.chars().count() as u16;
    ui.status.clear();
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
    let right: Vec<&Segment> = line
        .right
        .iter()
        .filter(|s| !s.text.is_empty() || s.icon.is_some())
        .collect();
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
        if let Some(cmd) = &seg.action {
            let r = Rect {
                x,
                y: area.y,
                width: width(text),
                height: 1,
            };
            ui.status.push((r, cmd.clone()));
        }
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
        if let Some(cmd) = &seg.action {
            let r = Rect {
                x,
                y: area.y,
                width: width(&shown),
                height: 1,
            };
            ui.status.push((r, cmd.clone()));
        }
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
        Mode::Normal | Mode::Insert | Mode::Visual | Mode::Tracks | Mode::Properties => {
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

fn draw_which_key(f: &mut Frame, app: &App, title: &str, rows: &[(String, String)], main: Rect) {
    let lines: Vec<Line> = rows
        .iter()
        .map(|(keys, desc)| {
            Line::from(vec![
                Span::styled(
                    format!("{keys:>8} "),
                    Style::default().fg(rc(app.config.theme.accent)),
                ),
                Span::raw(desc.clone()),
            ])
        })
        .collect();
    let widest = rows
        .iter()
        .map(|(_, d)| d.chars().count())
        .max()
        .unwrap_or(0) as u16;
    let h = (lines.len() as u16 + 2).min(main.height);
    let w = (widest + 12).clamp(40, main.width);
    let area = Rect {
        x: main.x + main.width - w,
        y: main.y + main.height - h,
        width: w,
        height: h,
    };
    f.render_widget(Clear, area);
    f.render_widget(
        Paragraph::new(lines).block(Block::bordered().title(format!(" {title} "))),
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
        assert!(app.has_tool(PaneKind::Tracks));
        // Hidden with :toolbar.
        app.run(muse_app::Command::Toolbar(Some(false)));
        let screen = render(&mut app, 120, 20);
        let first = screen.lines().next().unwrap();
        assert!(!first.contains(" save "), "{screen}");
    }

    #[test]
    fn fretboard_and_piano_panels_show_the_cursors_notes() {
        let mut app = App::new(muse_app::Config::default(), None);
        app.run(muse_app::Command::New);
        for k in muse_app::keys::parse_keys("i3<Down>5<Esc>", &[]).unwrap() {
            app.handle_key(k);
        }
        // (The first frame sets the size panes are fitted to.)
        render(&mut app, 100, 24);
        app.run(muse_app::Command::Fretboard(Some(true)));
        app.run(muse_app::Command::Piano(Some(true)));
        let screen = render(&mut app, 100, 24);
        let lines: Vec<&str> = screen.lines().collect();
        // High E fret 3 is a G, B fret 5 an E; the open strings are named.
        let fretboard: Vec<&&str> = lines.iter().filter(|l| l.contains('║')).collect();
        assert_eq!(fretboard.len(), 6, "{screen}");
        assert!(fretboard[0].contains(" G "), "{screen}");
        assert!(fretboard[1].contains(" E "), "{screen}");
        // Clicking a fret enters the note; a key adds a chord note.
        let mut terminal = Terminal::new(TestBackend::new(100, 24)).unwrap();
        let mut ui = Ui::default();
        terminal.draw(|f| draw(f, &mut app, None, &mut ui)).unwrap();
        let &(_, s, k) = ui
            .frets
            .iter()
            .find(|(_, s, k)| *s == 2 && *k == 2)
            .unwrap();
        app.click_fret(s, k);
        let v = app.instrument_view(None).unwrap();
        assert!(v.notes.iter().any(|n| n.string == 2 && n.fret == 2));
        // A capo shows as a solid bar across its fret.
        let b = app.pane().buffer;
        app.buffers[b].score.tracks[0].capo = 2;
        let screen = render(&mut app, 100, 24);
        assert_eq!(
            screen
                .lines()
                .filter(|l| l.contains('║') && l.contains('▌'))
                .count(),
            6,
            "{screen}"
        );
        // The piano row with octave numbers.
        assert!(
            lines
                .iter()
                .any(|l| l.contains('4') && l.contains('3') && l.contains('5'))
        );
    }

    #[test]
    fn the_tuner_panel_shows_the_reading() {
        let mut app = App::new(muse_app::Config::default(), None);
        app.run(muse_app::Command::New);
        app.set_tool(PaneKind::Tuner, Some(true));
        app.tuner_reading = Some(muse_audio::tuner::Reading {
            freq: 109.0,
            clarity: 0.95,
        });
        let screen = render(&mut app, 100, 24);
        assert!(screen.contains(" A2 "), "{screen}");
        assert!(
            screen.contains("-16¢") && screen.contains("109.0 Hz"),
            "{screen}"
        );
        assert!(screen.contains("string 5 (A2)"), "{screen}");
        assert!(screen.contains('█') && screen.contains("+50"), "{screen}");
    }

    #[test]
    fn the_properties_pane_shows_rows_and_takes_clicks() {
        let mut app = App::new(muse_app::Config::default(), None);
        app.run(muse_app::Command::New);
        let mut terminal = Terminal::new(TestBackend::new(100, 30)).unwrap();
        let mut ui = Ui::default();
        terminal.draw(|f| draw(f, &mut app, None, &mut ui)).unwrap();
        let screen = render(&mut app, 100, 30);
        assert!(screen.contains("properties"), "{screen}");
        assert!(screen.contains("score") && screen.contains("lyrics"), "{screen}");
        assert!(screen.contains("▾ Info") && screen.contains("Title"), "{screen}");
        assert!(screen.contains("Tempo") && screen.contains("Width (mm)"), "{screen}");
        // Clicking the tracks tab shows the cursor track's properties.
        let tab = ui
            .props
            .iter()
            .find_map(|(_, h)| match h {
                PropHit::Tab(t) if *t == muse_app::properties::PropTab::Tracks => Some(*h),
                _ => None,
            })
            .unwrap();
        click_properties(&mut app, tab);
        let screen = render(&mut app, 100, 30);
        assert!(screen.contains("Instrument") && screen.contains("Tuning"), "{screen}");
        assert_eq!(app.focused_kind(), PaneKind::Properties);
        // A click on a row selects it; another edits it.
        click_properties(&mut app, PropHit::Row(2));
        assert_eq!(app.property_selected(), 2);
        click_properties(&mut app, PropHit::Row(2));
        assert!(app.props.edit.is_some());
        let screen = render(&mut app, 100, 30);
        assert!(screen.contains('▏'), "{screen}");
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
        // Clicking the layout section cycles the pane's layout.
        let (_, cmd) = ui
            .status
            .iter()
            .find(|(r, _)| {
                (r.x..r.x + r.width)
                    .map(|x| buf[(x, y)].symbol())
                    .collect::<String>()
                    .contains("vertical")
            })
            .cloned()
            .unwrap();
        app.run(cmd);
        assert_eq!(app.pane().layout, "page");
        // The tempo as a quarter note (Nerd Font glyph) and its value.
        assert!(row.contains("\u{f038b}=120"), "{row}");
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
        app.set_tool(PaneKind::Properties, Some(false));
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
        assert!(
            screen.contains(" tracks ") && screen.contains("NORMAL"),
            "{screen}"
        );
        assert!(screen.contains('■') || screen.contains('·'), "{screen}");
    }
}
