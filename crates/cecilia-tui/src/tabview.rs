//! Text tablature rendering for one pane.

use crate::{barline, rc, rhythm};
use cecilia_app::{App, Cursor};
use cecilia_core::{Bar, Beat, DurationValue, Note, Score, TabRhythm, Track, TrackKind, Voice};
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::widgets::Widget;

/// A clickable region mapped to a score position.
#[derive(Debug, Clone, Copy)]
pub struct Hit {
    pub area: Rect,
    pub pane: u32,
    pub cursor: Cursor,
}

/// Horizontal layout of one bar within a system.
struct BarLayout {
    bar: usize,
    x: u16,
    /// Columns of the bar line opening the bar (see [`barline::boundary`]).
    lead: u16,
    /// (x offset from bar start, width) per column: a column is a moment where a beat of any
    /// voice starts, so the voices line up by time.
    beats: Vec<(u16, u16)>,
    /// The start tick of each column.
    ticks: Vec<u32>,
    width: u16,
}

#[derive(Default)]
pub struct System {
    bars: Vec<BarLayout>,
    /// Page layout: the page this system is on.
    page: usize,
    /// Horizontal layout: the last bar does not fit whole.
    clipped: bool,
}

impl System {
    /// An empty system on `page` (the chant view's lines, which page like bars do).
    pub(crate) fn on_page(page: usize) -> Self {
        System {
            page,
            ..Default::default()
        }
    }

    pub(crate) fn page(&self) -> usize {
        self.page
    }
}

/// The first bar to show in the horizontal layout so that `focus` is whole on screen: `scroll`
/// if it already is, else the nearest change (`to` places it at the top or in the middle).
pub fn scroll_bars(
    score: &Score,
    track: usize,
    width: u16,
    scale: u16,
    scroll: usize,
    focus: usize,
    to: Option<cecilia_app::ScrollTo>,
) -> usize {
    let shown = |from: usize| {
        let line = layout_line(score, track, width, scale, from);
        let whole = line.bars.len() - usize::from(line.clipped && line.bars.len() > 1);
        (from..from + whole).contains(&focus)
    };
    let count = |from: usize| layout_line(score, track, width, scale, from).bars.len();
    match to {
        Some(cecilia_app::ScrollTo::Top) => focus,
        Some(cecilia_app::ScrollTo::Center) => {
            focus.saturating_sub(count(focus).saturating_sub(1) / 2)
        }
        Some(cecilia_app::ScrollTo::Bottom) => {
            let mut s = focus;
            while s > 0 && shown(s - 1) {
                s -= 1;
            }
            s
        }
        None if focus < scroll => focus,
        None if shown(scroll) => scroll,
        None => {
            // Slide right until the bar is whole.
            let mut s = scroll;
            while s < focus && !shown(s) {
                s += 1;
            }
            s
        }
    }
}

/// Rows used by a system: header + strings + rhythm rows + one row per lyric verse + spacer.
pub fn system_height(track: &Track, mode: TabRhythm) -> u16 {
    track.strings.len().max(1) as u16 + 2 + rhythm::rows(track, mode) + verses(track)
}

/// Number of lyric verses on a track's first voice.
fn verses(track: &Track) -> u16 {
    track
        .bars
        .iter()
        .flat_map(|b| b.voices.first())
        .flat_map(|v| &v.beats)
        .map(|b| {
            b.lyrics
                .iter()
                .rposition(|l| !l.is_empty())
                .map_or(0, |i| i + 1)
        })
        .max()
        .unwrap_or(0)
        .min(5) as u16
}

const LABEL_WIDTH: u16 = 3;

fn beat_width(beat: &Beat, track: &Track, scale: u16) -> u16 {
    let base: u16 = match beat.duration.value {
        DurationValue::Whole => 8,
        DurationValue::Half => 6,
        DurationValue::Quarter => 4,
        _ => 3,
    };
    let fret_w = beat
        .notes
        .iter()
        .map(|n| note_text(n, track).len() as u16)
        .max()
        .unwrap_or(1)
        + 1;
    // Wide enough for the beat's syllables too.
    let lyric_w = beat
        .lyrics
        .iter()
        .map(|l| l.chars().count() as u16 + 1)
        .max()
        .unwrap_or(0);
    (base * scale / 2 + beat.duration.dots as u16)
        .max(fret_w)
        .max(lyric_w)
        .max(2)
}

/// Breaks a track into systems that fit `width` columns.
pub fn layout_systems(score: &Score, track: usize, width: u16, scale: u16) -> Vec<System> {
    layout_from(score, track, width, scale, 0, false)
}

/// The horizontal layout: one system of the bars from `from` on, as many as `width` columns
/// show (the last may be cut off, see [`System::clipped`]).
pub fn layout_line(score: &Score, track: usize, width: u16, scale: u16, from: usize) -> System {
    layout_from(score, track, width, scale, from, true)
        .pop()
        .unwrap_or_default()
}

fn layout_from(
    score: &Score,
    track: usize,
    width: u16,
    scale: u16,
    from: usize,
    horizontal: bool,
) -> Vec<System> {
    let Some(t) = score.tracks.get(track) else {
        return vec![];
    };
    let avail = width.saturating_sub(LABEL_WIDTH + 1).max(8);
    let mut systems = vec![];
    let mut current = System::default();
    let mut x = 0;
    for (bi, bar) in t.bars.iter().enumerate().skip(from) {
        if horizontal && x >= avail {
            break;
        }
        // Column offsets after `lead` columns of bar line.
        let place = |lead: u16| place_bar(bar, t, scale, lead);
        let lead_of = |first| barline::boundary(score, bi, first).len() as u16;
        // The line after the bar if it ends the system (one column is already reserved).
        let tail = barline::tail(score, bi).len() as u16 - 1;
        let mut lead = lead_of(current.bars.is_empty());
        let (mut offs, mut ticks, mut bx) = place(lead);
        if !horizontal && x + bx.min(avail) + tail > avail && !current.bars.is_empty() {
            systems.push(std::mem::take(&mut current));
            x = 0;
            lead = lead_of(true);
            (offs, ticks, bx) = place(lead);
        }
        let mut bw = bx.min(avail);
        if horizontal && x + bx + tail > avail {
            // Runs off the edge: keep what shows.
            bw = (avail - x).min(bx);
            current.clipped = true;
        }
        current.bars.push(BarLayout {
            bar: bi,
            x,
            lead,
            beats: offs,
            ticks,
            width: bw,
        });
        x += bw;
        if current.clipped {
            break;
        }
    }
    if !current.bars.is_empty() {
        systems.push(current);
    }
    systems
}

/// The columns of a bar at `scale`, after `lead` columns of bar line: (offset, width) and start
/// tick of each, and the bar's width. A column is where a beat of any voice starts.
fn place_bar(bar: &Bar, t: &Track, scale: u16, lead: u16) -> (Vec<(u16, u16)>, Vec<u32>, u16) {
    let mut cols: std::collections::BTreeMap<u32, u16> = Default::default();
    for (vi, voice) in bar.voices.iter().enumerate() {
        let mut tick = 0;
        for b in &voice.beats {
            // An unused voice's placeholder takes no room.
            if !(b.empty && vi > 0) {
                let w = cols.entry(tick).or_insert(0);
                *w = (*w).max(beat_width(b, t, scale));
            }
            tick += b.ticks();
        }
    }
    let mut offs = vec![];
    let mut ticks = vec![];
    let mut bx = lead;
    for (&tick, &w) in &cols {
        offs.push((bx, w));
        ticks.push(tick);
        bx += w;
    }
    if offs.is_empty() {
        offs.push((lead, 4 * scale / 2));
        ticks.push(0);
        bx += 4 * scale / 2;
    }
    (offs, ticks, bx)
}

/// The beat of `voice` starting at `tick`.
fn beat_starting(voice: &Voice, tick: u32) -> Option<usize> {
    let mut t = 0;
    voice.beats.iter().position(|b| {
        let hit = t == tick;
        t += b.ticks();
        hit
    })
}

/// The beat of `voice` sounding at `tick` (the last one past the end).
fn beat_sounding(voice: &Voice, tick: u32) -> usize {
    let mut t = 0;
    for (i, b) in voice.beats.iter().enumerate() {
        t += b.ticks();
        if tick < t {
            return i;
        }
    }
    voice.beats.len().saturating_sub(1)
}

/// One line (system) of the page layout as the engraver breaks it: which bars, on which page,
/// how wide each is and how much of the page width the line fills.
#[derive(Debug, Clone, PartialEq)]
pub struct RowSpec {
    pub page: usize,
    pub bars: std::ops::Range<usize>,
    /// Each bar's width on the page (points).
    pub widths: Vec<f64>,
    /// The line's length over the longest line's.
    pub fill: f64,
    /// The line's length on the page (points).
    pub span: f64,
}

/// A design-mode handle in the terminal: a cell (or column) the mouse can press.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum DesignHit {
    /// The "+" at the end of the line starting at bar `first` with `n` bars.
    Add { first: usize, n: usize },
    /// The "-" at the end of that line.
    Remove { first: usize, n: usize },
    /// The bar line after bar `k` of line `system`, dragged `points` page points per column.
    BarLine {
        system: usize,
        k: usize,
        points: f64,
    },
}

/// The bar-to-system assignment of an engraved page layout, so the TUI breaks lines (and pages)
/// where the GUI does, manual line breaks and bar widths of design mode included.
pub fn page_rows(layout: &cecilia_engrave::Layout) -> Vec<RowSpec> {
    let widest = layout
        .systems
        .iter()
        .map(|s| s.x1 - s.x0)
        .fold(0.0, f64::max)
        .max(1.0);
    layout
        .systems
        .iter()
        .map(|s| RowSpec {
            page: s.page,
            bars: s.range(),
            widths: s
                .range()
                .map(|b| layout.bars.get(b).map_or(1.0, |r| (r.2 - r.1).max(1.0)))
                .collect(),
            fill: ((s.x1 - s.x0) / widest).clamp(0.1, 1.0),
            span: (s.x1 - s.x0).max(1.0),
        })
        .collect()
}

/// Lays the track out in the given lines, scaled to `width` columns: each line's bars share
/// its columns in the proportions of the engraved page, and a bar whose beats do not fit
/// closes them up (down to `scale` 1) before it runs over.
pub fn layout_rows(
    score: &Score,
    track: usize,
    width: u16,
    scale: u16,
    rows: &[RowSpec],
) -> Vec<System> {
    let Some(t) = score.tracks.get(track) else {
        return vec![];
    };
    let avail = width.saturating_sub(LABEL_WIDTH + 1).max(8);
    rows.iter()
        .filter(|r| !r.bars.is_empty() && r.bars.end <= t.bars.len())
        .map(|row| {
            let tail = barline::tail(score, row.bars.end - 1).len() as u16 - 1;
            let line = (avail.saturating_sub(tail) as f64 * row.fill) as u16;
            let total: f64 = row.widths.iter().sum::<f64>().max(1.0);
            let mut x = 0u16;
            let mut bars = vec![];
            for (k, bi) in row.bars.clone().enumerate() {
                let lead = barline::boundary(score, bi, k == 0).len() as u16;
                let share = (line as f64 * row.widths[k] / total) as u16;
                let mut sc = scale;
                let (mut offs, mut ticks, mut bx) = place_bar(&t.bars[bi], t, sc, lead);
                while bx > share && sc > 1 {
                    sc -= 1;
                    (offs, ticks, bx) = place_bar(&t.bars[bi], t, sc, lead);
                }
                let width = share.max(bx).min(avail.saturating_sub(x)).max(1);
                bars.push(BarLayout {
                    bar: bi,
                    x,
                    lead,
                    beats: offs,
                    ticks,
                    width,
                });
                x += width;
            }
            System {
                bars,
                page: row.page,
                clipped: false,
            }
        })
        .collect()
}

/// A page's top or bottom border across `outer` on row `y`, with `label` after the corner.
pub(crate) fn page_edge(
    buf: &mut Buffer,
    outer: Rect,
    y: u16,
    (a, b): (char, char),
    label: &str,
    style: Style,
) {
    let mut line = format!("{a}─{label}");
    let fill = (outer.width as usize).saturating_sub(line.chars().count() + 1);
    line.push_str(&"─".repeat(fill));
    line.push(b);
    buf.set_string(outer.x, y, line, style);
}

/// The rows a system takes in the page layout besides its own: a page's top border above its
/// first system, and its bottom border and a blank row below its last.
pub(crate) fn page_pad(systems: &[System], i: usize) -> (u16, u16) {
    let first = i == 0 || systems[i - 1].page != systems[i].page;
    let last = systems.get(i + 1).is_none_or(|n| n.page != systems[i].page);
    (u16::from(first), if last { 2 } else { 0 })
}

/// Systems from `from` that fit whole in `height` rows of the page layout (at least one).
pub fn page_fit(systems: &[System], sh: u16, from: usize, height: u16) -> usize {
    let mut used = 0;
    let mut n = 0;
    for i in from..systems.len() {
        let (before, after) = page_pad(systems, i);
        used += before + sh + after;
        // The blank row after a page is not needed to count it as shown.
        if used.saturating_sub(after.min(1)) > height && n > 0 {
            break;
        }
        n += 1;
    }
    n.max(1)
}

/// The first system to show in the page layout so that `target` is whole on screen: `scroll`
/// if it is already, else the nearest change.
pub fn page_scroll(
    systems: &[System],
    sh: u16,
    scroll: usize,
    height: u16,
    target: usize,
) -> usize {
    if target < scroll {
        return target;
    }
    let mut s = scroll;
    while s < target && target >= s + page_fit(systems, sh, s, height) {
        s += 1;
    }
    s
}

/// The rows of bars a pane of `area` shows from system `scroll`, for half-page movement.
#[allow(clippy::too_many_arguments)]
pub fn view_info(
    score: &Score,
    track: usize,
    area: Rect,
    scale: u16,
    scroll: usize,
    horizontal: bool,
    page: Option<&[RowSpec]>,
    mode: TabRhythm,
) -> cecilia_app::ViewInfo {
    let Some(t) = score.tracks.get(track) else {
        return Default::default();
    };
    if horizontal {
        let line = layout_line(score, track, area.width, scale, scroll);
        return cecilia_app::ViewInfo {
            horizontal: true,
            visible_bars: line.bars.len(),
            ..Default::default()
        };
    }
    let sh = system_height(t, mode);
    let (systems, visible) = match page {
        Some(page) => {
            let systems = layout_rows(score, track, area.width.saturating_sub(4), scale, page);
            let visible = page_fit(&systems, sh, scroll, area.height);
            (systems, visible)
        }
        None => (
            layout_systems(score, track, area.width, scale),
            (area.height / sh).max(1) as usize,
        ),
    };
    let rows: Vec<std::ops::Range<usize>> = systems
        .iter()
        .map(|s| s.bars.first().map_or(0, |b| b.bar)..s.bars.last().map_or(0, |b| b.bar + 1))
        .collect();
    let shown = rows.iter().skip(scroll).take(visible);
    // The most lines on one page: half-page movement counts these, not the lines on screen.
    let page_rows = page.map_or(0, |_| {
        let mut per = std::collections::HashMap::new();
        systems
            .iter()
            .for_each(|s| *per.entry(s.page).or_insert(0) += 1);
        per.into_values().max().unwrap_or(0)
    });
    cecilia_app::ViewInfo {
        horizontal: false,
        visible_rows: shown.clone().count(),
        visible_bars: shown.map(|r| r.len()).sum(),
        rows,
        page_rows,
    }
}

pub fn system_of_bar(systems: &[System], bar: usize) -> usize {
    systems
        .iter()
        .position(|s| s.bars.iter().any(|b| b.bar == bar))
        .unwrap_or(0)
}

/// Tab line a note is shown on: its string, or on drum tracks its instrument's lane.
fn row_of(n: &Note, track: &Track) -> usize {
    if track.kind == TrackKind::Drums {
        cecilia_core::drums::lane(n.fret) as usize
    } else {
        n.string as usize
    }
}

fn note_text(n: &Note, track: &Track) -> String {
    if track.kind == TrackKind::Drums {
        let c = if cecilia_core::drums::is_cymbal(n.fret) {
            "x"
        } else {
            "o"
        };
        let c = if n.accent != cecilia_core::Accent::None {
            c.to_uppercase()
        } else {
            c.to_string()
        };
        return if n.ghost { format!("({c})") } else { c };
    }
    let fret = if n.dead {
        "x".to_string()
    } else if matches!(track.kind, TrackKind::Pitched | TrackKind::Chant) {
        format!("{}{}", pitch_name(n.fret), n.fret as i32 / 12 - 1)
    } else {
        n.fret.to_string()
    };
    let mut s = if n.tie || n.ghost {
        format!("({fret})")
    } else {
        fret
    };
    let fx = &n.effects;
    if fx.harmonic.is_some() {
        s = format!("<{s}>");
    }
    if fx.bend.is_some() {
        s.push('b');
    } else if fx.hammer_pull {
        s.push('h');
    } else if !fx.slides.is_empty() {
        s.push('/');
    } else if fx.vibrato {
        s.push('~');
    }
    s
}

fn pitch_name(midi: u8) -> &'static str {
    [
        "C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B",
    ][midi as usize % 12]
}

/// The first row shown when `zz` / `zt` / `zb` puts the cursor's `row` in the middle, at the
/// top or at the bottom of `visible` rows.
pub fn scroll_for(to: cecilia_app::ScrollTo, row: usize, visible: usize) -> usize {
    match to {
        cecilia_app::ScrollTo::Center => row.saturating_sub((visible - 1) / 2),
        cecilia_app::ScrollTo::Top => row,
        cecilia_app::ScrollTo::Bottom => (row + 1).saturating_sub(visible),
    }
}

pub struct TabView<'a> {
    pub app: &'a App,
    pub pane: u32,
    pub focused: bool,
    /// (bar, tick) being played in this pane's buffer.
    pub play: Option<(usize, u32)>,
    pub hits: &'a mut Vec<Hit>,
    pub scroll: &'a mut usize,
    /// Set to (lines in all, lines shown) for the scrollbar.
    pub extent: &'a mut (usize, usize),
    /// Keep the cursor (or playhead) in view; off after the scrollbar moved the view by hand.
    pub follow: bool,
    /// The page layout's lines as the engraver breaks them (see [`page_rows`]); none: the
    /// pane's layout is not "page", or wraps to the width.
    pub page: Option<&'a [RowSpec]>,
    /// Design-mode handles drawn this frame.
    pub design: &'a mut Vec<(Rect, DesignHit)>,
}

impl Widget for TabView<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let th = &self.app.config.theme;
        let pane = &self.app.panes[&self.pane];
        let score = &self.app.buffers[pane.buffer].score;
        let Some(track) = score.tracks.get(pane.cursor.track) else {
            let msg = "No score loaded — open one with :open <file>";
            buf.set_string(area.x + 1, area.y, msg, Style::default().fg(rc(th.dim)));
            return;
        };
        let scale = self.app.config.file.ui.tab_width.clamp(1, 8) as u16;
        let mode = TabRhythm::parse(&self.app.config.file.ui.tab_rhythm);
        let sh = system_height(track, mode);
        let rows = track.strings.len().max(1);
        let rhythm_rows = rhythm::rows(track, mode);
        let icons = self.app.config.file.tabline.icons.as_str();
        let visible = (area.height / sh).max(1) as usize;
        // The page layout draws each page in a box: the score sits two columns inside it.
        let paged = pane.layout == "page" && self.page.is_some();
        let outer = area;
        let area = if paged {
            Rect {
                x: area.x + 2,
                width: area.width.saturating_sub(4),
                ..area
            }
        } else {
            area
        };
        // Follow the playhead while playing (if following), otherwise the cursor.
        let focus_bar = match self
            .play
            .filter(|_| self.app.config.file.ui.follow_playback)
        {
            Some((bar, _)) => bar,
            None => pane.cursor.bar,
        };
        let (systems, scroll, first_row) = if pane.layout == "horizontal" {
            // One line of bars; the scroll is the first bar shown.
            let last = track.bars.len().saturating_sub(1);
            let mut scroll = (*self.scroll).min(last);
            if self.follow || pane.scroll_request.is_some() {
                let (tr, w, to) = (pane.cursor.track, area.width, pane.scroll_request);
                scroll = scroll_bars(score, tr, w, scale, scroll, focus_bar.min(last), to);
            }
            let line = layout_line(score, pane.cursor.track, area.width, scale, scroll);
            *self.extent = (0, 0);
            (vec![line], scroll, 0)
        } else if let Some(page) = self.page.filter(|_| paged) {
            let systems = layout_rows(score, pane.cursor.track, area.width, scale, page);
            let cursor_sys = system_of_bar(&systems, focus_bar);
            let mut scroll = (*self.scroll).min(systems.len().saturating_sub(1));
            if let Some(to) = pane.scroll_request {
                scroll = scroll_for(
                    to,
                    cursor_sys,
                    page_fit(&systems, sh, cursor_sys, area.height),
                );
            } else if self.follow {
                scroll = page_scroll(&systems, sh, scroll, area.height, cursor_sys);
            }
            *self.extent = (systems.len(), page_fit(&systems, sh, scroll, area.height));
            (systems, scroll, scroll)
        } else {
            let systems = layout_systems(score, pane.cursor.track, area.width, scale);
            let cursor_sys = system_of_bar(&systems, focus_bar);
            let mut scroll = (*self.scroll).min(systems.len().saturating_sub(1));
            if let Some(to) = pane.scroll_request {
                scroll = scroll_for(to, cursor_sys, visible);
            } else if !self.follow {
                scroll = scroll.min(systems.len().saturating_sub(visible));
            } else if cursor_sys < scroll {
                scroll = cursor_sys;
            } else if cursor_sys >= scroll + visible {
                scroll = cursor_sys + 1 - visible;
            }
            *self.extent = (systems.len(), visible);
            (systems, scroll, scroll)
        };
        *self.scroll = scroll;

        let dim = Style::default().fg(rc(th.dim));
        let line_style = Style::default().fg(rc(th.chrome_text.mix(th.dim, 0.6)));
        let on = |bg| Style::default().fg(rc(th.text_on(bg))).bg(rc(bg));
        let cursor_style = if self.focused {
            on(self.app.mode_color())
        } else {
            on(th.dim)
        };
        let playhead_style = on(th.playhead);
        // While `'` waits for a mark letter: where each mark would jump to.
        let above = self.app.config.file.ui.mark_labels == cecilia_app::config::MarkLabels::Above;
        let jump_targets = if self.focused {
            self.app.jump_targets(pane.buffer)
        } else {
            Vec::new()
        };

        // Where each shown system starts (relative to the pane): stacked, with the page layout's
        // borders between pages.
        let mut placed = vec![];
        let mut dy = 0;
        for row in first_row..systems.len() {
            let (before, after) = if paged {
                page_pad(&systems, row)
            } else {
                (0, 0)
            };
            if row > first_row
                && (placed.len() >= visible && !paged || dy + before + sh > area.height + 1)
            {
                break;
            }
            placed.push((row, dy + before, before > 0, after > 0));
            dy += before + sh + after;
        }
        let design_on = paged && self.focused && self.app.design_active();
        let page_style = Style::default().fg(rc(th.chrome_text.mix(th.dim, 0.5)));
        let width_of = |r: Rect| r.x + r.width;
        for (row, dy, top, bottom) in placed {
            let system = &systems[row];
            let y0 = area.y + dy;
            if y0 + sh > area.y + area.height + 1 {
                break;
            }
            if paged {
                let (l, r) = (outer.x, width_of(outer).saturating_sub(1));
                if top {
                    let label = format!(" Page {} ", system.page + 1);
                    page_edge(buf, outer, y0 - 1, ('┌', '┐'), &label, page_style);
                }
                for y in y0..(y0 + sh).min(outer.y + outer.height) {
                    buf.set_string(l, y, "│", page_style);
                    buf.set_string(r, y, "│", page_style);
                }
                if bottom && y0 + sh < outer.y + outer.height {
                    page_edge(buf, outer, y0 + sh, ('└', '┘'), "", page_style);
                }
            }
            let top_y = y0;
            // String labels.
            for (s, &open) in track.strings.iter().enumerate() {
                let label = if track.kind == TrackKind::Drums {
                    cecilia_core::drums::LANES.get(s).copied().unwrap_or("")
                } else {
                    pitch_name(open)
                };
                buf.set_string(area.x, y0 + 1 + s as u16, format!("{label:>2}"), dim);
            }
            if track.strings.is_empty() {
                buf.set_string(area.x, y0 + 1, " ♪", dim);
            }
            let x0 = area.x + LABEL_WIDTH;
            for bl in &system.bars {
                let bx = x0 + bl.x;
                let mb = &score.master_bars[bl.bar];
                // Header: bar number, section marker, time signature changes.
                let mut header = format!("{}", bl.bar + 1);
                let prev_ts = bl
                    .bar
                    .checked_sub(1)
                    .map(|p| score.master_bars[p].time_signature);
                if prev_ts != Some(mb.time_signature) {
                    header.push_str(&format!(
                        " {}/{}",
                        mb.time_signature.numerator, mb.time_signature.denominator
                    ));
                }
                if let Some(m) = &mb.marker {
                    header.push_str(&format!(" [{}]", m.title));
                }
                let header: String = header.chars().take(bl.width as usize).collect();
                buf.set_string(bx, top_y, header, Style::default().fg(rc(th.marker)));
                // Staff lines and bar lines; a bar without its full count has red staff lines
                // when `ui.highlight_incomplete_bars` is on.
                let staff_style = if self.app.config.file.ui.highlight_incomplete_bars
                    && score.bar_incomplete(pane.cursor.track, bl.bar)
                {
                    Style::default().fg(rc(th.incomplete_bar))
                } else {
                    dim
                };
                // Staff lines, then the bar line opening the bar over their start.
                let first = system.bars.first().is_some_and(|f| f.bar == bl.bar);
                let cols = barline::boundary(score, bl.bar, first);
                for s in 0..rows {
                    let y = y0 + 1 + s as u16;
                    for dx in bl.lead..bl.width {
                        buf.set_string(bx + dx, y, "─", staff_style);
                    }
                    for (i, &c) in cols.iter().enumerate() {
                        let (l, r) = barline::arms(cols, i, first, false);
                        let g = barline::glyph(c, s, rows, l, r).to_string();
                        buf.set_string(bx + i as u16, y, g, line_style);
                    }
                }
                let bar_voices = &track.bars[bl.bar].voices;
                let beats = bar_voices.first().map_or(&[][..], |v| &v.beats[..]);
                let cur_voice = pane.cursor.voice;
                for (i, &(off, w)) in bl.beats.iter().enumerate() {
                    if off + w > bl.width + 1 {
                        break;
                    }
                    let tick = bl.ticks[i];
                    // The beat each voice starts in this column; the first voice's drives the
                    // text under and over the staff.
                    let starts: Vec<Option<usize>> =
                        bar_voices.iter().map(|v| beat_starting(v, tick)).collect();
                    let beat = starts.first().copied().flatten().and_then(|k| beats.get(k));
                    let bx_beat = bx + off;
                    let len = bar_voices
                        .iter()
                        .zip(&starts)
                        .filter_map(|(v, k)| k.map(|k| v.beats[k].ticks()))
                        .max()
                        .unwrap_or(0);
                    // The cursor's voice: its beat here, else the one sounding.
                    let cur_beat = bar_voices.get(cur_voice).map(|v| {
                        starts
                            .get(cur_voice)
                            .copied()
                            .flatten()
                            .unwrap_or_else(|| beat_sounding(v, tick))
                    });
                    let cur_here = bar_voices
                        .get(cur_voice)
                        .and_then(|_| starts.get(cur_voice).copied().flatten());
                    // Column of the playhead inside this beat, in proportion to time.
                    let playhead = self.play.and_then(|(pb, pt)| {
                        (pb == bl.bar
                            && pt >= tick
                            && (pt < tick + len || (len == 0 && pt == tick)))
                            .then(|| {
                                let frac = (pt - tick) as f64 / len.max(1) as f64;
                                bx_beat + (frac * w as f64) as u16
                            })
                    });
                    let is_cursor_beat = pane.cursor.bar == bl.bar
                        && cur_here.is_some_and(|k| k == pane.cursor.beat);
                    let selected = self.focused
                        && cur_beat.is_some()
                        && self.app.selection().is_some_and(|(a, c, bars)| {
                            let (lo, hi) = if (a.bar, a.beat) <= (c.bar, c.beat) {
                                (a, c)
                            } else {
                                (c, a)
                            };
                            let k = cur_here.unwrap_or(0);
                            if bars {
                                (lo.bar..=hi.bar).contains(&bl.bar)
                            } else {
                                cur_here.is_some()
                                    && (lo.bar, lo.beat) <= (bl.bar, k)
                                    && (bl.bar, k) <= (hi.bar, hi.beat)
                            }
                        });
                    for s in 0..rows {
                        let y = y0 + 1 + s as u16;
                        let cell = Rect {
                            x: bx_beat,
                            y,
                            width: w,
                            height: 1,
                        };
                        self.hits.push(Hit {
                            area: cell,
                            pane: self.pane,
                            cursor: Cursor {
                                track: pane.cursor.track,
                                bar: bl.bar,
                                beat: cur_beat.unwrap_or(i),
                                string: s,
                                voice: cur_voice.min(bar_voices.len().saturating_sub(1)),
                                staff: Cursor::HOME,
                                line: 0,
                            },
                        });
                        if selected {
                            buf.set_style(
                                cell,
                                Style::default()
                                    .bg(rc(th.selection.mix(th.chrome, th.selection_mix()))),
                            );
                        }
                        // Every voice's note on the string; the cursor's voice is drawn last, on top.
                        let order = (0..bar_voices.len())
                            .filter(|&v| v != cur_voice)
                            .chain((cur_voice < bar_voices.len()).then_some(cur_voice));
                        for v in order {
                            let Some(n) = starts[v].and_then(|k| {
                                bar_voices[v].beats[k]
                                    .notes
                                    .iter()
                                    .find(|n| row_of(n, track) == s)
                            }) else {
                                continue;
                            };
                            let style = if n.ghost || n.tie || v != cur_voice {
                                dim
                            } else {
                                Style::default().fg(rc(th.chrome_text))
                            };
                            buf.set_string(
                                bx_beat,
                                y,
                                note_text(n, track),
                                style.add_modifier(Modifier::BOLD),
                            );
                        }
                        if is_cursor_beat
                            && pane.cursor.string == s
                            && !self.app.cursor_hidden(pane.buffer)
                        {
                            let text_w = cur_here
                                .and_then(|k| bar_voices[cur_voice].beats.get(k))
                                .and_then(|b| b.notes.iter().find(|n| row_of(n, track) == s))
                                .map_or(1, |n| note_text(n, track).len() as u16);
                            buf.set_style(
                                Rect {
                                    x: bx_beat,
                                    y,
                                    width: text_w,
                                    height: 1,
                                },
                                cursor_style,
                            );
                        }
                    }
                    // Above: on the bar's header row; exact: on the mark's string. Marks on one
                    // row share a label.
                    let here = jump_targets
                        .iter()
                        .filter(|(_, c)| {
                            c.track == pane.cursor.track && c.bar == bl.bar && c.beat == i
                        })
                        .map(|(name, c)| {
                            let y = if above {
                                top_y
                            } else {
                                y0 + 1 + c.string.min(rows - 1) as u16
                            };
                            (*name, y)
                        });
                    for (name, y) in cecilia_app::marks::merge_labels(here) {
                        buf.set_string(
                            bx_beat,
                            y,
                            name,
                            Style::default()
                                .fg(rc(th.playhead))
                                .add_modifier(Modifier::BOLD),
                        );
                    }
                    // Clicking the rhythm under the strings picks the beat too.
                    for r in 0..rhythm_rows {
                        self.hits.push(Hit {
                            area: Rect {
                                x: bx_beat,
                                y: y0 + 1 + rows as u16 + r,
                                width: w,
                                height: 1,
                            },
                            pane: self.pane,
                            cursor: Cursor {
                                track: pane.cursor.track,
                                bar: bl.bar,
                                beat: i,
                                string: pane.cursor.string.min(rows - 1),
                                voice: 0,
                                staff: Cursor::HOME,
                                line: 0,
                            },
                        });
                    }
                    // Lyrics under the staff, one row per verse.
                    for (li, syl) in beat
                        .map_or(&[][..], |b| &b.lyrics[..])
                        .iter()
                        .enumerate()
                        .take(5)
                    {
                        if !syl.is_empty() {
                            let room = (area.x + area.width).saturating_sub(bx_beat) as usize;
                            let t: String = syl.chars().take(room).collect();
                            buf.set_string(
                                bx_beat,
                                y0 + 1 + rows as u16 + rhythm_rows + li as u16,
                                t,
                                Style::default().fg(rc(th.lyrics)),
                            );
                        }
                    }
                    // Beat text above the staff, if room.
                    if let Some(text) = beat.and_then(|b| b.text.as_deref()) {
                        if bl.beats.len() > 1 || bl.width > 6 {
                            let t: String = text.chars().take(w as usize).collect();
                            buf.set_string(
                                bx_beat,
                                top_y + sh - 1,
                                t,
                                Style::default().fg(rc(th.marker)),
                            );
                        }
                    }
                    if let Some(px) = playhead.filter(|&x| x < area.x + area.width) {
                        for s in 0..rows as u16 {
                            let cell = Rect {
                                x: px,
                                y: y0 + 1 + s,
                                width: 1,
                                height: 1,
                            };
                            buf.set_style(cell, playhead_style);
                        }
                    }
                }
                // Tab+ rhythm: one glyph per beat under the strings, beams under the glyphs.
                if rhythm_rows > 0 {
                    // The first voice's beats, at the columns they start in.
                    let mut t0 = 0;
                    let placed: Vec<(u16, u16)> = beats
                        .iter()
                        .map(|b| {
                            let k = bl.ticks.iter().position(|&c| c == t0).unwrap_or(0);
                            t0 += b.ticks();
                            bl.beats[k]
                        })
                        .collect();
                    let cols: Vec<u16> = placed.iter().map(|b| b.0).collect();
                    buf.set_string(
                        bx,
                        y0 + 2 + rows as u16,
                        rhythm::beams(beats, &cols, bl.width, icons),
                        Style::default().fg(rc(th.chrome_text)),
                    );
                    let rstyle = Style::default().fg(rc(th.chrome_text));
                    for (i, &(off, w)) in placed.iter().enumerate() {
                        if off + w > bl.width + 1 {
                            break;
                        }
                        if let Some(b) = beats.get(i) {
                            buf.set_stringn(
                                bx + off,
                                y0 + 1 + rows as u16,
                                rhythm::glyph(b, icons),
                                w as usize,
                                rstyle,
                            );
                        }
                    }
                }
                // Repeat count above the closing repeat line.
                if let Some(n) = mb.repeat_end {
                    buf.set_string(
                        (bx + bl.width).saturating_sub(3),
                        top_y,
                        format!("x{n}"),
                        Style::default().fg(rc(th.marker)),
                    );
                }
            }
            // Design mode: the handles over the line, as in the GUI. Bar lines drag, the "+" and
            // "-" at the line's end pull a bar up or push one down, and a manual line break is
            // marked in the margin.
            if design_on {
                let first = system.bars.first().map_or(0, |b| b.bar);
                let n = system.bars.len();
                let cols = system.bars.last().map_or(1, |b| b.x + b.width).max(1);
                let points =
                    self.page.and_then(|p| p.get(row)).map_or(1.0, |r| r.span) / cols as f64;
                if score.design.line_start(first).is_some() {
                    buf.set_string(outer.x + 1, top_y, "↵", on(th.marker));
                }
                for (k, bl) in system.bars.iter().enumerate().skip(1) {
                    self.design.push((
                        Rect::new(x0 + bl.x, y0 + 1, 1, rows as u16),
                        DesignHit::BarLine {
                            system: row,
                            k: k - 1,
                            points,
                        },
                    ));
                }
                let (add, remove) = (first + n < track.bars.len(), n > 1);
                let bx = area.x + area.width;
                let lit = on(self.app.mode_color());
                let y_minus = y0 + 1 + u16::from(rows > 1);
                buf.set_string(bx, y0 + 1, "+", if add { lit } else { dim });
                buf.set_string(bx, y_minus, "-", if remove { lit } else { dim });
                if add {
                    let hit = DesignHit::Add { first, n };
                    self.design.push((Rect::new(bx, y0 + 1, 1, 1), hit));
                }
                if remove {
                    let hit = DesignHit::Remove { first, n };
                    self.design.push((Rect::new(bx, y_minus, 1, 1), hit));
                }
            }
            // Bar line closing the system.
            if let Some(last) = system.bars.last() {
                let x = x0 + last.x + last.width;
                let cols = barline::tail(score, last.bar);
                for (i, &c) in cols.iter().enumerate() {
                    let cx = x + i as u16;
                    if cx >= area.x + area.width {
                        break;
                    }
                    for s in 0..rows {
                        let (l, r) = barline::arms(cols, i, false, true);
                        let g = barline::glyph(c, s, rows, l, r).to_string();
                        buf.set_string(cx, y0 + 1 + s as u16, g, line_style);
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cecilia_core::*;

    #[test]
    fn systems_wrap_to_width() {
        let beat = Beat {
            notes: vec![Note::new(0, 12)],
            ..Beat::default()
        };
        let bar = Bar {
            voices: vec![Voice {
                beats: vec![beat; 4],
            }],
            ..Default::default()
        };
        let track = Track {
            strings: vec![64; 6],
            bars: vec![bar; 10],
            ..Track::default()
        };
        let score = Score {
            master_bars: vec![MasterBar::default(); 10],
            tracks: vec![track],
            ..Score::default()
        };
        let systems = layout_systems(&score, 0, 60, 2);
        // Each bar: 1 + 4 * 4 = 17 columns; 56 available → 3 bars per system.
        assert_eq!(systems[0].bars.len(), 3);
        assert_eq!(systems.len(), 4);
        assert_eq!(system_of_bar(&systems, 9), 3);
    }

    #[test]
    fn repeat_lines_widen_the_bar_columns() {
        let beat = Beat {
            notes: vec![Note::new(0, 12)],
            ..Beat::default()
        };
        let bar = Bar {
            voices: vec![Voice { beats: vec![beat] }],
            ..Default::default()
        };
        let track = Track {
            strings: vec![64; 6],
            bars: vec![bar; 4],
            ..Track::default()
        };
        let mut score = Score {
            master_bars: vec![MasterBar::default(); 4],
            tracks: vec![track],
            ..Score::default()
        };
        let lead = |s: &Score, w| {
            let sys = layout_systems(s, 0, w, 2);
            sys.iter()
                .flat_map(|s| &s.bars)
                .map(|b| b.lead)
                .collect::<Vec<_>>()
        };
        assert_eq!(lead(&score, 80), [1, 1, 1, 1]);
        score.master_bars[1].repeat_end = Some(2);
        score.master_bars[2].repeat_start = true;
        // Repeat end joined with repeat start: dots, thin, heavy, thin, dots.
        assert_eq!(lead(&score, 80), [1, 1, 5, 1]);
        score.master_bars[2].repeat_start = false;
        assert_eq!(lead(&score, 80), [1, 1, 3, 1]);
        score.master_bars[1].repeat_end = None;
        score.master_bars[1].repeat_start = true;
        assert_eq!(lead(&score, 80), [1, 3, 1, 1]);
        // Beat offsets follow the lead, so clicks and the cursor stay on the beats.
        let sys = layout_systems(&score, 0, 80, 2);
        assert_eq!(sys[0].bars[1].beats[0].0, 3);
        assert_eq!(sys[0].bars[1].width, 3 + sys[0].bars[1].beats[0].1);
        // The last bar ends with a two-column final line: it needs a column more to fit.
        let narrow = layout_systems(&score, 0, 4 + 4 * 5 + 2, 2);
        assert!(narrow.len() > 1);
    }

    fn ten_bars() -> Score {
        let beat = Beat {
            notes: vec![Note::new(0, 12)],
            ..Beat::default()
        };
        let bar = Bar {
            voices: vec![Voice {
                beats: vec![beat; 4],
            }],
            ..Default::default()
        };
        let track = Track {
            strings: vec![64; 6],
            bars: vec![bar; 10],
            ..Track::default()
        };
        Score {
            master_bars: vec![MasterBar::default(); 10],
            tracks: vec![track],
            ..Score::default()
        }
    }

    #[test]
    fn horizontal_is_one_line_that_follows_the_cursor() {
        let score = ten_bars();
        // 17 columns a bar, 56 available: three whole bars, the fourth cut off.
        let line = layout_line(&score, 0, 60, 2, 0);
        assert_eq!(line.bars.len(), 4);
        assert!(line.clipped);
        assert_eq!(layout_systems(&score, 0, 60, 2).len(), 4);
        let at = |scroll, focus, to| scroll_bars(&score, 0, 60, 2, scroll, focus, to);
        assert_eq!(at(0, 2, None), 0);
        assert_eq!(at(0, 3, None), 1);
        assert_eq!(at(5, 2, None), 2);
        assert_eq!(at(0, 9, None), 7);
        assert_eq!(at(0, 5, Some(cecilia_app::ScrollTo::Top)), 5);
        assert_eq!(at(0, 5, Some(cecilia_app::ScrollTo::Bottom)), 3);
        let tail = layout_line(&score, 0, 60, 2, 7);
        assert_eq!(tail.bars.len(), 3);
        assert!(!tail.clipped);
        assert!(
            view_info(
                &score,
                0,
                Rect::new(0, 0, 60, 20),
                2,
                0,
                true,
                None,
                TabRhythm::Auto
            )
            .horizontal
        );
    }

    #[test]
    fn page_lines_follow_the_engraver_and_scale_to_the_width() {
        let mut score = ten_bars();
        let layout = cecilia_engrave::engrave(&score, 0, &cecilia_engrave::Options::default());
        let rows = page_rows(&layout);
        // Every bar is on exactly one line, in order, as the engraver has it.
        let ranges: Vec<_> = rows.iter().map(|r| r.bars.clone()).collect();
        let engraved: Vec<_> = layout.systems.iter().map(|s| s.range()).collect();
        assert_eq!(ranges, engraved);
        assert_eq!(ranges.iter().map(|r| r.len()).sum::<usize>(), 10);
        // A manual line start splits the line where the engraver does.
        score.design.set_line_start(2, Some(None));
        let layout = cecilia_engrave::engrave(&score, 0, &cecilia_engrave::Options::default());
        let rows = page_rows(&layout);
        assert!(rows.iter().any(|r| r.bars.start == 2));
        // Laid out in columns: the same bars per line, none wider than the pane.
        for width in [60u16, 100, 160] {
            let systems = layout_rows(&score, 0, width, 2, &rows);
            assert_eq!(systems.len(), rows.len());
            for (s, r) in systems.iter().zip(&rows) {
                let bars: Vec<_> = s.bars.iter().map(|b| b.bar).collect();
                assert_eq!(bars, r.bars.clone().collect::<Vec<_>>());
                let last = s.bars.last().unwrap();
                assert!(
                    last.x + last.width < width,
                    "{width}: {}",
                    last.x + last.width
                );
            }
        }
        // Wider bars on the page get more columns.
        let mut wide = rows.clone();
        wide[0].widths[0] *= 3.0;
        let s = layout_rows(&score, 0, 160, 2, &wide);
        assert!(s[0].bars[0].width > s[0].bars[1].width);
    }

    #[test]
    fn page_scrolling_counts_the_borders() {
        let mk = |pages: &[usize]| -> Vec<System> {
            pages
                .iter()
                .map(|&p| System {
                    page: p,
                    ..Default::default()
                })
                .collect()
        };
        let systems = mk(&[0, 0, 1, 1, 1, 2]);
        // A system of 8 rows: page 0 takes 1 + 8 + 8 + 2 rows, so 19 rows show both of its
        // lines and 18 only the first (the bottom border and blank row come after).
        assert_eq!(page_fit(&systems, 8, 0, 19), 2);
        assert_eq!(page_fit(&systems, 8, 0, 9), 1);
        assert_eq!(page_fit(&systems, 8, 0, 1), 1);
        // The cursor line scrolls into view, and back.
        assert_eq!(page_scroll(&systems, 8, 0, 19, 1), 0);
        assert!(page_scroll(&systems, 8, 0, 19, 4) > 0);
        assert_eq!(page_scroll(&systems, 8, 3, 19, 1), 1);
        let s = page_scroll(&systems, 8, 0, 19, 5);
        assert!(s <= 5 && 5 < s + page_fit(&systems, 8, s, 19));
    }

    #[test]
    fn note_markers() {
        let mut n = Note::new(0, 7);
        n.effects.hammer_pull = true;
        assert_eq!(note_text(&n, &Track::default()), "7h");
        n.tie = true;
        assert_eq!(note_text(&n, &Track::default()), "(7)h");
    }
}

#[cfg(test)]
mod scroll_tests {
    use super::scroll_for;
    use cecilia_app::ScrollTo::*;

    #[test]
    fn scroll_for_places_the_row() {
        assert_eq!(scroll_for(Center, 10, 5), 8);
        assert_eq!(scroll_for(Center, 1, 5), 0);
        assert_eq!(scroll_for(Top, 10, 5), 10);
        assert_eq!(scroll_for(Bottom, 10, 5), 6);
        assert_eq!(scroll_for(Bottom, 2, 5), 0);
    }
}
