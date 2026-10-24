//! Builds the vello scene for a frame: chrome, panes with engraved notation, cursor and
//! playback highlights. Engraved pages are converted to scenes once and cached per pane.

use std::collections::HashMap;

use muse_app::layout::PaneId;
use muse_app::{App, Mode, beat_at_tick};
use muse_audio::Location;
use muse_engrave::display::{Align, BeatRef, FontKind};
use muse_engrave::output::{Paint, outlines};
use muse_engrave::{Layout, Options};
use vello::Scene;
use vello::kurbo::{Affine, BezPath, Rect, RoundedRect, Stroke};
use vello::peniko::{Color, Fill, Mix};

pub const BACKGROUND: Color = Color::from_rgba8(232, 233, 236, 255);
const CHROME: Color = Color::from_rgba8(40, 42, 48, 255);
const CHROME_TEXT: Color = Color::from_rgba8(225, 225, 230, 255);
const ACCENT: Color = Color::from_rgba8(250, 190, 40, 255);
const DIM: Color = Color::from_rgba8(150, 152, 160, 255);

const TAB_BAR: f64 = 30.0;
const STATUS: f64 = 26.0;
const CMDLINE: f64 = 26.0;
const TITLE: f64 = 24.0;
const UI_FONT: f64 = 14.0;
/// Pixels per point at zoom 1.
const BASE_SCALE: f64 = 1.6;

struct Cached {
    key: (usize, usize, String, i64),
    layout: Layout,
    scenes: Vec<Scene>,
    /// Y offset (points) of each page in the pane's scroll space.
    offsets: Vec<f64>,
}

#[derive(Clone, Copy)]
struct Geometry {
    pane: PaneId,
    content: Rect,
    scale: f64,
    scroll: (f64, f64),
}

/// Per-pane view state kept between frames.
#[derive(Default)]
pub struct Views {
    cache: HashMap<PaneId, Cached>,
    scroll: HashMap<PaneId, (f64, f64)>,
    geometry: Vec<Geometry>,
    last_cursor: HashMap<PaneId, (usize, usize, usize)>,
}

fn rgb(c: muse_engrave::display::Color) -> Color {
    Color::from_rgba8(c.r, c.g, c.b, c.a)
}

fn page_scene(page: &muse_engrave::display::Page) -> Scene {
    let mut s = Scene::new();
    s.fill(
        Fill::NonZero,
        Affine::IDENTITY,
        Color::WHITE,
        None,
        &Rect::new(0.0, 0.0, page.width, page.height),
    );
    for item in &page.items {
        for (path, paint) in outlines(item) {
            match paint {
                Paint::Fill(c) => s.fill(Fill::NonZero, Affine::IDENTITY, rgb(c), None, &path),
                Paint::Stroke(c, w) => {
                    s.stroke(&Stroke::new(w), Affine::IDENTITY, rgb(c), None, &path)
                }
            }
        }
    }
    s
}

#[allow(clippy::too_many_arguments)]
fn text(
    scene: &mut Scene,
    s: &str,
    x: f64,
    y: f64,
    size: f64,
    align: Align,
    color: Color,
    bold: bool,
) {
    let font = if bold { FontKind::Bold } else { FontKind::Text };
    let path: BezPath = muse_engrave::fonts::fonts().text_path(font, s, x, y, size, align);
    scene.fill(Fill::NonZero, Affine::IDENTITY, color, None, &path);
}

fn fill(scene: &mut Scene, r: Rect, c: Color) {
    scene.fill(Fill::NonZero, Affine::IDENTITY, c, None, &r);
}

pub fn build(
    scene: &mut Scene,
    app: &mut App,
    views: &mut Views,
    play: Option<(usize, Location, f64, f64)>,
    width: f64,
    height: f64,
) {
    let main = Rect::new(0.0, TAB_BAR, width, height - STATUS - CMDLINE);
    app.viewport = muse_app::layout::Rect {
        x: main.x0 as u16,
        y: main.y0 as u16,
        w: main.width() as u16,
        h: main.height() as u16,
    };
    draw_tabs(scene, app, width);
    views.geometry.clear();
    let focused = app.focused();
    let layout = app.layout();
    for (id, r) in layout {
        let rect = Rect::new(
            r.x as f64 + 3.0,
            r.y as f64 + 3.0,
            (r.x + r.w) as f64 - 3.0,
            (r.y + r.h) as f64 - 3.0,
        );
        draw_pane(scene, app, views, id, rect, id == focused, play);
    }
    draw_status(scene, app, play, width, height);
    if !app.pending.is_empty() {
        draw_which_key(scene, app, main);
    }
    if app.show_help {
        draw_help(scene, app, main);
    }
    views.cache.retain(|id, _| app.panes.contains_key(id));
}

fn draw_tabs(scene: &mut Scene, app: &App, width: f64) {
    fill(scene, Rect::new(0.0, 0.0, width, TAB_BAR), CHROME);
    let mut x = 8.0;
    for (i, tab) in app.tabs.iter().enumerate() {
        let name = app
            .panes
            .get(&tab.focus)
            .map_or("?", |p| app.buffers[p.buffer].name.as_str());
        let label = format!("{}  {}", i + 1, name);
        let w = muse_engrave::fonts::fonts().text_width(FontKind::Text, &label, UI_FONT) + 24.0;
        let active = i == app.active_tab;
        let bg = if active {
            BACKGROUND
        } else {
            Color::from_rgba8(60, 63, 72, 255)
        };
        scene.fill(
            Fill::NonZero,
            Affine::IDENTITY,
            bg,
            None,
            &RoundedRect::new(x, 5.0, x + w, TAB_BAR + 4.0, 6.0),
        );
        text(
            scene,
            &label,
            x + 12.0,
            21.0,
            UI_FONT,
            Align::Left,
            if active { CHROME } else { CHROME_TEXT },
            active,
        );
        x += w + 4.0;
    }
}

fn mode_tag(app: &App) -> &'static str {
    match app.score_layout.as_str() {
        "page" => "page",
        "horizontal" => "horizontal",
        _ => "vertical",
    }
}

#[allow(clippy::too_many_arguments)]
fn draw_pane(
    scene: &mut Scene,
    app: &mut App,
    views: &mut Views,
    id: PaneId,
    rect: Rect,
    focused: bool,
    play: Option<(usize, Location, f64, f64)>,
) {
    let pane = app.panes[&id].clone();
    let buffer = &app.buffers[pane.buffer];
    let score = &buffer.score;
    // Frame and title strip.
    scene.fill(
        Fill::NonZero,
        Affine::IDENTITY,
        Color::WHITE,
        None,
        &RoundedRect::from_rect(rect, 6.0),
    );
    let border = if focused {
        ACCENT
    } else {
        Color::from_rgba8(190, 192, 198, 255)
    };
    scene.stroke(
        &Stroke::new(if focused { 2.0 } else { 1.0 }),
        Affine::IDENTITY,
        border,
        None,
        &RoundedRect::from_rect(rect, 6.0),
    );
    let title = match score.tracks.get(pane.cursor.track) {
        Some(t) => format!(
            "{} — {}/{}  {}",
            buffer.name,
            pane.cursor.track + 1,
            score.tracks.len(),
            t.name.trim()
        ),
        None => buffer.name.clone(),
    };
    text(
        scene,
        &title,
        rect.x0 + 10.0,
        rect.y0 + 17.0,
        13.0,
        Align::Left,
        DIM,
        false,
    );
    let content = Rect::new(rect.x0 + 2.0, rect.y0 + TITLE, rect.x1 - 2.0, rect.y1 - 2.0);
    if score.tracks.is_empty() {
        text(
            scene,
            "No score loaded — open one with :e <file>",
            content.center().x,
            content.center().y,
            16.0,
            Align::Center,
            DIM,
            false,
        );
        return;
    }

    // Layout for this pane's width, mode and track (cached).
    let mode_name = mode_tag(app);
    // Pages fit the pane's width; zoom is relative to that.
    let scale = if mode_name == "page" {
        app.zoom * (content.width() - 24.0).max(100.0) / 595.0
    } else {
        BASE_SCALE * app.zoom
    };
    let width_pt = (content.width() / scale).floor();
    let key = (
        pane.buffer,
        pane.cursor.track,
        mode_name.to_string(),
        if mode_name == "vertical" {
            (width_pt / 8.0) as i64
        } else {
            0
        },
    );
    if views.cache.get(&id).is_none_or(|c| c.key != key) {
        let mode = match mode_name {
            "page" => muse_engrave::Mode::Page {
                width: 595.0,
                height: 842.0,
            },
            "horizontal" => muse_engrave::Mode::Horizontal,
            _ => muse_engrave::Mode::Vertical {
                width: (key.3 as f64 * 8.0).max(300.0),
            },
        };
        let options = Options {
            mode,
            header: mode_name != "horizontal",
            margin: if mode_name == "page" { 40.0 } else { 16.0 },
            ..Options::default()
        };
        let layout = muse_engrave::engrave(score, pane.cursor.track, &options);
        let scenes = layout.pages.iter().map(page_scene).collect();
        let gap = if mode_name == "page" { 24.0 } else { 0.0 };
        let mut offsets = Vec::new();
        let mut y = 0.0;
        for p in &layout.pages {
            offsets.push(y);
            y += p.height + gap;
        }
        views.cache.insert(
            id,
            Cached {
                key,
                layout,
                scenes,
                offsets,
            },
        );
    }
    let cached = &views.cache[&id];

    // Keep the cursor in view when it moves.
    let cursor_ref = BeatRef {
        track: pane.cursor.track,
        bar: pane.cursor.bar,
        voice: 0,
        beat: pane.cursor.beat,
    };
    let mut scroll = views.scroll.get(&id).copied().unwrap_or((0.0, 0.0));
    let view_w = content.width() / scale;
    let view_h = content.height() / scale;
    let cursor_key = (pane.cursor.track, pane.cursor.bar, pane.cursor.beat);
    let moved = views.last_cursor.insert(id, cursor_key) != Some(cursor_key);
    if let Some(pos) = cached
        .layout
        .beats
        .get(&cursor_ref)
        .or_else(|| beat_fallback(&cached.layout, &cursor_ref))
    {
        let top = cached.offsets[pos.page] + pos.top;
        let bottom = cached.offsets[pos.page] + pos.bottom;
        if moved || app.playing {
            if top < scroll.1 || bottom > scroll.1 + view_h {
                scroll.1 = (top - view_h * 0.25).max(0.0);
            }
            if pos.x < scroll.0 || pos.x + pos.width > scroll.0 + view_w {
                scroll.0 = (pos.x - view_w * 0.3).max(0.0);
            }
        }
    }
    let total_h = cached.offsets.last().copied().unwrap_or(0.0)
        + cached.layout.pages.last().map_or(0.0, |p| p.height);
    scroll.1 = scroll.1.clamp(0.0, (total_h - view_h * 0.5).max(0.0));
    views.scroll.insert(id, scroll);
    views.geometry.push(Geometry {
        pane: id,
        content,
        scale,
        scroll,
    });

    // Pages.
    scene.push_layer(Mix::Clip, 1.0, Affine::IDENTITY, &content);
    let page_mode = mode_name == "page";
    if page_mode {
        fill(scene, content, BACKGROUND);
    }
    let center = if page_mode {
        ((view_w - 595.0) / 2.0).max(0.0)
    } else {
        0.0
    };
    let to_screen = |page: usize| {
        Affine::translate((
            content.x0 + (center - scroll.0) * scale,
            content.y0 + (cached.offsets[page] - scroll.1) * scale,
        )) * Affine::scale(scale)
    };
    for (i, page_scene) in cached.scenes.iter().enumerate() {
        let top = cached.offsets[i];
        let h = cached.layout.pages[i].height;
        if top + h < scroll.1 || top > scroll.1 + view_h {
            continue;
        }
        scene.append(page_scene, Some(to_screen(i)));
    }
    // Playback position.
    if let Some((pb, loc, _, _)) = play.filter(|p| p.0 == pane.buffer && app.playing) {
        let beat = beat_at_tick(score, pane.cursor.track, loc.bar, loc.tick);
        if let Some(pos) = cached.layout.beats.get(&BeatRef {
            track: pane.cursor.track,
            bar: loc.bar,
            voice: 0,
            beat,
        }) {
            let r = Rect::new(pos.x, pos.top, pos.x + pos.width, pos.bottom);
            scene.fill(
                Fill::NonZero,
                to_screen(pos.page),
                Color::from_rgba8(60, 190, 110, 70),
                None,
                &r,
            );
        }
        let _ = pb;
    }
    // Cursor.
    if let Some(pos) = cached.layout.beats.get(&cursor_ref) {
        let alpha = if focused { 60 } else { 30 };
        let r = Rect::new(pos.x, pos.top, pos.x + pos.width, pos.bottom);
        scene.fill(
            Fill::NonZero,
            to_screen(pos.page),
            Color::from_rgba8(250, 190, 40, alpha),
            None,
            &r,
        );
        if let Some(&y) = pos.strings.get(pane.cursor.string) {
            let r = RoundedRect::new(pos.x, y - 4.5, pos.x + pos.width.min(16.0), y + 4.5, 2.0);
            scene.stroke(
                &Stroke::new(1.2 / scale),
                to_screen(pos.page),
                Color::from_rgba8(230, 140, 0, 255),
                None,
                &r,
            );
        }
    }
    scene.pop_layer();
}

/// Cursor on a beat the layout skipped (e.g. an empty bar): use the bar's first beat.
fn beat_fallback<'a>(layout: &'a Layout, r: &BeatRef) -> Option<&'a muse_engrave::BeatPos> {
    layout.beats.get(&BeatRef { beat: 0, ..*r }).or_else(|| {
        layout
            .beats
            .iter()
            .find(|(k, _)| k.bar == r.bar)
            .map(|(_, v)| v)
    })
}

fn draw_status(
    scene: &mut Scene,
    app: &App,
    play: Option<(usize, Location, f64, f64)>,
    width: f64,
    height: f64,
) {
    let y0 = height - STATUS - CMDLINE;
    fill(scene, Rect::new(0.0, y0, width, height), CHROME);
    let (label, color) = match app.mode {
        Mode::Normal => (" NORMAL ", Color::from_rgba8(90, 150, 230, 255)),
        Mode::Command => (" COMMAND ", Color::from_rgba8(100, 190, 120, 255)),
    };
    let w = muse_engrave::fonts::fonts().text_width(FontKind::Bold, label, 13.0) + 8.0;
    scene.fill(
        Fill::NonZero,
        Affine::IDENTITY,
        color,
        None,
        &RoundedRect::new(6.0, y0 + 4.0, 6.0 + w, y0 + STATUS - 4.0, 4.0),
    );
    text(
        scene,
        label,
        10.0,
        y0 + 17.5,
        13.0,
        Align::Left,
        CHROME,
        true,
    );
    let pane = app.pane();
    let score = &app.buffer().score;
    let mut left = score.meta.title.clone();
    if !score.meta.artist.is_empty() {
        left.push_str(&format!(" — {}", score.meta.artist));
    }
    text(
        scene,
        &left,
        20.0 + w,
        y0 + 17.5,
        13.0,
        Align::Left,
        CHROME_TEXT,
        false,
    );
    let mut right = String::new();
    if !score.tracks.is_empty() {
        right.push_str(&format!(
            "bar {}/{}  beat {}  string {}  tempo {}",
            pane.cursor.bar + 1,
            score.master_bars.len(),
            pane.cursor.beat + 1,
            pane.cursor.string + 1,
            score.tempo
        ));
    }
    if let Some((_, _, t, total)) = play {
        let f = |t: f64| format!("{}:{:02}", t as u64 / 60, t as u64 % 60);
        right.push_str(&format!(
            "   {} {}/{}",
            if app.playing { "▶" } else { "⏸" },
            f(t),
            f(total)
        ));
    }
    right.push_str(&format!("   {}  {:.0}%", mode_tag(app), app.zoom * 100.0));
    text(
        scene,
        &right,
        width - 12.0,
        y0 + 17.5,
        13.0,
        Align::Right,
        CHROME_TEXT,
        false,
    );

    // Command line / message.
    let cy = height - 8.0;
    match app.mode {
        Mode::Command => {
            let line = format!(":{}", app.cmdline);
            text(
                scene,
                &line,
                10.0,
                cy,
                14.0,
                Align::Left,
                Color::WHITE,
                false,
            );
            let cx =
                10.0 + muse_engrave::fonts::fonts().text_width(FontKind::Text, &line, 14.0) + 1.0;
            fill(scene, Rect::new(cx, cy - 13.0, cx + 2.0, cy + 3.0), ACCENT);
        }
        Mode::Normal => {
            if let Some(m) = &app.message {
                let c = if m.error {
                    Color::from_rgba8(255, 110, 100, 255)
                } else {
                    CHROME_TEXT
                };
                text(scene, &m.text, 10.0, cy, 14.0, Align::Left, c, false);
            }
            let mut pending = app.count.map(|c| c.to_string()).unwrap_or_default();
            pending.push_str(&muse_app::keys::format_keys(&app.pending));
            text(
                scene,
                &pending,
                width - 12.0,
                cy,
                14.0,
                Align::Right,
                ACCENT,
                false,
            );
        }
    }
}

fn command_name(cmd: &muse_app::Command) -> String {
    let dbg = format!("{cmd:?}");
    let head = dbg
        .split(['(', ' ', '{'])
        .next()
        .unwrap_or(&dbg)
        .to_string();
    let mut out = String::new();
    for (i, c) in head.chars().enumerate() {
        if c.is_uppercase() && i > 0 {
            out.push('-');
        }
        out.push(c.to_ascii_lowercase());
    }
    out
}

fn panel(scene: &mut Scene, r: Rect, title: &str) {
    scene.fill(
        Fill::NonZero,
        Affine::IDENTITY,
        Color::from_rgba8(30, 32, 38, 240),
        None,
        &RoundedRect::from_rect(r, 8.0),
    );
    text(
        scene,
        title,
        r.x0 + 14.0,
        r.y0 + 22.0,
        14.0,
        Align::Left,
        ACCENT,
        true,
    );
}

fn draw_which_key(scene: &mut Scene, app: &App, main: Rect) {
    let items = app.config.keymap(Mode::Normal).continuations(&app.pending);
    if items.is_empty() {
        return;
    }
    let h = 36.0 + items.len() as f64 * 20.0;
    let r = Rect::new(
        main.x1 - 300.0,
        main.y1 - h - 10.0,
        main.x1 - 10.0,
        main.y1 - 10.0,
    );
    panel(scene, r, &muse_app::keys::format_keys(&app.pending));
    for (i, (keys, cmds)) in items.iter().enumerate() {
        let y = r.y0 + 44.0 + i as f64 * 20.0;
        text(
            scene,
            &muse_app::keys::format_keys(keys),
            r.x0 + 90.0,
            y,
            14.0,
            Align::Right,
            ACCENT,
            false,
        );
        let names: Vec<String> = cmds.iter().map(command_name).collect();
        text(
            scene,
            &names.join(", "),
            r.x0 + 104.0,
            y,
            14.0,
            Align::Left,
            CHROME_TEXT,
            false,
        );
    }
}

fn draw_help(scene: &mut Scene, app: &App, main: Rect) {
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
    let r = main.inset(-60.0);
    panel(scene, r, "Key bindings — Esc to close");
    let rows = ((r.height() - 50.0) / 19.0).floor().max(1.0) as usize;
    for (i, (k, c)) in items.iter().enumerate() {
        let col = (i / rows) as f64;
        let x = r.x0 + 20.0 + col * 280.0;
        if x + 260.0 > r.x1 {
            break;
        }
        let y = r.y0 + 48.0 + (i % rows) as f64 * 19.0;
        text(scene, k, x + 110.0, y, 13.0, Align::Right, ACCENT, false);
        text(
            scene,
            c,
            x + 122.0,
            y,
            13.0,
            Align::Left,
            CHROME_TEXT,
            false,
        );
    }
}

impl Views {
    /// Pane and page-space point under a window position.
    fn locate(&self, x: f64, y: f64) -> Option<(Geometry, f64, f64)> {
        let g = *self.geometry.iter().find(|g| g.content.contains((x, y)))?;
        Some((
            g,
            (x - g.content.x0) / g.scale + g.scroll.0,
            (y - g.content.y0) / g.scale + g.scroll.1,
        ))
    }

    pub fn click(&mut self, app: &mut App, x: f64, y: f64) {
        let Some((g, px, py)) = self.locate(x, y) else {
            return;
        };
        app.tabs[app.active_tab].focus = g.pane;
        let Some(cached) = self.cache.get(&g.pane) else {
            return;
        };
        let page_mode = cached.key.2 == "page";
        let view_w = g.content.width() / g.scale;
        let center = if page_mode {
            ((view_w - 595.0) / 2.0).max(0.0)
        } else {
            0.0
        };
        let px = px - center;
        let Some(page) = cached.offsets.iter().rposition(|&o| o <= py) else {
            return;
        };
        let local = (px, py - cached.offsets[page]);
        let hits = &cached.layout.pages[page].hits;
        // Prefer the precise tab-string hit, then the beat column.
        let hit = hits
            .iter()
            .find(|h| h.string.is_some() && h.rect.contains(local))
            .or_else(|| hits.iter().find(|h| h.rect.contains(local)));
        if let (Some(h), Some(p)) = (hit, app.panes.get_mut(&g.pane)) {
            p.cursor.track = h.beat.track;
            p.cursor.bar = h.beat.bar;
            if h.beat.voice == 0 {
                p.cursor.beat = h.beat.beat;
            }
            if let Some(s) = h.string {
                p.cursor.string = s as usize;
            }
        }
    }

    pub fn scroll(&mut self, app: &App, x: f64, y: f64, dy: f64) {
        let Some((g, _, _)) = self.locate(x, y) else {
            return;
        };
        let horizontal = app.score_layout == "horizontal";
        let entry = self.scroll.entry(g.pane).or_insert((0.0, 0.0));
        if horizontal {
            entry.0 = (entry.0 + dy / g.scale).max(0.0);
        } else {
            entry.1 = (entry.1 + dy / g.scale).max(0.0);
        }
    }
}
