//! Builds the vello scene for a frame: chrome, panes with engraved notation, cursor and
//! playback highlights. Engraved pages are converted to scenes once and cached per pane.

use std::collections::HashMap;

use muse_app::layout::PaneId;
use muse_app::playback::Playback;
use muse_app::theme::Rgba;
use muse_app::{App, Mode};
use muse_audio::Location;
use muse_engrave::display::{Align, BeatRef, FontKind};
use muse_engrave::output::{Paint, outlines};
use muse_engrave::{Layout, Options};
use vello::Scene;
use vello::kurbo::{Affine, BezPath, Circle, Rect, RoundedRect, Stroke};
use vello::peniko::{Color, Fill, Mix};

thread_local! {
    /// The theme of the frame being built (set by [`build`]).
    static THEME: std::cell::RefCell<muse_app::theme::Theme> = Default::default();
}

/// A colour of the current theme.
fn tc(f: impl FnOnce(&muse_app::theme::Theme) -> Rgba) -> Color {
    THEME.with(|t| col(f(&t.borrow())))
}

fn col(c: Rgba) -> Color {
    Color::from_rgba8(c.r, c.g, c.b, c.a)
}

/// Behind the score (the theme's paper).
pub fn background() -> Color {
    tc(|t| t.paper)
}

fn chrome() -> Color {
    tc(|t| t.chrome)
}

fn chrome_text() -> Color {
    tc(|t| t.chrome_text)
}

fn accent() -> Color {
    tc(|t| t.accent)
}

/// Dark or light text, whichever reads on `bg`.
fn text_on(bg: Color) -> Color {
    let c = bg.to_rgba8();
    tc(|t| t.text_on(Rgba::rgb(c.r, c.g, c.b)))
}

fn dim() -> Color {
    tc(|t| t.dim)
}

const TAB_BAR: f64 = 30.0;
const STATUS: f64 = 26.0;
const CMDLINE: f64 = 26.0;
const TITLE: f64 = 24.0;
const UI_FONT: f64 = 14.0;
/// Height of the waveform strip.
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
    /// The drawn cursor per pane (page, top, x, width) and when: while following playback it
    /// glides after the playhead instead of jumping from beat to beat.
    glide: HashMap<PaneId, (usize, f64, f64, f64, std::time::Instant)>,
    /// Mouse position (logical), for hover effects on the tabline.
    pub hover: (f64, f64),
    /// Clickable areas of the tabline drawn last frame.
    tabline: Vec<(Rect, TablineHit)>,
    /// Clickable areas of the tracks panel drawn last frame.
    tracks: Vec<(Rect, TracksHit)>,
    /// The tracks panel's first visible row and horizontal scroll of its bar grid.
    tracks_scroll: (usize, f64),
    /// What the tracks panel last scrolled to follow (cursor track, bar): it scrolls again
    /// only when that moves, so the mouse wheel can look elsewhere meanwhile.
    tracks_anchor: Option<(usize, usize)>,
    /// The tracks panel's area drawn last frame.
    tracks_rect: Option<Rect>,
    /// The waveforms drawn last frame (`None`: the window bar, else a pane's strip).
    wave_rects: Vec<(Rect, Option<PaneId>)>,
    /// Scroll and zoom of each waveform.
    waves: HashMap<Option<PaneId>, WaveView>,
    /// Edges that resize something when dragged, drawn last frame.
    edges: Vec<(Rect, Edge)>,
    /// What the mouse is dragging.
    drag: Option<Drag>,
}

/// A waveform's view: where it starts when scrolled by hand (else it follows the playhead or
/// cursor), and the seconds it spans.
#[derive(Debug, Clone, PartialEq)]
struct WaveView {
    start: Option<f64>,
    span: f64,
    /// The cursor position and play state it last followed; when they change it follows again.
    anchor: Option<(usize, usize, bool)>,
    /// Where it started when last drawn.
    drawn: f64,
}

impl Default for WaveView {
    fn default() -> Self {
        Self {
            start: None,
            span: 16.0,
            anchor: None,
            drawn: 0.0,
        }
    }
}

/// An edge that resizes something.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Edge {
    /// The top of the tracks panel.
    Tracks,
    /// The top of a waveform strip or of the window's waveform bar.
    Wave,
}

/// A mouse drag in progress.
#[derive(Debug, Clone, PartialEq)]
enum Drag {
    /// A track's volume slider (track, left, right).
    Volume(usize, f64, f64),
    /// A border between panes.
    Divider(muse_app::layout::Divider),
    /// The tracks panel's top edge: where the drag began and the rows shown then.
    Tracks(f64, usize),
    /// A waveform's top edge: where the drag began and the height then.
    Wave(f64, u16),
}

/// The pointer shape over something draggable.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Pointer {
    Default,
    /// Drag left and right.
    ResizeColumns,
    /// Drag up and down.
    ResizeRows,
}

/// What a click on the tracks panel does.
#[derive(Debug, Clone, Copy, PartialEq)]
enum TracksHit {
    /// A track's volume slider (left and right ends).
    Volume(usize, f64, f64),
    Solo(usize),
    Mute(usize),
    /// A track's row outside the bar grid.
    Row(usize),
    /// A bar of a track.
    Bar(usize, usize),
}

/// What a click on the tabline does.
#[derive(Debug, Clone, Copy, PartialEq)]
enum TablineHit {
    /// Show the buffer of bufferline tab n (0-based).
    Buffer(usize),
    /// Close bufferline tab n.
    Close(usize),
    /// Switch to view n.
    View(usize),
}

const TRACK_HEAD: f64 = 20.0;
const TRACK_ROW: f64 = 20.0;
const BLOCK_W: f64 = 14.0;
const BLOCK_STEP: f64 = 16.0;
/// Where the tracks panel's columns start: number, volume, solo, mute, name, instrument, bars.
const COL_NUM: f64 = 10.0;
const COL_VOL: (f64, f64) = (36.0, 100.0);
const COL_SOLO: f64 = 110.0;
const COL_MUTE: f64 = 130.0;
const COL_NAME: f64 = 154.0;
const COL_INSTR: f64 = 290.0;
const COL_GRID: f64 = 430.0;

fn tracks_panel_height(app: &App) -> f64 {
    let n = app
        .buffer()
        .score
        .tracks
        .len()
        .clamp(1, app.tracks_rows.max(1));
    TRACK_HEAD + n as f64 * TRACK_ROW + 6.0
}

/// Shortens text to `max` characters with an ellipsis.
fn clip_text(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let mut t: String = s.chars().take(max.saturating_sub(1)).collect();
        t.push('…');
        t
    }
}

/// The tracks panel: a row per track (number, volume, solo, mute, name, instrument) and a
/// block per bar, coloured where the bar has notes, with the edit cursor and the playhead.
fn draw_tracks(
    scene: &mut Scene,
    app: &App,
    views: &mut Views,
    r: Rect,
    play: Option<(usize, Location, f64, f64)>,
) {
    let theme = app.config.theme.clone();
    fill(scene, r, col(theme.panel));
    let focused = app.mode == Mode::Tracks;
    if focused {
        fill(
            scene,
            Rect::new(r.x0, r.y0, r.x1, r.y0 + 2.0),
            col(theme.accent),
        );
    } else {
        fill(
            scene,
            Rect::new(r.x0, r.y0, r.x1, r.y0 + 1.0),
            col(theme.tab),
        );
    }
    let rows = app.track_rows();
    let buffer = app.pane().buffer;
    let score = &app.buffers[buffer].score;
    let cursor = app.pane().cursor;
    let visible = rows.len().clamp(1, app.tracks_rows.max(1));
    views.tracks_rect = Some(r);
    views
        .edges
        .push((Rect::new(r.x0, r.y0 - 3.0, r.x1, r.y0 + 3.0), Edge::Tracks));
    // Scroll so the cursor's track and bar are in view, when they move (the wheel scrolls
    // freely in between).
    let (mut first, mut hs) = views.tracks_scroll;
    let gx = r.x0 + COL_GRID;
    let gw = (r.x1 - 8.0 - gx).max(BLOCK_STEP);
    let playing_here = play.filter(|p| p.0 == buffer && (app.playing || app.paused));
    let follow_bar = match playing_here {
        Some((_, loc, ..)) if app.follow && app.playing => loc.bar,
        _ => cursor.bar,
    };
    if views.tracks_anchor != Some((cursor.track, follow_bar)) {
        views.tracks_anchor = Some((cursor.track, follow_bar));
        if cursor.track < first {
            first = cursor.track;
        } else if cursor.track >= first + visible {
            first = cursor.track + 1 - visible;
        }
        let bx = follow_bar as f64 * BLOCK_STEP;
        if bx < hs {
            hs = (bx - 4.0 * BLOCK_STEP).max(0.0);
        } else if bx + BLOCK_W > hs + gw {
            hs = bx + BLOCK_W - gw + 4.0 * BLOCK_STEP;
        }
    }
    first = first.min(rows.len().saturating_sub(visible));
    let bars_w = score.master_bars.len() as f64 * BLOCK_STEP;
    hs = hs.clamp(0.0, (bars_w - gw + BLOCK_STEP).max(0.0));
    views.tracks_scroll = (first, hs);

    // Header: column names and bar numbers.
    let hy = r.y0 + 14.0;
    let head = col(theme.dim);
    for (x, label) in [
        (COL_NUM, "#"),
        (COL_VOL.0, "Volume"),
        (COL_SOLO, "S"),
        (COL_MUTE, "M"),
        (COL_NAME, "Track"),
        (COL_INSTR, "Instrument"),
    ] {
        text(scene, label, r.x0 + x, hy, 11.0, Align::Left, head, false);
    }
    let bars = score.master_bars.len();
    let grid_clip = Rect::new(gx - 2.0, r.y0, gx + gw + 2.0, r.y1);
    scene.push_layer(Mix::Clip, 1.0, Affine::IDENTITY, &grid_clip);
    let first_bar = (hs / BLOCK_STEP) as usize;
    let last_bar = (((hs + gw) / BLOCK_STEP) as usize + 1).min(bars);
    for b in first_bar..last_bar {
        if b % 4 == 0 {
            let x = gx + b as f64 * BLOCK_STEP - hs;
            text(
                scene,
                &(b + 1).to_string(),
                x,
                hy,
                10.0,
                Align::Left,
                head,
                false,
            );
        }
    }
    let rows_y0 = r.y0 + TRACK_HEAD;
    let rows_y1 = rows_y0 + visible as f64 * TRACK_ROW;
    scene.pop_layer();

    let text_c = col(theme.chrome_text);
    for (k, row) in rows.iter().enumerate().skip(first).take(visible) {
        let y = rows_y0 + (k - first) as f64 * TRACK_ROW;
        let cy = y + TRACK_ROW / 2.0;
        let row_rect = Rect::new(r.x0, y, gx - 6.0, y + TRACK_ROW);
        if row.index == cursor.track {
            fill(
                scene,
                Rect::new(r.x0, y, r.x1, y + TRACK_ROW),
                col(theme.tab.with_alpha(160)),
            );
            fill(
                scene,
                Rect::new(r.x0, y + 2.0, r.x0 + 3.0, y + TRACK_ROW - 2.0),
                col(row.color),
            );
        }
        let fg = if row.audible { text_c } else { head };
        text(
            scene,
            &(row.index + 1).to_string(),
            r.x0 + COL_NUM,
            cy + 4.5,
            12.0,
            Align::Left,
            fg,
            false,
        );
        // Volume slider.
        let (v0, v1) = (r.x0 + COL_VOL.0, r.x0 + COL_VOL.1);
        let frac = row.volume as f64 / 127.0;
        let groove = RoundedRect::new(v0, cy - 2.0, v1, cy + 2.0, 2.0);
        scene.fill(
            Fill::NonZero,
            Affine::IDENTITY,
            col(theme.empty_bar),
            None,
            &groove,
        );
        let level = RoundedRect::new(v0, cy - 2.0, v0 + (v1 - v0) * frac, cy + 2.0, 2.0);
        scene.fill(
            Fill::NonZero,
            Affine::IDENTITY,
            col(row.color),
            None,
            &level,
        );
        scene.fill(
            Fill::NonZero,
            Affine::IDENTITY,
            text_c,
            None,
            &Circle::new((v0 + (v1 - v0) * frac, cy), 4.5),
        );
        // Solo and mute.
        for (x, on, label, on_color, hit) in [
            (
                COL_SOLO,
                row.solo,
                "S",
                theme.mode_insert,
                TracksHit::Solo(row.index),
            ),
            (
                COL_MUTE,
                row.mute,
                "M",
                theme.playhead,
                TracksHit::Mute(row.index),
            ),
        ] {
            let b = Rect::new(r.x0 + x, cy - 7.0, r.x0 + x + 15.0, cy + 7.0);
            let rr = b.to_rounded_rect(3.0);
            if on {
                scene.fill(Fill::NonZero, Affine::IDENTITY, col(on_color), None, &rr);
            } else {
                scene.stroke(&Stroke::new(1.0), Affine::IDENTITY, head, None, &rr);
            }
            let c = if on {
                col(theme.text_on(on_color))
            } else {
                head
            };
            text(
                scene,
                label,
                b.center().x,
                cy + 4.0,
                10.0,
                Align::Center,
                c,
                true,
            );
            views.tracks.push((b, hit));
        }
        text(
            scene,
            &clip_text(&row.name, 18),
            r.x0 + COL_NAME,
            cy + 4.5,
            12.0,
            Align::Left,
            fg,
            row.index == cursor.track,
        );
        text(
            scene,
            &clip_text(&row.instrument, 18),
            r.x0 + COL_INSTR,
            cy + 4.5,
            12.0,
            Align::Left,
            head,
            false,
        );
        views.tracks.push((
            Rect::new(v0 - 6.0, y, v1 + 6.0, y + TRACK_ROW),
            TracksHit::Volume(row.index, v0, v1),
        ));
        views.tracks.push((row_rect, TracksHit::Row(row.index)));
        // Bars.
        scene.push_layer(Mix::Clip, 1.0, Affine::IDENTITY, &grid_clip);
        for b in first_bar..last_bar {
            let x = gx + b as f64 * BLOCK_STEP - hs;
            let block = Rect::new(x, y + 3.0, x + BLOCK_W, y + TRACK_ROW - 3.0);
            let c = if row.bars.get(b).copied().unwrap_or(false) {
                row.color.with_alpha(if row.audible { 255 } else { 90 })
            } else {
                theme.empty_bar
            };
            scene.fill(
                Fill::NonZero,
                Affine::IDENTITY,
                col(c),
                None,
                &block.to_rounded_rect(2.0),
            );
            // The edit cursor: a black dot in its bar.
            if row.index == cursor.track && b == cursor.bar {
                scene.fill(
                    Fill::NonZero,
                    Affine::IDENTITY,
                    Color::BLACK,
                    None,
                    &Circle::new(block.center(), 3.0),
                );
            }
            views
                .tracks
                .push((block.intersect(grid_clip), TracksHit::Bar(row.index, b)));
        }
        scene.pop_layer();
    }
    // The playhead, across all rows.
    if let Some((_, loc, ..)) = playing_here {
        let frac = loc.tick as f64 / muse_app::tracks::bar_ticks(score, loc.bar) as f64;
        let x =
            gx + (loc.bar as f64 + frac.clamp(0.0, 1.0) * BLOCK_W / BLOCK_STEP) * BLOCK_STEP - hs;
        if x >= gx - 1.0 && x <= gx + gw + 1.0 {
            fill(
                scene,
                Rect::new(x - 1.0, rows_y0 - 2.0, x + 1.0, rows_y1 + 2.0),
                col(theme.playhead),
            );
        }
    }
}

/// How fast the cursor glides after the playhead (per second: it covers 1 − e^(−rate·t)).
const GLIDE_RATE: f64 = 16.0;

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
    THEME.with(|t| *t.borrow_mut() = app.config.theme.clone());
    // The window-wide waveform bar sits between the panes and the status line, the tracks
    // panel above it.
    let global_wave = app.global_waveform && height > 300.0;
    let wave_h = app.waveform_height as f64;
    let bottom = height - STATUS - CMDLINE - if global_wave { wave_h + 4.0 } else { 0.0 };
    let tracks_h = if app.tracks_shown() && height > 300.0 {
        tracks_panel_height(app)
    } else {
        0.0
    };
    let main = Rect::new(0.0, TAB_BAR, width, bottom - tracks_h);
    views.tracks.clear();
    views.edges.clear();
    views.wave_rects.clear();
    views.tracks_rect = None;
    if tracks_h > 0.0 {
        draw_tracks(
            scene,
            app,
            views,
            Rect::new(0.0, bottom - tracks_h, width, bottom),
            play,
        );
    }
    app.viewport = muse_app::layout::Rect {
        x: main.x0 as u16,
        y: main.y0 as u16,
        w: main.width() as u16,
        h: main.height() as u16,
    };
    draw_tabs(scene, app, views, width);
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
        let r = Rect::new(4.0, bottom + 2.0, width - 4.0, bottom + 2.0 + wave_h);
        views
            .edges
            .push((Rect::new(0.0, r.y0 - 3.0, width, r.y0 + 3.0), Edge::Wave));
        views.wave_rects.push((r, None));
        let buffer = app.pane().buffer;
        match playback {
            Some(pb) if app.buffers[buffer].score.audio.is_some() => {
                pb.ensure_audio(app, buffer);
                let view = views.waves.entry(None).or_default();
                draw_waveform(scene, app, pb, buffer, r, play, view);
            }
            _ => {
                fill(scene, r, tc(|t| t.panel));
                let name = &app.buffers[buffer].name;
                text(
                    scene,
                    &format!("{name}: no audio attached (:audio FILE)"),
                    r.x0 + 10.0,
                    r.center().y + 5.0,
                    13.0,
                    Align::Left,
                    dim(),
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
    if let Some(p) = app.palette() {
        draw_palette(scene, &p, width, height);
    }
    views.cache.retain(|id, _| app.panes.contains_key(id));
}

/// The tabline, as in bufferline.nvim: the active view's buffers as tabs on the left (file-type
/// icon, name shortened in the middle, × on hover or ● when modified), the views on the right.
fn draw_tabs(scene: &mut Scene, app: &App, views: &mut Views, width: f64) {
    let fonts = muse_engrave::fonts::fonts();
    fill(scene, Rect::new(0.0, 0.0, width, TAB_BAR), chrome());
    views.tabline.clear();
    let (hx, hy) = views.hover;
    let hovering = (0.0..TAB_BAR).contains(&hy);

    // Views, right-aligned.
    let labels = app.tabline_views();
    let mut right = width - 6.0;
    let mut chips = Vec::new();
    for (i, label) in labels.iter().enumerate().rev() {
        let w = fonts.text_width(FontKind::Bold, label, 12.0) + 16.0;
        let r = Rect::new(right - w, 6.0, right, TAB_BAR - 6.0);
        chips.push((i, label, r));
        right -= w + 4.0;
    }
    for (i, label, r) in chips {
        let active = i == app.active_view;
        let bg = if active {
            accent()
        } else if hovering && r.contains((hx, hy)) {
            tc(|t| t.tab_hover)
        } else {
            tc(|t| t.tab)
        };
        scene.fill(
            Fill::NonZero,
            Affine::IDENTITY,
            bg,
            None,
            &r.to_rounded_rect(5.0),
        );
        let color = if active {
            text_on(accent())
        } else {
            chrome_text()
        };
        text(
            scene,
            label,
            r.center().x,
            r.y1 - 5.0,
            12.0,
            Align::Center,
            color,
            true,
        );
        views.tabline.push((r, TablineHit::View(i)));
    }
    let views_x = right - 6.0;

    // Buffer tabs, clipped before the views.
    let t = &app.config.file.tabline.truncation;
    let clip = Rect::new(0.0, 0.0, views_x.max(0.0), TAB_BAR);
    scene.push_layer(Mix::Clip, 1.0, Affine::IDENTITY, &clip);
    let mut x = 6.0;
    for (n, tab) in app.bufferline().into_iter().enumerate() {
        let name =
            muse_app::truncate_middle(&tab.name, t.max_width, t.prefix_width, t.suffix_width);
        // The active tab's name is bold, and wider.
        let font = if tab.active {
            FontKind::Bold
        } else {
            FontKind::Text
        };
        let name_w = fonts.text_width(font, &name, UI_FONT);
        // Room for the play indicator after the name, on the file playing audio.
        let w = 10.0 + 14.0 + 7.0 + name_w + 10.0 + 16.0 + 6.0;
        let r = Rect::new(x, 4.0, x + w, TAB_BAR);
        let over = hovering && r.contains((hx, hy));
        let bg = if tab.active {
            background()
        } else if over {
            tc(|t| t.tab_hover)
        } else {
            tc(|t| t.tab)
        };
        scene.fill(
            Fill::NonZero,
            Affine::IDENTITY,
            bg,
            None,
            &RoundedRect::new(r.x0, r.y0, r.x1, r.y1 + 6.0, 6.0),
        );
        if tab.active {
            fill(
                scene,
                Rect::new(r.x0 + 4.0, r.y0, r.x1 - 4.0, r.y0 + 2.0),
                accent(),
            );
        }
        let cy = (r.y0 + r.y1) / 2.0 + 1.0;
        // The file playing shows ▶ (or ⏸) where its file-type icon goes.
        if tab.playing || tab.paused {
            let px = r.x0 + 12.0;
            let red = tc(|t| t.playhead);
            if tab.playing {
                let mut p = BezPath::new();
                p.move_to((px, cy - 6.0));
                p.line_to((px + 10.0, cy));
                p.line_to((px, cy + 6.0));
                p.close_path();
                scene.fill(Fill::NonZero, Affine::IDENTITY, red, None, &p);
            } else {
                fill(scene, Rect::new(px, cy - 6.0, px + 3.5, cy + 6.0), red);
                fill(
                    scene,
                    Rect::new(px + 6.5, cy - 6.0, px + 10.0, cy + 6.0),
                    red,
                );
            }
        } else {
            draw_file_icon(scene, tab.kind, r.x0 + 10.0, cy);
        }
        let fg = if tab.active {
            text_on(background())
        } else {
            chrome_text()
        };
        text(
            scene,
            &name,
            r.x0 + 31.0,
            cy + 5.0,
            UI_FONT,
            Align::Left,
            fg,
            tab.active,
        );
        // Close button on hover; otherwise a dot for unsaved changes.
        let bx = r.x1 - 6.0 - 8.0;
        let close = Rect::new(bx - 8.0, cy - 8.0, bx + 8.0, cy + 8.0);
        if over {
            if close.contains((hx, hy)) {
                let c = Color::from_rgba8(220, 70, 60, if tab.active { 70 } else { 120 });
                scene.fill(
                    Fill::NonZero,
                    Affine::IDENTITY,
                    c,
                    None,
                    &Circle::new(close.center(), 8.0),
                );
            }
            let mut p = BezPath::new();
            p.move_to((bx - 3.5, cy - 3.5));
            p.line_to((bx + 3.5, cy + 3.5));
            p.move_to((bx + 3.5, cy - 3.5));
            p.line_to((bx - 3.5, cy + 3.5));
            scene.stroke(&Stroke::new(1.6), Affine::IDENTITY, fg, None, &p);
        } else if tab.modified {
            scene.fill(
                Fill::NonZero,
                Affine::IDENTITY,
                accent(),
                None,
                &Circle::new((bx, cy), 3.5),
            );
        }
        views.tabline.push((r, TablineHit::Buffer(n)));
        views.tabline.push((close, TablineHit::Close(n)));
        x += w + 3.0;
    }
    scene.pop_layer();
}

/// A small file-type icon centred vertically at `cy`, starting at `x` (14 px wide).
fn draw_file_icon(scene: &mut Scene, kind: &str, x: f64, cy: f64) {
    let color = match kind {
        "guitar-pro" => Color::from_rgba8(240, 140, 50, 255),
        "musicxml" => Color::from_rgba8(80, 150, 230, 255),
        "midi" => Color::from_rgba8(90, 190, 110, 255),
        "chant" => Color::from_rgba8(180, 110, 210, 255),
        "muse" => Color::from_rgba8(60, 190, 190, 255),
        _ => Color::from_rgba8(150, 152, 160, 255),
    };
    fn f(s: &mut Scene, color: Color, shape: &impl vello::kurbo::Shape) {
        s.fill(Fill::NonZero, Affine::IDENTITY, color, None, shape);
    }
    match kind {
        // A guitar pick.
        "guitar-pro" | "muse" => {
            let mut p = BezPath::new();
            p.move_to((x + 7.0, cy + 7.0));
            p.curve_to(
                (x + 1.0, cy + 2.0),
                (x - 1.0, cy - 6.0),
                (x + 7.0, cy - 6.5),
            );
            p.curve_to(
                (x + 15.0, cy - 6.0),
                (x + 13.0, cy + 2.0),
                (x + 7.0, cy + 7.0),
            );
            p.close_path();
            f(scene, color, &p);
            if kind == "muse" {
                scene.fill(
                    Fill::NonZero,
                    Affine::IDENTITY,
                    chrome(),
                    None,
                    &Circle::new((x + 7.0, cy - 1.5), 2.2),
                );
            }
        }
        // An eighth note.
        "musicxml" => {
            f(
                scene,
                color,
                &vello::kurbo::Ellipse::new((x + 5.0, cy + 4.0), (3.6, 2.6), -0.4),
            );
            f(
                scene,
                color,
                &Rect::new(x + 7.6, cy - 7.0, x + 8.9, cy + 4.0),
            );
            let mut flag = BezPath::new();
            flag.move_to((x + 8.9, cy - 7.0));
            flag.curve_to(
                (x + 10.0, cy - 3.0),
                (x + 14.0, cy - 2.5),
                (x + 12.0, cy + 1.5),
            );
            flag.curve_to(
                (x + 12.5, cy - 1.5),
                (x + 10.5, cy - 3.0),
                (x + 8.9, cy - 3.5),
            );
            flag.close_path();
            f(scene, color, &flag);
        }
        // Piano keys.
        "midi" => {
            f(
                scene,
                color,
                &RoundedRect::new(x, cy - 6.0, x + 14.0, cy + 6.0, 1.5),
            );
            for k in [3.5, 7.0, 10.5] {
                scene.fill(
                    Fill::NonZero,
                    Affine::IDENTITY,
                    chrome(),
                    None,
                    &Rect::new(x + k - 0.4, cy - 6.0, x + k + 0.4, cy + 6.0),
                );
            }
            for k in [3.5, 10.5] {
                scene.fill(
                    Fill::NonZero,
                    Affine::IDENTITY,
                    chrome(),
                    None,
                    &Rect::new(x + k - 1.5, cy - 6.0, x + k + 1.5, cy + 0.5),
                );
            }
        }
        // Chant: a punctum and a virga on a staff line.
        "chant" => {
            f(scene, color, &Rect::new(x, cy + 2.0, x + 14.0, cy + 2.8));
            f(
                scene,
                color,
                &Rect::new(x + 1.0, cy - 1.0, x + 5.5, cy + 3.0),
            );
            f(
                scene,
                color,
                &Rect::new(x + 8.0, cy - 6.0, x + 12.5, cy - 2.0),
            );
            f(
                scene,
                color,
                &Rect::new(x + 11.6, cy - 6.0, x + 12.5, cy + 6.0),
            );
        }
        // A page.
        _ => {
            let mut p = BezPath::new();
            p.move_to((x + 2.0, cy - 7.0));
            p.line_to((x + 9.0, cy - 7.0));
            p.line_to((x + 12.5, cy - 3.5));
            p.line_to((x + 12.5, cy + 7.0));
            p.line_to((x + 2.0, cy + 7.0));
            p.close_path();
            scene.stroke(&Stroke::new(1.4), Affine::IDENTITY, color, None, &p);
        }
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
    let border = if focused && app.mode != Mode::Tracks {
        accent()
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
        dim(),
        false,
    );
    let mut content = Rect::new(rect.x0 + 2.0, rect.y0 + TITLE, rect.x1 - 2.0, rect.y1 - 2.0);
    let waveform =
        (pane.waveform && score.audio.is_some() && content.height() > 200.0).then(|| {
            let wave_h = app.waveform_height as f64;
            let strip = Rect::new(content.x0, content.y1 - wave_h, content.x1, content.y1);
            content.y1 -= wave_h + 2.0;
            strip
        });
    if score.tracks.is_empty() {
        text(
            scene,
            "No score loaded — open one with :open <file>",
            content.center().x,
            content.center().y,
            16.0,
            Align::Center,
            dim(),
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
        fill(scene, content, background());
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
                    tc(|t| t.selection.with_alpha(55)),
                    None,
                    &rect,
                );
            }
        }
    }
    // Cursor.
    if let Some(pos) = cached.layout.beats.get(&cursor_ref) {
        // Following playback, it slides forward along a system towards its beat (an easing that
        // settles well within a beat); moves back or to another system jump.
        let now = std::time::Instant::now();
        let gliding = app.playing && app.follow && playhead.is_some();
        let (x, w) = match views.glide.get(&id) {
            Some(&(page, top, gx, gw, t))
                if gliding && page == pos.page && top == pos.top && pos.x >= gx - 0.5 =>
            {
                let k = 1.0 - (-(now - t).as_secs_f64() * GLIDE_RATE).exp();
                (gx + (pos.x - gx) * k, gw + (pos.width - gw) * k)
            }
            _ => (pos.x, pos.width),
        };
        views.glide.insert(id, (pos.page, pos.top, x, w, now));
        let alpha = if focused { 60 } else { 30 };
        let r = Rect::new(x, pos.top, x + w, pos.bottom);
        scene.fill(
            Fill::NonZero,
            to_screen(pos.page),
            tc(|t| t.cursor.with_alpha(alpha)),
            None,
            &r,
        );
        if let Some(&y) = pos.strings.get(pane.cursor.string) {
            let r = RoundedRect::new(x, y - 4.5, x + w.min(16.0), y + 4.5, 2.0);
            scene.stroke(
                &Stroke::new(1.2 / scale),
                to_screen(pos.page),
                tc(|t| t.cursor_outline),
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
            tc(|t| t.playhead.with_alpha(230)),
            None,
            &Rect::new(x - w / 2.0, top, x + w / 2.0, bottom),
        );
    }
    scene.pop_layer();
    if let (Some(strip), Some(pb)) = (waveform, playback) {
        views.edges.push((
            Rect::new(strip.x0, strip.y0 - 3.0, strip.x1, strip.y0 + 3.0),
            Edge::Wave,
        ));
        views.wave_rects.push((strip, Some(id)));
        let view = views.waves.entry(Some(id)).or_default();
        draw_waveform(scene, app, pb, pane.buffer, strip, play, view);
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
    view: &mut WaveView,
) {
    fill(scene, r, tc(|t| t.panel));
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
            dim(),
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
    // Follow the playhead (or cursor) unless scrolled by hand since they last moved.
    let anchor = (cursor.bar, cursor.beat, app.playing);
    if view.anchor != Some(anchor) {
        view.anchor = Some(anchor);
        view.start = None;
    }
    let span = view.span;
    let t0 = view.start.unwrap_or(now - span * 0.35);
    view.drawn = t0;
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
                fill(scene, Rect::new(x, r.y0, x + 1.5, r.y1), accent());
                text(
                    scene,
                    &format!("{}", p.bar + 1),
                    x + 3.0,
                    r.y0 + 12.0,
                    11.0,
                    Align::Left,
                    accent(),
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
        chrome_text(),
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

/// Rows of candidates the palette shows at once.
const PALETTE_ROWS: usize = 12;

/// The command palette: a rounded box near the top centre with the typed command and, below
/// it, the completions of the word being typed (descriptions right-aligned for commands).
fn draw_palette(scene: &mut Scene, p: &muse_app::Palette, width: f64, height: f64) {
    let fonts = muse_engrave::fonts::fonts();
    let w = (width - 40.0).clamp(200.0, 680.0);
    let x0 = (width - w) / 2.0;
    let y0 = TAB_BAR + 36.0;
    let input_h = 38.0;
    let row_h = 26.0;
    let room = ((height - y0 - input_h - 60.0) / row_h).max(0.0) as usize;
    let rows = p.items.len().min(PALETTE_ROWS).min(room);
    let h = input_h
        + if rows > 0 {
            rows as f64 * row_h + 10.0
        } else {
            0.0
        };
    // Shadow, frame, input.
    scene.fill(
        Fill::NonZero,
        Affine::IDENTITY,
        Color::from_rgba8(0, 0, 0, 70),
        None,
        &RoundedRect::new(x0 - 2.0, y0 + 2.0, x0 + w + 2.0, y0 + h + 6.0, 12.0),
    );
    let frame = RoundedRect::new(x0, y0, x0 + w, y0 + h, 10.0);
    scene.fill(Fill::NonZero, Affine::IDENTITY, chrome(), None, &frame);
    scene.stroke(&Stroke::new(1.0), Affine::IDENTITY, accent(), None, &frame);
    let line = if p.prompt == ":" {
        format!(":{}", p.input)
    } else {
        format!("{} {}", p.prompt, p.input)
    };
    let ty = y0 + 25.0;
    text(
        scene,
        &line,
        x0 + 14.0,
        ty,
        16.0,
        Align::Left,
        Color::WHITE,
        false,
    );
    let cx = x0 + 14.0 + fonts.text_width(FontKind::Text, &line, 16.0) + 1.0;
    fill(
        scene,
        Rect::new(cx, ty - 15.0, cx + 2.0, ty + 4.0),
        accent(),
    );
    if rows == 0 {
        return;
    }
    fill(
        scene,
        Rect::new(x0 + 8.0, y0 + input_h - 1.0, x0 + w - 8.0, y0 + input_h),
        Color::from_rgba8(70, 72, 80, 255),
    );
    // Scroll so the selected candidate is visible.
    let first = match p.selected {
        Some(sel) if sel >= rows => sel + 1 - rows,
        _ => 0,
    };
    for (k, item) in p.items.iter().enumerate().skip(first).take(rows) {
        let ry = y0 + input_h + 5.0 + (k - first) as f64 * row_h;
        let selected = p.selected == Some(k);
        if selected {
            scene.fill(
                Fill::NonZero,
                Affine::IDENTITY,
                tc(|t| t.accent.with_alpha(60)),
                None,
                &RoundedRect::new(x0 + 6.0, ry, x0 + w - 6.0, ry + row_h - 2.0, 6.0),
            );
        }
        let base = ry + row_h / 2.0 + 4.5;
        let color = if selected { accent() } else { chrome_text() };
        text(
            scene,
            &item.text,
            x0 + 16.0,
            base,
            14.0,
            Align::Left,
            color,
            selected,
        );
        // The characters that matched what was typed, over the text in a stronger colour.
        let font = if selected {
            FontKind::Bold
        } else {
            FontKind::Text
        };
        let hl = if selected { Color::WHITE } else { accent() };
        let chars: Vec<char> = item.text.chars().collect();
        for &i in &item.matched {
            let Some(c) = chars.get(i) else { continue };
            let before: String = chars[..i].iter().collect();
            let cx = x0 + 16.0 + fonts.text_width(font, &before, 14.0);
            text(
                scene,
                &c.to_string(),
                cx,
                base,
                14.0,
                Align::Left,
                hl,
                selected,
            );
        }
        if let Some(d) = &item.description {
            text(
                scene,
                d,
                x0 + w - 16.0,
                base,
                13.0,
                Align::Right,
                dim(),
                false,
            );
        }
    }
    if p.items.len() > 1 {
        let more = match p.selected {
            Some(i) => format!("{} / {}", i + 1, p.items.len()),
            None => format!("{} matches", p.items.len()),
        };
        text(
            scene,
            &more,
            x0 + w - 12.0,
            y0 + 24.0,
            11.0,
            Align::Right,
            dim(),
            false,
        );
    }
}

/// The command line while typing a command, otherwise the last message.
fn draw_command_line(scene: &mut Scene, app: &App, width: f64, height: f64) {
    // Command line / message.
    let cy = height - 8.0;
    match app.mode {
        // Typed in the command palette.
        Mode::Command | Mode::Picker => {}
        Mode::Normal | Mode::Insert | Mode::Visual | Mode::Tracks => {
            if let Some(m) = &app.message {
                let c = if m.error {
                    Color::from_rgba8(255, 110, 100, 255)
                } else {
                    chrome_text()
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
                accent(),
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
    fill(scene, Rect::new(0.0, y0, width, height), chrome());
    let (label, color) = match app.mode {
        Mode::Normal => (" NORMAL ", tc(|t| t.mode_normal)),
        Mode::Command => (" COMMAND ", tc(|t| t.mode_command)),
        Mode::Insert => (" INSERT ", tc(|t| t.mode_insert)),
        Mode::Visual => (" VISUAL ", tc(|t| t.mode_visual)),
        Mode::Picker => (" PICK ", tc(|t| t.mode_picker)),
        Mode::Tracks => (" TRACKS ", tc(|t| t.mode_tracks)),
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
        text_on(color),
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
        chrome_text(),
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
    if let Some(ws) = app.workspace_name() {
        right.push_str(&format!("   ws:{ws}"));
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
        chrome_text(),
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
        tc(|t| t.panel.with_alpha(240)),
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
        accent(),
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
            accent(),
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
            chrome_text(),
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
        text(scene, k, x + 110.0, y, 13.0, Align::Right, accent(), false);
        text(
            scene,
            c,
            x + 122.0,
            y,
            13.0,
            Align::Left,
            chrome_text(),
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

    /// Middle-click: closes the buffer tab under the mouse.
    pub fn middle_click(&mut self, app: &mut App, x: f64, y: f64) {
        let hit = self
            .tabline
            .iter()
            .find(|(r, _)| r.contains((x, y)))
            .map(|(_, h)| *h);
        if let Some(TablineHit::Buffer(n) | TablineHit::Close(n)) = hit {
            app.run(muse_app::Command::BufferClose(Some(n + 1), false));
        }
    }

    /// Whether a window position is over the tabline (hovering there needs redraws).
    pub fn over_tabline(&self, y: f64) -> bool {
        y < TAB_BAR
    }

    /// Mouse down over the tracks panel: select, solo, mute, or start dragging a volume.
    /// True when the panel took the click.
    fn click_tracks(&mut self, app: &mut App, x: f64, y: f64) -> bool {
        let Some(&(_, hit)) = self.tracks.iter().find(|(r, _)| r.contains((x, y))) else {
            return false;
        };
        match hit {
            TracksHit::Volume(t, x0, x1) => {
                app.select_track_bar(t, None);
                app.group_edits(true);
                self.drag = Some(Drag::Volume(t, x0, x1));
                self.drag_to(app, x, y);
            }
            TracksHit::Solo(t) => app.run(muse_app::Command::Solo(Some(t + 1))),
            TracksHit::Mute(t) => app.run(muse_app::Command::Mute(Some(t + 1))),
            TracksHit::Row(t) => app.select_track_bar(t, None),
            TracksHit::Bar(t, b) => app.select_track_bar(t, Some(b)),
        }
        true
    }

    /// Mouse moved with the button down: drags a slider, a pane border or a panel's edge.
    pub fn drag_to(&mut self, app: &mut App, x: f64, y: f64) -> bool {
        match self.drag.clone() {
            Some(Drag::Volume(t, x0, x1)) => {
                let v = ((x - x0) / (x1 - x0)).clamp(0.0, 1.0) * 127.0;
                app.set_track_volume(t, v.round() as u8);
            }
            Some(Drag::Divider(d)) => {
                let pos = match d.axis {
                    muse_app::layout::Axis::SideBySide => x,
                    muse_app::layout::Axis::Stacked => y,
                };
                app.drag_divider(&d, pos.max(0.0).round() as u16);
            }
            Some(Drag::Tracks(y0, rows0)) => {
                let rows = rows0 as f64 + ((y0 - y) / TRACK_ROW).round();
                app.run(muse_app::Command::TracksResize(
                    muse_app::command::Amount::To(rows.max(1.0) as u32),
                ));
            }
            Some(Drag::Wave(y0, h0)) => {
                let h = h0 as f64 + (y0 - y);
                app.run(muse_app::Command::WaveformResize(
                    muse_app::command::Amount::To(h.max(0.0).round() as u32),
                ));
            }
            None => return false,
        }
        true
    }

    /// Mouse up: ends a drag (a volume drag is one undo step).
    pub fn release(&mut self, app: &mut App) {
        if let Some(Drag::Volume(..)) = self.drag.take() {
            app.group_edits(false);
        }
    }

    /// A pane border near a window position.
    fn divider_at(&self, app: &App, x: f64, y: f64) -> Option<muse_app::layout::Divider> {
        app.dividers().into_iter().find(|d| {
            let a = d.area;
            let pos = d.pos as f64;
            match d.axis {
                muse_app::layout::Axis::SideBySide => {
                    (x - pos).abs() <= 4.0 && y >= a.y as f64 && y <= (a.y + a.h) as f64
                }
                muse_app::layout::Axis::Stacked => {
                    (y - pos).abs() <= 4.0 && x >= a.x as f64 && x <= (a.x + a.w) as f64
                }
            }
        })
    }

    /// The pointer shape at a window position (resize arrows over draggable edges).
    pub fn pointer(&self, app: &App, x: f64, y: f64) -> Pointer {
        if let Some(Drag::Divider(d)) = &self.drag {
            return match d.axis {
                muse_app::layout::Axis::SideBySide => Pointer::ResizeColumns,
                muse_app::layout::Axis::Stacked => Pointer::ResizeRows,
            };
        }
        if matches!(self.drag, Some(Drag::Tracks(..) | Drag::Wave(..))) {
            return Pointer::ResizeRows;
        }
        if self.edges.iter().any(|(r, _)| r.contains((x, y))) {
            return Pointer::ResizeRows;
        }
        match self.divider_at(app, x, y).map(|d| d.axis) {
            Some(muse_app::layout::Axis::SideBySide) => Pointer::ResizeColumns,
            Some(muse_app::layout::Axis::Stacked) => Pointer::ResizeRows,
            None => Pointer::Default,
        }
    }

    /// Mouse wheel: scrolls the tracks panel or a waveform under the mouse (`dx`, `dy` in
    /// pixels the view moves right and down; `zoom` for Ctrl, which zooms a waveform). False
    /// when nothing there takes it.
    pub fn wheel(&mut self, app: &App, x: f64, y: f64, dx: f64, dy: f64, zoom: bool) -> bool {
        if let Some(&(r, key)) = self.wave_rects.iter().find(|(r, _)| r.contains((x, y))) {
            let view = self.waves.entry(key).or_default();
            if zoom {
                view.span = (view.span * if dy > 0.0 { 1.25 } else { 0.8 }).clamp(1.0, 900.0);
            } else {
                // Both wheels move along the timeline.
                let shift = (dx + dy) / r.width().max(1.0) * view.span;
                view.start = Some(view.start.unwrap_or(view.drawn) + shift);
            }
            return true;
        }
        if zoom {
            return false;
        }
        if self.tracks_rect.is_some_and(|r| r.contains((x, y))) {
            let n = app.buffer().score.tracks.len();
            let visible = n.clamp(1, app.tracks_rows.max(1));
            let (first, hs) = self.tracks_scroll;
            let rows = (dy / TRACK_ROW).round() as i64;
            let rows = if rows == 0 && dy != 0.0 {
                dy.signum() as i64
            } else {
                rows
            };
            let first = (first as i64 + rows).clamp(0, n.saturating_sub(visible) as i64) as usize;
            self.tracks_scroll = (first, (hs + dx).max(0.0));
            return true;
        }
        false
    }

    pub fn click(&mut self, app: &mut App, x: f64, y: f64) {
        if self.click_tracks(app, x, y) {
            return;
        }
        // Edges and pane borders start a resize.
        if let Some(&(_, edge)) = self.edges.iter().find(|(r, _)| r.contains((x, y))) {
            self.drag = Some(match edge {
                Edge::Tracks => Drag::Tracks(
                    y,
                    app.buffer()
                        .score
                        .tracks
                        .len()
                        .clamp(1, app.tracks_rows.max(1)),
                ),
                Edge::Wave => Drag::Wave(y, app.waveform_height),
            });
            return;
        }
        if let Some(d) = self.divider_at(app, x, y) {
            self.drag = Some(Drag::Divider(d));
            return;
        }
        if let Some(&(_, hit)) = self.tabline.iter().rev().find(|(r, _)| r.contains((x, y))) {
            app.run(match hit {
                TablineHit::Buffer(n) => muse_app::Command::BufferGoto(n + 1),
                TablineHit::Close(n) => muse_app::Command::BufferClose(Some(n + 1), false),
                TablineHit::View(n) => muse_app::Command::ViewGoto(n + 1),
            });
            return;
        }
        let Some((g, px, py)) = self.locate(x, y) else {
            return;
        };
        app.views[app.active_view].focus = g.pane;
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

    pub fn scroll(&mut self, app: &App, x: f64, y: f64, dx: f64, dy: f64) {
        let Some((g, _, _)) = self.locate(x, y) else {
            return;
        };
        let horizontal = app
            .panes
            .get(&g.pane)
            .is_some_and(|p| p.layout == "horizontal");
        let entry = self.scroll.entry(g.pane).or_insert((0.0, 0.0));
        // The vertical wheel scrolls along the score's flow; the horizontal one sideways.
        if horizontal {
            entry.0 = (entry.0 + (dy + dx) / g.scale).max(0.0);
        } else {
            entry.0 = (entry.0 + dx / g.scale).max(0.0);
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
