//! Builds the vello scene for a frame: chrome, panes with engraved notation, cursor and
//! playback highlights. Engraved pages are converted to scenes once and cached per pane.

use std::collections::HashMap;

use muse_app::layout::PaneId;
use muse_app::playback::Playback;
use muse_app::{App, Mode};
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
/// Height of the waveform strip.
const WAVE: f64 = 64.0;
/// Pixels per point at zoom 1.
const BASE_SCALE: f64 = 1.6;

struct Cached {
    /// (buffer, track, layout mode, width bucket, score version)
    key: (usize, usize, String, i64, u64, bool),
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
    mut playback: Option<&mut Playback>,
    play: Option<(usize, Location, f64, f64)>,
    width: f64,
    height: f64,
) {
    // The window-wide waveform bar sits between the panes and the status line.
    let global_wave = app.global_waveform && height > 300.0;
    let bottom = height - STATUS - CMDLINE - if global_wave { WAVE + 4.0 } else { 0.0 };
    let main = Rect::new(0.0, TAB_BAR, width, bottom);
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
        draw_pane(
            scene,
            app,
            views,
            playback.as_deref_mut(),
            id,
            rect,
            id == focused,
            play,
        );
    }
    if global_wave {
        let r = Rect::new(4.0, bottom + 2.0, width - 4.0, bottom + 2.0 + WAVE);
        let buffer = app.pane().buffer;
        match playback.as_deref_mut() {
            Some(pb) if app.buffers[buffer].score.audio.is_some() => {
                pb.ensure_audio(app, buffer);
                draw_waveform(scene, app, pb, buffer, r, play);
            }
            _ => {
                fill(scene, r, Color::from_rgba8(28, 30, 36, 255));
                let name = &app.buffers[buffer].name;
                text(
                    scene,
                    &format!("{name}: no audio attached (:audio FILE)"),
                    r.x0 + 10.0,
                    r.center().y + 5.0,
                    13.0,
                    Align::Left,
                    DIM,
                    false,
                );
            }
        }
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

fn mode_tag(pane: &muse_app::Pane) -> &'static str {
    match pane.layout.as_str() {
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
    playback: Option<&mut Playback>,
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
    let mut content = Rect::new(rect.x0 + 2.0, rect.y0 + TITLE, rect.x1 - 2.0, rect.y1 - 2.0);
    let waveform =
        (pane.waveform && score.audio.is_some() && content.height() > 200.0).then(|| {
            let strip = Rect::new(content.x0, content.y1 - WAVE, content.x1, content.y1);
            content.y1 -= WAVE + 2.0;
            strip
        });
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
    let mode_name = mode_tag(&pane);
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
        buffer.version,
        app.chant_view,
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
            chant: app.chant_view,
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
        voice: pane.cursor.voice,
        beat: pane.cursor.beat,
    };
    let mut scroll = views.scroll.get(&id).copied().unwrap_or((0.0, 0.0));
    let view_w = content.width() / scale;
    let view_h = content.height() / scale;
    let cursor_key = (pane.cursor.track, pane.cursor.bar, pane.cursor.beat);
    let moved = views.last_cursor.insert(id, cursor_key) != Some(cursor_key);
    // The playhead, while this buffer plays: (page, x, top, bottom).
    let playhead = play
        .filter(|p| p.0 == pane.buffer && app.playing)
        .and_then(|(_, loc, _, _)| {
            playhead_at(&cached.layout, score, pane.cursor.track, loc.bar, loc.tick)
        });
    // Keep the playhead in view while following playback, otherwise the cursor when it moves.
    let target = match playhead.filter(|_| app.follow) {
        Some((page, x, top, bottom)) => Some((page, x, 1.0, top, bottom)),
        None if moved => cached
            .layout
            .beats
            .get(&cursor_ref)
            .or_else(|| beat_fallback(&cached.layout, &cursor_ref))
            .map(|p| (p.page, p.x, p.width, p.top, p.bottom)),
        None => None,
    };
    if let Some((page, x, w, top, bottom)) = target {
        let top = cached.offsets[page] + top;
        let bottom = cached.offsets[page] + bottom;
        if top < scroll.1 || bottom > scroll.1 + view_h {
            scroll.1 = (top - view_h * 0.25).max(0.0);
        }
        if x < scroll.0 || x + w > scroll.0 + view_w {
            scroll.0 = (x - view_w * 0.3).max(0.0);
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
    // Visual selection.
    if let Some((anchor, cursor, bars)) = app.selection().filter(|_| focused) {
        let (a, b) = if (anchor.bar, anchor.beat) <= (cursor.bar, cursor.beat) {
            (anchor, cursor)
        } else {
            (cursor, anchor)
        };
        for (r, pos) in &cached.layout.beats {
            if r.track != pane.cursor.track || r.voice != pane.cursor.voice {
                continue;
            }
            let inside = if bars {
                (a.bar..=b.bar).contains(&r.bar)
            } else {
                (a.bar, a.beat) <= (r.bar, r.beat) && (r.bar, r.beat) <= (b.bar, b.beat)
            };
            if inside {
                let rect = Rect::new(pos.x, pos.top, pos.x + pos.width, pos.bottom);
                scene.fill(
                    Fill::NonZero,
                    to_screen(pos.page),
                    Color::from_rgba8(150, 90, 220, 55),
                    None,
                    &rect,
                );
            }
        }
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
    // Playhead: a red bar at the exact playback position, drawn over the cursor.
    if let Some((page, x, top, bottom)) = playhead {
        let w = 2.0 / scale;
        scene.fill(
            Fill::NonZero,
            to_screen(page),
            Color::from_rgba8(225, 40, 40, 230),
            None,
            &Rect::new(x - w / 2.0, top, x + w / 2.0, bottom),
        );
    }
    scene.pop_layer();
    if let (Some(strip), Some(pb)) = (waveform, playback) {
        draw_waveform(scene, app, pb, pane.buffer, strip, play);
    }
}

/// The audio track around the playhead (or cursor): envelope, sync points, loop and playhead.
fn draw_waveform(
    scene: &mut Scene,
    app: &App,
    playback: &mut Playback,
    buffer: usize,
    r: Rect,
    play: Option<(usize, Location, f64, f64)>,
) {
    fill(scene, r, Color::from_rgba8(28, 30, 36, 255));
    let Some(audio) = playback.loaded_audio(buffer) else {
        let label = if playback.audio_loading(buffer) {
            "loading audio…"
        } else {
            "audio not loaded (play or :sync-detect to load)"
        };
        text(
            scene,
            label,
            r.x0 + 10.0,
            r.center().y + 5.0,
            13.0,
            Align::Left,
            DIM,
            false,
        );
        return;
    };
    let (peaks, duration) = (audio.peaks.clone(), audio.clip.duration());
    // The edit cursor: the focused pane's if it shows this file, else another pane's.
    let cursor = Some(app.pane())
        .filter(|p| p.buffer == buffer)
        .or_else(|| app.panes.values().find(|p| p.buffer == buffer))
        .map(|p| p.cursor)
        .unwrap_or_default();
    let tick = muse_app::cursor_tick(&app.buffers[buffer].score, &cursor);
    let now = match play.filter(|p| p.0 == buffer && app.playing) {
        Some((_, _, t, _)) => t,
        None => playback
            .time_of(app, buffer, cursor.bar, tick)
            .unwrap_or(0.0),
    };
    let span = 16.0;
    let t0 = now - span * 0.35;
    let x_of = |t: f64| r.x0 + (t - t0) / span * r.width();
    let mid = r.center().y;
    let amp = r.height() * 0.45;
    let wave = Color::from_rgba8(110, 170, 230, 255);
    let mut x = r.x0;
    while x < r.x1 {
        let t = t0 + (x - r.x0) / r.width() * span;
        let t2 = t + span / r.width();
        let (a, b) = (
            (t.max(0.0) * 100.0) as usize,
            ((t2.max(0.0) * 100.0) as usize).max((t.max(0.0) * 100.0) as usize + 1),
        );
        if t >= 0.0 && t < duration {
            let (lo, hi) = peaks[a.min(peaks.len())..b.min(peaks.len())]
                .iter()
                .fold((0f32, 0f32), |acc, p| (acc.0.min(p.0), acc.1.max(p.1)));
            let rr = Rect::new(
                x,
                mid - hi as f64 * amp,
                x + 1.0,
                mid - lo as f64 * amp + 1.0,
            );
            scene.fill(Fill::NonZero, Affine::IDENTITY, wave, None, &rr);
        }
        x += 1.0;
    }
    // Loop region.
    if let Some((a, b)) = app.pa.loop_bars {
        if let (Some(ta), Some(tb)) = (
            playback.time_of(app, buffer, a, 0),
            playback.time_of(app, buffer, b + 1, 0).or(Some(duration)),
        ) {
            let rr = Rect::new(x_of(ta).max(r.x0), r.y0, x_of(tb).min(r.x1), r.y1);
            if rr.width() > 0.0 {
                scene.fill(
                    Fill::NonZero,
                    Affine::IDENTITY,
                    Color::from_rgba8(60, 190, 110, 50),
                    None,
                    &rr,
                );
            }
        }
    }
    // Sync points.
    let score = &app.buffers[buffer].score;
    if let Some(sync) = &score.audio {
        let pad = sync.frame_padding as f64 / muse_audio::SYNC_RATE;
        for p in &sync.points {
            let t = p.frame as f64 / muse_audio::SYNC_RATE - pad;
            let x = x_of(t);
            if x >= r.x0 && x <= r.x1 {
                fill(scene, Rect::new(x, r.y0, x + 1.5, r.y1), ACCENT);
                text(
                    scene,
                    &format!("{}", p.bar + 1),
                    x + 3.0,
                    r.y0 + 12.0,
                    11.0,
                    Align::Left,
                    ACCENT,
                    false,
                );
            }
        }
    }
    let px = x_of(now);
    fill(
        scene,
        Rect::new(px - 1.0, r.y0, px + 1.0, r.y1),
        Color::from_rgba8(240, 80, 70, 255),
    );
    let f = |t: f64| format!("{}:{:04.1}", (t.max(0.0) / 60.0) as u64, t.max(0.0) % 60.0);
    text(
        scene,
        &f(now),
        r.x1 - 8.0,
        r.y1 - 6.0,
        11.0,
        Align::Right,
        CHROME_TEXT,
        false,
    );
}

/// Cursor on a beat the layout skipped (e.g. an empty bar): use the bar's first beat.
/// Where the playhead is drawn at (bar, tick): between the notes of the bar in proportion to
/// time (on the track's first voice; the bar's edges without notes), never across a line break.
/// Returns (page, x, top, bottom) in page coordinates.
fn playhead_at(
    layout: &Layout,
    score: &muse_core::Score,
    track: usize,
    bar: usize,
    tick: u32,
) -> Option<(usize, f64, f64, f64)> {
    let beats = &score
        .tracks
        .get(track)?
        .bars
        .get(bar)?
        .voices
        .first()?
        .beats;
    // (tick, page, x, top, bottom) of each drawn beat, in time order.
    let mut points = Vec::new();
    let mut t = 0;
    for (i, b) in beats.iter().enumerate() {
        if !b.empty {
            let r = BeatRef {
                track,
                bar,
                voice: 0,
                beat: i,
            };
            if let Some(p) = layout.beats.get(&r) {
                points.push((t, p.page, p.x + 4.0, p.top, p.bottom));
            }
        }
        t += b.ticks();
    }
    let &(bar_page, bar_x0, bar_x1) = layout.bars.get(bar)?;
    let end_tick = score.played_ticks(bar).max(1);
    let (top, bottom) = points.first().map(|p| (p.3, p.4)).or_else(|| {
        layout
            .beats
            .iter()
            .find(|(r, _)| r.bar == bar && r.track == track)
            .map(|(_, p)| (p.top, p.bottom))
    })?;
    if points.is_empty() {
        let x = bar_x0 + (bar_x1 - bar_x0) * tick as f64 / end_tick as f64;
        return Some((bar_page, x, top, bottom));
    }
    let k = points.iter().rposition(|p| p.0 <= tick).unwrap_or(0);
    let a = points[k];
    let (next_tick, next_x, same_line) = match points.get(k + 1) {
        Some(b) => (b.0, b.2, b.1 == a.1 && (b.3 - a.3).abs() < 0.5),
        None => (end_tick, bar_x1 - 2.0, bar_page == a.1 && bar_x1 > a.2),
    };
    let frac = if next_tick > a.0 {
        ((tick.saturating_sub(a.0)) as f64 / (next_tick - a.0) as f64).clamp(0.0, 1.0)
    } else {
        0.0
    };
    let x = if same_line {
        a.2 + (next_x - a.2) * frac
    } else {
        a.2
    };
    Some((a.1, x, a.3, a.4))
}

fn beat_fallback<'a>(layout: &'a Layout, r: &BeatRef) -> Option<&'a muse_engrave::BeatPos> {
    layout.beats.get(&BeatRef { beat: 0, ..*r }).or_else(|| {
        layout
            .beats
            .iter()
            .find(|(k, _)| k.bar == r.bar)
            .map(|(_, v)| v)
    })
}

/// The command line while typing a command, otherwise the last message.
fn draw_command_line(scene: &mut Scene, app: &App, width: f64, height: f64) {
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
        Mode::Normal | Mode::Insert | Mode::Visual => {
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

fn draw_status(
    scene: &mut Scene,
    app: &App,
    play: Option<(usize, Location, f64, f64)>,
    width: f64,
    height: f64,
) {
    let y0 = height - STATUS - CMDLINE;
    fill(scene, Rect::new(0.0, y0, width, height), CHROME);
    if let Some((items, selected)) = app.wildmenu() {
        // Completion candidates replace the status line while completing.
        let fonts = muse_engrave::fonts::fonts();
        let widths: Vec<f64> = items
            .iter()
            .map(|s| fonts.text_width(FontKind::Text, s, 13.0) + 18.0)
            .collect();
        let mut first = 0;
        if let Some(sel) = selected {
            while widths[first..=sel].iter().sum::<f64>() > width - 40.0 && first < sel {
                first += 1;
            }
        }
        let mut x = 10.0;
        for (i, item) in items.iter().enumerate().skip(first) {
            if x + widths[i] > width - 10.0 {
                text(
                    scene,
                    "…",
                    x,
                    y0 + 17.5,
                    13.0,
                    Align::Left,
                    CHROME_TEXT,
                    false,
                );
                break;
            }
            if Some(i) == selected {
                scene.fill(
                    Fill::NonZero,
                    Affine::IDENTITY,
                    ACCENT,
                    None,
                    &RoundedRect::new(
                        x - 4.0,
                        y0 + 4.0,
                        x + widths[i] - 14.0,
                        y0 + STATUS - 4.0,
                        4.0,
                    ),
                );
                text(
                    scene,
                    item,
                    x + 1.0,
                    y0 + 17.5,
                    13.0,
                    Align::Left,
                    CHROME,
                    true,
                );
            } else {
                text(
                    scene,
                    item,
                    x + 1.0,
                    y0 + 17.5,
                    13.0,
                    Align::Left,
                    CHROME_TEXT,
                    false,
                );
            }
            x += widths[i];
        }
        draw_command_line(scene, app, width, height);
        return;
    }
    let (label, color) = match app.mode {
        Mode::Normal => (" NORMAL ", Color::from_rgba8(90, 150, 230, 255)),
        Mode::Command => (" COMMAND ", Color::from_rgba8(100, 190, 120, 255)),
        Mode::Insert => (" INSERT ", Color::from_rgba8(240, 180, 60, 255)),
        Mode::Visual => (" VISUAL ", Color::from_rgba8(190, 120, 220, 255)),
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
    right.push_str("   ");
    right.push_str(&app.pa.status(
        app.config.file.audio.speed,
        app.buffer().score.audio.is_some(),
    ));
    if let Some(r) = app.recording_macro() {
        right.push_str(&format!("   recording @{r}"));
    }
    if app.mode == Mode::Insert {
        let d = app.insert_duration();
        right.push_str(&format!(
            "   new beats: 1/{}{}",
            d.value.denominator(),
            ".".repeat(d.dots as usize)
        ));
    }
    if app.buffer().modified {
        right.push_str("   [+]");
    }
    right.push_str(&format!(
        "   {}  {:.0}%",
        mode_tag(app.pane()),
        app.zoom * 100.0
    ));
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

    draw_command_line(scene, app, width, height);
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
        let horizontal = app
            .panes
            .get(&g.pane)
            .is_some_and(|p| p.layout == "horizontal");
        let entry = self.scroll.entry(g.pane).or_insert((0.0, 0.0));
        if horizontal {
            entry.0 = (entry.0 + dy / g.scale).max(0.0);
        } else {
            entry.1 = (entry.1 + dy / g.scale).max(0.0);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use muse_core::*;

    #[test]
    fn playhead_moves_with_time_between_notes() {
        let beat = |fret| Beat {
            duration: Duration::new(DurationValue::Quarter),
            notes: vec![Note::new(0, fret)],
            ..Beat::default()
        };
        let score = Score {
            tempo: 100,
            master_bars: vec![MasterBar::default(); 2],
            tracks: vec![Track {
                strings: vec![64, 59, 55, 50, 45, 40],
                bars: vec![
                    Bar {
                        voices: vec![Voice {
                            beats: vec![beat(0), beat(1), beat(2), beat(3)],
                        }],
                    };
                    2
                ],
                ..Track::default()
            }],
            ..Score::default()
        };
        let layout = muse_engrave::engrave(&score, 0, &Options::default());
        let x = |tick| playhead_at(&layout, &score, 0, 0, tick).unwrap().1;
        let beat_x = |i| {
            layout.beats[&BeatRef {
                track: 0,
                bar: 0,
                voice: 0,
                beat: i,
            }]
                .x
                + 4.0
        };
        assert!((x(0) - beat_x(0)).abs() < 1e-6);
        // Halfway through the first beat: halfway to the second note.
        assert!((x(480) - (beat_x(0) + beat_x(1)) / 2.0).abs() < 1e-6);
        assert!(x(960) > x(480) && x(3800) > x(2880));
        // The last beat runs on towards the barline.
        assert!(x(3800) < layout.bars[0].2);
    }
}
