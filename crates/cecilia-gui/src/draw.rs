//! Builds the vello scene for a frame: chrome, panes with engraved notation, cursor and
//! playback highlights. Engraved pages are converted to scenes once and cached per pane.

use std::collections::HashMap;

use cecilia_app::layout::PaneId;
use cecilia_app::playback::Playback;
use cecilia_app::theme::Rgba;
use cecilia_app::{App, Mode, PaneKind, Strip};
use cecilia_audio::Location;
use cecilia_engrave::display::{Align, BeatRef, FontKind};
use cecilia_engrave::output::{Paint, outlines};
use cecilia_engrave::{Layout, Options};
use vello::Scene;
use vello::kurbo::{Affine, BezPath, Circle, Rect, RoundedRect, Shape, Stroke};
use vello::peniko::{Color, Fill, Mix};

thread_local! {
    /// The theme of the frame being built (set by [`build`]).
    static THEME: std::cell::RefCell<cecilia_app::theme::Theme> = Default::default();
}

/// A color of the current theme.
fn tc(f: impl FnOnce(&cecilia_app::theme::Theme) -> Rgba) -> Color {
    THEME.with(|t| col(f(&t.borrow())))
}

fn col(c: Rgba) -> Color {
    Color::from_rgba8(c.r, c.g, c.b, c.a)
}

/// Behind the score (the theme's paper).
/// The window's background, behind the panes and around the scores' pages.
pub fn background() -> Color {
    tc(|t| t.chrome)
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

fn popover() -> Color {
    tc(|t| t.popover)
}

fn dim() -> Color {
    tc(|t| t.dim)
}

const TAB_BAR: f64 = 30.0;
const TOOLBAR: f64 = 34.0;
/// How long the mouse rests on a toolbar button before its tooltip shows.
const TOOLTIP_DELAY: std::time::Duration = std::time::Duration::from_millis(500);
const STATUS: f64 = 26.0;
const CMDLINE: f64 = 26.0;
const TITLE: f64 = 24.0;
const UI_FONT: f64 = 14.0;
/// Pixels per point at zoom 1.
const BASE_SCALE: f64 = 1.6;

/// A linked note's color-recolored pages: the note, its color, and (page, scene) pairs.
type Lit = (cecilia_app::LinkedNote, [u8; 4], Vec<(usize, Scene)>);

type CacheKey = (
    usize,
    usize,
    String,
    i64,
    u64,
    bool,
    [u8; 8],
    &'static str,
    bool,
    // The incomplete-bar color, while highlighted.
    Option<[u8; 3]>,
);

struct Cached {
    /// (buffer, track, layout mode, width bucket, score version, chant view, page + ink colors,
    /// chord policy)
    key: CacheKey,
    layout: Layout,
    scenes: Vec<Scene>,
    /// The note shown in the mode's color on the track's other staves, its color, and the pages
    /// redrawn for it (replacing `scenes`'): recolored when the cursor or mode changes, without
    /// engraving again.
    lit: Option<Lit>,
    /// Y offset (points) of each page in the pane's scroll space.
    offsets: Vec<f64>,
}

#[derive(Clone, Copy)]
struct Geometry {
    pane: PaneId,
    content: Rect,
    scale: f64,
    scroll: (f64, f64),
    /// The vertical and horizontal scrollbars, where the score scrolls that way.
    bars: [Option<ScrollBar>; 2],
}

/// A pane's scrollbar: its track and thumb, and how far the view can scroll (points).
#[derive(Clone, Copy)]
struct ScrollBar {
    track: Rect,
    thumb: Rect,
    horizontal: bool,
    range: f64,
}

/// Thickness of the scrollbar's gutter, which the score never draws into.
const SCROLLBAR: f64 = 10.0;

/// Per-pane view state kept between frames.
#[derive(Default)]
pub struct Views {
    cache: HashMap<PaneId, Cached>,
    scroll: HashMap<PaneId, (f64, f64)>,
    geometry: Vec<Geometry>,
    /// What each score pane showed this frame, for the app's half-page movement.
    view_infos: Vec<(PaneId, Option<cecilia_app::ViewInfo>)>,
    /// Panes whose `zz` / `zt` / `zb` request was applied this frame.
    scroll_taken: Vec<PaneId>,
    last_cursor: HashMap<PaneId, cecilia_app::Cursor>,
    /// The drawn cursor per pane (page, top, x, width) and when: while following playback it
    /// glides after the playhead instead of jumping from beat to beat.
    glide: HashMap<PaneId, (usize, f64, f64, f64, std::time::Instant)>,
    /// Mouse position (logical), for hover effects on the tabline.
    pub hover: (f64, f64),
    /// Clickable areas of the tabline drawn last frame.
    tabline: Vec<(Rect, TablineHit)>,
    /// Fretboard cells (string, frets from the nut) and piano keys (pitch) drawn last frame;
    /// black keys come first, as they lie over the white ones.
    frets: Vec<(Rect, usize, u8)>,
    keys: Vec<(Rect, u8)>,
    /// The dashboard's buttons drawn last frame (area, index into `DashboardButton::ALL`).
    dashboard: Vec<(Rect, usize)>,
    /// The status line's clickable sections drawn last frame, and what they run.
    status: Vec<(Rect, cecilia_app::Command)>,
    /// The toolbar's buttons drawn last frame (area, index into `App::toolbar_items`).
    toolbar: Vec<(Rect, usize)>,
    /// The button under the mouse and since when (its tooltip shows after a moment).
    toolbar_hover: Option<(usize, std::time::Instant)>,
    /// The editing palette's button under the mouse and since when (its name shows after a
    /// moment).
    palette_hover: Option<(usize, std::time::Instant)>,
    /// The tooltip to draw this frame (the button's area and text).
    tooltip: Option<(Rect, String)>,
    /// Where the tabline and toolbar end.
    chrome_bottom: f64,
    /// Clickable areas of the tracks panel drawn last frame.
    tracks: Vec<(Rect, TracksHit)>,
    /// The tracks panel's first visible row and horizontal scroll of its bar grid.
    tracks_scroll: (usize, f64),
    /// What the tracks panel last scrolled to follow (cursor track, bar): it scrolls again
    /// only when that moves, so the mouse wheel can look elsewhere meanwhile.
    tracks_anchor: Option<(usize, usize)>,
    /// The tracks panel's area drawn last frame.
    tracks_rect: Option<Rect>,
    /// The properties pane, for the wheel.
    props_rect: Option<Rect>,
    /// The lyrics text area as drawn, to put the caret where a click lands.
    props_text: Option<PropsText>,
    /// Clickable areas of the properties pane drawn last frame.
    props: Vec<(Rect, PropHit)>,
    /// The editing palette's rows drawn last frame, and its area for the wheel.
    palette: Vec<(Rect, usize)>,
    palette_rect: Option<Rect>,
    /// Tool panes on screen: clicking one focuses it.
    tool_rects: Vec<(Rect, PaneId)>,
    /// Score panes' progress bars: clicking one moves the cursor to that bar.
    progress: Vec<(Rect, PaneId)>,
    /// The last click: when and where, to tell a double click.
    last_click: Option<(std::time::Instant, f64, f64)>,
    /// The waveforms drawn last frame (`None`: the window bar, else a pane's strip).
    wave_rects: Vec<(Rect, Option<PaneId>)>,
    /// Scroll and zoom of each waveform.
    waves: HashMap<Option<PaneId>, WaveView>,
    /// Edges that resize something when dragged, drawn last frame.
    edges: Vec<(Rect, Edge)>,
    /// What the mouse is dragging.
    drag: Option<Drag>,
    /// Shift and Ctrl/Alt held, as the next press sees them.
    pub select_mods: (bool, bool),
    /// When a selection drag last scrolled the view at an edge.
    select_tick: Option<std::time::Instant>,
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
    /// The top of a pane's waveform strip.
    Wave,
}

/// A mouse drag in progress.
#[derive(Debug, Clone, PartialEq)]
enum Drag {
    /// A selection in the lyrics text area.
    Text,
    /// A staff row of the properties pane (where it began, and the row under the mouse now).
    Prop(usize, usize, bool),
    /// A row's volume slider (row, left, right).
    Volume(Strip, f64, f64),
    /// A border between panes.
    Divider(cecilia_app::layout::Divider),
    /// A waveform's top edge: where the drag began and the height then.
    Wave(f64, u16),
    /// A buffer tab: its position now, and where the press was (it moves past a few pixels).
    Tab(usize, f64),
    /// A pane's scrollbar thumb (horizontal or vertical), held this far (pixels) from its start.
    Scrollbar(PaneId, bool, f64),
    /// A design-mode handle of a pane's page layout.
    Design(PaneId, Box<crate::design::Grab>),
    /// A press on a beat of a score pane, which may become a Visual selection (the app holds
    /// the state; see `App::mouse_press`).
    Select(PaneId),
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

/// The lyrics text area as drawn.
struct PropsText {
    area: Rect,
    /// Where the first line's text starts (x) and its top (y), and the line height.
    x: f64,
    top: f64,
    line_h: f64,
    size: f64,
    skip: usize,
    lines: Vec<(usize, usize)>,
    chars: Vec<char>,
}

impl PropsText {
    /// The character index at a point.
    fn at(&self, x: f64, y: f64) -> usize {
        let fonts = cecilia_engrave::fonts::fonts();
        let row = (((y - self.top) / self.line_h).floor().max(0.0) as usize + self.skip)
            .min(self.lines.len().saturating_sub(1));
        let Some(&(s, e)) = self.lines.get(row) else {
            return 0;
        };
        let w = |i: usize| {
            let t: String = self.chars[s..i].iter().collect();
            fonts.text_width(FontKind::Text, &t, self.size)
        };
        (s..e)
            .find(|&i| x < self.x + (w(i) + w(i + 1)) / 2.0)
            .unwrap_or(e)
    }
}

/// What a click on the properties pane does.
#[derive(Debug, Clone, Copy, PartialEq)]
enum PropHit {
    Tab(cecilia_app::properties::PropTab),
    Row(usize),
    /// A box of a staff row: 0 the notation, 1 the clef.
    StaffBox(usize, u8),
}

/// What a click on the tracks panel does.
#[derive(Debug, Clone, Copy, PartialEq)]
enum TracksHit {
    /// A row's volume slider (left and right ends).
    Volume(Strip, f64, f64),
    Solo(Strip),
    Mute(Strip),
    /// A track's name and instrument cells.
    Name(usize),
    Instrument(usize),
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
/// block per bar, colored where the bar has notes, with the edit cursor and the playhead.
fn draw_tracks(
    scene: &mut Scene,
    app: &App,
    views: &mut Views,
    mut playback: Option<&mut Playback>,
    r: Rect,
    play: Option<(usize, Location, f64, f64)>,
) {
    let theme = app.config.theme.clone();
    fill(scene, r, col(theme.panel));
    let focused = app.focused_kind() == PaneKind::Tracks;
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
    // As many rows as fit.
    let fit = ((r.height() - TRACK_HEAD) / TRACK_ROW).floor().max(1.0) as usize;
    let visible = rows.len().clamp(1, fit);
    views.tracks_rect = Some(r);
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
    // The row to keep in view: the panel's selection while it has the focus, else the
    // pane's track.
    let sel_row = if focused {
        app.tracks_selected
    } else {
        cursor.track
    };
    if views.tracks_anchor != Some((sel_row, follow_bar)) {
        views.tracks_anchor = Some((sel_row, follow_bar));
        if sel_row < first {
            first = sel_row;
        } else if sel_row >= first + visible {
            first = sel_row + 1 - visible;
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
    let cursor_hidden = app.cursor_hidden(buffer);
    let rows_y0 = r.y0 + TRACK_HEAD;
    let rows_y1 = rows_y0 + visible as f64 * TRACK_ROW;
    scene.pop_layer();

    let text_c = col(theme.chrome_text);
    for (k, row) in rows.iter().enumerate().skip(first).take(visible) {
        let y = rows_y0 + (k - first) as f64 * TRACK_ROW;
        let cy = y + TRACK_ROW / 2.0;
        let row_rect = Rect::new(r.x0, y, gx - 6.0, y + TRACK_ROW);
        let track = matches!(row.strip, Strip::Track(_));
        // The selected row is shaded; the track the pane shows has its color at the edge.
        let shown = track && row.index == cursor.track;
        if (focused && k == sel_row) || (!focused && shown) {
            fill(
                scene,
                Rect::new(r.x0, y, r.x1, y + TRACK_ROW),
                col(theme.tab.with_alpha(160)),
            );
        }
        if shown {
            fill(
                scene,
                Rect::new(r.x0, y + 2.0, r.x0 + 3.0, y + TRACK_ROW - 2.0),
                col(row.color),
            );
        }
        let fg = if row.audible { text_c } else { head };
        let number = if track {
            (row.index + 1).to_string()
        } else {
            String::new()
        };
        text(
            scene,
            &number,
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
                TracksHit::Solo(row.strip),
            ),
            (
                COL_MUTE,
                row.mute,
                "M",
                theme.playhead,
                TracksHit::Mute(row.strip),
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
        // The name, or the name being typed with a caret while it is renamed.
        let renaming = app
            .renaming
            .as_ref()
            .filter(|(t, _)| track && *t == row.index);
        let name = match renaming {
            Some((_, typed)) => format!("{typed}|"),
            None => clip_text(&row.name, 18),
        };
        if renaming.is_some() {
            fill(
                scene,
                Rect::new(
                    r.x0 + COL_NAME - 3.0,
                    y + 2.0,
                    r.x0 + COL_INSTR - 6.0,
                    y + TRACK_ROW - 2.0,
                ),
                col(theme.page),
            );
        }
        text(
            scene,
            &name,
            r.x0 + COL_NAME,
            cy + 4.5,
            12.0,
            Align::Left,
            if renaming.is_some() {
                col(theme.ink)
            } else {
                fg
            },
            track && row.index == cursor.track,
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
            TracksHit::Volume(row.strip, v0, v1),
        ));
        scene.push_layer(Mix::Clip, 1.0, Affine::IDENTITY, &grid_clip);
        let bar_x = |b: usize| gx + b as f64 * BLOCK_STEP - hs;
        match row.strip {
            // The audio: a simplified waveform along the bars it plays under.
            Strip::Audio => {
                if let Some(pb) = playback.as_deref_mut() {
                    draw_track_waveform(
                        scene,
                        app,
                        pb,
                        buffer,
                        (first_bar, last_bar),
                        &bar_x,
                        (y + 3.0, y + TRACK_ROW - 3.0),
                        row.color.with_alpha(if row.audible { 230 } else { 90 }),
                    );
                }
                scene.pop_layer();
                continue;
            }
            // The master: the section markers, a flag or the boxed letter, then the title.
            Strip::Master => {
                for (b, letter, title) in &row.markers {
                    let x = bar_x(*b);
                    if x > gx + gw || x + 160.0 < gx {
                        continue;
                    }
                    let mut tx = x + 2.0;
                    if letter.is_empty() {
                        let mut flag = BezPath::new();
                        flag.move_to((x + 2.0, cy + 6.0));
                        flag.line_to((x + 2.0, cy - 6.0));
                        flag.line_to((x + 10.0, cy - 6.0));
                        flag.line_to((x + 10.0, cy + 1.0));
                        flag.line_to((x + 6.0, cy - 2.0));
                        flag.line_to((x + 2.0, cy + 1.0));
                        scene.fill(Fill::NonZero, Affine::IDENTITY, text_c, None, &flag);
                        tx += 14.0;
                    } else {
                        let w = (letter.chars().count() as f64 * 7.5).max(13.0);
                        let b = Rect::new(tx, cy - 7.0, tx + w, cy + 7.0).to_rounded_rect(2.0);
                        scene.stroke(&Stroke::new(1.0), Affine::IDENTITY, text_c, None, &b);
                        text(
                            scene,
                            letter,
                            tx + w / 2.0,
                            cy + 4.0,
                            10.0,
                            Align::Center,
                            text_c,
                            true,
                        );
                        tx += w + 6.0;
                    }
                    text(scene, title, tx, cy + 4.5, 12.0, Align::Left, text_c, false);
                }
                scene.pop_layer();
                continue;
            }
            Strip::Track(_) => {}
        }
        // The name and instrument cells: double-click renames / picks an instrument.
        views.tracks.push((
            Rect::new(
                r.x0 + COL_NAME - 3.0,
                y,
                r.x0 + COL_INSTR - 6.0,
                y + TRACK_ROW,
            ),
            TracksHit::Name(row.index),
        ));
        views.tracks.push((
            Rect::new(r.x0 + COL_INSTR - 3.0, y, gx - 6.0, y + TRACK_ROW),
            TracksHit::Instrument(row.index),
        ));
        views.tracks.push((row_rect, TracksHit::Row(row.index)));
        // Bars.
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
            if row.index == cursor.track && b == cursor.bar && !cursor_hidden {
                scene.fill(
                    Fill::NonZero,
                    Affine::IDENTITY,
                    tc(|t| t.ink),
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
        let frac = loc.tick as f64 / cecilia_app::tracks::bar_ticks(score, loc.bar) as f64;
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

/// Height of a score pane's progress bar.
const PROGRESS_H: f64 = 24.0;

/// A score pane's progress bar: the song's sections as labelled segments (letter and title),
/// a translucent red marker at the play position (or the cursor's), and the time.
fn draw_progress(
    scene: &mut Scene,
    app: &App,
    views: &mut Views,
    id: PaneId,
    r: Rect,
    play: Option<(usize, Location, f64, f64)>,
    on_page: bool,
) {
    let play = play.map(|(b, loc, t, total)| (b, loc.bar, loc.tick, t, total));
    let Some(p) = app.progress(id, play) else {
        return;
    };
    let theme = app.config.theme.clone();
    // On the score's background: the page, or the window's behind the page layout's pages.
    let (ground, ink) = if on_page {
        (theme.page, theme.ink)
    } else {
        (theme.chrome, theme.chrome_text)
    };
    let label_ink = ink;
    fill(scene, r, col(ground));
    let time = format!(
        "{} / {}",
        cecilia_app::progress::clock(p.now),
        cecilia_app::progress::clock(p.total)
    );
    text_heavy(
        scene,
        &time,
        r.x1 - 8.0,
        r.center().y + 4.5,
        12.0,
        Align::Right,
        col(ground.mix(ink, 0.8)),
        false,
        0.5,
    );
    let bar = Rect::new(r.x0 + 2.0, r.y0 + 2.0, r.x1 - 96.0, r.y1 - 2.0);
    views.progress.push((bar, id));
    let x_at = |f: f64| bar.x0 + bar.width() * f.clamp(0.0, 1.0);
    for sec in &p.sections {
        let seg = Rect::new(x_at(sec.start) + 1.0, bar.y0, x_at(sec.end) - 1.0, bar.y1);
        if seg.width() < 1.0 {
            continue;
        }
        let label = match (sec.letter.is_empty(), sec.title.is_empty()) {
            (false, false) => format!("[{}] {}", sec.letter, sec.title),
            (false, true) => format!("[{}]", sec.letter),
            (true, _) => sec.title.clone(),
        };
        if !label.is_empty() {
            scene.push_layer(Mix::Clip, 1.0, Affine::IDENTITY, &seg);
            let now = p.at >= sec.start && p.at < sec.end.max(sec.start + 1e-9);
            text_heavy(
                scene,
                &label,
                seg.x0 + 5.0,
                seg.center().y + 4.5,
                12.0,
                Align::Left,
                col(label_ink),
                // The section being played (or under the cursor) is bold.
                now,
                if now { 0.6 } else { 0.5 },
            );
            scene.pop_layer();
        }
    }
    // A thin gray border along the top, and the same lines between the sections.
    let line = col(ground.mix(ink, 0.35));
    fill(scene, Rect::new(r.x0, r.y0, r.x1, r.y0 + 1.0), line);
    // Between the sections and at both ends.
    let mut edges: Vec<f64> = p.sections.iter().map(|s| s.start).collect();
    edges.push(1.0);
    for f in edges {
        // Not `clamp`: in a pane narrower than a pixel the bounds cross.
        let x = x_at(f).min(bar.x1 - 0.5).max(bar.x0 + 0.5);
        fill(scene, Rect::new(x - 0.5, bar.y0, x + 0.5, bar.y1), line);
    }
    // The position: see-through, so the section under it stays readable.
    let x = x_at(p.at);
    fill(
        scene,
        Rect::new(x - 2.5, r.y0, x + 2.5, r.y1),
        col(theme.playhead.with_alpha(180)),
    );
}

/// The audio track's row: under each bar, a few bars of the audio's loudness there, mirrored
/// about the row's middle (blank until the audio has loaded).
#[allow(clippy::too_many_arguments)]
fn draw_track_waveform(
    scene: &mut Scene,
    app: &App,
    pb: &mut Playback,
    buffer: usize,
    (first_bar, last_bar): (usize, usize),
    bar_x: &dyn Fn(usize) -> f64,
    (top, bottom): (f64, f64),
    color: Rgba,
) {
    let Some(peaks) = pb.loaded_audio(buffer).map(|a| a.peaks.clone()) else {
        return;
    };
    let bars = app.buffers[buffer].score.master_bars.len();
    let mid = (top + bottom) / 2.0;
    let half = (bottom - top) / 2.0;
    const COLUMNS: usize = 4;
    for b in first_bar..last_bar {
        let (Some(t0), Some(t1)) = (
            pb.time_of(app, buffer, b, 0),
            if b + 1 < bars {
                pb.time_of(app, buffer, b + 1, 0)
            } else {
                pb.time_of(
                    app,
                    buffer,
                    b,
                    cecilia_app::tracks::bar_ticks(&app.buffers[buffer].score, b),
                )
            },
        ) else {
            continue;
        };
        let (x0, x1) = (bar_x(b), bar_x(b) + BLOCK_W);
        for k in 0..COLUMNS {
            let (a, z) = (k as f64 / COLUMNS as f64, (k + 1) as f64 / COLUMNS as f64);
            let (s0, s1) = (
                ((t0 + (t1 - t0) * a) * 100.0).max(0.0) as usize,
                ((t0 + (t1 - t0) * z) * 100.0).max(0.0) as usize,
            );
            let amp = peaks
                .get(s0.min(peaks.len())..s1.max(s0 + 1).min(peaks.len()))
                .unwrap_or_default()
                .iter()
                .map(|&(lo, hi)| lo.abs().max(hi.abs()))
                .fold(0.0f32, f32::max) as f64;
            let h = (amp.min(1.0) * half).max(0.5);
            let cx0 = x0 + (x1 - x0) * a + 0.5;
            let cx1 = x0 + (x1 - x0) * z - 0.5;
            fill(scene, Rect::new(cx0, mid - h, cx1, mid + h), col(color));
        }
    }
}

/// How fast the cursor glides after the playhead (per second: it covers 1 − e^(−rate·t)).
const GLIDE_RATE: f64 = 16.0;

/// An engraving color in the theme: black and greys become shades between the page and the
/// ink (so notation reads on dark pages too); other colors stay as engraved.
fn rgb(c: cecilia_engrave::display::Color, page: Rgba, ink: Rgba) -> Color {
    if c.r == c.g && c.g == c.b {
        let darkness = 1.0 - c.r as f32 / 255.0;
        let m = page.mix(ink, darkness);
        Color::from_rgba8(m.r, m.g, m.b, c.a)
    } else {
        Color::from_rgba8(c.r, c.g, c.b, c.a)
    }
}

fn page_scene(page: &cecilia_engrave::display::Page, paper: Rgba, ink: Rgba) -> Scene {
    let mut s = Scene::new();
    s.fill(
        Fill::NonZero,
        Affine::IDENTITY,
        col(paper),
        None,
        &Rect::new(0.0, 0.0, page.width, page.height),
    );
    for item in &page.items {
        for (path, paint) in outlines(item) {
            match paint {
                Paint::Fill(c) => s.fill(
                    Fill::NonZero,
                    Affine::IDENTITY,
                    rgb(c, paper, ink),
                    None,
                    &path,
                ),
                Paint::Stroke(c, w) => s.stroke(
                    &Stroke::new(w),
                    Affine::IDENTITY,
                    rgb(c, paper, ink),
                    None,
                    &path,
                ),
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
    let path: BezPath = cecilia_engrave::fonts::fonts().text_path(font, s, x, y, size, align);
    scene.fill(Fill::NonZero, Affine::IDENTITY, color, None, &path);
}

/// `text` with a stroke along the outlines, which thickens the glyphs by `heft` px.
#[allow(clippy::too_many_arguments)]
fn text_heavy(
    scene: &mut Scene,
    s: &str,
    x: f64,
    y: f64,
    size: f64,
    align: Align,
    color: Color,
    bold: bool,
    heft: f64,
) {
    let font = if bold { FontKind::Bold } else { FontKind::Text };
    let path: BezPath = cecilia_engrave::fonts::fonts().text_path(font, s, x, y, size, align);
    scene.fill(Fill::NonZero, Affine::IDENTITY, color, None, &path);
    scene.stroke(&Stroke::new(heft), Affine::IDENTITY, color, None, &path);
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
    // Panes fill the window between the tab bar and the status line; tool panes are among
    // them.
    let bottom = height - STATUS - CMDLINE;
    let tabline = app.tabline_visible();
    let top = if tabline { TAB_BAR } else { 0.0 } + if app.toolbar { TOOLBAR } else { 0.0 };
    views.chrome_bottom = top;
    // The tracks peeking after a change of track (no tracks pane shown) take the bottom.
    let peek = if app.tracks_peeking() && height > 300.0 {
        app.peek_height() as f64
    } else {
        0.0
    };
    let main = Rect::new(0.0, top, width, bottom - peek);
    views.frets.clear();
    views.keys.clear();
    views.tracks.clear();
    views.props.clear();
    views.palette.clear();
    views.palette_rect = None;
    views.edges.clear();
    views.wave_rects.clear();
    views.tracks_rect = None;
    views.props_rect = None;
    views.props_text = None;
    views.tool_rects.clear();
    views.progress.clear();
    app.row_height = TRACK_ROW as u16;
    app.pane_frame = 12;
    app.col_width = 8;
    if peek > 0.0 {
        draw_tracks(
            scene,
            app,
            views,
            playback.as_deref_mut(),
            Rect::new(3.0, bottom - peek + 3.0, width - 3.0, bottom - 3.0),
            play,
        );
    }
    app.set_viewport(cecilia_app::layout::Rect {
        x: main.x0 as u16,
        y: main.y0 as u16,
        w: main.width() as u16,
        h: main.height() as u16,
    });
    // The toolbar at the very top, the tabline under it (drawn at the origin, then moved).
    views.toolbar.clear();
    if app.toolbar {
        draw_toolbar(scene, app, views, width);
    }
    let tab_y = if app.toolbar { TOOLBAR } else { 0.0 };
    // No tabline over the dashboard: its space goes to the dashboard.
    views.tabline.clear();
    if tabline {
        let hover = views.hover;
        views.hover.1 -= tab_y;
        let mut tabs = Scene::new();
        draw_tabs(&mut tabs, app, views, width);
        views.hover = hover;
        scene.append(&tabs, Some(Affine::translate((0.0, tab_y))));
        for (r, _) in views.tabline.iter_mut() {
            *r = *r + vello::kurbo::Vec2::new(0.0, tab_y);
        }
    }
    views.dashboard.clear();
    if app.dashboard_visible() {
        draw_dashboard(scene, app, views, main);
    }
    views.geometry.clear();
    views.view_infos.clear();
    views.scroll_taken.clear();
    let focused = app.window_focus();
    let layout = app.layout();
    for (id, r) in layout {
        let rect = Rect::new(
            r.x as f64 + 3.0,
            r.y as f64 + 3.0,
            (r.x + r.w) as f64 - 3.0,
            (r.y + r.h) as f64 - 3.0,
        );
        let kind = app.panes.get(&id).map_or(PaneKind::Score, |p| p.kind);
        if kind == PaneKind::Score {
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
            continue;
        }
        views.tool_rects.push((rect, id));
        scene.push_layer(Mix::Clip, 1.0, Affine::IDENTITY, &rect);
        // Drawn for its score pane (its own for a local tool), as if that were the view's.
        let view_score = app.view().score;
        let active = app.active_view;
        app.views[active].score = app.tool_score(id);
        let instrument = app.instrument_view(play.map(|(b, loc, ..)| (b, loc.bar, loc.tick)));
        match kind {
            PaneKind::Tracks => draw_tracks(scene, app, views, playback.as_deref_mut(), rect, play),
            PaneKind::Audio => {
                let buffer = app.pane().buffer;
                views.wave_rects.push((rect, None));
                match playback.as_deref_mut() {
                    Some(pb) if app.buffers[buffer].score.audio.is_some() => {
                        pb.ensure_audio(app, buffer);
                        let view = views.waves.entry(None).or_default();
                        draw_waveform(scene, app, pb, buffer, rect, play, view);
                    }
                    _ => {
                        fill(scene, rect, tc(|t| t.panel));
                        let name = &app.buffers[buffer].name;
                        text(
                            scene,
                            &format!("{name}: no audio attached (:audio FILE)"),
                            rect.x0 + 10.0,
                            rect.center().y + 5.0,
                            13.0,
                            Align::Left,
                            dim(),
                            false,
                        );
                    }
                }
            }
            PaneKind::Fretboard | PaneKind::Piano => match &instrument {
                Some(v) if kind == PaneKind::Fretboard => {
                    draw_fretboard(scene, app, views, v, rect)
                }
                Some(v) => draw_piano_panel(scene, app, views, v, rect),
                None => {
                    fill(scene, rect, tc(|t| t.panel));
                    text(
                        scene,
                        "no instrument on this track",
                        rect.x0 + 10.0,
                        rect.center().y + 5.0,
                        13.0,
                        Align::Left,
                        dim(),
                        false,
                    );
                }
            },
            PaneKind::Tuner => draw_tuner(scene, app, rect),
            PaneKind::Properties => draw_properties(scene, app, views, rect),
            PaneKind::Palette => draw_edit_palette(scene, app, views, rect),
            PaneKind::Score => {}
        }
        app.views[active].score = view_score;
        scene.pop_layer();
        // The focused tool pane is outlined in the mode's color.
        if id == focused {
            ring(scene, app, rect, 3.0);
        }
    }
    for (id, info) in std::mem::take(&mut views.view_infos) {
        app.set_view_info(id, info);
    }
    for id in std::mem::take(&mut views.scroll_taken) {
        app.take_scroll_request(id);
    }
    draw_status(scene, app, views, play, width, height);
    // Keys that can follow the ones typed, or the marks while ' ` m wait for one.
    if let Some((title, rows)) = app.which_key() {
        draw_which_key(scene, &title, &rows, main);
    }
    if app.show_help {
        draw_help(scene, app, main);
    }
    if let Some((r, tip)) = views.tooltip.take() {
        draw_tooltip(scene, r, &tip, width);
    }
    if let Some(p) = app.palette() {
        draw_palette(scene, &p, width, height);
    }
    views.cache.retain(|id, _| app.panes.contains_key(id));
}

/// Width in pixels the dashboard's lily is rasterized at: three times its 96 logical px, so it
/// is sharp up to 300% display scaling and merely smoothed below that.
const LILY_PX: u32 = 288;

thread_local! {
    /// The dashboard's lily: `assets/lily.svg` (embedded) rendered once by resvg. A vector scene
    /// from vello_svg drew the petals wrongly (its `<use>` transforms) and without the drop
    /// shadows, so the picture is rasterized instead.
    static LILY: Option<vello::peniko::Image> = lily_image();
}

fn lily_image() -> Option<vello::peniko::Image> {
    let svg = include_str!("../../../assets/lily.svg");
    let tree = resvg::usvg::Tree::from_str(svg, &resvg::usvg::Options::default()).ok()?;
    let mut pixmap = resvg::tiny_skia::Pixmap::new(LILY_PX, LILY_PX)?;
    let k = LILY_PX as f32 / tree.size().width();
    resvg::render(
        &tree,
        resvg::tiny_skia::Transform::from_scale(k, k),
        &mut pixmap.as_mut(),
    );
    // tiny-skia holds premultiplied colors; the scene takes straight alpha.
    let data: Vec<u8> = pixmap
        .pixels()
        .iter()
        .flat_map(|p| {
            let c = p.demultiply();
            [c.red(), c.green(), c.blue(), c.alpha()]
        })
        .collect();
    Some(vello::peniko::Image::new(
        data.into(),
        vello::peniko::ImageFormat::Rgba8,
        LILY_PX,
        LILY_PX,
    ))
}

/// The dashboard shown instead of the panes: the name and a column of buttons, centred in `main`
/// (the focused or hovered one highlighted).
fn draw_dashboard(scene: &mut Scene, app: &App, views: &mut Views, main: Rect) {
    use cecilia_app::dashboard::DashboardButton;
    let (w, h, gap) = (260.0, 48.0, 14.0);
    let (icon, header) = (96.0, 96.0 + 24.0 + 44.0 + 22.0 + 36.0);
    let n = DashboardButton::ALL.len() as f64;
    let total = header + n * h + (n - 1.0) * gap;
    let cx = main.center().x;
    let top = (main.center().y - total / 2.0).max(main.y0 + 10.0);
    // The lily (assets/lily.svg, 512 px wide as drawn), the name and the tagline under it.
    LILY.with(|l| {
        if let Some(lily) = l {
            let k = icon / f64::from(LILY_PX);
            let at = Affine::translate((cx - icon / 2.0, top)) * Affine::scale(k);
            scene.draw_image(lily, at);
        }
    });
    let name_y = top + icon + 24.0 + 24.0;
    text(
        scene,
        "cecilia",
        cx,
        name_y,
        34.0,
        Align::Center,
        chrome_text(),
        true,
    );
    let tagline = "Tablature, notation and chant editor and player";
    let path: BezPath = cecilia_engrave::fonts::fonts().text_path(
        FontKind::Italic,
        tagline,
        cx,
        name_y + 30.0,
        15.0,
        Align::Center,
    );
    scene.fill(Fill::NonZero, Affine::IDENTITY, dim(), None, &path);
    let top = top + header - 70.0;
    for (i, b) in DashboardButton::ALL.into_iter().enumerate() {
        let y = top + 70.0 + i as f64 * (h + gap);
        let r = Rect::new(cx - w / 2.0, y, cx + w / 2.0, y + h);
        let focused = i == app.dashboard_focus();
        let enabled = app.dashboard_enabled(i);
        let hovered = enabled && r.contains(views.hover);
        let shape = RoundedRect::new(r.x0, r.y0, r.x1, r.y1, 8.0);
        let bg = if focused {
            accent()
        } else if hovered {
            tc(|t| t.tab_hover)
        } else {
            tc(|t| t.toolbar)
        };
        scene.fill(Fill::NonZero, Affine::IDENTITY, bg, None, &shape);
        if focused {
            let ring = RoundedRect::new(r.x0 - 3.0, r.y0 - 3.0, r.x1 + 3.0, r.y1 + 3.0, 10.0);
            scene.stroke(&Stroke::new(2.0), Affine::IDENTITY, accent(), None, &ring);
        }
        text(
            scene,
            b.label(),
            cx,
            r.center().y + 6.0,
            17.0,
            Align::Center,
            if focused {
                text_on(bg)
            } else if enabled {
                chrome_text()
            } else {
                dim()
            },
            false,
        );
        views.dashboard.push((r, i));
    }
}

/// The tabline, as in bufferline.nvim: the active view's buffers as tabs on the left (file-type
/// icon, name shortened in the middle, × on hover or ● when modified), the views on the right.
fn draw_tabs(scene: &mut Scene, app: &App, views: &mut Views, width: f64) {
    let fonts = cecilia_engrave::fonts::fonts();
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
            cecilia_app::truncate_middle(&tab.name, t.max_width, t.prefix_width, t.suffix_width);
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
                let c = tc(|t| t.error.with_alpha(if tab.active { 70 } else { 120 }));
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

/// The toolbar: a button per configured command, separators between groups, and the
/// tooltip of the button the mouse rests on.
fn draw_toolbar(scene: &mut Scene, app: &App, views: &mut Views, width: f64) {
    let y0 = 0.0;
    fill(
        scene,
        Rect::new(0.0, y0, width, y0 + TOOLBAR),
        tc(|t| t.toolbar),
    );
    fill(
        scene,
        Rect::new(0.0, y0 + TOOLBAR - 1.0, width, y0 + TOOLBAR),
        tc(|t| t.tab),
    );
    let (hx, hy) = views.hover;
    let mut x = 8.0;
    let mut hovered = None;
    let mut tooltip = None;
    for (i, item) in app.toolbar_items().into_iter().enumerate() {
        let b = match item {
            cecilia_app::toolbar::ToolItem::Separator => {
                fill(
                    scene,
                    Rect::new(x + 3.0, y0 + 9.0, x + 4.0, y0 + TOOLBAR - 9.0),
                    tc(|t| t.dim.with_alpha(120)),
                );
                x += 8.0;
                continue;
            }
            cecilia_app::toolbar::ToolItem::Button(b) => b,
        };
        let r = Rect::new(x, y0 + 4.0, x + 30.0, y0 + TOOLBAR - 4.0);
        x += 32.0;
        if r.x1 > width - 4.0 {
            break;
        }
        let over = r.contains((hx, hy));
        if b.active || (over && b.enabled) {
            let bg = if b.active {
                tc(|t| t.accent.with_alpha(if over { 90 } else { 60 }))
            } else {
                tc(|t| t.button_hover)
            };
            scene.fill(
                Fill::NonZero,
                Affine::IDENTITY,
                bg,
                None,
                &r.to_rounded_rect(5.0),
            );
        }
        let color = if !b.enabled {
            tc(|t| t.dim.with_alpha(110))
        } else if b.active {
            tc(|t| t.text_on(t.accent.mix(t.toolbar, 0.6)))
        } else {
            chrome_text()
        };
        draw_tool_icon(scene, &b, r.center(), color);
        if over {
            hovered = Some(i);
            tooltip = Some((r, b.tooltip.clone()));
        }
        views.toolbar.push((r, i));
    }
    // The tooltip of the button the mouse rests on, after a moment.
    match (hovered, views.toolbar_hover) {
        (Some(i), Some((j, _))) if i == j => {}
        (Some(i), _) => views.toolbar_hover = Some((i, std::time::Instant::now())),
        (None, _) => views.toolbar_hover = None,
    }
    views.tooltip = match (tooltip, views.toolbar_hover) {
        (Some(t), Some((_, since))) if since.elapsed() >= TOOLTIP_DELAY => Some(t),
        _ => None,
    };
}

/// The tooltip of the toolbar button the mouse rests on (drawn over everything).
fn draw_tooltip(scene: &mut Scene, r: Rect, tip: &str, width: f64) {
    let w = cecilia_engrave::fonts::fonts().text_width(FontKind::Text, tip, 12.5) + 16.0;
    let x = r.x0.min(width - w - 4.0).max(4.0);
    let tr = Rect::new(x, r.y1 + 6.0, x + w, r.y1 + 30.0);
    scene.fill(
        Fill::NonZero,
        Affine::IDENTITY,
        popover(),
        None,
        &tr.to_rounded_rect(5.0),
    );
    scene.stroke(
        &Stroke::new(1.0),
        Affine::IDENTITY,
        dim(),
        None,
        &tr.to_rounded_rect(5.0),
    );
    text(
        scene,
        tip,
        tr.x0 + 8.0,
        tr.y1 - 8.0,
        12.5,
        Align::Left,
        text_on(popover()),
        false,
    );
}

/// A toolbar button's icon, drawn in a 16 px box centred at `c`.
fn draw_tool_icon(
    scene: &mut Scene,
    b: &cecilia_app::toolbar::ToolButton,
    c: vello::kurbo::Point,
    color: Color,
) {
    let (x, y) = (c.x - 8.0, c.y - 8.0);
    let stroke = Stroke::new(1.6)
        .with_caps(vello::kurbo::Cap::Round)
        .with_join(vello::kurbo::Join::Round);
    let line = |scene: &mut Scene, path: &BezPath| {
        scene.stroke(&stroke, Affine::IDENTITY, color, None, path);
    };
    let solid = |scene: &mut Scene, shape: &dyn Fn() -> BezPath| {
        scene.fill(Fill::NonZero, Affine::IDENTITY, color, None, &shape());
    };
    let path = |points: &[(f64, f64)], close: bool| {
        let mut p = BezPath::new();
        for (k, &(px, py)) in points.iter().enumerate() {
            if k == 0 {
                p.move_to((x + px, y + py));
            } else {
                p.line_to((x + px, y + py));
            }
        }
        if close {
            p.close_path();
        }
        p
    };
    let rect = |x0: f64, y0: f64, x1: f64, y1: f64, r: f64| {
        RoundedRect::new(x + x0, y + y0, x + x1, y + y1, r).to_path(0.1)
    };
    match b.name.as_str() {
        "new" => {
            line(
                scene,
                &path(
                    &[
                        (3.0, 1.0),
                        (10.0, 1.0),
                        (13.0, 4.0),
                        (13.0, 15.0),
                        (3.0, 15.0),
                    ],
                    true,
                ),
            );
            line(scene, &path(&[(8.0, 6.0), (8.0, 12.0)], false));
            line(scene, &path(&[(5.0, 9.0), (11.0, 9.0)], false));
        }
        "open" | "e" | "edit" | "open-dialog" => {
            line(
                scene,
                &path(
                    &[
                        (1.0, 3.0),
                        (6.0, 3.0),
                        (7.5, 5.0),
                        (15.0, 5.0),
                        (15.0, 14.0),
                        (1.0, 14.0),
                    ],
                    true,
                ),
            );
            line(scene, &path(&[(1.0, 8.0), (15.0, 8.0)], false));
        }
        "write" | "w" | "wall" | "wa" => {
            line(scene, &rect(1.5, 1.5, 14.5, 14.5, 2.0));
            line(
                scene,
                &path(&[(4.5, 1.5), (4.5, 5.5), (11.0, 5.5), (11.0, 1.5)], false),
            );
            line(scene, &rect(4.5, 9.5, 11.5, 14.5, 0.5));
        }
        "undo" | "u" | "redo" => {
            let flip = b.name == "redo";
            let fx = |px: f64| if flip { 16.0 - px } else { px };
            let mut p = BezPath::new();
            p.move_to((x + fx(3.0), y + 6.0));
            p.line_to((x + fx(10.0), y + 6.0));
            p.curve_to(
                (x + fx(15.5), y + 6.0),
                (x + fx(15.5), y + 14.0),
                (x + fx(10.0), y + 14.0),
            );
            p.line_to((x + fx(6.0), y + 14.0));
            line(scene, &p);
            line(
                scene,
                &path(&[(fx(6.5), 2.5), (fx(3.0), 6.0), (fx(6.5), 9.5)], false),
            );
        }
        "play-toggle" | "play" if b.active => {
            solid(scene, &|| rect(3.0, 2.0, 6.5, 14.0, 1.0));
            solid(scene, &|| rect(9.5, 2.0, 13.0, 14.0, 1.0));
        }
        "play-toggle" | "play" => solid(scene, &|| {
            path(&[(4.0, 2.0), (14.0, 8.0), (4.0, 14.0)], true)
        }),
        "pause" => {
            solid(scene, &|| rect(3.0, 2.0, 6.5, 14.0, 1.0));
            solid(scene, &|| rect(9.5, 2.0, 13.0, 14.0, 1.0));
        }
        "stop" => solid(scene, &|| rect(3.0, 3.0, 13.0, 13.0, 1.5)),
        "follow-toggle" => {
            line(scene, &Circle::new((x + 8.0, y + 8.0), 5.0).to_path(0.1));
            for (a, b2) in [
                ((8.0, 0.5), (8.0, 4.0)),
                ((8.0, 12.0), (8.0, 15.5)),
                ((0.5, 8.0), (4.0, 8.0)),
                ((12.0, 8.0), (15.5, 8.0)),
            ] {
                line(scene, &path(&[a, b2], false));
            }
            solid(scene, &|| Circle::new((x + 8.0, y + 8.0), 1.6).to_path(0.1));
        }
        "loop-toggle" => {
            let mut p = BezPath::new();
            p.move_to((x + 3.0, y + 10.0));
            p.line_to((x + 3.0, y + 7.0));
            p.curve_to((x + 3.0, y + 4.5), (x + 4.5, y + 4.0), (x + 7.0, y + 4.0));
            p.line_to((x + 13.0, y + 4.0));
            p.move_to((x + 13.0, y + 6.0));
            p.line_to((x + 13.0, y + 9.0));
            p.curve_to(
                (x + 13.0, y + 11.5),
                (x + 11.5, y + 12.0),
                (x + 9.0, y + 12.0),
            );
            p.line_to((x + 3.0, y + 12.0));
            line(scene, &p);
            line(
                scene,
                &path(&[(11.0, 2.0), (13.0, 4.0), (11.0, 6.0)], false),
            );
            line(
                scene,
                &path(&[(5.0, 10.0), (3.0, 12.0), (5.0, 14.0)], false),
            );
        }
        "metronome" => {
            line(
                scene,
                &path(&[(5.5, 1.5), (10.5, 1.5), (14.0, 14.5), (2.0, 14.5)], true),
            );
            line(scene, &path(&[(8.0, 11.0), (12.5, 3.5)], false));
            line(scene, &path(&[(4.0, 11.0), (12.0, 11.0)], false));
        }
        "count-in" => text(
            scene,
            "1 2",
            c.x,
            c.y + 4.5,
            12.0,
            Align::Center,
            color,
            true,
        ),
        "tracks" | "tracks-focus" => {
            for (k, len) in [(0.0, 14.0), (1.0, 9.0), (2.0, 12.0)] {
                solid(scene, &|| {
                    rect(1.0, 2.5 + k * 4.5, 1.0 + len, 4.5 + k * 4.5, 1.0)
                });
            }
        }
        "split" | "sp" => {
            line(scene, &rect(1.5, 1.5, 14.5, 14.5, 2.0));
            line(scene, &path(&[(1.5, 8.0), (14.5, 8.0)], false));
        }
        "vsplit" | "vs" => {
            line(scene, &rect(1.5, 1.5, 14.5, 14.5, 2.0));
            line(scene, &path(&[(8.0, 1.5), (8.0, 14.5)], false));
        }
        "maximize" | "max" => {
            line(scene, &rect(1.5, 1.5, 14.5, 14.5, 2.0));
            line(scene, &path(&[(5.0, 11.0), (11.0, 5.0)], false));
            line(scene, &path(&[(7.0, 5.0), (11.0, 5.0), (11.0, 9.0)], false));
        }
        "command-mode" => {
            line(scene, &rect(1.0, 2.5, 15.0, 13.5, 2.0));
            solid(scene, &|| Circle::new((x + 5.0, y + 6.0), 1.2).to_path(0.1));
            solid(scene, &|| {
                Circle::new((x + 5.0, y + 10.0), 1.2).to_path(0.1)
            });
            line(scene, &path(&[(8.0, 10.0), (12.0, 10.0)], false));
        }
        "pick-keymap" | "keymaps" | "help" => {
            line(scene, &rect(0.5, 3.5, 15.5, 12.5, 2.0));
            for k in 0..4 {
                solid(scene, &|| {
                    rect(2.5 + k as f64 * 3.3, 5.5, 4.3 + k as f64 * 3.3, 7.3, 0.4)
                });
            }
            line(scene, &path(&[(5.0, 10.0), (11.0, 10.0)], false));
        }
        // A window with a panel on its left.
        "palette" => {
            line(scene, &rect(1.5, 2.5, 14.5, 13.5, 2.0));
            line(scene, &path(&[(6.5, 2.5), (6.5, 13.5)], false));
            for k in 0..3 {
                solid(scene, &|| {
                    rect(2.8, 4.5 + k as f64 * 2.6, 5.2, 5.6 + k as f64 * 2.6, 0.4)
                });
            }
        }
        // A window with a side panel.
        "properties" => {
            line(scene, &rect(1.5, 2.5, 14.5, 13.5, 2.0));
            line(scene, &path(&[(9.5, 2.5), (9.5, 13.5)], false));
            for k in 0..3 {
                solid(scene, &|| {
                    rect(10.8, 4.5 + k as f64 * 2.6, 13.2, 5.6 + k as f64 * 2.6, 0.4)
                });
            }
        }
        // Bars of an audio wave.
        "waveform" => {
            for (k, h) in [3.0, 7.0, 12.0, 6.0, 10.0, 4.0, 8.0]
                .into_iter()
                .enumerate()
            {
                solid(scene, &|| {
                    rect(
                        1.0 + k as f64 * 2.1,
                        8.0 - h / 2.0,
                        2.4 + k as f64 * 2.1,
                        8.0 + h / 2.0,
                        0.5,
                    )
                });
            }
        }
        // A tuning fork.
        "tuner" => {
            line(
                scene,
                &path(
                    &[
                        (5.0, 1.5),
                        (5.0, 7.5),
                        (6.0, 9.5),
                        (10.0, 9.5),
                        (11.0, 7.5),
                        (11.0, 1.5),
                    ],
                    false,
                ),
            );
            line(scene, &path(&[(8.0, 9.5), (8.0, 14.5)], false));
        }
        // Piano keys.
        "piano" => {
            line(scene, &rect(1.5, 3.5, 14.5, 12.5, 1.5));
            for xk in [4.8, 8.0, 11.2] {
                line(scene, &path(&[(xk, 8.0), (xk, 12.5)], false));
            }
            for xk in [3.9, 7.1, 10.3] {
                solid(scene, &|| rect(xk, 3.5, xk + 1.8, 8.0, 0.3));
            }
        }
        // A fretboard: strings across, frets down.
        "fretboard" => {
            line(scene, &rect(1.5, 3.5, 14.5, 12.5, 1.0));
            for yk in [6.5, 9.5] {
                line(scene, &path(&[(1.5, yk), (14.5, yk)], false));
            }
            for xk in [6.0, 10.5] {
                line(scene, &path(&[(xk, 3.5), (xk, 12.5)], false));
            }
        }
        "toolbar" | "layout" => {
            line(scene, &rect(1.5, 2.5, 14.5, 13.5, 2.0));
            line(scene, &path(&[(1.5, 6.0), (14.5, 6.0)], false));
        }
        // Anything else: its name, shortened.
        name => {
            let label: String = name.chars().take(3).collect();
            text(
                scene,
                &label,
                c.x,
                c.y + 4.0,
                11.0,
                Align::Center,
                color,
                true,
            );
        }
    }
}

/// A small file-type icon centred vertically at `cy`, starting at `x` (14 px wide).
fn draw_file_icon(scene: &mut Scene, kind: &str, x: f64, cy: f64) {
    let color = tc(|t| t.file_icon(kind));
    fn f(s: &mut Scene, color: Color, shape: &impl vello::kurbo::Shape) {
        s.fill(Fill::NonZero, Affine::IDENTITY, color, None, shape);
    }
    match kind {
        // A guitar pick.
        "guitar-pro" | "cecilia" => {
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
            if kind == "cecilia" {
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

fn mode_tag(pane: &cecilia_app::Pane) -> &'static str {
    match pane.layout.as_str() {
        "page" => "page",
        "horizontal" => "horizontal",
        _ => "vertical",
    }
}

/// Space around a pane rect that belongs to the pane (panes are inset this far in their cell).
const PANE_MARGIN: f64 = 3.0;

/// The path the focus ring of `width` is stroked along for a pane `rect` of corner `radius`:
/// the pane's edge, moved in so the ring's outer edge stays within the pane's margin (wide
/// rings would otherwise spill over the neighbouring pane). The radius follows the offset.
fn ring_shape(rect: Rect, radius: f64, width: f64) -> RoundedRect {
    let inset = (width / 2.0 - PANE_MARGIN).max(0.0);
    let r = rect.inset(-inset);
    RoundedRect::from_rect(r, (radius - inset).max(0.0))
}

/// The ring around the focused pane: `ui.focus_ring_width` thick, `ui.focus_ring_color`. It
/// is drawn after the pane's content so nothing (title bar, page) covers it.
fn ring(scene: &mut Scene, app: &App, rect: Rect, radius: f64) {
    let width = app.config.file.ui.focus_ring_width;
    if width <= 0.0 {
        return;
    }
    let c = app.config.focus_ring_color(app.mode);
    scene.stroke(
        &Stroke::new(width),
        Affine::IDENTITY,
        col(c),
        None,
        &ring_shape(rect, radius, width),
    );
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
    draw_pane_content(scene, app, views, playback, id, rect, focused, play);
    if focused {
        ring(scene, app, rect, 6.0);
    }
}

#[allow(clippy::too_many_arguments)]
fn draw_pane_content(
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
    // Frame and title strip, on the score's background: the page in the vertical and
    // horizontal layouts, the window's background behind the pages of the page layout.
    let ground = if mode_tag(&pane) == "page" {
        background()
    } else {
        tc(|t| t.page)
    };
    scene.fill(
        Fill::NonZero,
        Affine::IDENTITY,
        ground,
        None,
        &RoundedRect::from_rect(rect, 6.0),
    );
    // The focused pane's ring is drawn last, by `draw_pane`, over everything else.
    if !focused {
        scene.stroke(
            &Stroke::new(1.0),
            Affine::IDENTITY,
            tc(|t| t.chrome.mix(t.chrome_text, 0.25)),
            None,
            &RoundedRect::from_rect(rect, 6.0),
        );
    }
    let title = match score.tracks.get(pane.cursor.track) {
        Some(t) => {
            // The horizontal layout has no header on the score: its tuning goes here.
            let tuning = t
                .tuning_text()
                .filter(|_| pane.layout == "horizontal")
                .map(|s| format!("  ·  {s}"))
                .unwrap_or_default();
            format!(
                "{} — {}/{}  {}{tuning}",
                buffer.name,
                pane.cursor.track + 1,
                score.tracks.len(),
                t.name.trim()
            )
        }
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
    // Maximize indicator on the right of the title bar when this pane is in the zoom state.
    if app.is_maximized(id) {
        draw_expand(
            scene,
            rect.x1 - 24.0,
            rect.y0 + TITLE / 2.0,
            text_on(ground),
        );
    }
    let mut content = Rect::new(rect.x0 + 2.0, rect.y0 + TITLE, rect.x1 - 2.0, rect.y1 - 2.0);
    // The progress bar along the bottom, under the waveform strip.
    if app.progress_bar && content.height() > 120.0 {
        let r = Rect::new(content.x0, content.y1 - PROGRESS_H, content.x1, content.y1);
        content.y1 -= PROGRESS_H + 2.0;
        draw_progress(scene, app, views, id, r, play, mode_tag(&pane) != "page");
    }
    let waveform = (app.pane_waveform(&pane) && score.audio.is_some() && content.height() > 200.0)
        .then(|| {
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
    // The scrollbars' gutters (right and bottom): the score lays out and draws beside them,
    // never under them.
    let gutters = app.scrollbar.then(|| {
        let right = Rect::new(
            content.x1 - SCROLLBAR,
            content.y0,
            content.x1,
            content.y1 - SCROLLBAR,
        );
        let bottom = Rect::new(
            content.x0,
            content.y1 - SCROLLBAR,
            content.x1 - SCROLLBAR,
            content.y1,
        );
        content.x1 -= SCROLLBAR;
        content.y1 -= SCROLLBAR;
        (right, bottom)
    });
    // Pages fit the pane's width; zoom is relative to that.
    let scale = if mode_name == "page" {
        pane.zoom * (content.width() - 24.0).max(100.0) / 595.0
    } else {
        BASE_SCALE * pane.zoom
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
        // Pages are recolored when the theme's page or ink change.
        {
            let (p, i) = (app.config.theme.page, app.config.theme.ink);
            [p.r, p.g, p.b, p.a, i.r, i.g, i.b, i.a]
        },
        app.chord_policy().name(),
        app.every_bar_number,
        app.highlight_incomplete_bars.then(|| {
            let c = app.config.theme.incomplete_bar;
            [c.r, c.g, c.b]
        }),
    );
    if views.cache.get(&id).is_none_or(|c| c.key != key) {
        let mode = match mode_name {
            "page" => cecilia_engrave::Mode::Page {
                width: 595.0,
                height: 842.0,
            },
            "horizontal" => cecilia_engrave::Mode::Horizontal,
            _ => cecilia_engrave::Mode::Vertical {
                width: (key.3 as f64 * 8.0).max(100.0),
            },
        };
        let options = Options {
            mode,
            header: mode_name != "horizontal",
            margin: if mode_name == "page" { 40.0 } else { 16.0 },
            chant: app.chant_view,
            chords: app.chord_policy(),
            every_bar_number: app.every_bar_number,
            incomplete_bars: app.highlight_incomplete_bars.then(|| {
                let c = app.config.theme.incomplete_bar;
                cecilia_engrave::display::Color::rgb(c.r, c.g, c.b)
            }),
            ..Options::default()
        };
        let layout = cecilia_engrave::engrave(score, pane.cursor.track, &options);
        let (page, ink) = (app.config.theme.page, app.config.theme.ink);
        let scenes = layout
            .pages
            .iter()
            .map(|p| page_scene(p, page, ink))
            .collect();
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
                lit: None,
                offsets,
            },
        );
    }
    let linked = app.linked_note(&pane).map(|n| {
        let m = app.config.theme.mode_color(app.mode);
        (n, [m.r, m.g, m.b, m.a])
    });
    let cached = views.cache.get_mut(&id).unwrap();
    if cached.lit.as_ref().map(|(n, c, _)| (*n, *c)) != linked {
        let (page, ink) = (app.config.theme.page, app.config.theme.ink);
        cached.lit = linked.map(|(n, c)| {
            let beat = BeatRef {
                track: n.track,
                bar: n.bar,
                voice: n.voice,
                beat: n.beat,
            };
            let color = cecilia_engrave::display::Color {
                r: c[0],
                g: c[1],
                b: c[2],
                a: c[3],
            };
            let pages = cached
                .layout
                .pages
                .iter()
                .enumerate()
                .filter_map(|(i, p)| {
                    let lit = p.with_note_color(beat, n.string, n.staff, color)?;
                    Some((i, page_scene(&lit, page, ink)))
                })
                .collect();
            (n, c, pages)
        });
    }
    let cached = &*cached;

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
    let cursor_key = pane.cursor;
    let moved = views.last_cursor.insert(id, cursor_key) != Some(cursor_key);
    // The playhead, while this buffer plays: (page, x, top, bottom).
    let playhead = play
        .filter(|p| p.0 == pane.buffer && app.playing)
        .and_then(|(_, loc, _, _)| {
            playhead_at(&cached.layout, score, pane.cursor.track, loc.bar, loc.tick)
        });
    // Keep the playhead in view while following playback, otherwise the cursor when it moves.
    // The page's x offset in scroll space (the page layout centres a page narrower than the view).
    let page_x = if mode_name == "page" {
        ((view_w - 595.0) / 2.0).max(0.0)
    } else {
        0.0
    };
    let target = match playhead.filter(|_| app.follow) {
        Some((page, x, top, bottom)) => Some((page, (x, x + 1.0), (top, bottom))),
        None if moved => cached
            .layout
            .beats
            .get(&cursor_ref)
            .or_else(|| beat_fallback(&cached.layout, &cursor_ref))
            .map(|p| {
                let staff = score
                    .tracks
                    .get(cursor_ref.track)
                    .map_or(0, |t| cecilia_app::staff_index(t, pane.cursor.staff));
                let (xs, ys) = cursor_extent(p, staff, &pane.cursor);
                (p.page, xs, ys)
            }),
        None => None,
    };
    // The score may scroll past its end along its flow (down the vertical and page layouts,
    // across the horizontal one), by half a view; the other way only as far as it extends.
    // What the cursor needs in view always stays scrollable (a bar can be wider than its page).
    let mut reach = (0.0f64, 0.0f64);
    if let Some(to) = pane.scroll_request {
        // `zz` / `zt` / `zb`: the cursor's own extent goes where asked, whatever the follow
        // logic would do; later frames find it in view and leave the scroll alone.
        views.scroll_taken.push(id);
        let at = cached
            .layout
            .beats
            .get(&cursor_ref)
            .or_else(|| beat_fallback(&cached.layout, &cursor_ref));
        if let Some(p) = at {
            let staff = score
                .tracks
                .get(cursor_ref.track)
                .map_or(0, |t| cecilia_app::staff_index(t, pane.cursor.staff));
            let ((x0, x1), (top, bottom)) = cursor_extent(p, staff, &pane.cursor);
            let off = cached.offsets[p.page];
            scroll = align_scroll(
                scroll,
                (view_w, view_h),
                (x0 + page_x, x1 + page_x),
                (top + off, bottom + off),
                to,
                mode_name == "horizontal",
            );
        }
    } else if let Some((page, (x0, x1), (top, bottom))) = target {
        let off = cached.offsets[page];
        scroll = follow_scroll(
            scroll,
            (view_w, view_h),
            (x0 + page_x, x1 + page_x),
            (top + off, bottom + off),
        );
        reach = (scroll.0, scroll.1);
    }
    let total_h = cached.offsets.last().copied().unwrap_or(0.0)
        + cached.layout.pages.last().map_or(0.0, |p| p.height);
    let total_w = cached
        .layout
        .pages
        .iter()
        .map(|p| p.width)
        .fold(0.0, f64::max);
    let (max_x, max_y) = if mode_name == "horizontal" {
        (total_w - view_w * 0.5, total_h - view_h)
    } else {
        (total_w - view_w, total_h - view_h * 0.5)
    };
    scroll.0 = scroll.0.clamp(0.0, max_x.max(reach.0));
    scroll.1 = scroll.1.clamp(0.0, max_y.max(reach.1));
    views.scroll.insert(id, scroll);
    let info = view_info(
        cached,
        cursor_ref.track,
        mode_name == "horizontal",
        mode_name == "page",
        scroll,
        (view_w, view_h),
    );
    views.view_infos.push((id, Some(info)));
    views.geometry.push(Geometry {
        pane: id,
        content,
        scale,
        scroll,
        bars: [None, None],
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
        let lit = cached
            .lit
            .as_ref()
            .and_then(|(_, _, pages)| pages.iter().find(|(p, _)| *p == i));
        scene.append(lit.map_or(page_scene, |(_, s)| s), Some(to_screen(i)));
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
    if let Some(pos) = cached
        .layout
        .beats
        .get(&cursor_ref)
        .filter(|_| !app.cursor_hidden(pane.buffer))
    {
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
        // A translucent shade in the mode's color over just what an edit changes: the note
        // place on the cursor's string, or the whole beat on staves without strings.
        let alpha = if focused { 110 } else { 45 };
        let color = tc(|t| t.mode_color(app.mode).with_alpha(alpha));
        // A square over the cursor's string or staff position. Chant positions only match
        // the square-notation layout (a chant track whose one staff is chant).
        let c = pane.cursor;
        let track = score.tracks.get(c.track);
        let c = cecilia_app::Cursor {
            staff: track.map_or(0, |t| cecilia_app::staff_index(t, c.staff)),
            ..c
        };
        let staff = track.and_then(|t| t.staves().get(c.staff).copied());
        let chant_layout = track.is_some_and(|t| {
            t.chant.is_some() && app.chant_view && t.staves() == [cecilia_core::Staff::Chant]
        });
        let square = |cx: f64, y: f64, side: f64| {
            RoundedRect::new(
                cx - side / 2.0,
                y - side / 2.0,
                cx + side / 2.0,
                y + side / 2.0,
                3.0,
            )
        };
        let whole = RoundedRect::new(x - 1.5, pos.top - 1.5, x + w + 1.5, pos.bottom + 1.5, 3.0);
        let r = if let Some(ys) = pos.strings(c.staff) {
            match ys.get(c.string) {
                Some(&y) => {
                    let gap = match ys {
                        [a, b, ..] => (b - a).abs(),
                        _ => 9.0,
                    };
                    // Centred where the engraver centres the fret number (0.4 staff spaces in
                    // from the beat's left, plus half a notehead; a string gap is 1.5 spaces).
                    square(x + gap * 0.66, y, (gap * 1.5).clamp(10.0, 22.0))
                }
                None => whole,
            }
        } else if let Some((y, step)) = pos.line_y(c.staff, c.line) {
            // The square sits on the note head: chant beats are the neume glyph itself, others
            // start 0.4 spaces before the head.
            let cx = if chant_layout { x + w / 2.0 } else { pos.head };
            square(cx, y, (step * 3.0).clamp(10.0, 22.0))
        } else if staff.is_some_and(|s| s.is_chant()) && !chant_layout {
            // A chant staff that is not drawn (a chord under the error policy): no position.
            whole
        } else {
            whole
        };
        scene.fill(Fill::NonZero, to_screen(pos.page), color, None, &r);
        // An outline in a contrasting color, so the cursor shows in every mode.
        let border = tc(|t| {
            t.cursor_border()
                .with_alpha(if focused { 255 } else { 130 })
        });
        scene.stroke(&Stroke::new(1.5), to_screen(pos.page), border, None, &r);
    }
    // Design mode: handles at the bar lines, the line ends and the cursor bar's beats.
    if page_mode && focused && app.design_active() {
        let dragged = match &views.drag {
            Some(Drag::Design(_, grab)) => Some(grab.handle),
            _ => None,
        };
        // The mouse in page points, for the buttons' hover.
        let hover = |page: usize| {
            (
                (views.hover.0 - content.x0) / scale + scroll.0 - center,
                (views.hover.1 - content.y0) / scale + scroll.1 - cached.offsets[page],
            )
        };
        draw_design_handles(
            scene,
            &cached.layout,
            &to_screen,
            &hover,
            scale,
            (pane.cursor.track, pane.cursor.bar),
            dragged,
            views.drag.is_some(),
        );
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
    // Scrollbars, for each direction the score scrolls: the thumb is the visible part of its
    // extent, tinted lightly from the ground it sits on.
    let mut bars = [None, None];
    if let Some((right, bottom)) = gutters {
        let t = &app.config.theme;
        let (ground, ink) = if mode_name == "page" {
            (t.chrome, t.chrome_text)
        } else {
            (t.page, t.ink)
        };
        fill(scene, right, col(ground));
        fill(scene, bottom, col(ground));
        let axes = [
            (right, false, total_h, view_h, scroll.1, content.height()),
            (bottom, true, total_w, view_w, scroll.0, content.width()),
        ];
        for (i, (gutter, horizontal, total, view, pos, len)) in axes.into_iter().enumerate() {
            if total <= view {
                continue;
            }
            let size = (len * view / total).clamp(24.0_f64.min(len), len);
            let at = (len - size) * (pos / (total - view)).clamp(0.0, 1.0);
            let inset = 2.0;
            let (track, thumb) = if horizontal {
                let (y0, y1) = (gutter.y0 + inset, gutter.y1 - inset);
                (
                    Rect::new(gutter.x0, y0, gutter.x1, y1),
                    Rect::new(gutter.x0 + at, y0, gutter.x0 + at + size, y1),
                )
            } else {
                let (x0, x1) = (gutter.x0 + inset, gutter.x1 - inset);
                (
                    Rect::new(x0, gutter.y0, x1, gutter.y1),
                    Rect::new(x0, gutter.y0 + at, x1, gutter.y0 + at + size),
                )
            };
            let pill = |r: Rect| RoundedRect::from_rect(r, r.width().min(r.height()) / 2.0);
            let tint = |t: f32| col(ground.mix(ink, t));
            scene.fill(
                Fill::NonZero,
                Affine::IDENTITY,
                tint(0.06),
                None,
                &pill(track),
            );
            scene.fill(
                Fill::NonZero,
                Affine::IDENTITY,
                tint(0.28),
                None,
                &pill(thumb),
            );
            bars[i] = Some(ScrollBar {
                track,
                thumb,
                horizontal,
                range: total - view,
            });
        }
    }
    if let Some(g) = views.geometry.last_mut() {
        g.bars = bars;
    }
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
    let tick = cecilia_app::cursor_tick(&app.buffers[buffer].score, &cursor);
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
    let wave = tc(|t| t.waveform);
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
                    tc(|t| t.loop_region),
                    None,
                    &rr,
                );
            }
        }
    }
    // Sync points.
    let score = &app.buffers[buffer].score;
    if let Some(sync) = &score.audio {
        let pad = sync.frame_padding as f64 / cecilia_audio::SYNC_RATE;
        for p in &sync.points {
            let t = p.frame as f64 / cecilia_audio::SYNC_RATE - pad;
            let x = x_of(t);
            if x >= r.x0 && x <= r.x1 {
                fill(
                    scene,
                    Rect::new(x, r.y0, x + 1.5, r.y1),
                    tc(|t| t.sync_point),
                );
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
        tc(|t| t.playhead),
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

/// Draws design mode's handles over the pages: a line with a grip at each bar line of a system,
/// "+" and "-" buttons at the end of each line (dimmed when they cannot work, brighter under
/// the mouse), a dot under each beat of the cursor's bar. The one being dragged is solid.
#[allow(clippy::too_many_arguments)]
fn draw_design_handles(
    scene: &mut Scene,
    layout: &Layout,
    to_screen: &impl Fn(usize) -> Affine,
    hover: &impl Fn(usize) -> (f64, f64),
    scale: f64,
    cursor: (usize, usize),
    dragged: Option<crate::design::Handle>,
    dragging: bool,
) {
    use crate::design::{BEAT_R, Handle, beat_handle, line_buttons, line_enabled};
    let color = accent();
    let soft = |c: Color, held: bool| if held { c } else { c.with_alpha(0.55) };
    let px = 1.0 / scale;
    for (i, sys) in layout.systems.iter().enumerate() {
        let at = to_screen(sys.page);
        for k in 0..sys.bars.saturating_sub(1) {
            let x = layout.bars[sys.first_bar + k].2;
            let held = dragged == Some(Handle::BarLine { system: i, k });
            let r = Rect::new(x - px * 1.5, sys.top - 3.0, x + px * 1.5, sys.bottom + 3.0);
            scene.fill(Fill::NonZero, at, soft(color, held), None, &r);
            // A grip in the middle.
            let mid = (sys.top + sys.bottom) / 2.0;
            let grip = RoundedRect::new(x - px * 4.0, mid - 6.0, x + px * 4.0, mid + 6.0, 2.0);
            scene.fill(Fill::NonZero, at, soft(color, held), None, &grip);
        }
        let (mx, my) = hover(sys.page);
        let enabled = line_enabled(sys, layout.bars.len());
        for (n, (l, t, r, b)) in line_buttons(sys).into_iter().enumerate() {
            let on = [enabled.0, enabled.1][n];
            let over = on && !dragging && mx >= l && mx <= r && my >= t && my <= b;
            let alpha = if !on {
                0.2
            } else if over {
                1.0
            } else {
                0.6
            };
            let button = RoundedRect::new(l, t, r, b, 2.0);
            scene.fill(Fill::NonZero, at, color.with_alpha(alpha), None, &button);
            // The sign, in the color that reads on the button.
            let ink = text_on(color).with_alpha(if on { 1.0 } else { 0.5 });
            let (cx, cy) = ((l + r) / 2.0, (t + b) / 2.0);
            let arm = (r - l) * 0.28;
            let mut sign = BezPath::new();
            sign.move_to((cx - arm, cy));
            sign.line_to((cx + arm, cy));
            if n == 0 {
                sign.move_to((cx, cy - arm));
                sign.line_to((cx, cy + arm));
            }
            scene.stroke(&Stroke::new(1.4), at, ink, None, &sign);
        }
    }
    let mut beats: Vec<&BeatRef> = layout
        .beats
        .keys()
        .filter(|b| (b.track, b.bar) == cursor)
        .collect();
    beats.sort_by_key(|b| (b.voice, b.beat));
    for b in beats {
        if let Some((page, x, y)) = beat_handle(layout, b) {
            let held = dragged == Some(Handle::Beat(*b));
            let dot = Circle::new((x, y), BEAT_R);
            scene.fill(
                Fill::NonZero,
                to_screen(page),
                soft(color, held),
                None,
                &dot,
            );
        }
    }
}

/// The rows of bars the layout has and how many of them (and of its bars) the view shows.
fn view_info(
    cached: &Cached,
    track: usize,
    horizontal: bool,
    paged: bool,
    scroll: (f64, f64),
    view: (f64, f64),
) -> cecilia_app::ViewInfo {
    let bars = &cached.layout.bars;
    if horizontal {
        let visible_bars = bars
            .iter()
            .filter(|b| b.2 > scroll.0 && b.1 < scroll.0 + view.0)
            .count();
        return cecilia_app::ViewInfo {
            horizontal,
            visible_bars,
            ..Default::default()
        };
    }
    // A bar starts a row when it does not lie to the right of the one before.
    let mut rows: Vec<std::ops::Range<usize>> = Vec::new();
    for (i, b) in bars.iter().enumerate() {
        match rows.last_mut() {
            Some(r) if b.0 == bars[i - 1].0 && b.1 > bars[i - 1].1 + 0.5 => r.end = i + 1,
            _ => rows.push(i..i + 1),
        }
    }
    let shown = |r: &std::ops::Range<usize>| {
        r.clone().any(|bar| {
            let at = BeatRef {
                track,
                bar,
                voice: 0,
                beat: 0,
            };
            cached
                .layout
                .beats
                .get(&at)
                .or_else(|| beat_fallback(&cached.layout, &at))
                .is_some_and(|p| {
                    let (top, bottom) = (
                        cached.offsets[p.page] + p.top,
                        cached.offsets[p.page] + p.bottom,
                    );
                    bottom > scroll.1 && top < scroll.1 + view.1
                })
        })
    };
    let visible: Vec<_> = rows.iter().filter(|r| shown(r)).collect();
    // The page each row is on: the page layout moves by rows per page, whatever the zoom.
    let page_rows = if paged {
        let mut per_page = std::collections::HashMap::<usize, usize>::new();
        for r in &rows {
            let at = BeatRef {
                track,
                bar: r.start,
                voice: 0,
                beat: 0,
            };
            let page = cached
                .layout
                .beats
                .get(&at)
                .or_else(|| beat_fallback(&cached.layout, &at))
                .map(|p| p.page);
            if let Some(page) = page {
                *per_page.entry(page).or_default() += 1;
            }
        }
        per_page.values().copied().max().unwrap_or(0)
    } else {
        0
    };
    cecilia_app::ViewInfo {
        page_rows,
        horizontal,
        visible_rows: visible.len(),
        visible_bars: visible.iter().map(|r| r.len()).sum(),
        rows,
    }
}

/// Cursor on a beat the layout skipped (e.g. an empty bar): use the bar's first beat.
/// Where the playhead is drawn at (bar, tick): between the notes of the bar in proportion to
/// time (on the track's first voice; the bar's edges without notes), never across a line break.
/// Returns (page, x, top, bottom) in page coordinates.
fn playhead_at(
    layout: &Layout,
    score: &cecilia_core::Score,
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

/// The horizontal and vertical extents (page points) of the edit cursor within a beat: its
/// string or staff position with some margin, else the staff it is on, else the whole beat.
fn cursor_extent(
    pos: &cecilia_engrave::BeatPos,
    staff: usize,
    cursor: &cecilia_app::Cursor,
) -> ((f64, f64), (f64, f64)) {
    let margin = 12.0;
    // The cursor square sits near the beat's left, so a wide beat need not show its end.
    let xs = (
        pos.x.min(pos.head - margin),
        (pos.x + pos.width.min(30.0)).max(pos.head + margin),
    );
    if let Some(ys) = pos.strings(staff) {
        if let Some(&y) = ys.get(cursor.string) {
            return (xs, (y - margin, y + margin));
        }
        let (lo, hi) = ys
            .iter()
            .fold((f64::MAX, f64::MIN), |a, &y| (a.0.min(y), a.1.max(y)));
        if lo <= hi {
            return (xs, (lo - margin, hi + margin));
        }
    } else if let Some((y, step)) = pos.line_y(staff, cursor.line) {
        let m = margin.max(step * 2.0);
        return (xs, (y - m, y + m));
    }
    ((pos.x, pos.x + pos.width), (pos.top, pos.bottom))
}

/// The scroll (points) that brings the `x` and `y` ranges into a `view` (width, height), moving
/// it only when they are not already in view; the range is placed a quarter (a third across)
/// into the view, but always so that it is wholly visible when it fits, else its start is.
/// How close to a pane's edge (points) a selection drag starts to scroll it.
const EDGE_MARGIN: f64 = 28.0;

/// Scroll speed (points per second, negative towards `lo`) of a selection drag with the pointer
/// at `pos` in a pane spanning `lo..hi`: zero in the middle, growing the nearer the edge and the
/// further beyond it.
fn edge_velocity(pos: f64, lo: f64, hi: f64) -> f64 {
    let margin = EDGE_MARGIN.min((hi - lo) / 4.0);
    let depth = if pos < lo + margin {
        -(lo + margin - pos)
    } else if pos > hi - margin {
        pos - (hi - margin)
    } else {
        return 0.0;
    };
    (120.0 + 14.0 * depth.abs()).min(1600.0) * depth.signum()
}

/// The beat column or string of voice 1 on `track` nearest a point of a page (the point itself,
/// when it is over one).
fn nearest_hit(
    hits: &[cecilia_engrave::display::Hit],
    track: usize,
    p: (f64, f64),
) -> Option<&cecilia_engrave::display::Hit> {
    let dist = |r: &Rect| {
        let dx = (r.x0 - p.0).max(0.0).max(p.0 - r.x1);
        let dy = (r.y0 - p.1).max(0.0).max(p.1 - r.y1);
        dx * dx + dy * dy
    };
    hits.iter()
        .filter(|h| h.beat.track == track && h.beat.voice == 0)
        .min_by(|a, b| dist(&a.rect).total_cmp(&dist(&b.rect)))
}

/// The scroll (points) that places the `x` and `y` ranges of the cursor in the middle, at the
/// start (top, or left in the `horizontal` layout) or at the end of the `view` along the axis the
/// score flows on (`zz`, `zt`, `zb`), leaving the other axis alone unless the range is out of
/// view there. Not yet clamped to the scrollable range.
fn align_scroll(
    scroll: (f64, f64),
    view: (f64, f64),
    x: (f64, f64),
    y: (f64, f64),
    to: cecilia_app::ScrollTo,
    horizontal: bool,
) -> (f64, f64) {
    use cecilia_app::ScrollTo::*;
    let place = |view: f64, (lo, hi): (f64, f64)| match to {
        Center => (lo + hi) / 2.0 - view / 2.0,
        Top => lo,
        Bottom => hi - view,
    };
    let kept = follow_scroll(scroll, view, x, y);
    if horizontal {
        (place(view.0, x), kept.1)
    } else {
        (kept.0, place(view.1, y))
    }
}

fn follow_scroll(scroll: (f64, f64), view: (f64, f64), x: (f64, f64), y: (f64, f64)) -> (f64, f64) {
    let axis = |s: f64, view: f64, (lo, hi): (f64, f64), frac: f64| {
        if lo >= s && hi <= s + view {
            return s;
        }
        let mut n = lo - view * frac;
        if hi > n + view {
            n = hi - view;
        }
        n.min(lo).max(0.0)
    };
    (
        axis(scroll.0, view.0, x, 0.3),
        axis(scroll.1, view.1, y, 0.25),
    )
}

fn beat_fallback<'a>(layout: &'a Layout, r: &BeatRef) -> Option<&'a cecilia_engrave::BeatPos> {
    layout.beats.get(&BeatRef { beat: 0, ..*r }).or_else(|| {
        layout
            .beats
            .iter()
            .find(|(k, _)| k.bar == r.bar)
            .map(|(_, v)| v)
    })
}

/// A soft gaussian drop shadow under a floating box (palette, pickers): offset 8 px down,
/// blurred over ~24 px, so the box looks like it hovers a little above the page.
fn popover_shadow(scene: &mut Scene, r: Rect, radius: f64) {
    scene.draw_blurred_rounded_rect(
        Affine::IDENTITY,
        Rect::new(r.x0, r.y0 + 8.0, r.x1, r.y1 + 8.0),
        Color::from_rgba8(0, 0, 0, 150),
        radius,
        12.0,
    );
}

/// Rows of candidates the palette shows at once.
const PALETTE_ROWS: usize = 12;

/// The command palette: a rounded box near the top centre with the typed command and, below
/// it, the completions of the word being typed (descriptions right-aligned for commands).
fn draw_palette(scene: &mut Scene, p: &cecilia_app::Palette, width: f64, height: f64) {
    let fonts = cecilia_engrave::fonts::fonts();
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
    popover_shadow(scene, Rect::new(x0, y0, x0 + w, y0 + h), 10.0);
    let frame = RoundedRect::new(x0, y0, x0 + w, y0 + h, 10.0);
    scene.fill(Fill::NonZero, Affine::IDENTITY, popover(), None, &frame);
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
        text_on(popover()),
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
        dim(),
    );
    // Scroll so the selected candidate is visible.
    let first = match p.selected {
        Some(sel) if sel >= rows => sel + 1 - rows,
        _ => 0,
    };
    let swatches = p
        .items
        .iter()
        .any(|i| i.swatch.is_some() || i.icon.is_some());
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
        // Colors get a swatch before their name.
        let tx0 = if swatches { x0 + 48.0 } else { x0 + 16.0 };
        if let Some(c) = item.swatch {
            let sw = RoundedRect::new(x0 + 16.0, ry + 4.0, x0 + 40.0, ry + row_h - 6.0, 4.0);
            scene.fill(Fill::NonZero, Affine::IDENTITY, col(c), None, &sw);
            scene.stroke(&Stroke::new(1.0), Affine::IDENTITY, dim(), None, &sw);
        }
        if let Some(icon) = item.icon {
            draw_icon(
                scene,
                icon,
                x0 + 18.0,
                ry + (row_h - 18.0) / 2.0 - 1.0,
                color,
            );
        }
        text(
            scene,
            &item.text,
            tx0,
            base,
            14.0,
            Align::Left,
            color,
            selected,
        );
        // The characters that matched what was typed, over the text in a stronger color.
        let font = if selected {
            FontKind::Bold
        } else {
            FontKind::Text
        };
        let hl = if selected {
            text_on(popover())
        } else {
            accent()
        };
        let chars: Vec<char> = item.text.chars().collect();
        for &i in &item.matched {
            let Some(c) = chars.get(i) else { continue };
            let before: String = chars[..i].iter().collect();
            let cx = tx0 + fonts.text_width(font, &before, 14.0);
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
        Mode::Normal
        | Mode::Insert
        | Mode::Visual
        | Mode::Tracks
        | Mode::Properties
        | Mode::Palette => {
            if let Some(m) = &app.message {
                let c = if m.error {
                    tc(|t| t.error)
                } else {
                    chrome_text()
                };
                text(scene, &m.text, 10.0, cy, 14.0, Align::Left, c, false);
            }
            let mut pending = app.count.map(|c| c.to_string()).unwrap_or_default();
            pending.push_str(&cecilia_app::keys::format_keys(&app.pending));
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

/// The status line, as lualine.nvim: mode, layout, workspace and file on the left; the rest of
/// the state, progress and `bar:beat` on the right. Colored sections are rounded chips.
fn draw_status(
    scene: &mut Scene,
    app: &App,
    views: &mut Views,
    play: Option<(usize, Location, f64, f64)>,
    width: f64,
    height: f64,
) {
    use cecilia_app::statusline::{SegKind, Segment, StatusIcon};
    let y0 = height - STATUS - CMDLINE;
    fill(scene, Rect::new(0.0, y0, width, height), chrome());
    let line = app.statusline(play.map(|(_, _, now, total)| (now, total)), true);
    let th = app.config.theme.clone();
    let mode = th.mode_color(app.display_mode());
    let bg_of = |k: SegKind| match k {
        SegKind::Mode => mode,
        SegKind::ModeText => th.status_b,
        SegKind::Plain | SegKind::Warn => th.chrome,
    };
    let fg_of = |k: SegKind| match k {
        SegKind::Mode => th.text_on(mode),
        SegKind::ModeText => mode,
        SegKind::Plain => th.chrome_text,
        SegKind::Warn => th.error,
    };
    let fonts = cecilia_engrave::fonts::fonts();
    // Chips have a little padding inside; plain sections a little more room around them.
    const PAD: f64 = 5.0;
    const GAP: f64 = 6.0;
    const ICON: f64 = 18.0;
    const TEMPO_ICON: f64 = 9.0;
    let seg_width = |seg: &Segment| {
        let font = if seg.bold {
            FontKind::Bold
        } else {
            FontKind::Text
        };
        fonts.text_width(font, &seg.text, 13.0)
            + 2.0 * PAD
            + match seg.icon {
                Some(StatusIcon::Workspace | StatusIcon::Zoom | StatusIcon::Speed) => ICON,
                Some(StatusIcon::Synth | StatusIcon::Audio) => ICON - 4.0,
                Some(StatusIcon::SynthAndAudio) => 2.0 * ICON + 8.0,
                Some(StatusIcon::Tempo) => TEMPO_ICON,
                None => 0.0,
            }
    };
    let draw = |scene: &mut Scene, x: f64, w: f64, seg: &Segment| {
        if !matches!(seg.kind, SegKind::Plain | SegKind::Warn) {
            scene.fill(
                Fill::NonZero,
                Affine::IDENTITY,
                col(bg_of(seg.kind)),
                None,
                &RoundedRect::new(x, y0 + 4.0, x + w, y0 + STATUS - 4.0, 4.0),
            );
        }
        let color = col(fg_of(seg.kind));
        let mut tx = x + PAD;
        match seg.icon {
            Some(StatusIcon::Workspace) => {
                draw_workspace_icon(scene, tx, y0 + STATUS / 2.0, color);
                tx += ICON;
            }
            Some(StatusIcon::Tempo) => {
                draw_quarter_note(scene, tx, y0 + STATUS / 2.0, color);
                tx += TEMPO_ICON;
            }
            Some(StatusIcon::Zoom) => {
                draw_magnifier(scene, tx, y0 + STATUS / 2.0, color);
                tx += ICON;
            }
            Some(StatusIcon::Speed) => {
                draw_speedometer(scene, tx, y0 + STATUS / 2.0, color);
                tx += ICON;
            }
            Some(StatusIcon::Synth) => draw_piano(scene, tx, y0 + STATUS / 2.0, color),
            Some(StatusIcon::Audio) => draw_music(scene, tx, y0 + STATUS / 2.0, color),
            Some(StatusIcon::SynthAndAudio) => {
                let cy = y0 + STATUS / 2.0;
                draw_piano(scene, tx, cy, color);
                text(
                    scene,
                    "+",
                    tx + ICON + 1.0,
                    y0 + 17.5,
                    13.0,
                    Align::Left,
                    color,
                    false,
                );
                draw_music(scene, tx + ICON + 12.0, cy, color);
            }
            None => {}
        }
        text(
            scene,
            &seg.text,
            tx,
            y0 + 17.5,
            13.0,
            Align::Left,
            color,
            seg.bold,
        );
    };

    views.status.clear();
    let mut hit = |x: f64, w: f64, seg: &Segment| {
        if let Some(cmd) = &seg.action {
            views
                .status
                .push((Rect::new(x, y0, x + w, y0 + STATUS), cmd.clone()));
        }
    };
    // Right side first, from the right edge (section x gives way when narrow).
    let mut x = width - GAP;
    for seg in line
        .right
        .iter()
        .rev()
        .filter(|s| !s.text.is_empty() || s.icon.is_some())
    {
        let w = seg_width(seg);
        if x - w < width * 0.45 {
            continue;
        }
        x -= w;
        draw(scene, x, w, seg);
        hit(x, w, seg);
        x -= GAP;
    }
    let right_x = x;

    // Left side.
    let mut x = GAP;
    for seg in &line.left {
        let w = seg_width(seg);
        if x + w > right_x - GAP {
            break;
        }
        draw(scene, x, w, seg);
        hit(x, w, seg);
        x += w + GAP;
    }

    draw_command_line(scene, app, width, height);
}

/// A quarter note (the tempo): a slanted head and a stem, as the 󰎋 glyph.
fn draw_quarter_note(scene: &mut Scene, x: f64, cy: f64, color: Color) {
    let head = vello::kurbo::Ellipse::new((x + 3.2, cy + 4.0), (3.4, 2.4), -0.35);
    scene.fill(Fill::NonZero, Affine::IDENTITY, color, None, &head);
    fill(
        scene,
        Rect::new(x + 5.6, cy - 7.0, x + 6.8, cy + 3.6),
        color,
    );
}

/// A speedometer (the playback speed), as the 󰓅 glyph.
fn draw_speedometer(scene: &mut Scene, x: f64, cy: f64, color: Color) {
    let (cx, cy) = (x + 7.0, cy + 3.0);
    let mut dial = BezPath::new();
    dial.move_to((cx - 6.5, cy));
    dial.curve_to((cx - 6.5, cy - 8.7), (cx + 6.5, cy - 8.7), (cx + 6.5, cy));
    scene.stroke(
        &Stroke::new(1.6).with_caps(vello::kurbo::Cap::Round),
        Affine::IDENTITY,
        color,
        None,
        &dial,
    );
    let mut needle = BezPath::new();
    needle.move_to((cx, cy));
    needle.line_to((cx + 3.2, cy - 4.2));
    scene.stroke(
        &Stroke::new(1.8).with_caps(vello::kurbo::Cap::Round),
        Affine::IDENTITY,
        color,
        None,
        &needle,
    );
    scene.fill(
        Fill::NonZero,
        Affine::IDENTITY,
        color,
        None,
        &Circle::new((cx, cy), 1.6),
    );
}

/// The properties pane's text sizes: keys as the status line's, values and tabs 1.2 times,
/// section titles 1.5 times.
const PROPS_KEY: f64 = 13.0;
const PROPS_VALUE: f64 = 13.0 * 1.2;
const PROPS_SECTION: f64 = 13.0 * 1.5;

/// An icon in an 18 px square with its corner at (`x`, `y`).
fn draw_icon(scene: &mut Scene, icon: cecilia_app::icons::Icon, x: f64, y: f64, c: Color) {
    use cecilia_app::icons::Icon;
    use vello::kurbo::{Cap, Ellipse, Line};
    let (cx, cy) = (x + 9.0, y + 9.0);
    let line = |scene: &mut Scene, a: (f64, f64), b: (f64, f64), w: f64| {
        let stroke = Stroke::new(w).with_caps(Cap::Round);
        scene.stroke(&stroke, Affine::IDENTITY, c, None, &Line::new(a, b));
    };
    fn outline(scene: &mut Scene, c: Color, shape: &impl Shape) {
        scene.stroke(&Stroke::new(1.3), Affine::IDENTITY, c, None, shape);
    }
    fn solid(scene: &mut Scene, c: Color, shape: &impl Shape) {
        scene.fill(Fill::NonZero, Affine::IDENTITY, c, None, shape);
    }
    let letter = |scene: &mut Scene, s: &str, lx: f64, ly: f64, size: f64| {
        text(scene, s, lx, ly, size, Align::Center, c, true);
    };
    match icon {
        Icon::Treble | Icon::Bass | Icon::Grand => {
            for dy in [-5.0, 0.0, 5.0] {
                line(scene, (x + 1.0, cy + dy), (x + 17.0, cy + dy), 0.8);
            }
            let (s, size) = match icon {
                Icon::Treble => ("G", 14.0),
                Icon::Bass => ("F", 14.0),
                _ => ("GF", 10.5),
            };
            letter(scene, s, cx, cy + size * 0.36, size);
        }
        Icon::Standard => {
            for dy in [-4.0, -2.0, 0.0, 2.0, 4.0] {
                line(scene, (x + 1.0, cy + dy), (x + 17.0, cy + dy), 0.8);
            }
            solid(scene, c, &Circle::new((cx, cy + 2.0), 2.4));
        }
        Icon::Neutral => {
            for dx in [-2.5, 2.5] {
                line(scene, (cx + dx, cy - 5.0), (cx + dx, cy + 5.0), 2.0);
            }
        }
        Icon::ChantDo | Icon::ChantFa => {
            for dy in [-6.0, -2.0, 2.0, 6.0] {
                line(scene, (x + 1.0, cy + dy), (x + 17.0, cy + dy), 0.8);
            }
            let (s, size) = if icon == Icon::ChantDo {
                ("C", 13.0)
            } else {
                ("F", 13.0)
            };
            letter(scene, s, cx, cy + size * 0.36, size);
        }
        Icon::Tab | Icon::TabRhythm => {
            for (i, l) in ["T", "A", "B"].into_iter().enumerate() {
                letter(scene, l, cx - 2.0, y + 6.0 + i as f64 * 5.5, 6.5);
            }
            if icon == Icon::TabRhythm {
                line(scene, (x + 14.0, y + 3.0), (x + 14.0, y + 15.0), 1.2);
                line(scene, (x + 14.0, y + 15.0), (x + 17.0, y + 12.0), 1.2);
            }
        }
        Icon::Slash => line(scene, (x + 4.0, y + 15.0), (x + 14.0, y + 3.0), 2.4),
        Icon::Chant => {
            for dy in [-6.0, -2.0, 2.0, 6.0] {
                line(scene, (x + 1.0, cy + dy), (x + 17.0, cy + dy), 0.8);
            }
            solid(scene, c, &Rect::new(cx - 3.0, cy - 3.0, cx + 3.0, cy + 3.0));
        }
        Icon::Guitar => {
            outline(scene, c, &Circle::new((cx - 3.0, cy + 3.5), 5.5));
            outline(scene, c, &Circle::new((cx - 1.0, cy - 1.0), 3.5));
            line(scene, (cx, cy - 2.0), (x + 16.0, y + 2.0), 1.8);
            solid(scene, c, &Circle::new((cx - 3.0, cy + 3.5), 1.6));
        }
        Icon::Piano => {
            outline(scene, c, &Rect::new(x + 1.0, y + 4.0, x + 17.0, y + 14.0));
            for bx in [5.0, 9.0, 13.0] {
                solid(
                    scene,
                    c,
                    &Rect::new(x + bx - 1.0, y + 4.0, x + bx + 1.0, y + 10.0),
                );
            }
        }
        Icon::Organ => {
            for (px, h) in [(3.0, 10.0), (8.0, 14.0), (13.0, 8.0)] {
                outline(
                    scene,
                    c,
                    &Rect::new(x + px, y + 16.0 - h, x + px + 3.0, y + 16.0),
                );
            }
        }
        Icon::Drums => {
            outline(scene, c, &Ellipse::new((cx, y + 8.0), (7.0, 3.0), 0.0));
            line(scene, (x + 2.0, y + 8.0), (x + 2.0, y + 14.0), 1.3);
            line(scene, (x + 16.0, y + 8.0), (x + 16.0, y + 14.0), 1.3);
            outline(scene, c, &Ellipse::new((cx, y + 14.0), (7.0, 3.0), 0.0));
            line(scene, (x + 5.0, y + 1.0), (cx - 1.0, y + 7.0), 1.3);
            line(scene, (x + 13.0, y + 1.0), (cx + 1.0, y + 7.0), 1.3);
        }
        Icon::Violin => {
            outline(
                scene,
                c,
                &Ellipse::new((cx - 2.0, cy + 3.0), (4.5, 6.0), 0.0),
            );
            line(scene, (cx - 2.0, cy - 3.0), (cx - 2.0, y + 1.0), 1.6);
            line(scene, (x + 14.0, y + 2.0), (x + 16.0, y + 16.0), 1.2);
        }
        Icon::Voice => {
            outline(
                scene,
                c,
                &RoundedRect::new(cx - 3.0, y + 1.0, cx + 3.0, y + 10.0, 3.0),
            );
            let mut arc = BezPath::new();
            arc.move_to((cx - 5.5, y + 7.0));
            arc.quad_to((cx, y + 17.0), (cx + 5.5, y + 7.0));
            outline(scene, c, &arc);
            line(scene, (cx, y + 12.0), (cx, y + 16.0), 1.3);
            line(scene, (cx - 3.0, y + 16.5), (cx + 3.0, y + 16.5), 1.3);
        }
        Icon::Brass => {
            line(scene, (x + 1.0, cy), (x + 11.0, cy), 1.8);
            let mut bell = BezPath::new();
            bell.move_to((x + 11.0, cy - 1.0));
            bell.line_to((x + 17.0, cy - 6.0));
            bell.line_to((x + 17.0, cy + 6.0));
            bell.line_to((x + 11.0, cy + 1.0));
            bell.close_path();
            outline(scene, c, &bell);
            line(scene, (x + 5.0, cy), (x + 5.0, cy - 4.0), 1.3);
            line(scene, (x + 8.0, cy), (x + 8.0, cy - 4.0), 1.3);
        }
        Icon::Wind => {
            line(scene, (x + 2.0, y + 15.0), (x + 15.0, y + 3.0), 2.4);
            for t in [0.35, 0.55, 0.75] {
                let (px, py) = (x + 2.0 + 13.0 * t, y + 15.0 - 12.0 * t);
                solid(scene, c, &Circle::new((px + 2.0, py + 2.0), 1.1));
            }
        }
        Icon::Synth => {
            let mut wave = BezPath::new();
            wave.move_to((x + 1.0, cy));
            wave.curve_to((x + 4.0, cy - 9.0), (x + 7.0, cy - 9.0), (x + 9.0, cy));
            wave.curve_to((x + 11.0, cy + 9.0), (x + 14.0, cy + 9.0), (x + 17.0, cy));
            outline(scene, c, &wave);
        }
        Icon::Note => {
            solid(
                scene,
                c,
                &Ellipse::new((cx - 2.0, cy + 4.0), (3.4, 2.5), -0.4),
            );
            line(scene, (cx + 1.0, cy + 3.5), (cx + 1.0, y + 2.0), 1.3);
            line(scene, (cx + 1.0, y + 2.0), (cx + 5.0, y + 6.0), 1.3);
        }
    }
}

/// A chevron centred on (`x`, `y`): pointing right, or down when `open`.
fn draw_chevron(scene: &mut Scene, x: f64, y: f64, open: bool, color: Color) {
    let mut path = BezPath::new();
    if open {
        path.move_to((x - 4.5, y - 2.5));
        path.line_to((x, y + 2.5));
        path.line_to((x + 4.5, y - 2.5));
    } else {
        path.move_to((x - 2.5, y - 4.5));
        path.line_to((x + 2.5, y));
        path.line_to((x - 2.5, y + 4.5));
    }
    let stroke = Stroke::new(2.0)
        .with_caps(vello::kurbo::Cap::Round)
        .with_join(vello::kurbo::Join::Round);
    scene.stroke(&stroke, Affine::IDENTITY, color, None, &path);
}
/// One line unit of a row's height (see `PropRow::height`).
const PROPS_UNIT: f64 = 24.0;

/// `s` in small capitals starting at `x`: a word's first letter full size, the rest capitals at
/// four fifths of it. Returns where it ends.
#[allow(clippy::too_many_arguments)]
fn text_small_caps(
    scene: &mut Scene,
    s: &str,
    mut x: f64,
    y: f64,
    size: f64,
    color: Color,
    bold: bool,
) -> f64 {
    let fonts = cecilia_engrave::fonts::fonts();
    let font = if bold { FontKind::Bold } else { FontKind::Text };
    let mut start = true;
    for c in s.chars() {
        let (shown, sz) = if !start && c.is_lowercase() {
            (c.to_uppercase().to_string(), size * 0.8)
        } else {
            (c.to_string(), size)
        };
        text(scene, &shown, x, y, sz, Align::Left, color, bold);
        x += fonts.text_width(font, &shown, sz) + size * 0.04;
        start = !c.is_alphanumeric();
    }
    x
}

/// The editing palette: section titles with a chevron over grids of icon buttons wrapped to
/// the pane's width. The buttons in force are tinted with the accent; the highlighted button
/// (the mouse-over, which is also the keyboard selection) is a neutral gray.
fn draw_edit_palette(scene: &mut Scene, app: &App, views: &mut Views, r: Rect) {
    use cecilia_app::palette::{PaletteKind, PaletteLine};
    const PAD: f64 = 10.0;
    const CELL: f64 = 32.0;
    const HEAD: f64 = 30.0;
    let th = &app.config.theme;
    views.palette_rect = Some(r);
    fill(scene, r, col(th.panel));
    let cols = ((r.width() - 2.0 * PAD) / CELL).floor().max(1.0) as usize;
    app.set_palette_cols(cols);
    let rows = app.palette_rows();
    let lines = app.palette_lines();
    let height = |l: &PaletteLine| {
        if matches!(l, PaletteLine::Header(_)) {
            HEAD
        } else {
            CELL
        }
    };
    // As many lines as fit from the first one shown.
    let first = {
        let mut avg = 0usize;
        let mut room = r.height() - 2.0 * PAD;
        for l in &lines {
            if room < height(l) {
                break;
            }
            room -= height(l);
            avg += 1;
        }
        app.palette_window(avg.max(1))
    };
    let sel = app.palette_selected();
    let (hx, hy) = views.hover;
    // The one highlight shows while the pane has the focus or the mouse is over it.
    let show_sel = app.focused_kind() == PaneKind::Palette || r.contains((hx, hy));
    let mut hovered = None;
    let mut y = r.y0 + PAD;
    for line in lines.iter().skip(first) {
        if y + height(line) > r.y1 {
            break;
        }
        match line {
            PaletteLine::Header(i) => {
                let row = &rows[*i];
                let PaletteKind::Header { folded, .. } = row.kind else {
                    continue;
                };
                let b = Rect::new(r.x0 + PAD, y + 2.0, r.x1 - PAD, y + HEAD - 2.0);
                let shape = b.to_rounded_rect(5.0);
                if *i == sel && show_sel {
                    scene.fill(
                        Fill::NonZero,
                        Affine::IDENTITY,
                        col(th.button_hover),
                        None,
                        &shape,
                    );
                }
                let ty = b.y0 + (b.height()) / 2.0 + 5.0;
                let c = chrome_text();
                draw_chevron(
                    scene,
                    b.x0 + 10.0,
                    b.y0 + b.height() / 2.0 - 1.0,
                    !folded,
                    c,
                );
                text(
                    scene,
                    row.label,
                    b.x0 + 22.0,
                    ty,
                    13.0 * 1.15,
                    Align::Left,
                    c,
                    true,
                );
                // A rule after the title.
                let end = b.x0
                    + 22.0
                    + cecilia_engrave::fonts::fonts().text_width(
                        FontKind::Bold,
                        row.label,
                        13.0 * 1.15,
                    );
                fill(
                    scene,
                    Rect::new(end + 10.0, ty - 5.0, b.x1, ty - 4.0),
                    col(th.tab),
                );
                views.palette.push((Rect::new(r.x0, y, r.x1, y + HEAD), *i));
            }
            PaletteLine::Cells(cells) => {
                for (k, &i) in cells.iter().enumerate() {
                    let row = &rows[i];
                    let b = Rect::new(
                        r.x0 + PAD + k as f64 * CELL + 2.0,
                        y + 2.0,
                        r.x0 + PAD + (k + 1) as f64 * CELL - 2.0,
                        y + CELL - 2.0,
                    );
                    let shape = b.to_rounded_rect(5.0);
                    let over = b.contains((hx, hy));
                    let selected = i == sel && show_sel;
                    let off = row.unavailable.is_some();
                    if row.active && !off {
                        scene.fill(
                            Fill::NonZero,
                            Affine::IDENTITY,
                            col(th.accent.with_alpha(60)),
                            None,
                            &shape,
                        );
                    } else if selected {
                        scene.fill(
                            Fill::NonZero,
                            Affine::IDENTITY,
                            col(th.button_hover),
                            None,
                            &shape,
                        );
                    }
                    let color = if off {
                        col(th.dim.with_alpha(90))
                    } else if row.active {
                        col(th.text_on(th.accent.mix(th.panel, 0.6)))
                    } else {
                        chrome_text()
                    };
                    draw_palette_icon(scene, row.command, row.short, b.center(), color);
                    if over {
                        hovered = Some((i, b, row.name()));
                    }
                    views.palette.push((b, i));
                }
            }
        }
        y += height(line);
    }
    // The search field along the bottom of the pane, as in the properties pane.
    if let Some(search) = &app.palette.search {
        let b = Rect::new(r.x0 + 8.0, r.y1 - 40.0, r.x1 - 8.0, r.y1 - 8.0);
        let shape = b.to_rounded_rect(6.0);
        scene.fill(Fill::NonZero, Affine::IDENTITY, col(th.tab), None, &shape);
        scene.stroke(
            &Stroke::new(1.0),
            Affine::IDENTITY,
            col(th.mode_normal),
            None,
            &shape,
        );
        let line = format!("/{}", search.query);
        text(
            scene,
            &line,
            b.x0 + 10.0,
            b.y0 + 21.0,
            13.0,
            Align::Left,
            chrome_text(),
            false,
        );
        let cx = b.x0
            + 10.0
            + cecilia_engrave::fonts::fonts().text_width(FontKind::Text, &line, 13.0)
            + 1.0;
        fill(
            scene,
            Rect::new(cx, b.y0 + 8.0, cx + 2.0, b.y1 - 8.0),
            col(th.mode_normal),
        );
    }
    // The name of the highlighted button along the bottom of the pane.
    if let Some(row) = rows
        .get(sel)
        .filter(|r| show_sel && r.kind == PaletteKind::Item && app.palette.search.is_none())
    {
        let name = row.name();
        let w = cecilia_engrave::fonts::fonts().text_width(FontKind::Text, &name, 12.5) + 16.0;
        let x0 = r.x0 + ((r.width() - w) / 2.0).max(4.0);
        let b = Rect::new(x0, r.y1 - 32.0, (x0 + w).min(r.x1 - 4.0), r.y1 - 8.0);
        scene.fill(
            Fill::NonZero,
            Affine::IDENTITY,
            popover(),
            None,
            &b.to_rounded_rect(5.0),
        );
        text(
            scene,
            row.label,
            b.x0 + 8.0,
            b.y0 + 17.0,
            12.5,
            Align::Left,
            chrome_text(),
            false,
        );
    }
    // The name of the button the mouse rests on, after a moment (as for the toolbar).
    match (&hovered, views.palette_hover) {
        (Some((i, ..)), Some((j, _))) if *i == j => {}
        (Some((i, ..)), _) => views.palette_hover = Some((*i, std::time::Instant::now())),
        (None, _) => views.palette_hover = None,
    }
    if let (Some((_, b, name)), Some((_, since))) = (hovered, views.palette_hover) {
        if since.elapsed() >= TOOLTIP_DELAY {
            views.tooltip = Some((b, name));
        }
    }
}

/// An icon of the editing palette for its command, in a 16 px square around `c`.
/// Draws music-font glyphs (in a row, or stacked) centred on `c`, as large as fit the icon.
fn draw_icon_glyphs(
    scene: &mut Scene,
    glyphs: &[char],
    stacked: bool,
    c: vello::kurbo::Point,
    color: Color,
) {
    use cecilia_engrave::display::FontKind;
    let fonts = cecilia_engrave::fonts::fonts();
    const EM: f64 = 100.0;
    // Where each glyph's origin goes, at EM, and the box they fill together.
    let mut origins = Vec::new();
    let mut at = 0.0;
    let mut bounds: Option<Rect> = None;
    for (i, &g) in glyphs.iter().enumerate() {
        let b = fonts.bbox(FontKind::Music, g, EM);
        let o = if stacked {
            (
                0.0,
                (i as f64 - (glyphs.len() as f64 - 1.0) / 2.0) * (b.height() + 4.0),
            )
        } else {
            (at, 0.0)
        };
        let moved = Rect::new(b.x0 + o.0, b.y0 + o.1, b.x1 + o.0, b.y1 + o.1);
        bounds = Some(bounds.map_or(moved, |r| r.union(moved)));
        origins.push(o);
        at += fonts.advance(FontKind::Music, g, EM);
    }
    let Some(b) = bounds.filter(|b| b.width() > 0.0 && b.height() > 0.0) else {
        return;
    };
    // The icon is about 17 x 16 px.
    let k = (17.0 / b.width()).min(16.0 / b.height()).min(0.3);
    let (cx, cy) = ((b.x0 + b.x1) / 2.0, (b.y0 + b.y1) / 2.0);
    for (&g, o) in glyphs.iter().zip(origins) {
        let x = c.x + (o.0 - cx) * k;
        let y = c.y + (o.1 - cy) * k;
        if let Some(p) = fonts.glyph_path(FontKind::Music, g, x, y, EM * k) {
            scene.fill(Fill::NonZero, Affine::IDENTITY, color, None, &p);
        }
    }
}

fn draw_palette_icon(
    scene: &mut Scene,
    command: &str,
    short: &str,
    c: vello::kurbo::Point,
    color: Color,
) {
    use vello::kurbo::{Cap, Ellipse, Join};
    let (x, y) = (c.x - 8.0, c.y - 8.0);
    let stroke = |w: f64| Stroke::new(w).with_caps(Cap::Round).with_join(Join::Round);
    let path = |points: &[(f64, f64)], close: bool| {
        let mut p = BezPath::new();
        for (k, &(px, py)) in points.iter().enumerate() {
            if k == 0 {
                p.move_to((x + px, y + py));
            } else {
                p.line_to((x + px, y + py));
            }
        }
        if close {
            p.close_path();
        }
        p
    };
    let line = |scene: &mut Scene, p: &BezPath, w: f64| {
        scene.stroke(&stroke(w), Affine::IDENTITY, color, None, p);
    };
    let label = |scene: &mut Scene, s: &str, size: f64| {
        text(
            scene,
            s,
            c.x,
            c.y + size * 0.36,
            size,
            Align::Center,
            color,
            true,
        );
    };
    // A note head (filled or hollow) with its stem and `flags` flags.
    let note = |scene: &mut Scene, filled: bool, stem: bool, flags: usize, dx: f64| {
        let head = Ellipse::new((x + 6.0 + dx, y + 12.0), (3.4, 2.5), -0.4);
        if filled {
            scene.fill(Fill::NonZero, Affine::IDENTITY, color, None, &head);
        } else {
            scene.stroke(&stroke(1.3), Affine::IDENTITY, color, None, &head);
        }
        if stem {
            line(
                scene,
                &path(&[(9.1 + dx, 11.6), (9.1 + dx, 1.5)], false),
                1.3,
            );
            for k in 0..flags {
                let fy = 1.5 + k as f64 * 2.4;
                line(
                    scene,
                    &path(
                        &[(9.1 + dx, fy), (11.5 + dx, fy + 2.0), (12.6 + dx, fy + 4.4)],
                        false,
                    ),
                    1.3,
                );
            }
        }
    };
    let arrow = |scene: &mut Scene, from: (f64, f64), to: (f64, f64)| {
        line(scene, &path(&[from, to], false), 1.4);
        let (dx, dy) = (to.0 - from.0, to.1 - from.1);
        let n = (dx * dx + dy * dy).sqrt().max(0.1);
        let (ux, uy) = (dx / n, dy / n);
        let head = path(
            &[
                (to.0 - 3.2 * ux + 2.0 * uy, to.1 - 3.2 * uy - 2.0 * ux),
                to,
                (to.0 - 3.2 * ux - 2.0 * uy, to.1 - 3.2 * uy + 2.0 * ux),
            ],
            false,
        );
        line(scene, &head, 1.4);
    };
    let zigzag = |scene: &mut Scene, amp: f64| {
        let pts: Vec<(f64, f64)> = (0..7)
            .map(|k| {
                (
                    1.0 + k as f64 * 2.3,
                    8.0 + if k % 2 == 0 { amp } else { -amp },
                )
            })
            .collect();
        line(scene, &path(&pts, false), 1.3);
    };
    let (name, arg) = command
        .trim_end()
        .split_once(' ')
        .unwrap_or((command.trim_end(), ""));
    // Symbols the engraver draws with a music-font glyph use that same glyph.
    match cecilia_engrave::symbols::palette_icon(command) {
        cecilia_engrave::symbols::Icon::Row(glyphs) => {
            return draw_icon_glyphs(scene, glyphs, false, c, color);
        }
        cecilia_engrave::symbols::Icon::Stack(glyphs) => {
            return draw_icon_glyphs(scene, glyphs, true, c, color);
        }
        cecilia_engrave::symbols::Icon::Text => {
            return label(
                scene,
                short,
                if short.chars().count() > 2 { 8.0 } else { 10.0 },
            );
        }
        cecilia_engrave::symbols::Icon::Drawn => {}
    }
    match (name, arg) {
        // Beams: two eighths, as the engraver beams them (the mode shows in the gap).
        ("beams", mode) => {
            let (xs, flags): (&[f64], bool) = match mode {
                "break" => (&[3.5, 11.5], true),
                "force" => (&[2.0, 6.5, 11.0, 15.0], false),
                _ => (&[4.5, 11.5], false),
            };
            for &sx in xs {
                let head = Ellipse::new((x + sx - 1.2, y + 13.0), (2.4, 1.8), -0.4);
                scene.fill(Fill::NonZero, Affine::IDENTITY, color, None, &head);
                line(
                    scene,
                    &path(&[(sx + 0.9, 12.5), (sx + 0.9, 3.0)], false),
                    1.1,
                );
                if flags {
                    line(
                        scene,
                        &path(&[(sx + 0.9, 3.0), (sx + 3.5, 6.0)], false),
                        1.1,
                    );
                }
            }
            if !flags {
                let (a, b) = (xs[0] + 0.9, xs[xs.len() - 1] + 0.9);
                line(scene, &path(&[(a, 3.2), (b, 3.2)], false), 2.0);
                if mode == "break-secondary" {
                    line(scene, &path(&[(a, 6.0), (a + 2.5, 6.0)], false), 1.6);
                    line(scene, &path(&[(b - 2.5, 6.0), (b, 6.0)], false), 1.6);
                } else if mode == "auto" {
                    label(scene, "A", 6.0);
                }
            }
        }
        // Stems: a quarter note with its stem up or down (both for automatic).
        ("stems", dir) => {
            let one = |scene: &mut Scene, hx: f64, up: bool| {
                let head = Ellipse::new((x + hx, y + 8.0), (2.8, 2.0), -0.4);
                scene.fill(Fill::NonZero, Affine::IDENTITY, color, None, &head);
                let (sx, a, b) = if up {
                    (hx + 2.5, 7.5, -0.5)
                } else {
                    (hx - 2.5, 8.5, 16.5)
                };
                line(scene, &path(&[(sx, a), (sx, b)], false), 1.2);
            };
            match dir {
                "up" => one(scene, 7.0, true),
                "down" => one(scene, 9.0, false),
                _ => {
                    one(scene, 4.5, true);
                    one(scene, 11.5, false);
                }
            }
        }
        ("duration", "1") => note(scene, false, false, 0, 0.0),
        ("double-dot", _) => {
            note(scene, true, true, 0, -3.0);
            for dx in [11.0, 14.0] {
                let d = Circle::new((x + dx, y + 12.0), 1.2);
                scene.fill(Fill::NonZero, Affine::IDENTITY, color, None, &d);
            }
        }
        ("toggle", "crescendo") => {
            line(scene, &path(&[(2.0, 8.0), (14.0, 3.0)], false), 1.5);
            line(scene, &path(&[(2.0, 8.0), (14.0, 13.0)], false), 1.5);
        }
        ("toggle", "decrescendo") => {
            line(scene, &path(&[(2.0, 3.0), (14.0, 8.0)], false), 1.5);
            line(scene, &path(&[(2.0, 13.0), (14.0, 8.0)], false), 1.5);
        }
        // The fades are filled shapes (the hairpins are only outlined).
        ("toggle", "fade-in") => {
            let p = path(&[(2.0, 8.0), (14.0, 3.5), (14.0, 12.5)], true);
            scene.fill(Fill::NonZero, Affine::IDENTITY, color, None, &p);
        }
        ("toggle", "fade-out") => {
            let p = path(&[(2.0, 3.5), (2.0, 12.5), (14.0, 8.0)], true);
            scene.fill(Fill::NonZero, Affine::IDENTITY, color, None, &p);
        }
        ("toggle", "volume-swell") => {
            let p = path(&[(1.0, 8.0), (8.0, 3.5), (15.0, 8.0), (8.0, 12.5)], true);
            scene.fill(Fill::NonZero, Affine::IDENTITY, color, None, &p);
        }
        ("toggle", "fermata") => {
            let mut p = BezPath::new();
            p.move_to((x + 2.0, y + 11.0));
            p.curve_to(
                (x + 2.0, y + 1.0),
                (x + 14.0, y + 1.0),
                (x + 14.0, y + 11.0),
            );
            line(scene, &p, 1.5);
            let d = Circle::new((x + 8.0, y + 10.0), 1.4);
            scene.fill(Fill::NonZero, Affine::IDENTITY, color, None, &d);
        }
        ("duration", "2") => note(scene, false, true, 0, 0.0),
        ("duration", "4") => note(scene, true, true, 0, 0.0),
        ("duration", "8") => note(scene, true, true, 1, 0.0),
        ("duration", "16") => note(scene, true, true, 2, 0.0),
        ("duration", "32") => note(scene, true, true, 3, -1.0),
        ("duration", _) => note(scene, true, true, 4, -1.5),
        ("dot", _) => {
            note(scene, true, true, 0, -2.0);
            let d = Circle::new((x + 13.0, y + 12.0), 1.4);
            scene.fill(Fill::NonZero, Affine::IDENTITY, color, None, &d);
        }
        ("tuplet", _) => {
            label(scene, "3", 11.0);
            line(
                scene,
                &path(
                    &[(1.5, 13.0), (1.5, 15.0), (14.5, 15.0), (14.5, 13.0)],
                    false,
                ),
                1.0,
            );
        }
        ("rest", _) => {
            line(
                scene,
                &path(
                    &[
                        (5.0, 2.0),
                        (10.0, 6.0),
                        (6.5, 9.0),
                        (10.5, 12.5),
                        (8.0, 15.0),
                    ],
                    false,
                ),
                1.6,
            );
        }
        ("accidental", "sharp") => {
            line(scene, &path(&[(6.0, 2.0), (6.0, 14.0)], false), 1.2);
            line(scene, &path(&[(10.0, 1.5), (10.0, 13.5)], false), 1.2);
            line(scene, &path(&[(3.5, 7.0), (12.5, 5.0)], false), 2.0);
            line(scene, &path(&[(3.5, 11.0), (12.5, 9.0)], false), 2.0);
        }
        ("accidental", "flat") => {
            line(scene, &path(&[(5.5, 1.5), (5.5, 14.0)], false), 1.3);
            let mut p = BezPath::new();
            p.move_to((x + 5.5, y + 14.0));
            p.curve_to((x + 15.0, y + 9.0), (x + 12.0, y + 4.5), (x + 5.5, y + 9.0));
            line(scene, &p, 1.4);
        }
        ("accidental", _) => {
            line(scene, &path(&[(5.0, 2.0), (5.0, 11.0)], false), 1.2);
            line(scene, &path(&[(11.0, 5.0), (11.0, 14.0)], false), 1.2);
            line(scene, &path(&[(5.0, 7.0), (11.0, 5.0)], false), 1.6);
            line(scene, &path(&[(5.0, 11.0), (11.0, 9.0)], false), 1.6);
        }
        ("octave-up", _) => {
            label(scene, "8", 11.0);
            arrow(scene, (13.0, 6.0), (13.0, 1.5));
        }
        ("octave-down", _) => {
            label(scene, "8", 11.0);
            arrow(scene, (13.0, 10.0), (13.0, 14.5));
        }
        ("dynamic", d) => label(scene, d, if d.len() > 2 { 12.5 } else { 15.0 }),
        ("toggle", "tie") => {
            let mut p = BezPath::new();
            p.move_to((x + 1.5, y + 6.0));
            p.quad_to((x + 8.0, y + 14.0), (x + 14.5, y + 6.0));
            line(scene, &p, 1.5);
        }
        ("toggle", "dead") => {
            line(scene, &path(&[(3.5, 3.5), (12.5, 12.5)], false), 1.8);
            line(scene, &path(&[(12.5, 3.5), (3.5, 12.5)], false), 1.8);
        }
        ("toggle", "ghost") => {
            for (sx, dir) in [(4.0, -1.0), (12.0, 1.0)] {
                let mut p = BezPath::new();
                p.move_to((x + sx, y + 2.0));
                p.quad_to((x + sx + dir * -3.0, y + 8.0), (x + sx, y + 14.0));
                line(scene, &p, 1.4);
            }
            let d = Circle::new((x + 8.0, y + 8.0), 2.0);
            scene.fill(Fill::NonZero, Affine::IDENTITY, color, None, &d);
        }
        ("toggle", "accent") => line(
            scene,
            &path(&[(3.0, 4.0), (13.0, 8.0), (3.0, 12.0)], false),
            1.6,
        ),
        ("toggle", "heavy-accent") => line(
            scene,
            &path(&[(3.0, 13.0), (8.0, 3.0), (13.0, 13.0)], false),
            1.6,
        ),
        ("toggle", "staccato") => {
            let d = Circle::new((x + 8.0, y + 8.0), 2.0);
            scene.fill(Fill::NonZero, Affine::IDENTITY, color, None, &d);
        }
        ("toggle", "let-ring") => {
            text(
                scene,
                "let",
                c.x,
                c.y - 0.5,
                8.5,
                Align::Center,
                color,
                true,
            );
            text(
                scene,
                "ring",
                c.x,
                c.y + 8.0,
                8.5,
                Align::Center,
                color,
                true,
            );
        }
        ("toggle", "palm-mute") => label(scene, "P.M.", 8.5),
        ("toggle", "hammer") => label(scene, "H/P", 8.5),
        ("toggle", "slide") => {
            arrow(scene, (2.5, 13.0), (13.5, 3.5));
        }
        ("toggle", "bend") => {
            let mut p = BezPath::new();
            p.move_to((x + 2.5, y + 14.0));
            p.curve_to(
                (x + 9.0, y + 14.0),
                (x + 12.0, y + 9.0),
                (x + 12.0, y + 3.5),
            );
            line(scene, &p, 1.4);
            line(
                scene,
                &path(&[(9.0, 6.0), (12.0, 2.5), (15.0, 6.0)], false),
                1.4,
            );
        }
        ("toggle", "half-bend") => {
            let mut p = BezPath::new();
            p.move_to((x + 2.5, y + 14.0));
            p.curve_to(
                (x + 8.0, y + 14.0),
                (x + 10.0, y + 11.0),
                (x + 10.0, y + 7.0),
            );
            line(scene, &p, 1.4);
            line(
                scene,
                &path(&[(7.5, 8.5), (10.0, 5.5), (12.5, 8.5)], false),
                1.4,
            );
            text(
                scene,
                "½",
                x + 13.5,
                y + 14.0,
                7.0,
                Align::Center,
                color,
                true,
            );
        }
        ("toggle", "vibrato") => zigzag(scene, 2.0),
        ("toggle", "vibrato-wide") => zigzag(scene, 4.0),
        ("toggle", "harmonic") => line(
            scene,
            &path(&[(8.0, 2.0), (14.0, 8.0), (8.0, 14.0), (2.0, 8.0)], true),
            1.4,
        ),
        ("toggle", "tremolo") => {
            line(scene, &path(&[(8.0, 1.5), (8.0, 15.0)], false), 1.3);
            for k in 0..3 {
                let ty = 3.5 + k as f64 * 2.6;
                line(
                    scene,
                    &path(&[(4.5, ty + 2.0), (11.5, ty - 1.0)], false),
                    1.5,
                );
            }
        }
        ("toggle", "tap") => label(scene, "tap", 9.0),
        ("toggle", "slap") => label(scene, "slap", 9.0),
        ("toggle", "pop") => label(scene, "pop", 9.0),
        ("toggle", "stroke-down") => arrow(scene, (8.0, 2.0), (8.0, 14.0)),
        ("toggle", "stroke-up") => arrow(scene, (8.0, 14.0), (8.0, 2.0)),
        // The short label the terminal uses.
        _ => label(
            scene,
            short,
            if short.chars().count() > 2 { 8.0 } else { 10.0 },
        ),
    }
}

/// The properties pane: its tabs, then the shown tab's rows (section titles, and per property
/// its label above a box holding the value), with room around everything.
fn draw_properties(scene: &mut Scene, app: &App, views: &mut Views, r: Rect) {
    use cecilia_app::properties::{Field, PropTab, RowKind, caret_in, wrap_ranges};
    const PAD: f64 = 16.0;
    const BOX_H: f64 = 36.0;
    let th = &app.config.theme;
    let fonts = cecilia_engrave::fonts::fonts();
    let width_of = |s: &str, size: f64| fonts.text_width(FontKind::Text, s, size);
    views.props_rect = Some(r);
    fill(scene, r, col(th.panel));
    // Tabs.
    let tab_size = PROPS_VALUE;
    let mut x = r.x0 + PAD;
    let tab_y = r.y0 + 14.0;
    for tab in PropTab::ALL {
        let label = tab.title();
        let w = fonts.text_width(FontKind::Bold, &label, tab_size) + 20.0;
        let b = Rect::new(x, tab_y, x + w, tab_y + 36.0);
        let current = tab == app.props.tab;
        if current {
            scene.fill(
                Fill::NonZero,
                Affine::IDENTITY,
                col(th.mode_normal),
                None,
                &b.to_rounded_rect(6.0),
            );
        }
        let c = if current {
            text_on(col(th.mode_normal))
        } else {
            dim()
        };
        text(
            scene,
            &label,
            x + w / 2.0,
            b.y0 + 25.0,
            tab_size,
            Align::Center,
            c,
            true,
        );
        views.props.push((b, PropHit::Tab(tab)));
        x += w + 6.0;
    }
    let top = tab_y + 36.0 + 14.0;
    let bottom = r.y1 - 12.0;
    let rows = app.property_rows();
    let first = app.property_window(((bottom - top) / PROPS_UNIT).floor().max(1.0) as usize);
    let (bx0, bx1) = (r.x0 + PAD, r.x1 - PAD);
    let mut y = top;
    // The staff rows' boxes: row, left, top, bottom, and whether a label sits above.
    let mut staff_rows: Vec<(usize, f64, f64, f64, bool)> = Vec::new();
    for (k, row) in rows.iter().enumerate().skip(first) {
        if y >= bottom {
            break;
        }
        let h = row.height() as f64 * PROPS_UNIT;
        let sel = app.property_row_selected(k);
        let staff_row = app.property_is_staff(k);
        let box_sel = if staff_row {
            app.property_box_selected(k, 0)
        } else {
            k == app.property_selected()
        };
        let mut hit = Rect::new(r.x0, y, r.x1, (y + h).min(bottom));
        match &row.kind {
            RowKind::Header { folded, .. } => {
                let c = col(if sel { th.mode_normal } else { th.chrome_text });
                let size = PROPS_SECTION;
                let by = y + h - 8.0;
                draw_chevron(scene, bx0 + 6.0, by - size * 0.3, !*folded, c);
                let end = text_small_caps(scene, &row.label, bx0 + 20.0, by, size, c, true);
                // A rule after the title.
                fill(
                    scene,
                    Rect::new(end + 10.0, by - size * 0.3, bx1, by - size * 0.3 + 1.0),
                    col(th.tab),
                );
            }
            RowKind::Note => text(
                scene,
                &row.label,
                bx0,
                y + 20.0,
                PROPS_KEY,
                Align::Left,
                dim(),
                false,
            ),
            RowKind::Field { field, editable } => {
                let typing = app.props.edit.as_ref().filter(|e| e.field == *field);
                let label_c = if sel { col(th.mode_normal) } else { dim() };
                // A staff after the first has no label: its box takes the label's place.
                let labelled = !row.label.is_empty();
                if labelled {
                    text(
                        scene,
                        &row.label,
                        bx0,
                        y + 18.0,
                        PROPS_KEY,
                        Align::Left,
                        label_c,
                        sel,
                    );
                }
                let area = *field == Field::Lyrics;
                if area {
                    // The text area fills the pane down to its end.
                    hit.y1 = bottom;
                }
                let by0 = if labelled { y + PROPS_UNIT } else { y };
                // Staff fields have a drag handle before them (the empty one, a gap).
                let staff = matches!(field, Field::StaffAt(_));
                let box_x0 = if staff { bx0 + 22.0 } else { bx0 };
                if staff && !row.value.is_empty() {
                    for dy in [-6.0, 0.0, 6.0] {
                        for dx in [-3.0, 3.0] {
                            let dot = Circle::new((bx0 + 9.0 + dx, by0 + BOX_H / 2.0 + dy), 1.6);
                            scene.fill(Fill::NonZero, Affine::IDENTITY, dim(), None, &dot);
                        }
                    }
                }
                if staff {
                    staff_rows.push((k, box_x0, by0, by0 + BOX_H, labelled));
                }
                let by1 = if area { bottom } else { by0 + BOX_H };
                // A property with text beside it (a color's code, a tuning's pitches) has a
                // narrower box, the text in grey to its right.
                let toggle = matches!(field, Field::Mute | Field::Solo);
                let box_x1 = if toggle {
                    box_x0 + 52.0
                } else if staff {
                    // Two boxes: the notation, and the clef.
                    box_x0 + (bx1 - box_x0 - 8.0) * 0.5
                } else if row.aside.is_empty() {
                    bx1
                } else {
                    let aside_w = width_of(&row.aside, PROPS_VALUE);
                    (bx1 - aside_w - 14.0).min(box_x0 + (bx1 - box_x0) * 0.62)
                };
                let on = row.value == "on";
                // A toggle is a switch: a pill with a knob, and on/off printed beside it.
                let b = if toggle {
                    Rect::new(box_x0, by0 + 6.0, box_x1, by0 + 30.0)
                } else {
                    Rect::new(box_x0, by0, box_x1, by1)
                };
                let shape = b.to_rounded_rect(if toggle { 12.0 } else { 6.0 });
                let track = if toggle && on { th.mode_normal } else { th.tab };
                scene.fill(Fill::NonZero, Affine::IDENTITY, col(track), None, &shape);
                if toggle {
                    let kx = if on { b.x1 - 12.0 } else { b.x0 + 12.0 };
                    let knob = Circle::new((kx, b.y0 + 12.0), 9.0);
                    let kc = if on {
                        text_on(col(th.mode_normal))
                    } else {
                        dim()
                    };
                    scene.fill(Fill::NonZero, Affine::IDENTITY, kc, None, &knob);
                    let ty = b.y0 + 12.0 + PROPS_VALUE * 0.35;
                    let word = if on { "on" } else { "off" };
                    text(
                        scene,
                        word,
                        b.x1 + 12.0,
                        ty,
                        PROPS_VALUE,
                        Align::Left,
                        chrome_text(),
                        false,
                    );
                }
                if box_sel {
                    let a = if typing.is_some() { 80 } else { 40 };
                    scene.fill(
                        Fill::NonZero,
                        Affine::IDENTITY,
                        col(th.mode_normal.with_alpha(a)),
                        None,
                        &shape,
                    );
                    scene.stroke(
                        &Stroke::new(1.0),
                        Affine::IDENTITY,
                        col(th.mode_normal),
                        None,
                        &shape,
                    );
                }
                let c = if *editable { chrome_text() } else { dim() };
                let size = PROPS_VALUE;
                let mut tx = box_x0 + 10.0;
                let inner = box_x1 - box_x0 - 20.0;
                if let Some(c) = row.swatch {
                    let sw = RoundedRect::new(tx, by0 + 9.0, tx + 18.0, by0 + BOX_H - 9.0, 4.0);
                    scene.fill(Fill::NonZero, Affine::IDENTITY, col(c), None, &sw);
                    scene.stroke(&Stroke::new(1.0), Affine::IDENTITY, dim(), None, &sw);
                    tx += 28.0;
                }
                if let Some(icon) = row.icon {
                    draw_icon(scene, icon, tx, by0 + (BOX_H - 18.0) / 2.0, c);
                    tx += 28.0;
                }
                if !row.aside.is_empty() {
                    let ay = by0 + BOX_H / 2.0 + size * 0.35;
                    text(scene, &row.aside, bx1, ay, size, Align::Right, dim(), false);
                }
                if area {
                    let value = typing.map_or(row.value.as_str(), |e| e.text.as_str());
                    let lines = wrap_ranges(value, |l| width_of(l, size) <= inner);
                    *app.props.visual.borrow_mut() = lines.clone();
                    let chars: Vec<char> = value.chars().collect();
                    let line_h = size * 1.35;
                    let shown = (((by1 - by0 - 14.0) / line_h).floor().max(1.0)) as usize;
                    let caret = typing.map(|e| caret_in(&lines, e.caret));
                    let skip = app.text_window(
                        typing.zip(caret).map(|(e, (r, _))| (e.caret, r)),
                        lines.len(),
                        shown,
                    );
                    views.props_text = Some(PropsText {
                        area: b,
                        x: tx,
                        top: by0 + 7.0,
                        line_h,
                        size,
                        skip,
                        lines: lines.clone(),
                        chars: chars.clone(),
                    });
                    // The selection, behind the text.
                    if let Some((a, z)) = typing.and_then(|e| e.selection()) {
                        for (n, &(s, e)) in lines.iter().enumerate().skip(skip).take(shown) {
                            let (lo, hi) = (a.max(s), z.min(e));
                            if lo < hi || (a < s && z > e && s == e) {
                                let before: String = chars[s..lo.max(s)].iter().collect();
                                let inside: String = chars[lo.max(s)..hi.max(lo)].iter().collect();
                                let x0 = tx + width_of(&before, size);
                                // An empty line inside the selection shows a sliver.
                                let w = width_of(&inside, size).max(if s == e { 6.0 } else { 0.0 });
                                let ly = by0 + 7.0 + line_h * (n - skip) as f64;
                                fill(
                                    scene,
                                    Rect::new(x0, ly + 2.0, x0 + w, ly + size + 4.0),
                                    col(th.mode_normal.with_alpha(110)),
                                );
                            }
                        }
                    }
                    for (n, &(s, e)) in lines.iter().enumerate().skip(skip).take(shown) {
                        let line: String = chars[s..e].iter().collect();
                        let ly = by0 + 7.0 + line_h * (n - skip) as f64 + size;
                        text(scene, &line, tx, ly, size, Align::Left, c, false);
                    }
                    if let Some((cr, cc)) = caret.filter(|&(r, _)| r >= skip) {
                        let (s, _) = lines[cr];
                        let before: String = chars[s..s + cc].iter().collect();
                        let cx = tx + width_of(&before, size);
                        let ly = by0 + 7.0 + line_h * (cr - skip) as f64;
                        // Normal mode: a block over the character; insert mode: a bar.
                        let wide = match typing {
                            Some(e) if e.modal && !e.insert => {
                                let ch = chars.get(s + cc).filter(|c| **c != '\n');
                                ch.map_or(8.0, |c| width_of(&c.to_string(), size))
                            }
                            _ => 2.0,
                        };
                        let a = if wide > 2.0 { 150 } else { 255 };
                        fill(
                            scene,
                            Rect::new(cx, ly + 2.0, cx + wide, ly + size + 4.0),
                            col(th.mode_normal.with_alpha(a)),
                        );
                    }
                } else {
                    let ty = by0 + BOX_H / 2.0 + size * 0.35;
                    match typing {
                        Some(e) => {
                            // Start far enough in that the caret stays in the box.
                            let chars: Vec<char> = e.text.chars().collect();
                            let mut from = 0;
                            let upto = e.caret.min(chars.len());
                            while from < upto {
                                let seg: String = chars[from..upto].iter().collect();
                                if width_of(&seg, size) <= inner - 4.0 {
                                    break;
                                }
                                from += 1;
                            }
                            let mut shown = String::new();
                            for &ch in &chars[from..] {
                                shown.push(ch);
                                if width_of(&shown, size) > inner {
                                    shown.pop();
                                    break;
                                }
                            }
                            text(scene, &shown, tx, ty, size, Align::Left, c, false);
                            let before: String = chars[from..upto].iter().collect();
                            let cx = tx + width_of(&before, size);
                            fill(
                                scene,
                                Rect::new(cx, by0 + 7.0, cx + 2.0, by1 - 7.0),
                                col(th.mode_normal),
                            );
                        }
                        None => {
                            let mut shown: String = if toggle {
                                String::new()
                            } else {
                                row.value.clone()
                            };
                            if shown.is_empty() && !row.placeholder.is_empty() {
                                text(
                                    scene,
                                    row.placeholder,
                                    tx,
                                    ty,
                                    size,
                                    Align::Left,
                                    dim(),
                                    false,
                                );
                            }
                            if width_of(&shown, size) > inner {
                                while width_of(&format!("{shown}…"), size) > inner
                                    && !shown.is_empty()
                                {
                                    shown.pop();
                                }
                                shown.push('…');
                            }
                            text(scene, &shown, tx, ty, size, Align::Left, c, false);
                        }
                    }
                }
                if staff {
                    // The clef box beside the notation box (dim and empty for tab and slash);
                    // each box takes its own clicks.
                    let left = Rect::new(box_x0, by0, box_x1, by1);
                    let mut boxes = vec![(left, 0u8)];
                    if !row.value.is_empty() {
                        let b2 = Rect::new(box_x1 + 8.0, by0, bx1, by1);
                        let shape = b2.to_rounded_rect(6.0);
                        let has = row.icon2.is_some();
                        let fill_c = if has { th.tab } else { th.tab.with_alpha(110) };
                        scene.fill(Fill::NonZero, Affine::IDENTITY, col(fill_c), None, &shape);
                        if app.property_box_selected(k, 1) {
                            scene.fill(
                                Fill::NonZero,
                                Affine::IDENTITY,
                                col(th.mode_normal.with_alpha(40)),
                                None,
                                &shape,
                            );
                            scene.stroke(
                                &Stroke::new(1.0),
                                Affine::IDENTITY,
                                col(th.mode_normal),
                                None,
                                &shape,
                            );
                        }
                        let mut tx = b2.x0 + 10.0;
                        if let Some(icon) = row.icon2 {
                            draw_icon(scene, icon, tx, by0 + (BOX_H - 18.0) / 2.0, c);
                            tx += 28.0;
                        }
                        let ty = by0 + BOX_H / 2.0 + size * 0.35;
                        text(scene, &row.value2, tx, ty, size, Align::Left, c, false);
                        boxes.push((b2, 1));
                    }
                    for (b, n) in boxes {
                        let b = Rect::new(b.x0, hit.y0.max(b.y0), b.x1, hit.y1.min(b.y1));
                        views.props.push((b, PropHit::StaffBox(k, n)));
                    }
                }
            }
        }
        views.props.push((hit, PropHit::Row(k)));
        y += h;
    }
    // The search field along the bottom of the pane.
    if let Some(search) = &app.props.search {
        let b = Rect::new(bx0, r.y1 - 52.0, bx1, r.y1 - 14.0);
        let shape = b.to_rounded_rect(6.0);
        scene.fill(Fill::NonZero, Affine::IDENTITY, col(th.tab), None, &shape);
        scene.stroke(
            &Stroke::new(1.0),
            Affine::IDENTITY,
            col(th.mode_normal),
            None,
            &shape,
        );
        let line = format!("/{}", search.query);
        let ty = b.y0 + 19.0 + PROPS_VALUE * 0.35;
        text(
            scene,
            &line,
            b.x0 + 10.0,
            ty,
            PROPS_VALUE,
            Align::Left,
            chrome_text(),
            false,
        );
        let cx = b.x0 + 10.0 + width_of(&line, PROPS_VALUE) + 1.0;
        fill(
            scene,
            Rect::new(cx, b.y0 + 9.0, cx + 2.0, b.y1 - 9.0),
            col(th.mode_normal),
        );
    }
    // While a staff is dragged: a line in the accent color where it would land.
    if let Some(Drag::Prop(from, to, _)) = views.drag {
        if from != to {
            if let Some(&(_, x0, top, bottom, labelled)) = staff_rows.iter().find(|r| r.0 == to) {
                let ly = if to > from { bottom + 6.0 } else { top - 6.0 };
                let x0 = if labelled && to < from { x0 + 44.0 } else { x0 };
                fill(
                    scene,
                    Rect::new(x0, ly - 1.5, bx1, ly + 1.5),
                    col(th.accent),
                );
            }
        }
    }
}

/// The tuner: the note heard (big), a cents scale with a needle (green when in tune), the
/// frequency, and the nearest string of the track's tuning.
fn draw_tuner(scene: &mut Scene, app: &App, r: Rect) {
    let th = &app.config.theme;
    fill(scene, r, col(th.panel));
    fill(scene, Rect::new(r.x0, r.y0, r.x1, r.y0 + 1.0), col(th.tab));
    let view = app.tuner_view();
    let dim = col(th.dim);
    // The note.
    match &view {
        Some(v) => {
            text(
                scene,
                v.note,
                r.x0 + 44.0,
                r.y0 + 48.0,
                34.0,
                Align::Center,
                col(th.chrome_text),
                true,
            );
            text(
                scene,
                &v.octave.to_string(),
                r.x0 + 72.0,
                r.y0 + 56.0,
                14.0,
                Align::Left,
                dim,
                false,
            );
        }
        None => {
            let msg = match &app.tuner_error {
                Some(e) => format!("Tuner: {e}"),
                None => "Play a note…".into(),
            };
            text(
                scene,
                &msg,
                r.x0 + 16.0,
                r.y0 + 44.0,
                13.0,
                Align::Left,
                dim,
                false,
            );
        }
    }
    // The scale, −50 to +50 cents.
    let (x0, x1) = (r.x0 + 180.0, (r.x1 - 240.0).max(r.x0 + 300.0));
    let mid = (x0 + x1) / 2.0;
    let y = r.y0 + 42.0;
    let x_of = |c: f32| mid + (c.clamp(-50.0, 50.0) as f64 / 50.0) * (x1 - x0) / 2.0;
    fill(
        scene,
        Rect::new(x0, y - 0.5, x1, y + 0.5),
        col(th.dim.with_alpha(140)),
    );
    for c in (-50..=50).step_by(10) {
        let x = x_of(c as f32);
        let h = if c == 0 {
            14.0
        } else if c % 50 == 0 {
            10.0
        } else {
            6.0
        };
        fill(
            scene,
            Rect::new(x - 0.6, y - h, x + 0.6, y + h),
            col(th.dim),
        );
        if c % 50 == 0 {
            let label = if c > 0 {
                format!("+{c}")
            } else {
                c.to_string()
            };
            text(
                scene,
                &label,
                x,
                r.y1 - 6.0,
                10.0,
                Align::Center,
                dim,
                false,
            );
        }
    }
    // The in-tune zone, and the needle.
    let good = th.mode_command;
    scene.fill(
        Fill::NonZero,
        Affine::IDENTITY,
        col(good.with_alpha(45)),
        None,
        &RoundedRect::new(x_of(-3.0), y - 16.0, x_of(3.0), y + 16.0, 3.0),
    );
    if let Some(v) = &view {
        let off = v.cents.abs();
        let c = if off < 3.0 {
            good
        } else if off < 15.0 {
            th.accent
        } else {
            th.playhead
        };
        let x = x_of(v.cents);
        fill(
            scene,
            Rect::new(x - 1.5, y - 20.0, x + 1.5, y + 20.0),
            col(c),
        );
        scene.fill(
            Fill::NonZero,
            Affine::IDENTITY,
            col(c),
            None,
            &Circle::new((x, y), 5.0),
        );
        let sign = if v.cents >= 0.0 { "+" } else { "−" };
        text(
            scene,
            &format!("{sign}{:.0} ¢", v.cents.abs()),
            x,
            r.y0 + 16.0,
            12.0,
            Align::Center,
            col(c),
            true,
        );
        // Frequency and the nearest string.
        let rx = r.x1 - 16.0;
        text(
            scene,
            &format!("{:.1} Hz", v.freq),
            rx,
            r.y0 + 30.0,
            14.0,
            Align::Right,
            col(th.chrome_text),
            false,
        );
        if let Some((s, name, octave, cents)) = v.string {
            let sign = if cents >= 0.0 { "+" } else { "−" };
            text(
                scene,
                &format!(
                    "string {} · {name}{octave} {sign}{:.0} ¢",
                    s + 1,
                    cents.abs()
                ),
                rx,
                r.y0 + 50.0,
                12.0,
                Align::Right,
                dim,
                false,
            );
        }
    }
    text(
        scene,
        &format!("A4 = {:.0} Hz", app.config.file.audio.tuner_a4),
        r.x1 - 16.0,
        r.y1 - 6.0,
        10.0,
        Align::Right,
        dim,
        false,
    );
}

/// Height of the piano panel.
/// The lowest and highest keys of the piano panel (A0 to C8, 88 keys).
const PIANO_KEYS: (u8, u8) = (21, 108);

/// The fretboard: the track's strings and frets (spaced as on a real neck), inlays, and the
/// notes at the cursor or playhead as dots in the track's color with their names.
fn draw_fretboard(
    scene: &mut Scene,
    app: &App,
    views: &mut Views,
    v: &cecilia_app::instrument::InstrumentView,
    r: Rect,
) {
    use cecilia_app::instrument::NOTE_NAMES;
    let th = &app.config.theme;
    fill(scene, r, col(th.panel));
    fill(scene, Rect::new(r.x0, r.y0, r.x1, r.y0 + 1.0), col(th.tab));
    if !v.fretted {
        text(
            scene,
            "Fretboard: not a fretted track",
            r.x0 + 12.0,
            r.center().y + 5.0,
            12.0,
            Align::Left,
            col(th.dim),
            false,
        );
        return;
    }
    let n = v.strings.len();
    let (left, right) = (r.x0 + 44.0, r.x1 - 16.0);
    let (top, bottom) = (r.y0 + 14.0, r.y1 - 20.0);
    let string_y = |s: usize| top + (bottom - top) * s as f64 / (n.max(2) - 1) as f64;
    let frets = v.frets.max(1) as f64;
    // Fret k sits where it would on a neck: frets get closer up the neck.
    let span = 1.0 - 2f64.powf(-frets / 12.0);
    let fret_x = |k: f64| left + (right - left) * (1.0 - 2f64.powf(-k / 12.0)) / span;
    let wood = th.panel.mix(th.chrome_text, 0.05);
    fill(
        scene,
        Rect::new(left, top - 6.0, right, bottom + 6.0),
        col(wood),
    );
    // Inlays.
    let mid_y = (top + bottom) / 2.0;
    for k in [3u8, 5, 7, 9, 12, 15, 17, 19, 21, 24] {
        if k > v.frets {
            continue;
        }
        let x = (fret_x(k as f64 - 1.0) + fret_x(k as f64)) / 2.0;
        let ys: Vec<f64> = if k % 12 == 0 {
            vec![(top + mid_y) / 2.0, (mid_y + bottom) / 2.0]
        } else {
            vec![mid_y]
        };
        for y in ys {
            scene.fill(
                Fill::NonZero,
                Affine::IDENTITY,
                col(th.dim.with_alpha(90)),
                None,
                &Circle::new((x, y), 3.5),
            );
        }
    }
    // Frets (the nut thicker), and their numbers under the neck.
    for k in 0..=v.frets {
        let x = fret_x(k as f64);
        let w = if k == 0 { 3.0 } else { 1.0 };
        fill(
            scene,
            Rect::new(x - w / 2.0, top - 6.0, x + w / 2.0, bottom + 6.0),
            col(th.dim.with_alpha(if k == 0 { 255 } else { 150 })),
        );
        if [3u8, 5, 7, 9, 12, 15, 17, 19, 21, 24].contains(&k) {
            let cx = (fret_x(k as f64 - 1.0) + x) / 2.0;
            text(
                scene,
                &k.to_string(),
                cx,
                r.y1 - 3.0,
                9.0,
                Align::Center,
                col(th.dim),
                false,
            );
        }
    }
    // A capo across its fret (in the theme's capo color).
    if v.capo > 0 && v.capo <= v.frets {
        let x = fret_x(v.capo as f64) - 4.0;
        scene.fill(
            Fill::NonZero,
            Affine::IDENTITY,
            col(th.capo),
            None,
            &RoundedRect::new(x - 3.0, top - 7.0, x + 3.0, bottom + 7.0, 3.0),
        );
    }
    // Strings (thicker going down), with their open notes on the left. Each string's cells
    // (open, then fret by fret) can be clicked.
    let half = (bottom - top) / (n.max(2) - 1) as f64 / 2.0;
    for (s, &open) in v.strings.iter().enumerate() {
        let y = string_y(s);
        views
            .frets
            .push((Rect::new(left - 24.0, y - half, left, y + half), s, 0));
        for k in 1..=v.frets {
            let cell = Rect::new(fret_x(k as f64 - 1.0), y - half, fret_x(k as f64), y + half);
            views.frets.push((cell, s, k));
        }
        let w = 0.8 + s as f64 * 0.25;
        let cursor = v.cursor_string == Some(s);
        let c = if cursor {
            th.mode_color(app.mode)
        } else {
            th.chrome_text.mix(th.panel, 0.35)
        };
        fill(
            scene,
            Rect::new(left, y - w / 2.0, right, y + w / 2.0),
            col(c),
        );
        text(
            scene,
            NOTE_NAMES[open as usize % 12],
            r.x0 + 12.0,
            y + 4.0,
            11.0,
            Align::Center,
            col(if cursor { c } else { th.dim }),
            cursor,
        );
    }
    // The notes.
    for note in &v.notes {
        if note.string >= n {
            continue;
        }
        let y = string_y(note.string);
        let fret = note.fret as f64 + v.capo as f64;
        let x = if note.fret == 0 && v.capo == 0 {
            left - 12.0
        } else {
            (fret_x(fret - 1.0) + fret_x(fret)) / 2.0
        };
        scene.fill(
            Fill::NonZero,
            Affine::IDENTITY,
            col(v.color),
            None,
            &Circle::new((x, y), 7.0),
        );
        text(
            scene,
            NOTE_NAMES[note.pitch as usize % 12],
            x,
            y + 3.5,
            9.5,
            Align::Center,
            col(th.text_on(v.color)),
            true,
        );
    }
}

/// The piano: 88 keys with the notes at the cursor or playhead pressed in the track's color.
fn draw_piano_panel(
    scene: &mut Scene,
    app: &App,
    views: &mut Views,
    v: &cecilia_app::instrument::InstrumentView,
    r: Rect,
) {
    let th = &app.config.theme;
    fill(scene, r, col(th.panel));
    fill(scene, Rect::new(r.x0, r.y0, r.x1, r.y0 + 1.0), col(th.tab));
    let is_black = |p: u8| matches!(p % 12, 1 | 3 | 6 | 8 | 10);
    let (lo, hi) = PIANO_KEYS;
    let whites = (lo..=hi).filter(|&p| !is_black(p)).count() as f64;
    let (x0, x1) = (r.x0 + 8.0, r.x1 - 8.0);
    let (top, bottom) = (r.y0 + 6.0, r.y1 - 6.0);
    let kw = (x1 - x0) / whites;
    let pressed = |p: u8| v.notes.iter().any(|n| n.pitch == p);
    let white = th.page;
    let black = th.ink.mix(th.page, 0.15);
    // White keys, with C names under middle C's octave marks.
    let mut x = x0;
    let mut white_keys = Vec::new();
    let mut white_x = std::collections::HashMap::new();
    for p in (lo..=hi).filter(|&p| !is_black(p)) {
        white_x.insert(p, x);
        white_keys.push((Rect::new(x, top, x + kw, bottom), p));
        let c = if pressed(p) { v.color } else { white };
        scene.fill(
            Fill::NonZero,
            Affine::IDENTITY,
            col(c),
            None,
            &RoundedRect::new(x + 0.5, top, x + kw - 0.5, bottom, 2.0),
        );
        if p % 12 == 0 {
            text(
                scene,
                &format!("C{}", p / 12 - 1),
                x + kw / 2.0,
                bottom - 4.0,
                8.0,
                Align::Center,
                col(th.text_on(c).with_alpha(160)),
                p == 60,
            );
        }
        x += kw;
    }
    // Black keys over them.
    for p in (lo..=hi).filter(|&p| is_black(p)) {
        let Some(&wx) = white_x.get(&(p - 1)) else {
            continue;
        };
        let bx = wx + kw * 0.68;
        views.keys.push((
            Rect::new(bx, top, bx + kw * 0.64, top + (bottom - top) * 0.62),
            p,
        ));
        let c = if pressed(p) { v.color } else { black };
        scene.fill(
            Fill::NonZero,
            Affine::IDENTITY,
            col(c),
            None,
            &RoundedRect::new(bx, top, bx + kw * 0.64, top + (bottom - top) * 0.62, 1.5),
        );
    }
    // Black keys lie over white ones: they are hit first.
    views.keys.extend(white_keys);
}

/// A piano keyboard (the synth plays), as the 󰣱 glyph.
fn draw_piano(scene: &mut Scene, x: f64, cy: f64, color: Color) {
    let body = RoundedRect::new(x, cy - 6.0, x + 14.0, cy + 6.0, 1.5);
    scene.stroke(&Stroke::new(1.4), Affine::IDENTITY, color, None, &body);
    for k in [3.5, 7.0, 10.5] {
        fill(
            scene,
            Rect::new(x + k - 0.5, cy - 6.0, x + k + 0.5, cy + 6.0),
            color,
        );
    }
    for k in [3.5, 10.5] {
        fill(
            scene,
            Rect::new(x + k - 1.6, cy - 6.0, x + k + 1.6, cy + 1.0),
            color,
        );
    }
}

/// Two beamed notes (the recording plays), as the 󰝚 glyph.
fn draw_music(scene: &mut Scene, x: f64, cy: f64, color: Color) {
    for (hx, hy) in [(x + 2.8, cy + 4.5), (x + 10.8, cy + 3.0)] {
        let head = vello::kurbo::Ellipse::new((hx, hy), (2.8, 2.1), -0.35);
        scene.fill(Fill::NonZero, Affine::IDENTITY, color, None, &head);
    }
    fill(
        scene,
        Rect::new(x + 4.7, cy - 5.0, x + 5.9, cy + 4.0),
        color,
    );
    fill(
        scene,
        Rect::new(x + 12.7, cy - 6.5, x + 13.9, cy + 2.5),
        color,
    );
    let mut beam = BezPath::new();
    beam.move_to((x + 4.7, cy - 5.0));
    beam.line_to((x + 13.9, cy - 6.5));
    beam.line_to((x + 13.9, cy - 3.8));
    beam.line_to((x + 4.7, cy - 2.3));
    beam.close_path();
    scene.fill(Fill::NonZero, Affine::IDENTITY, color, None, &beam);
}

/// A magnifying glass (the zoom), as the  glyph.
fn draw_magnifier(scene: &mut Scene, x: f64, cy: f64, color: Color) {
    scene.stroke(
        &Stroke::new(1.6),
        Affine::IDENTITY,
        color,
        None,
        &Circle::new((x + 5.5, cy - 1.0), 4.3),
    );
    let mut handle = BezPath::new();
    handle.move_to((x + 8.6, cy + 2.1));
    handle.line_to((x + 12.5, cy + 6.0));
    scene.stroke(
        &Stroke::new(2.2).with_caps(vello::kurbo::Cap::Round),
        Affine::IDENTITY,
        color,
        None,
        &handle,
    );
}

/// Four corner arrows pointing outwards: a maximized pane.
fn draw_expand(scene: &mut Scene, x: f64, cy: f64, color: Color) {
    let stroke = Stroke::new(1.5).with_caps(vello::kurbo::Cap::Round);
    let (x0, x1, y0, y1) = (x + 1.0, x + 13.0, cy - 6.0, cy + 6.0);
    // Each corner: a diagonal to the tip, and the two short sides of its arrowhead.
    for (tx, ty, sx, sy) in [
        (x0, y0, 1.0, 1.0),
        (x1, y0, -1.0, 1.0),
        (x0, y1, 1.0, -1.0),
        (x1, y1, -1.0, -1.0),
    ] {
        let mut p = BezPath::new();
        p.move_to((tx + sx * 5.0, ty + sy * 5.0));
        p.line_to((tx, ty));
        p.move_to((tx + sx * 4.0, ty));
        p.line_to((tx, ty));
        p.line_to((tx, ty + sy * 4.0));
        scene.stroke(&stroke, Affine::IDENTITY, color, None, &p);
    }
}

/// A small briefcase: the workspace.
fn draw_workspace_icon(scene: &mut Scene, x: f64, cy: f64, color: Color) {
    let body = RoundedRect::new(x, cy - 4.0, x + 13.0, cy + 6.0, 1.5);
    scene.stroke(&Stroke::new(1.5), Affine::IDENTITY, color, None, &body);
    let mut handle = BezPath::new();
    handle.move_to((x + 4.0, cy - 4.0));
    handle.line_to((x + 4.0, cy - 6.5));
    handle.line_to((x + 9.0, cy - 6.5));
    handle.line_to((x + 9.0, cy - 4.0));
    scene.stroke(&Stroke::new(1.5), Affine::IDENTITY, color, None, &handle);
    fill(scene, Rect::new(x, cy - 0.3, x + 13.0, cy + 0.9), color);
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

fn draw_which_key(scene: &mut Scene, title: &str, rows: &[(String, String)], main: Rect) {
    let fonts = cecilia_engrave::fonts::fonts();
    let desc_w = rows
        .iter()
        .map(|(_, d)| fonts.text_width(FontKind::Text, d, 14.0))
        .fold(180.0, f64::max);
    let w = (desc_w + 130.0).min(main.width() - 20.0);
    let h = 36.0 + rows.len() as f64 * 20.0;
    let r = Rect::new(
        main.x1 - w - 10.0,
        main.y1 - h - 10.0,
        main.x1 - 10.0,
        main.y1 - 10.0,
    );
    panel(scene, r, title);
    for (i, (keys, desc)) in rows.iter().enumerate() {
        let y = r.y0 + 44.0 + i as f64 * 20.0;
        text(
            scene,
            keys,
            r.x0 + 90.0,
            y,
            14.0,
            Align::Right,
            accent(),
            false,
        );
        text(
            scene,
            desc,
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
    let (title, items) = app.help_view();
    let r = main.inset(-60.0);
    panel(scene, r, &format!("{title} — F1 or Esc to close"));
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

    /// The design-mode handle under a window position, if the pane there is in design mode.
    fn design_handle_at(&self, app: &App, x: f64, y: f64) -> Option<crate::design::Handle> {
        if !app.design_active() {
            return None;
        }
        let (g, px, py) = self.locate(x, y)?;
        if g.pane != app.focused() {
            return None;
        }
        let cached = self.cache.get(&g.pane).filter(|c| c.key.2 == "page")?;
        let center = ((g.content.width() / g.scale - 595.0) / 2.0).max(0.0);
        let page = cached.offsets.iter().rposition(|&o| o <= py)?;
        crate::design::handle_at(
            &cached.layout,
            (app.pane().cursor.track, app.pane().cursor.bar),
            page,
            (px - center, py - cached.offsets[page]),
            4.0 / g.scale,
        )
    }

    /// Middle-click: closes the buffer tab under the mouse.
    pub fn middle_click(&mut self, app: &mut App, x: f64, y: f64) {
        let hit = self
            .tabline
            .iter()
            .find(|(r, _)| r.contains((x, y)))
            .map(|(_, h)| *h);
        if let Some(TablineHit::Buffer(n) | TablineHit::Close(n)) = hit {
            app.run(cecilia_app::Command::BufferClose(Some(n + 1), false));
        }
    }

    /// Whether a window position is over the tabline (hovering there needs redraws).
    /// Whether a window position is over the tabline or toolbar (hovering there needs
    /// redraws).
    pub fn over_tabline(&self, y: f64) -> bool {
        y < self.chrome_bottom
    }

    /// A toolbar tooltip is waiting to show: redraw until it does.
    pub fn tooltip_pending(&self) -> bool {
        self.toolbar_hover
            .into_iter()
            .chain(self.palette_hover)
            .any(|(_, t)| t.elapsed() < TOOLTIP_DELAY + std::time::Duration::from_millis(50))
    }

    /// The editing palette's row under a point, as drawn last frame.
    pub fn palette_row_at(&self, x: f64, y: f64) -> Option<usize> {
        self.palette
            .iter()
            .find(|(r, _)| r.contains((x, y)))
            .map(|&(_, i)| i)
    }

    /// Mouse down over the properties pane: a tab shows, a row selects, a selected row edits.
    /// True when the pane took the click.
    fn click_properties(&mut self, app: &mut App, x: f64, y: f64, double: bool) -> bool {
        let Some(&(_, mut hit)) = self.props.iter().find(|(r, _)| r.contains((x, y))) else {
            return false;
        };
        // A click in a box of a staff selects that box, then acts as on the row.
        if let PropHit::StaffBox(i, b) = hit {
            app.set_staff_box(b);
            hit = PropHit::Row(i);
        }
        match hit {
            PropHit::StaffBox(..) => {}
            PropHit::Tab(t) => app.set_property_tab(t),
            // A staff can be dragged to another place; a plain click waits for the release.
            PropHit::Row(i)
                if app.text_editing(i)
                    && self
                        .props_text
                        .as_ref()
                        .is_some_and(|p| p.area.contains((x, y))) =>
            {
                // In the text area being edited: a click places the caret, a drag selects.
                let at = self.props_text.as_ref().map_or(0, |p| p.at(x, y));
                if double {
                    app.text_double_click(at);
                } else {
                    app.text_click(at);
                    self.drag = Some(Drag::Text);
                }
            }
            PropHit::Row(i) if app.property_is_staff(i) => {
                self.drag = Some(Drag::Prop(i, i, double));
            }
            PropHit::Row(i) => {
                app.click_property(i, double);
                // A double click in the lyrics puts the caret where it landed.
                if double {
                    if let Some(pt) = self.props_text.as_ref().filter(|p| p.area.contains((x, y))) {
                        app.set_text_caret(pt.at(x, y));
                    }
                }
            }
        }
        true
    }

    /// Mouse down over the tracks panel: select, solo, mute, or start dragging a volume.
    /// True when the panel took the click.
    fn click_tracks(&mut self, app: &mut App, x: f64, y: f64, double: bool) -> bool {
        let Some(&(_, hit)) = self.tracks.iter().find(|(r, _)| r.contains((x, y))) else {
            return false;
        };
        match hit {
            TracksHit::Volume(strip, x0, x1) => {
                if let Strip::Track(t) = strip {
                    app.select_track_bar(t, None);
                }
                app.group_edits(true);
                self.drag = Some(Drag::Volume(strip, x0, x1));
                self.drag_to(app, x, y);
            }
            TracksHit::Solo(strip) => app.run(cecilia_app::Command::Solo(Some(strip))),
            TracksHit::Mute(strip) => app.run(cecilia_app::Command::Mute(Some(strip))),
            // A double click on a name renames the track in place, on an instrument picks one.
            TracksHit::Name(t) | TracksHit::Instrument(t) if double => {
                app.tracks_selected = t;
                app.run(if matches!(hit, TracksHit::Name(_)) {
                    cecilia_app::Command::TrackRenameStart
                } else {
                    cecilia_app::Command::TrackInstrument(None)
                });
            }
            TracksHit::Row(t) | TracksHit::Name(t) | TracksHit::Instrument(t) => {
                app.tracks_selected = t;
                app.select_track_bar(t, None)
            }
            TracksHit::Bar(t, b) => app.select_track_bar(t, Some(b)),
        }
        true
    }

    /// Mouse moved with the button down: drags a slider, a pane border or a panel's edge.
    pub fn drag_to(&mut self, app: &mut App, x: f64, y: f64) -> bool {
        match self.drag.clone() {
            Some(Drag::Text) => {
                if let Some(pt) = &self.props_text {
                    // Past the top or bottom of the text scrolls it.
                    if y < pt.area.y0 {
                        app.scroll_properties(-1);
                    } else if y > pt.area.y1 {
                        app.scroll_properties(1);
                    }
                    app.text_drag(pt.at(x, y));
                }
            }
            Some(Drag::Prop(from, _, double)) => {
                let over = self.props.iter().find_map(|(r, h)| match h {
                    PropHit::Row(i) | PropHit::StaffBox(i, _) if r.contains((x, y)) => Some(*i),
                    _ => None,
                });
                if let Some(i) = over {
                    self.drag = Some(Drag::Prop(from, i, double));
                }
            }
            Some(Drag::Volume(strip, x0, x1)) => {
                app.set_strip_volume(strip, (x - x0) / (x1 - x0));
            }
            Some(Drag::Divider(d)) => {
                let pos = match d.axis {
                    cecilia_app::layout::Axis::SideBySide => x,
                    cecilia_app::layout::Axis::Stacked => y,
                };
                app.drag_divider(&d, pos.max(0.0).round() as u16);
            }
            Some(Drag::Tab(n, x0)) => {
                if (x - x0).abs() < 4.0 {
                    return true;
                }
                // The tab under the mouse (anywhere along its width) takes this one's place.
                let target = self.tabline.iter().find_map(|(r, hit)| match hit {
                    TablineHit::Buffer(m) if x >= r.x0 && x < r.x1 => Some(*m),
                    _ => None,
                });
                if let Some(m) = target.filter(|&m| m != n) {
                    app.move_tab(n, m);
                    // Once moving, it keeps following the mouse (no threshold any more).
                    self.drag = Some(Drag::Tab(m, f64::NEG_INFINITY));
                }
            }
            Some(Drag::Wave(y0, h0)) => {
                let h = h0 as f64 + (y0 - y);
                app.run(cecilia_app::Command::WaveformResize(
                    cecilia_app::command::Amount::To(h.max(0.0).round() as u32),
                ));
            }
            Some(Drag::Scrollbar(pane, horizontal, grab)) => {
                self.scrollbar_to(pane, horizontal, grab, x, y)
            }
            Some(Drag::Design(pane, grab)) => {
                let Some(g) = self.geometry.iter().find(|g| g.pane == pane) else {
                    return false;
                };
                let px = (x - g.content.x0) / g.scale + g.scroll.0;
                let unit = Options::default().sp / 10.0;
                app.design_preview(grab.apply(px - grab.start, unit));
            }
            Some(Drag::Select(pane)) => self.select_to(app, pane, x, y),
            None => return false,
        }
        true
    }

    /// A beat-selection drag is in progress (it scrolls the view at the pane's edges).
    pub fn selecting(&self) -> bool {
        matches!(self.drag, Some(Drag::Select(..)))
    }

    /// Called every frame while the button is down on a selection: near or past the pane's edge
    /// the view scrolls, faster the further out the pointer is, and the selection follows.
    /// Returns whether the view moved.
    pub fn autoscroll(&mut self, app: &mut App) -> bool {
        let Some(Drag::Select(pane)) = self.drag else {
            self.select_tick = None;
            return false;
        };
        let now = std::time::Instant::now();
        let dt = self
            .select_tick
            .replace(now)
            .map_or(0.0, |t| now.duration_since(t).as_secs_f64().min(0.05));
        let Some(g) = self.geometry.iter().find(|g| g.pane == pane).copied() else {
            return false;
        };
        let (x, y) = self.hover;
        let vx = edge_velocity(x, g.content.x0, g.content.x1);
        let vy = edge_velocity(y, g.content.y0, g.content.y1);
        if vx == 0.0 && vy == 0.0 {
            return false;
        }
        let entry = self.scroll.entry(pane).or_insert((0.0, 0.0));
        entry.0 = (entry.0 + vx * dt / g.scale).max(0.0);
        entry.1 = (entry.1 + vy * dt / g.scale).max(0.0);
        self.select_to(app, pane, x, y);
        true
    }

    /// Extends a mouse selection to the beat of the pressed track nearest the pointer (which may
    /// be outside the pane: it is held to the pane's edge).
    fn select_to(&mut self, app: &mut App, pane: PaneId, x: f64, y: f64) {
        let (Some(g), Some(track), Some(cached)) = (
            self.geometry.iter().find(|g| g.pane == pane),
            app.mouse_track(),
            self.cache.get(&pane),
        ) else {
            return;
        };
        let x = x.clamp(g.content.x0, (g.content.x1 - 1.0).max(g.content.x0));
        let y = y.clamp(g.content.y0, (g.content.y1 - 1.0).max(g.content.y0));
        let center = if cached.key.2 == "page" {
            ((g.content.width() / g.scale - 595.0) / 2.0).max(0.0)
        } else {
            0.0
        };
        let px = (x - g.content.x0) / g.scale + g.scroll.0 - center;
        let py = (y - g.content.y0) / g.scale + g.scroll.1;
        let page = cached.offsets.iter().rposition(|&o| o <= py).unwrap_or(0);
        let Some(h) = cached
            .layout
            .pages
            .get(page)
            .and_then(|p| nearest_hit(&p.hits, track, (px, py - cached.offsets[page])))
        else {
            return;
        };
        let at = cecilia_app::Cursor {
            bar: h.beat.bar,
            beat: h.beat.beat,
            ..app.pane().cursor
        };
        app.mouse_drag(at);
    }

    /// Scrolls a pane to where a scrollbar drag has its thumb, held `grab` pixels from the
    /// thumb's start.
    fn scrollbar_to(&mut self, pane: PaneId, horizontal: bool, grab: f64, x: f64, y: f64) {
        let Some(bar) = self
            .geometry
            .iter()
            .find(|g| g.pane == pane)
            .and_then(|g| g.bars[usize::from(horizontal)])
        else {
            return;
        };
        let (pos, start, len, size) = if bar.horizontal {
            (x, bar.track.x0, bar.track.width(), bar.thumb.width())
        } else {
            (y, bar.track.y0, bar.track.height(), bar.thumb.height())
        };
        let frac = ((pos - grab - start) / (len - size).max(1.0)).clamp(0.0, 1.0);
        let entry = self.scroll.entry(pane).or_insert((0.0, 0.0));
        if bar.horizontal {
            entry.0 = frac * bar.range;
        } else {
            entry.1 = frac * bar.range;
        }
    }

    /// Mouse up: ends a drag (a volume drag is one undo step).
    pub fn release(&mut self, app: &mut App) {
        match self.drag.take() {
            Some(Drag::Volume(..)) => app.group_edits(false),
            Some(Drag::Text) => app.text_release(),
            Some(Drag::Design(..)) => app.design_end(),
            Some(Drag::Select(..)) => app.mouse_release(),
            // Released on another staff: it moves there; on the same one: a click.
            Some(Drag::Prop(from, to, double)) if from == to => app.click_property(from, double),
            Some(Drag::Prop(from, to, _)) => app.drag_property(from, to),
            _ => {}
        }
    }

    /// A pane border near a window position.
    fn divider_at(&self, app: &App, x: f64, y: f64) -> Option<cecilia_app::layout::Divider> {
        app.dividers().into_iter().find(|d| {
            let a = d.area;
            let pos = d.pos as f64;
            match d.axis {
                cecilia_app::layout::Axis::SideBySide => {
                    (x - pos).abs() <= 4.0 && y >= a.y as f64 && y <= (a.y + a.h) as f64
                }
                cecilia_app::layout::Axis::Stacked => {
                    (y - pos).abs() <= 4.0 && x >= a.x as f64 && x <= (a.x + a.w) as f64
                }
            }
        })
    }

    /// The pointer shape at a window position (resize arrows over draggable edges).
    pub fn pointer(&self, app: &App, x: f64, y: f64) -> Pointer {
        if let Some(Drag::Divider(d)) = &self.drag {
            return match d.axis {
                cecilia_app::layout::Axis::SideBySide => Pointer::ResizeColumns,
                cecilia_app::layout::Axis::Stacked => Pointer::ResizeRows,
            };
        }
        if matches!(self.drag, Some(Drag::Wave(..))) {
            return Pointer::ResizeRows;
        }
        if matches!(self.drag, Some(Drag::Design(..))) {
            return Pointer::ResizeColumns;
        }
        if self.design_handle_at(app, x, y).is_some() {
            return Pointer::ResizeColumns;
        }
        if self.edges.iter().any(|(r, _)| r.contains((x, y))) {
            return Pointer::ResizeRows;
        }
        match self.divider_at(app, x, y).map(|d| d.axis) {
            Some(cecilia_app::layout::Axis::SideBySide) => Pointer::ResizeColumns,
            Some(cecilia_app::layout::Axis::Stacked) => Pointer::ResizeRows,
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
        if self.palette_rect.is_some_and(|r| r.contains((x, y))) {
            let rows = (dy / 30.0).round() as isize;
            app.scroll_palette(if rows == 0 {
                dy.signum() as isize
            } else {
                rows
            });
            return true;
        }
        if self.props_rect.is_some_and(|r| r.contains((x, y))) {
            // A couple of rows a notch, whichever way the wheel turns.
            let rows = (dy / 30.0).round() as isize;
            let rows = if rows == 0 {
                dy.signum() as isize
            } else {
                rows
            };
            app.scroll_properties(rows);
            return true;
        }
        if self.tracks_rect.is_some_and(|r| r.contains((x, y))) {
            let n = app.track_rows().len();
            let fit = self.tracks_rect.map_or(1, |r| {
                ((r.height() - TRACK_HEAD) / TRACK_ROW).floor().max(1.0) as usize
            });
            let visible = n.clamp(1, fit);
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
        if let Some(&(_, i)) = self.dashboard.iter().find(|(r, _)| r.contains((x, y))) {
            app.dashboard_activate(i);
            return;
        }
        if let Some(&(_, i)) = self.toolbar.iter().find(|(r, _)| r.contains((x, y))) {
            app.run_toolbar(i);
            return;
        }
        if let Some((_, cmd)) = self.status.iter().find(|(r, _)| r.contains((x, y))) {
            app.run(cmd.clone());
            return;
        }
        // A click on a scrollbar grabs its thumb (or jumps it under the pointer).
        let grabbed = self.geometry.iter().find_map(|g| {
            let bar = g.bars.iter().flatten().find(|b| b.track.contains((x, y)))?;
            let (pos, start, size) = if bar.horizontal {
                (x, bar.thumb.x0, bar.thumb.width())
            } else {
                (y, bar.thumb.y0, bar.thumb.height())
            };
            let held = pos - start;
            Some((
                g.pane,
                bar.horizontal,
                if (0.0..=size).contains(&held) {
                    held
                } else {
                    size / 2.0
                },
            ))
        });
        if let Some((pane, horizontal, grab)) = grabbed {
            app.set_focus(pane);
            self.drag = Some(Drag::Scrollbar(pane, horizontal, grab));
            self.scrollbar_to(pane, horizontal, grab, x, y);
            return;
        }
        let now = std::time::Instant::now();
        let double = self.last_click.is_some_and(|(t, x0, y0)| {
            now.duration_since(t) < std::time::Duration::from_millis(400)
                && (x - x0).abs() < 4.0
                && (y - y0).abs() < 4.0
        });
        self.last_click = Some((now, x, y));
        // A click on a progress bar moves that pane's cursor to the bar there.
        if let Some(&(r, id)) = self.progress.iter().find(|(r, _)| r.contains((x, y))) {
            app.set_focus(id);
            if let Some(b) = app.progress_bar_at(id, (x - r.x0) / r.width().max(1.0)) {
                app.run(cecilia_app::Command::GotoBar(b + 1));
            }
            return;
        }
        // A click in a tool pane focuses it.
        if let Some(&(_, id)) = self.tool_rects.iter().find(|(r, _)| r.contains((x, y))) {
            app.set_focus(id);
        }
        // A fret or a piano key enters (or removes) that note at the cursor.
        if let Some(&(_, string, fret)) = self.frets.iter().find(|(r, ..)| r.contains((x, y))) {
            app.click_fret(string, fret);
            return;
        }
        if let Some(&(_, pitch)) = self.keys.iter().find(|(r, _)| r.contains((x, y))) {
            app.click_key(pitch);
            return;
        }
        if let Some(&(_, row)) = self.palette.iter().find(|(r, _)| r.contains((x, y))) {
            app.click_palette(row);
            return;
        }
        if self.click_properties(app, x, y, double) {
            return;
        }
        if self.click_tracks(app, x, y, double) {
            return;
        }
        // Edges and pane borders start a resize.
        if let Some(&(_, edge)) = self.edges.iter().find(|(r, _)| r.contains((x, y))) {
            self.drag = Some(match edge {
                Edge::Wave => Drag::Wave(y, app.waveform_height),
            });
            return;
        }
        if let Some(d) = self.divider_at(app, x, y) {
            self.drag = Some(Drag::Divider(d));
            return;
        }
        if let Some(&(_, hit)) = self.tabline.iter().rev().find(|(r, _)| r.contains((x, y))) {
            // A tab can be dragged along the tabline to reorder it.
            if let TablineHit::Buffer(n) = hit {
                self.drag = Some(Drag::Tab(n, x));
            }
            app.run(match hit {
                TablineHit::Buffer(n) => cecilia_app::Command::BufferGoto(n + 1),
                TablineHit::Close(n) => cecilia_app::Command::BufferClose(Some(n + 1), false),
                TablineHit::View(n) => cecilia_app::Command::ViewGoto(n + 1),
            });
            return;
        }
        let Some((g, px, py)) = self.locate(x, y) else {
            return;
        };
        app.set_focus(g.pane);
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
        // A design-mode handle starts a drag instead of moving the cursor.
        if page_mode && app.design_active() {
            let cursor = (app.pane().cursor.track, app.pane().cursor.bar);
            let tol = 4.0 / g.scale;
            if let Some(handle) = crate::design::handle_at(&cached.layout, cursor, page, local, tol)
            {
                // The line-end buttons are clicks: one more or fewer bar, one undo step.
                let line = |system: usize| {
                    cached
                        .layout
                        .systems
                        .get(system)
                        .map(|s| (s.first_bar, s.bars))
                };
                match handle {
                    crate::design::Handle::LineAdd { system } => {
                        if let Some((start, n)) = line(system) {
                            app.design_line_bars(start, n + 1);
                        }
                        return;
                    }
                    crate::design::Handle::LineRemove { system } => {
                        if let Some((start, n)) = line(system) {
                            app.design_line_bars(start, n - 1);
                        }
                        return;
                    }
                    _ => {}
                }
                let grab = crate::design::Grab::new(
                    &cached.layout,
                    handle,
                    px + center,
                    app.design_overrides(),
                    app.buffer().score.master_bars.len(),
                );
                app.design_begin();
                self.drag = Some(Drag::Design(g.pane, Box::new(grab)));
                return;
            }
        }
        let hits = &cached.layout.pages[page].hits;
        // Prefer the precise tab-string hit, then the beat column.
        let hit = hits
            .iter()
            .find(|h| h.string.is_some() && h.rect.contains(local))
            .or_else(|| hits.iter().find(|h| h.rect.contains(local)));
        let at = hit.zip(app.panes.get(&g.pane)).map(|(h, p)| {
            let mut c = p.cursor;
            c.track = h.beat.track;
            c.bar = h.beat.bar;
            if h.beat.voice == 0 {
                c.beat = h.beat.beat;
            }
            c.staff = h.staff;
            if let Some(s) = h.string {
                c.string = s as usize;
            } else if let Some(line) = cached
                .layout
                .beats
                .get(&h.beat)
                .and_then(|pos| pos.position_at(h.staff, local.1))
            {
                c.line = line;
            }
            c
        });
        if let Some(at) = at {
            // Places the cursor (a slash staff is one line: a click above or below it still
            // lands on it); a drag from here selects.
            let mods = cecilia_app::mouse::MouseMods {
                extend: self.select_mods.0,
                bars: self.select_mods.1,
                double,
            };
            app.mouse_press(g.pane, at, mods);
            self.drag = Some(Drag::Select(g.pane));
        }
    }

    /// The score pane under the point.
    pub fn pane_at(&self, x: f64, y: f64) -> Option<PaneId> {
        self.locate(x, y).map(|(g, _, _)| g.pane)
    }

    /// Wheel over a score: the vertical wheel scrolls down and the horizontal one (or Shift
    /// with the vertical) across, in every layout. (The draw clamps to what the score extends.)
    pub fn scroll(&mut self, x: f64, y: f64, dx: f64, dy: f64) {
        let Some((g, _, _)) = self.locate(x, y) else {
            return;
        };
        let entry = self.scroll.entry(g.pane).or_insert((0.0, 0.0));
        entry.0 = (entry.0 + dx / g.scale).max(0.0);
        entry.1 = (entry.1 + dy / g.scale).max(0.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cecilia_core::*;

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
                        ..Default::default()
                    };
                    2
                ],
                ..Track::default()
            }],
            ..Score::default()
        };
        let layout = cecilia_engrave::engrave(&score, 0, &Options::default());
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

#[cfg(test)]
mod follow_tests {
    use super::*;
    use cecilia_engrave::{BeatPos, StaffPos};

    fn beat(x: f64, width: f64) -> BeatPos {
        BeatPos {
            page: 0,
            x,
            width,
            top: 0.0,
            bottom: 400.0,
            head: x + 3.0,
            staves: vec![
                StaffPos::Strings {
                    staff: 0,
                    ys: vec![10.0, 20.0],
                },
                StaffPos::Strings {
                    staff: 1,
                    ys: vec![300.0, 310.0],
                },
            ],
        }
    }

    #[test]
    fn align_scroll_places_the_cursor_in_the_view() {
        use cecilia_app::ScrollTo::*;
        let a = |to, h| {
            align_scroll(
                (0.0, 0.0),
                (100.0, 200.0),
                (10.0, 20.0),
                (500.0, 540.0),
                to,
                h,
            )
        };
        // Centre: the middle of the extent (520) in the middle of the view (100).
        assert_eq!(a(Center, false).1, 420.0);
        assert_eq!(a(Top, false).1, 500.0);
        assert_eq!(a(Bottom, false).1, 340.0);
        // The horizontal layout aligns along x and keeps y in view.
        assert_eq!(a(Center, true).0, -35.0);
        assert_eq!(a(Top, true).0, 10.0);
        assert_eq!(a(Bottom, true).0, -80.0);
        // An extent taller than the view is centred all the same.
        let tall = align_scroll(
            (0.0, 0.0),
            (100.0, 100.0),
            (0.0, 10.0),
            (300.0, 500.0),
            Center,
            false,
        );
        assert_eq!(tall.1, 350.0);
    }

    #[test]
    fn cursor_extent_targets_the_cursors_staff_not_the_top_one() {
        let c = cecilia_app::Cursor {
            staff: 1,
            string: 1,
            ..Default::default()
        };
        let (_, (top, bottom)) = cursor_extent(&beat(0.0, 10.0), 1, &c);
        assert!(top > 250.0 && bottom < 400.0, "{top} {bottom}");
        assert!(top <= 310.0 && bottom >= 310.0);
    }

    /// Whatever the zoom (view size), a cursor moved anywhere ends up inside the view.
    #[test]
    fn a_moved_cursor_is_always_in_view_at_any_zoom() {
        let c = cecilia_app::Cursor {
            staff: 1,
            string: 1,
            ..Default::default()
        };
        for view in [
            (60.0, 50.0),
            (119.0, 90.0),
            (400.0, 300.0),
            (2000.0, 1500.0),
        ] {
            // Bars far wider than the view, cursor jumping around them.
            let mut scroll = (0.0, 0.0);
            for x in [0.0, 500.0, 990.0, 20.0, 1500.0, 40.0, 700.0] {
                let b = beat(x, 80.0);
                let (xs, ys) = cursor_extent(&b, 1, &c);
                scroll = follow_scroll(scroll, view, xs, ys);
                let (sx, sy) = scroll;
                // The cursor square itself (the part that must show).
                let (cx, cy) = (b.head, 310.0);
                assert!(cx >= sx && cx <= sx + view.0, "x {cx} in {sx}+{}", view.0);
                assert!(cy >= sy && cy <= sy + view.1, "y {cy} in {sy}+{}", view.1);
            }
        }
    }

    #[test]
    fn an_already_visible_cursor_does_not_scroll() {
        assert_eq!(
            follow_scroll((10.0, 20.0), (100.0, 100.0), (30.0, 40.0), (50.0, 60.0)),
            (10.0, 20.0)
        );
    }
}

#[cfg(test)]
mod select_tests {
    use super::*;
    use cecilia_engrave::display::{BeatRef, Hit};

    fn hit(track: usize, beat: usize, x: f64, y: f64) -> Hit {
        Hit {
            rect: Rect::new(x, y, x + 10.0, y + 10.0),
            beat: BeatRef {
                track,
                bar: 0,
                voice: 0,
                beat,
            },
            string: None,
            staff: 0,
        }
    }

    #[test]
    fn the_nearest_beat_of_the_pressed_track_wins() {
        let hits = [
            hit(0, 0, 0.0, 0.0),
            hit(0, 1, 20.0, 0.0),
            hit(1, 0, 40.0, 0.0),
            hit(0, 2, 0.0, 100.0),
        ];
        let at = |p| nearest_hit(&hits, 0, p).unwrap().beat.beat;
        assert_eq!(at((5.0, 5.0)), 0);
        assert_eq!(at((22.0, 5.0)), 1);
        // Over another track's beat: the pressed track's nearest one.
        assert_eq!(at((41.0, 5.0)), 1);
        // Between lines, and far beyond the page.
        assert_eq!(at((5.0, 80.0)), 2);
        assert_eq!(at((-500.0, -500.0)), 0);
        assert!(nearest_hit(&hits, 3, (0.0, 0.0)).is_none());
    }

    #[test]
    fn edge_velocity_is_zero_inside_and_grows_towards_the_edge() {
        assert_eq!(edge_velocity(500.0, 0.0, 1000.0), 0.0);
        let near = edge_velocity(990.0, 0.0, 1000.0);
        let beyond = edge_velocity(1100.0, 0.0, 1000.0);
        assert!(near > 0.0 && beyond > near);
        assert!(edge_velocity(5.0, 0.0, 1000.0) < 0.0);
        assert!(edge_velocity(-5000.0, 0.0, 1000.0) >= -1600.0);
        // A tiny pane still has a calm middle.
        assert_eq!(edge_velocity(50.0, 0.0, 100.0), 0.0);
    }
}

#[cfg(test)]
mod ring_tests {
    use super::*;

    #[test]
    fn ring_stays_within_the_pane_margin() {
        let rect = Rect::new(10.0, 10.0, 110.0, 60.0);
        // Up to width 6 the ring is centered on the edge, as before.
        for w in [1.0, 2.0, 6.0] {
            assert_eq!(ring_shape(rect, 6.0, w).rect(), rect);
        }
        // Wider rings move in so their outer edge is `PANE_MARGIN` out at most.
        for w in [8.0, 12.0] {
            let shape = ring_shape(rect, 6.0, w);
            let outer = rect.x0 - (shape.rect().x0 - w / 2.0);
            assert!(outer.abs() <= PANE_MARGIN + 1e-9, "{outer}");
        }
    }
}
