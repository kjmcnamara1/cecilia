//! Lays out one track as standard notation and/or tablature.

use std::collections::HashMap;

use kurbo::{BezPath, Point, Rect};
use muse_core::*;

use crate::display::{Align, BeatRef, Color, FontKind, Hit, Item, Page};
use crate::fonts::fonts;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Mode {
    /// Paginated for printing.
    Page { width: f64, height: f64 },
    /// Systems wrap to `width` on one tall canvas.
    Vertical { width: f64 },
    /// One endless system.
    Horizontal,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Options {
    pub mode: Mode,
    /// Staff space in points.
    pub sp: f64,
    pub standard: bool,
    pub tab: bool,
    /// Title block above the first system.
    pub header: bool,
    pub margin: f64,
    /// Show chant tracks in square notation (otherwise as their modern transcription).
    pub chant: bool,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            mode: Mode::Page {
                width: 595.0,
                height: 842.0,
            },
            sp: 5.0,
            standard: true,
            tab: true,
            header: true,
            margin: 40.0,
            chant: true,
        }
    }
}

/// Where a beat was drawn, for cursors.
#[derive(Debug, Clone, PartialEq)]
pub struct BeatPos {
    pub page: usize,
    pub x: f64,
    pub width: f64,
    pub top: f64,
    pub bottom: f64,
    /// Y of each tab line (model string order), empty without tablature.
    pub strings: Vec<f64>,
}

#[derive(Debug, Clone, Default)]
pub struct Layout {
    pub pages: Vec<Page>,
    pub beats: HashMap<BeatRef, BeatPos>,
    /// Page index and x range of every bar.
    pub bars: Vec<(usize, f64, f64)>,
}

// SMuFL code points.
const G_CLEF: char = '\u{E050}';
const G_CLEF_8VB: char = '\u{E052}';
const F_CLEF: char = '\u{E062}';
const F_CLEF_8VB: char = '\u{E064}';
const PERC_CLEF: char = '\u{E069}';
const TAB_CLEF: char = '\u{E06D}';
const HEAD_WHOLE: char = '\u{E0A2}';
const HEAD_HALF: char = '\u{E0A3}';
const HEAD_BLACK: char = '\u{E0A4}';
const HEAD_X: char = '\u{E0A9}';
const FLAT: char = '\u{E260}';
const NATURAL: char = '\u{E261}';
const SHARP: char = '\u{E262}';
const DOT: char = '\u{E1E7}';
const METRONOME_QUARTER: char = '\u{E1D5}';

fn time_digit(d: u32) -> char {
    char::from_u32(0xE080 + d).unwrap_or('0')
}

fn tuplet_digit(d: u32) -> char {
    char::from_u32(0xE880 + d).unwrap_or('0')
}

fn rest_glyph(v: DurationValue) -> char {
    char::from_u32(0xE4E3 + v as u32).unwrap_or('\u{E4E5}')
}

/// Number of flags/beams of a note value (eighth = 1).
fn beam_count(v: DurationValue) -> usize {
    (v as usize).saturating_sub(2)
}

fn flag_glyph(v: DurationValue, up: bool) -> char {
    let base = 0xE240 + 2 * (beam_count(v).saturating_sub(1) as u32);
    char::from_u32(base + u32::from(!up)).unwrap_or('\u{E240}')
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Clef {
    Treble8vb,
    Treble,
    Bass8vb,
    Bass,
    Percussion,
}

impl Clef {
    fn for_track(t: &Track) -> Clef {
        match t.kind {
            TrackKind::Drums => Clef::Percussion,
            TrackKind::Fretted if t.strings.iter().copied().min().unwrap_or(40) < 36 => {
                Clef::Bass8vb
            }
            TrackKind::Fretted => Clef::Treble8vb,
            _ => {
                let pitches: Vec<u32> = t
                    .bars
                    .iter()
                    .flat_map(|b| &b.voices)
                    .flat_map(|v| &v.beats)
                    .flat_map(|b| &b.notes)
                    .map(|n| n.fret as u32)
                    .collect();
                let avg = pitches.iter().sum::<u32>() / pitches.len().max(1) as u32;
                if !pitches.is_empty() && avg < 55 {
                    Clef::Bass
                } else {
                    Clef::Treble
                }
            }
        }
    }

    /// Diatonic number of the bottom staff line (octave * 7 + step).
    fn bottom(self) -> i32 {
        match self {
            Clef::Treble | Clef::Treble8vb | Clef::Percussion => 4 * 7 + 2,
            Clef::Bass | Clef::Bass8vb => 2 * 7 + 4,
        }
    }

    /// Semitones added to sounding pitch to get the written pitch.
    fn transpose(self) -> i32 {
        match self {
            Clef::Treble8vb | Clef::Bass8vb => 12,
            _ => 0,
        }
    }

    fn glyph(self) -> (char, i32) {
        // Glyph and the staff position its origin sits on.
        match self {
            Clef::Treble => (G_CLEF, 2),
            Clef::Treble8vb => (G_CLEF_8VB, 2),
            Clef::Bass => (F_CLEF, 6),
            Clef::Bass8vb => (F_CLEF_8VB, 6),
            Clef::Percussion => (PERC_CLEF, 4),
        }
    }

    fn key_offset(self) -> i32 {
        match self {
            Clef::Bass | Clef::Bass8vb => -2,
            _ => 0,
        }
    }
}

/// Step (0 = C) and alteration of a pitch, spelled for a key.
fn spell(midi: i32, key: i8) -> (i32, i32, i32) {
    const SHARPS: [(i32, i32); 12] = [
        (0, 0),
        (0, 1),
        (1, 0),
        (1, 1),
        (2, 0),
        (3, 0),
        (3, 1),
        (4, 0),
        (4, 1),
        (5, 0),
        (5, 1),
        (6, 0),
    ];
    const FLATS: [(i32, i32); 12] = [
        (0, 0),
        (1, -1),
        (1, 0),
        (2, -1),
        (2, 0),
        (3, 0),
        (4, -1),
        (4, 0),
        (5, -1),
        (5, 0),
        (6, -1),
        (6, 0),
    ];
    let pc = midi.rem_euclid(12) as usize;
    let octave = midi.div_euclid(12) - 1;
    // Flat keys use flats; in C, B♭ is far commoner than A♯.
    let (step, alter) = if key < 0 || (key == 0 && pc == 10) {
        FLATS[pc]
    } else {
        SHARPS[pc]
    };
    (step, alter, octave)
}

/// Alteration each step letter gets from a key signature.
fn key_alters(key: i8) -> [i32; 7] {
    let mut out = [0; 7];
    const SHARP_ORDER: [usize; 7] = [3, 0, 4, 1, 5, 2, 6];
    const FLAT_ORDER: [usize; 7] = [6, 2, 5, 1, 4, 0, 3];
    for i in 0..key.unsigned_abs().min(7) as usize {
        if key > 0 {
            out[SHARP_ORDER[i]] = 1;
        } else {
            out[FLAT_ORDER[i]] = -1;
        }
    }
    out
}

fn drum_position(key: u8) -> (i32, bool) {
    // (staff position on the percussion staff, x notehead)
    match key {
        35 | 36 => (1, false),
        37..=40 => (5, false),
        41 | 43 => (2, false),
        45 | 47 => (6, false),
        48 | 50 => (7, false),
        42 | 46 => (9, true),
        44 => (-1, true),
        49 | 57 | 52 | 55 => (10, true),
        51 | 53 | 59 => (8, true),
        _ => (8, true),
    }
}

/// A beat's place in a measure.
struct Column {
    tick: u32,
    width: f64,
}

struct MeasurePlan {
    bar: usize,
    columns: Vec<Column>,
    /// Width before the first column (clef/key/time changes and padding).
    lead: f64,
    natural: f64,
}

struct Ctx<'a> {
    score: &'a Score,
    track: &'a Track,
    track_index: usize,
    o: Options,
    clef: Clef,
    sp: f64,
    standard: bool,
    tab: bool,
    lyrics: usize,
    nh_width: f64,
}

/// Note positions on the standard staff, for ties.
#[derive(Clone, Copy)]
struct Placed {
    page: usize,
    system: usize,
    x: f64,
    y: f64,
    tab_y: Option<f64>,
    above: bool,
}

pub fn engrave(score: &Score, track_index: usize, options: &Options) -> Layout {
    let Some(track) = score.tracks.get(track_index) else {
        return Layout::default();
    };
    if let Some(chant) = track.chant.as_deref().filter(|_| options.chant) {
        return crate::chant::engrave_chant(score, track_index, track, chant, options);
    }
    let sp = options.sp;
    let standard = options.standard || !matches!(track.kind, TrackKind::Fretted);
    let tab = options.tab && track.kind == TrackKind::Fretted && !track.strings.is_empty();
    let lyrics = track
        .bars
        .iter()
        .flat_map(|b| &b.voices)
        .flat_map(|v| &v.beats)
        .map(|b| b.lyrics.len())
        .max()
        .unwrap_or(0);
    let nh_width = fonts().bbox(FontKind::Music, HEAD_BLACK, 4.0 * sp).width();
    let ctx = Ctx {
        score,
        track,
        track_index,
        o: *options,
        clef: Clef::for_track(track),
        sp,
        standard: standard || !tab,
        tab,
        lyrics,
        nh_width,
    };
    ctx.run()
}

impl Ctx<'_> {
    fn staff_y(&self, bottom: f64, p: i32) -> f64 {
        bottom - p as f64 * self.sp / 2.0
    }

    /// Vertical metrics of a system relative to its top: (standard top, tab top, height).
    fn metrics(&self) -> (f64, f64, f64) {
        let sp = self.sp;
        let std_top = 10.0 * sp;
        let std_height = if self.standard { 4.0 * sp } else { 0.0 };
        let lyric_space = self.lyrics as f64 * 1.8 * sp;
        let tab_top = if self.standard {
            std_top + std_height + 6.0 * sp + lyric_space
        } else {
            4.0 * sp
        };
        let tab_height = if self.tab {
            (self.track.strings.len().max(1) - 1) as f64 * 1.5 * sp
        } else {
            0.0
        };
        let bottom = if self.tab {
            tab_top + tab_height
        } else {
            std_top + std_height + lyric_space
        };
        (std_top, tab_top, bottom + 5.0 * sp)
    }

    fn header_width(&self) -> f64 {
        let sp = self.sp;
        3.5 * sp
            + self.key_width(
                self.score
                    .master_bars
                    .first()
                    .map_or(0, |m| m.key_signature.accidentals),
            )
    }

    fn key_width(&self, key: i8) -> f64 {
        if self.standard {
            key.unsigned_abs() as f64 * 1.0 * self.sp
        } else {
            0.0
        }
    }

    fn plan_measure(&self, bi: usize) -> MeasurePlan {
        let sp = self.sp;
        let mb = &self.score.master_bars[bi];
        let bar_ticks = self.score.played_ticks(bi);
        let mut ticks: Vec<u32> = Vec::new();
        if let Some(bar) = self.track.bars.get(bi) {
            for v in &bar.voices {
                let mut t = 0;
                for b in &v.beats {
                    if !b.empty {
                        ticks.push(t);
                    }
                    t += b.ticks();
                }
            }
        }
        ticks.sort_unstable();
        ticks.dedup();
        if ticks.is_empty() {
            ticks.push(0);
        }
        let mut columns = Vec::new();
        for (i, &t) in ticks.iter().enumerate() {
            let next = ticks.get(i + 1).copied().unwrap_or(bar_ticks.max(t + 240));
            let gap = next.saturating_sub(t).max(60) as f64;
            let mut width = sp * (1.5 + 1.15 * (gap / 120.0 + 1.0).log2());
            if self.has_accidental_or_wide_fret(bi, t) {
                width += 0.8 * sp;
            }
            columns.push(Column { tick: t, width });
        }
        if ticks.len() == 1
            && self.track.bars.get(bi).is_none_or(|b| {
                b.voices
                    .iter()
                    .all(|v| v.beats.iter().all(|x| x.notes.is_empty()))
            })
        {
            columns[0].width = columns[0].width.max(6.0 * sp);
        }
        let prev = bi.checked_sub(1).map(|p| &self.score.master_bars[p]);
        let mut lead = 1.2 * sp;
        if prev.is_some_and(|p| p.key_signature.accidentals != mb.key_signature.accidentals) {
            lead += self.key_width(mb.key_signature.accidentals) + 0.5 * sp;
        }
        if !mb.free_time && prev.is_none_or(|p| p.time_signature != mb.time_signature) {
            lead += 2.5 * sp;
        }
        if mb.repeat_start {
            lead += 1.0 * sp;
        }
        let natural = lead
            + columns.iter().map(|c| c.width).sum::<f64>()
            + 0.6 * sp
            + if mb.repeat_end.is_some() {
                1.0 * sp
            } else {
                0.0
            };
        MeasurePlan {
            bar: bi,
            columns,
            lead,
            natural,
        }
    }

    fn has_accidental_or_wide_fret(&self, bi: usize, tick: u32) -> bool {
        let Some(bar) = self.track.bars.get(bi) else {
            return false;
        };
        bar.voices.iter().any(|v| {
            let mut t = 0;
            v.beats.iter().any(|b| {
                let here = t == tick;
                t += b.ticks();
                here && b.notes.iter().any(|n| n.fret >= 10 || n.tie || n.ghost)
            })
        })
    }

    fn run(&self) -> Layout {
        let sp = self.sp;
        let plans: Vec<MeasurePlan> = (0..self.score.master_bars.len())
            .map(|b| self.plan_measure(b))
            .collect();
        let margin = self.o.margin;
        let width = match self.o.mode {
            Mode::Page { width, .. } | Mode::Vertical { width } => width,
            Mode::Horizontal => {
                margin * 2.0
                    + self.header_width()
                    + plans.iter().map(|p| p.natural).sum::<f64>()
                    + sp
            }
        };
        let avail = (width - 2.0 * margin - self.header_width()).max(20.0 * sp);

        // Break into systems.
        let mut systems: Vec<Vec<usize>> = Vec::new();
        let mut current: Vec<usize> = Vec::new();
        let mut used = 0.0;
        for (i, p) in plans.iter().enumerate() {
            if !current.is_empty() && used + p.natural > avail && self.o.mode != Mode::Horizontal {
                systems.push(std::mem::take(&mut current));
                used = 0.0;
            }
            current.push(i);
            used += p.natural;
        }
        if !current.is_empty() {
            systems.push(current);
        }

        let (_, _, system_height) = self.metrics();
        let mut layout = Layout {
            pages: Vec::new(),
            beats: HashMap::new(),
            bars: vec![(0, 0.0, 0.0); plans.len()],
        };
        let page_height = match self.o.mode {
            Mode::Page { height, .. } => height,
            _ => 0.0,
        };
        let mut page = Page {
            width,
            height: page_height,
            ..Page::default()
        };
        let mut y = margin;
        if self.o.header {
            y = self.draw_header(&mut page, width, y);
        }
        let mut placed: HashMap<(usize, usize, usize, u8), Placed> = HashMap::new();
        let mut system_ends: Vec<(usize, f64, f64)> = Vec::new(); // page, x start, x end per system
        for (si, sys) in systems.iter().enumerate() {
            if let Mode::Page { height, .. } = self.o.mode {
                if y + system_height > height - margin && y > margin + 1.0 {
                    self.draw_page_number(&mut page, layout.pages.len());
                    layout.pages.push(std::mem::replace(
                        &mut page,
                        Page {
                            width,
                            height,
                            ..Page::default()
                        },
                    ));
                    y = margin;
                }
            }
            let natural: f64 = sys.iter().map(|&i| plans[i].natural).sum();
            let last = si + 1 == systems.len();
            let stretch = if self.o.mode == Mode::Horizontal || (last && natural < avail * 0.75) {
                1.0
            } else {
                avail / natural
            };
            let page_index = layout.pages.len();
            let x_end = self.draw_system(
                &mut page,
                page_index,
                si,
                sys,
                &plans,
                y,
                margin,
                stretch,
                &mut layout,
                &mut placed,
            );
            system_ends.push((page_index, margin, x_end));
            y += system_height;
        }
        if let Mode::Page { height, .. } = self.o.mode {
            page.height = height;
            self.draw_page_number(&mut page, layout.pages.len());
        } else {
            page.height = y + margin;
        }
        layout.pages.push(page);
        self.draw_ties(&mut layout, &placed, &system_ends);
        layout
    }

    fn draw_header(&self, page: &mut Page, width: f64, y: f64) -> f64 {
        let m = &self.score.meta;
        let sp = self.sp;
        let mut y = y;
        if !m.title.is_empty() {
            y += 5.0 * sp;
            page.text(
                FontKind::Bold,
                m.title.clone(),
                (width / 2.0, y),
                5.0 * sp,
                Align::Center,
            );
        }
        if !m.subtitle.is_empty() {
            y += 3.2 * sp;
            page.text(
                FontKind::Text,
                m.subtitle.clone(),
                (width / 2.0, y),
                3.0 * sp,
                Align::Center,
            );
        }
        let credit = if !m.artist.is_empty() {
            m.artist.clone()
        } else {
            m.music.clone()
        };
        if !credit.is_empty() {
            y += 3.2 * sp;
            page.text(
                FontKind::Text,
                credit,
                (width - self.o.margin, y),
                2.6 * sp,
                Align::Right,
            );
        }
        if !self.track.name.trim().is_empty() {
            page.text(
                FontKind::Text,
                self.track.name.trim().to_string(),
                (self.o.margin, y),
                2.6 * sp,
                Align::Left,
            );
        }
        y + 4.0 * sp
    }

    fn draw_page_number(&self, page: &mut Page, index: usize) {
        if let Mode::Page { width, height } = self.o.mode {
            page.text(
                FontKind::Text,
                format!("{}", index + 1),
                (width / 2.0, height - self.o.margin / 2.0),
                2.4 * self.sp,
                Align::Center,
            );
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn draw_system(
        &self,
        page: &mut Page,
        page_index: usize,
        system_index: usize,
        bars: &[usize],
        plans: &[MeasurePlan],
        top: f64,
        x0: f64,
        stretch: f64,
        layout: &mut Layout,
        placed: &mut HashMap<(usize, usize, usize, u8), Placed>,
    ) -> f64 {
        let sp = self.sp;
        let (std_rel, tab_rel, _) = self.metrics();
        let std_top = top + std_rel;
        let std_bottom = std_top + 4.0 * sp;
        let tab_top = top + tab_rel;
        let strings = self.track.strings.len().max(1);
        let tab_y = |s: usize| tab_top + s as f64 * 1.5 * sp;
        let tab_bottom = tab_y(strings - 1);
        let header = self.header_width();
        let total: f64 = bars.iter().map(|&i| plans[i].natural * stretch).sum();
        let x1 = x0 + header + total;

        // Staff lines.
        if self.standard {
            for l in 0..5 {
                let y = std_top + l as f64 * sp;
                page.line((x0, y), (x1, y), 0.13 * sp);
            }
        }
        if self.tab {
            for s in 0..strings {
                page.items.push(Item::Line {
                    from: Point::new(x0, tab_y(s)),
                    to: Point::new(x1, tab_y(s)),
                    width: 0.1 * sp,
                    color: Color::GREY,
                });
            }
        }
        // System start line and clefs.
        let first_top = if self.standard { std_top } else { tab_top };
        let last_bottom = if self.tab { tab_bottom } else { std_bottom };
        page.line((x0, first_top), (x0, last_bottom), 0.16 * sp);
        let first_bar = bars[0];
        if self.standard {
            let (g, p) = self.clef.glyph();
            page.glyph(g, (x0 + 0.8 * sp, self.staff_y(std_bottom, p)), 4.0 * sp);
            let key = self.score.master_bars[first_bar].key_signature.accidentals;
            self.draw_key(page, key, x0 + 3.8 * sp, std_bottom);
        }
        if self.tab {
            let mid = (tab_top + tab_bottom) / 2.0;
            // Scale the clef to 80% of the tab staff's height.
            let unit = fonts().bbox(FontKind::Music, TAB_CLEF, 4.0 * sp);
            let size = 4.0 * sp * 0.8 * (tab_bottom - tab_top) / unit.height().max(1.0);
            let b = fonts().bbox(FontKind::Music, TAB_CLEF, size);
            page.glyph(TAB_CLEF, (x0 + 0.8 * sp, mid - (b.y0 + b.y1) / 2.0), size);
        }
        // Bar number above the first bar.
        if system_index > 0 || first_bar > 0 {
            page.text(
                FontKind::Italic,
                format!("{}", first_bar + 1),
                (x0, first_top - 1.4 * sp),
                1.8 * sp,
                Align::Left,
            );
        }

        let mut x = x0 + header;
        for &bi in bars {
            let plan = &plans[bi];
            let w = plan.natural * stretch;
            layout.bars[bi] = (page_index, x, x + w);
            self.draw_measure(
                page,
                page_index,
                system_index,
                plan,
                x,
                w,
                stretch,
                std_bottom,
                tab_top,
                layout,
                placed,
            );
            x += w;
        }
        x1
    }

    fn draw_key(&self, page: &mut Page, key: i8, x: f64, bottom: f64) {
        let sharps = [8, 5, 9, 6, 3, 7, 4];
        let flats = [4, 7, 3, 6, 2, 5, 1];
        let off = self.clef.key_offset();
        for i in 0..key.unsigned_abs().min(7) as usize {
            let (g, p) = if key > 0 {
                (SHARP, sharps[i])
            } else {
                (FLAT, flats[i])
            };
            page.glyph(
                g,
                (x + i as f64 * self.sp, self.staff_y(bottom, p + off)),
                4.0 * self.sp,
            );
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn draw_measure(
        &self,
        page: &mut Page,
        page_index: usize,
        system: usize,
        plan: &MeasurePlan,
        x: f64,
        width: f64,
        stretch: f64,
        std_bottom: f64,
        tab_top: f64,
        layout: &mut Layout,
        placed: &mut HashMap<(usize, usize, usize, u8), Placed>,
    ) {
        let sp = self.sp;
        let bi = plan.bar;
        let mb = &self.score.master_bars[bi];
        let prev = bi.checked_sub(1).map(|p| &self.score.master_bars[p]);
        let std_top = std_bottom - 4.0 * sp;
        let strings = self.track.strings.len().max(1);
        let tab_bottom = tab_top + (strings - 1) as f64 * 1.5 * sp;
        let staves: Vec<(f64, f64)> = [
            self.standard.then_some((std_top, std_bottom)),
            self.tab.then_some((tab_top, tab_bottom)),
        ]
        .into_iter()
        .flatten()
        .collect();
        let top = staves.first().map_or(std_top, |s| s.0);
        let bottom = staves.last().map_or(std_bottom, |s| s.1);

        // Barlines and repeats.
        let end = x + width;
        for &(a, b) in &staves {
            if mb.repeat_end.is_some() {
                page.line((end - 0.8 * sp, a), (end - 0.8 * sp, b), 0.16 * sp);
                page.rect(Rect::new(end - 0.5 * sp, a, end, b), Color::BLACK);
                self.repeat_dots(page, end - 1.6 * sp, a, b);
            } else if mb.double_bar {
                page.line((end - 0.6 * sp, a), (end - 0.6 * sp, b), 0.16 * sp);
                page.line((end, a), (end, b), 0.16 * sp);
            } else {
                page.line((end, a), (end, b), 0.16 * sp);
            }
            if mb.repeat_start {
                page.rect(Rect::new(x, a, x + 0.5 * sp, b), Color::BLACK);
                page.line((x + 0.8 * sp, a), (x + 0.8 * sp, b), 0.16 * sp);
                self.repeat_dots(page, x + 1.4 * sp, a, b);
            }
        }
        if let Some(n) = mb.repeat_end.filter(|&n| n > 2) {
            page.text(
                FontKind::Bold,
                format!("x{n}"),
                (end, top - 0.6 * sp),
                1.8 * sp,
                Align::Right,
            );
        }
        if bi + 1 == self.score.master_bars.len() && mb.repeat_end.is_none() {
            for &(a, b) in &staves {
                page.rect(Rect::new(end - 0.5 * sp, a, end, b), Color::BLACK);
                page.line((end - 1.0 * sp, a), (end - 1.0 * sp, b), 0.16 * sp);
            }
        }
        // Alternate ending bracket.
        if mb.alternate_endings != 0 {
            let y = top - 5.5 * sp;
            page.line((x, y), (end - 0.3 * sp, y), 0.12 * sp);
            let starts = prev.is_none_or(|p| p.alternate_endings != mb.alternate_endings);
            if starts {
                page.line((x, y), (x, y + 2.0 * sp), 0.12 * sp);
                let label: Vec<String> = (0..8)
                    .filter(|i| mb.alternate_endings & (1 << i) != 0)
                    .map(|i| format!("{}.", i + 1))
                    .collect();
                page.text(
                    FontKind::Text,
                    label.join(" "),
                    (x + 0.5 * sp, y + 1.7 * sp),
                    1.8 * sp,
                    Align::Left,
                );
            }
        }
        // Section marker, directions and tempo.
        let mut above = top - 7.5 * sp;
        if let Some(m) = &mb.marker {
            if !m.title.trim().is_empty() {
                let label = m.title.trim().to_string();
                let w = fonts().text_width(FontKind::Bold, &label, 2.2 * sp);
                page.items.push(Item::Rect {
                    rect: Rect::new(
                        x - 0.3 * sp,
                        above - 2.0 * sp,
                        x + w + 0.3 * sp,
                        above + 0.5 * sp,
                    ),
                    color: Color::rgb(235, 235, 235),
                });
                page.text(FontKind::Bold, label, (x, above), 2.2 * sp, Align::Left);
                above -= 3.0 * sp;
            }
        }
        let tempos: Vec<(u32, f64)> = if bi == 0 {
            vec![(0, self.score.tempo as f64)]
        } else {
            vec![]
        }
        .into_iter()
        .chain(mb.tempo_changes.iter().map(|c| (c.tick, c.bpm)))
        .collect();
        if let Some(&(_, bpm)) = tempos.first() {
            page.glyph(METRONOME_QUARTER, (x + 0.5 * sp, above), 3.2 * sp);
            page.text(
                FontKind::Text,
                format!("= {}", bpm.round()),
                (x + 2.0 * sp, above),
                2.2 * sp,
                Align::Left,
            );
        }
        for d in &mb.directions {
            let label = format!("{d:?}");
            page.text(
                FontKind::Italic,
                label,
                (end, top - 2.0 * sp),
                1.8 * sp,
                Align::Right,
            );
        }

        // Key and time signature changes.
        let mut lead_x = x + 0.6 * sp;
        if self.standard
            && prev.is_some_and(|p| p.key_signature.accidentals != mb.key_signature.accidentals)
        {
            self.draw_key(page, mb.key_signature.accidentals, lead_x, std_bottom);
            lead_x += self.key_width(mb.key_signature.accidentals) + 0.5 * sp;
        }
        if !mb.free_time && prev.is_none_or(|p| p.time_signature != mb.time_signature) {
            let ts = mb.time_signature;
            for &(a, b) in &staves {
                let mid = (a + b) / 2.0;
                let draw = |page: &mut Page, n: u32, y: f64| {
                    let s = n.to_string();
                    let digits: Vec<u32> = s.chars().filter_map(|c| c.to_digit(10)).collect();
                    let w: f64 = digits
                        .iter()
                        .map(|d| fonts().advance(FontKind::Music, time_digit(*d), 4.0 * sp))
                        .sum();
                    let mut cx = lead_x + 1.0 * sp - w / 2.0;
                    for d in digits {
                        page.glyph(time_digit(d), (cx, y), 4.0 * sp);
                        cx += fonts().advance(FontKind::Music, time_digit(d), 4.0 * sp);
                    }
                };
                let half = (b - a) / 4.0;
                draw(page, ts.numerator as u32, mid - half);
                draw(page, ts.denominator as u32, mid + half);
            }
        }

        // Columns.
        let mut col_x: HashMap<u32, (f64, f64)> = HashMap::new();
        let mut cx = x + plan.lead * stretch;
        for c in &plan.columns {
            col_x.insert(c.tick, (cx, c.width * stretch));
            cx += c.width * stretch;
        }
        let Some(bar) = self.track.bars.get(bi) else {
            return;
        };
        let voices = bar
            .voices
            .iter()
            .filter(|v| !v.beats.iter().all(|b| b.empty))
            .count();
        let key_alter = key_alters(mb.key_signature.accidentals);
        let mut alters: HashMap<i32, i32> = HashMap::new();
        for (vi, voice) in bar.voices.iter().enumerate() {
            let mut t = 0;
            let mut beamable: Vec<(usize, f64, Vec<f64>, DurationValue, u32)> = Vec::new();
            let up_voice = if voices > 1 { Some(vi % 2 == 0) } else { None };
            let mut tuplet_group: Vec<(f64, f64)> = Vec::new();
            let mut tuplet_ratio: Option<Tuplet> = None;
            let mut tuplet_fill = 0;
            for (i, beat) in voice.beats.iter().enumerate() {
                let start = t;
                t += beat.ticks();
                if beat.empty {
                    continue;
                }
                let Some(&(bx, bw)) = col_x.get(&start) else {
                    continue;
                };
                let beat_ref = BeatRef {
                    track: self.track_index,
                    bar: bi,
                    voice: vi,
                    beat: i,
                };
                let string_ys: Vec<f64> = if self.tab {
                    (0..strings)
                        .map(|s| tab_top + s as f64 * 1.5 * sp)
                        .collect()
                } else {
                    vec![]
                };
                layout.beats.insert(
                    beat_ref,
                    BeatPos {
                        page: page_index,
                        x: bx - 0.4 * sp,
                        width: bw,
                        top: top - 2.0 * sp,
                        bottom: bottom + 2.0 * sp,
                        strings: string_ys.clone(),
                    },
                );
                page.hits.push(Hit {
                    rect: Rect::new(
                        bx - 0.4 * sp,
                        top - 2.0 * sp,
                        bx + bw - 0.4 * sp,
                        bottom + 2.0 * sp,
                    ),
                    beat: beat_ref,
                    string: None,
                });
                for (s, &sy) in string_ys.iter().enumerate() {
                    page.hits.push(Hit {
                        rect: Rect::new(
                            bx - 0.4 * sp,
                            sy - 0.75 * sp,
                            bx + bw - 0.4 * sp,
                            sy + 0.75 * sp,
                        ),
                        beat: beat_ref,
                        string: Some(s as u8),
                    });
                }

                // Standard notation.
                if self.standard {
                    if beat.notes.is_empty() {
                        let off = match up_voice {
                            Some(true) => 2,
                            Some(false) => -2,
                            None => 0,
                        };
                        let p = 4 + off;
                        page.glyph(
                            rest_glyph(beat.duration.value),
                            (bx, self.staff_y(std_bottom, p)),
                            4.0 * sp,
                        );
                        self.draw_dots(page, beat, bx + 1.6 * sp, std_bottom, p | 1);
                        if beam_count(beat.duration.value) > 0 {
                            self.flush_beams(page, &mut beamable, std_bottom);
                        }
                    } else {
                        let (stem_x, ys, up) = self.draw_chord(
                            page,
                            beat,
                            bx,
                            std_bottom,
                            up_voice,
                            key_alter,
                            &mut alters,
                            (bi, vi, i),
                            page_index,
                            system,
                            placed,
                        );
                        let level = beam_count(beat.duration.value);
                        if level > 0 && self.beam_breaks(mb, start) {
                            self.flush_beams(page, &mut beamable, std_bottom);
                        }
                        if level > 0 {
                            beamable.push((i, stem_x, ys, beat.duration.value, u32::from(up)));
                        } else {
                            self.flush_beams(page, &mut beamable, std_bottom);
                            if beat.duration.value != DurationValue::Whole {
                                self.draw_stem(page, stem_x, &ys, up, None);
                            }
                        }
                    }
                }
                // Tuplet groups.
                if let Some(tp) = beat.duration.tuplet {
                    if tuplet_ratio != Some(tp) {
                        self.flush_tuplet(page, &mut tuplet_group, tuplet_ratio, std_top);
                        tuplet_ratio = Some(tp);
                        tuplet_fill = 0;
                    }
                    tuplet_group.push((bx, bx + self.nh_width));
                    tuplet_fill += beat.duration.ticks();
                    if tuplet_fill + 2 >= tp.times as u32 * beat.duration.value.ticks() {
                        self.flush_tuplet(page, &mut tuplet_group, tuplet_ratio, std_top);
                        tuplet_ratio = None;
                    }
                } else if tuplet_ratio.is_some() {
                    self.flush_tuplet(page, &mut tuplet_group, tuplet_ratio, std_top);
                    tuplet_ratio = None;
                }
                // Text, chord names, lyrics.
                if let Some(text) = &beat.text {
                    page.text(
                        FontKind::Italic,
                        text.clone(),
                        (bx, top - 3.2 * sp),
                        1.8 * sp,
                        Align::Left,
                    );
                }
                if let Some(c) = &beat.chord {
                    if !c.name.is_empty() {
                        page.text(
                            FontKind::Bold,
                            c.name.clone(),
                            (bx, top - 5.2 * sp),
                            2.0 * sp,
                            Align::Left,
                        );
                    }
                }
                for (li, syl) in beat.lyrics.iter().enumerate() {
                    if !syl.is_empty() {
                        let y = std_bottom + 3.5 * sp + li as f64 * 1.8 * sp;
                        page.text(
                            FontKind::Text,
                            syl.clone(),
                            (bx + self.nh_width / 2.0, y),
                            1.9 * sp,
                            Align::Center,
                        );
                    }
                }
                // Tablature.
                if self.tab {
                    self.draw_tab_beat(page, beat, bx, tab_top, (bi, vi, i), placed);
                }
            }
            if self.standard {
                self.flush_beams(page, &mut beamable, std_bottom);
            }
            self.flush_tuplet(page, &mut tuplet_group, tuplet_ratio, std_top);
        }
    }

    fn repeat_dots(&self, page: &mut Page, x: f64, a: f64, b: f64) {
        let mid = (a + b) / 2.0;
        let r = 0.25 * self.sp;
        for y in [mid - 0.5 * self.sp, mid + 0.5 * self.sp] {
            page.items.push(Item::Path {
                path: kurbo::Shape::to_path(&kurbo::Circle::new((x, y), r), 0.1),
                color: Color::BLACK,
                stroke: None,
            });
        }
    }

    /// Beams break at each beat (quarter, or dotted quarter in compound meters).
    fn beam_breaks(&self, mb: &MasterBar, tick: u32) -> bool {
        let ts = mb.time_signature;
        let unit = if ts.denominator == 8 && ts.numerator % 3 == 0 {
            1440
        } else if ts.denominator == 2 {
            1920
        } else {
            960
        };
        tick % unit == 0
    }

    fn draw_dots(&self, page: &mut Page, beat: &Beat, x: f64, bottom: f64, p: i32) {
        for d in 0..beat.duration.dots {
            page.glyph(
                DOT,
                (x + d as f64 * 0.6 * self.sp, self.staff_y(bottom, p)),
                4.0 * self.sp,
            );
        }
    }

    /// Draws noteheads, ledger lines, accidentals and dots; returns (stem x, note ys, stem up).
    #[allow(clippy::too_many_arguments)]
    fn draw_chord(
        &self,
        page: &mut Page,
        beat: &Beat,
        x: f64,
        bottom: f64,
        up_voice: Option<bool>,
        key_alter: [i32; 7],
        alters: &mut HashMap<i32, i32>,
        pos: (usize, usize, usize),
        page_index: usize,
        system: usize,
        placed: &mut HashMap<(usize, usize, usize, u8), Placed>,
    ) -> (f64, Vec<f64>, bool) {
        let sp = self.sp;
        let key = self.score.master_bars[pos.0].key_signature.accidentals;
        // (staff position, accidental, x notehead, string)
        let mut heads: Vec<(i32, Option<char>, bool, u8, bool)> = Vec::new();
        for n in &beat.notes {
            if self.clef == Clef::Percussion {
                let (p, xhead) = drum_position(n.fret);
                heads.push((p, None, xhead, n.string, n.tie));
                continue;
            }
            let written = self.track.pitch_of(n) as i32 + self.clef.transpose();
            let (step, alter, octave) = spell(written, key);
            let d = octave * 7 + step;
            let p = d - self.clef.bottom();
            let current = alters.get(&d).copied().unwrap_or(key_alter[step as usize]);
            let acc = if n.tie {
                None
            } else if current != alter {
                alters.insert(d, alter);
                Some(match alter {
                    1 => SHARP,
                    -1 => FLAT,
                    _ => NATURAL,
                })
            } else {
                None
            };
            heads.push((p, acc, n.dead, n.string, n.tie));
        }
        heads.sort_by_key(|h| h.0);
        let avg = heads.iter().map(|h| h.0).sum::<i32>() as f64 / heads.len().max(1) as f64;
        let up = up_voice.unwrap_or(avg < 4.0);
        let head = match beat.duration.value {
            DurationValue::Whole => HEAD_WHOLE,
            DurationValue::Half => HEAD_HALF,
            _ => HEAD_BLACK,
        };
        let mut ys = Vec::new();
        let mut prev_p: Option<i32> = None;
        let mut shifted = false;
        for (k, &(p, acc, xhead, string, _)) in heads.iter().enumerate() {
            // Seconds sit on the other side of the stem.
            let second = prev_p.is_some_and(|q| p - q == 1) && !shifted;
            shifted = second;
            prev_p = Some(p);
            let hx = if second {
                if up {
                    x + self.nh_width
                } else {
                    x - self.nh_width
                }
            } else {
                x
            };
            let y = self.staff_y(bottom, p);
            ys.push(y);
            let g = if xhead { HEAD_X } else { head };
            page.glyph(g, (hx, y), 4.0 * sp);
            // Ledger lines.
            let lx0 = hx - 0.4 * sp;
            let lx1 = hx + self.nh_width + 0.4 * sp;
            let mut q = -2;
            while q >= p {
                page.line(
                    (lx0, self.staff_y(bottom, q)),
                    (lx1, self.staff_y(bottom, q)),
                    0.16 * sp,
                );
                q -= 2;
            }
            let mut q = 10;
            while q <= p {
                page.line(
                    (lx0, self.staff_y(bottom, q)),
                    (lx1, self.staff_y(bottom, q)),
                    0.16 * sp,
                );
                q += 2;
            }
            if let Some(a) = acc {
                let stagger = (k % 2) as f64 * 0.9 * sp;
                page.glyph(a, (x - 1.2 * sp - stagger, y), 4.0 * sp);
            }
            self.draw_dots(
                page,
                beat,
                x + self.nh_width + 0.5 * sp,
                bottom,
                if p % 2 == 0 { p + 1 } else { p },
            );
            placed.insert(
                (pos.0, pos.1, pos.2, string),
                Placed {
                    page: page_index,
                    system,
                    x: hx,
                    y,
                    tab_y: None,
                    above: !up,
                },
            );
        }
        let stem_x = if up {
            x + self.nh_width - 0.06 * sp
        } else {
            x + 0.06 * sp
        };
        (stem_x, ys, up)
    }

    fn draw_stem(&self, page: &mut Page, x: f64, ys: &[f64], up: bool, to: Option<f64>) {
        let sp = self.sp;
        let (lo, hi) = (
            ys.iter().cloned().fold(f64::MAX, f64::min),
            ys.iter().cloned().fold(f64::MIN, f64::max),
        );
        let (from, end) = if up {
            (hi, to.unwrap_or(lo - 3.5 * sp))
        } else {
            (lo, to.unwrap_or(hi + 3.5 * sp))
        };
        page.line((x, from), (x, end), 0.15 * sp);
    }

    /// Draws beams (or a flag for a lone note) for collected beats, then clears them.
    fn flush_beams(
        &self,
        page: &mut Page,
        group: &mut Vec<(usize, f64, Vec<f64>, DurationValue, u32)>,
        _bottom: f64,
    ) {
        let sp = self.sp;
        if group.len() == 1 {
            let (_, x, ys, v, up) = &group[0];
            let up = *up == 1;
            self.draw_stem(page, *x, ys, up, None);
            let lo = ys.iter().cloned().fold(f64::MAX, f64::min);
            let hi = ys.iter().cloned().fold(f64::MIN, f64::max);
            let end = if up { lo - 3.5 * sp } else { hi + 3.5 * sp };
            page.glyph(flag_glyph(*v, up), (*x - 0.06 * sp, end), 4.0 * sp);
        } else if group.len() > 1 {
            // The group's stem direction follows the majority.
            let ups = group.iter().filter(|g| g.4 == 1).count();
            let up = ups * 2 >= group.len();
            let ends: Vec<f64> = group
                .iter()
                .map(|(_, _, ys, _, _)| {
                    if up {
                        ys.iter().cloned().fold(f64::MAX, f64::min) - 3.5 * sp
                    } else {
                        ys.iter().cloned().fold(f64::MIN, f64::max) + 3.5 * sp
                    }
                })
                .collect();
            let (x0, x1) = (group[0].1, group[group.len() - 1].1);
            let extreme = if up {
                ends.iter().cloned().fold(f64::MAX, f64::min)
            } else {
                ends.iter().cloned().fold(f64::MIN, f64::max)
            };
            let slope = ((ends[ends.len() - 1] - ends[0]) / (x1 - x0).max(1.0)).clamp(-0.15, 0.15);
            // Beam line through the extreme end, with the clamped slope.
            let anchor = ends
                .iter()
                .zip(group.iter())
                .map(|(e, g)| (*e, g.1))
                .find(|(e, _)| (*e - extreme).abs() < 1e-6)
                .unwrap();
            let beam_y = |x: f64| anchor.0 + slope * (x - anchor.1);
            for (_, x, ys, _, _) in group.iter() {
                self.draw_stem(page, *x, ys, up, Some(beam_y(*x)));
            }
            let thick = 0.5 * sp;
            let gap = 0.75 * sp;
            let max_level = group.iter().map(|g| beam_count(g.3)).max().unwrap_or(1);
            for level in 0..max_level {
                let off = level as f64 * gap * if up { 1.0 } else { -1.0 };
                let mut i = 0;
                while i < group.len() {
                    if beam_count(group[i].3) <= level {
                        i += 1;
                        continue;
                    }
                    let mut j = i;
                    while j + 1 < group.len() && beam_count(group[j + 1].3) > level {
                        j += 1;
                    }
                    let (xa, mut xb) = (group[i].1, group[j].1);
                    let mut xa = xa;
                    if i == j {
                        // A partial beam pointing at its neighbour.
                        if i + 1 < group.len() {
                            xb = xa + 1.2 * sp
                        } else {
                            xa -= 1.2 * sp
                        }
                    }
                    let (ya, yb) = (beam_y(xa) + off, beam_y(xb) + off);
                    let dir = if up { 1.0 } else { -1.0 };
                    let mut p = BezPath::new();
                    p.move_to((xa, ya));
                    p.line_to((xb, yb));
                    p.line_to((xb, yb + thick * dir));
                    p.line_to((xa, ya + thick * dir));
                    p.close_path();
                    page.items.push(Item::Path {
                        path: p,
                        color: Color::BLACK,
                        stroke: None,
                    });
                    i = j + 1;
                }
            }
        }
        group.clear();
    }

    fn flush_tuplet(
        &self,
        page: &mut Page,
        group: &mut Vec<(f64, f64)>,
        ratio: Option<Tuplet>,
        std_top: f64,
    ) {
        if let (Some(tp), Some(first), Some(last)) = (ratio, group.first(), group.last()) {
            let x = (first.0 + last.1) / 2.0;
            let y = std_top - 0.8 * self.sp;
            let digits: Vec<u32> = tp
                .enters
                .to_string()
                .chars()
                .filter_map(|c| c.to_digit(10))
                .collect();
            let w: f64 = digits
                .iter()
                .map(|d| fonts().advance(FontKind::Music, tuplet_digit(*d), 3.0 * self.sp))
                .sum();
            let mut cx = x - w / 2.0;
            for d in digits {
                page.glyph(tuplet_digit(d), (cx, y), 3.0 * self.sp);
                cx += fonts().advance(FontKind::Music, tuplet_digit(d), 3.0 * self.sp);
            }
        }
        group.clear();
    }

    fn draw_tab_beat(
        &self,
        page: &mut Page,
        beat: &Beat,
        x: f64,
        tab_top: f64,
        pos: (usize, usize, usize),
        placed: &mut HashMap<(usize, usize, usize, u8), Placed>,
    ) {
        let sp = self.sp;
        let size = 1.9 * sp;
        let cx = x + self.nh_width / 2.0;
        let mut above_marks: Vec<String> = Vec::new();
        for n in &beat.notes {
            let y = tab_top + n.string as f64 * 1.5 * sp;
            let fret = if n.dead {
                "x".to_string()
            } else {
                n.fret.to_string()
            };
            let mut label = match n.effects.harmonic {
                Some(Harmonic::Natural) => format!("<{fret}>"),
                Some(_) => format!("{fret}<{}>", n.fret as u32 + 12),
                None if n.tie || n.ghost => format!("({fret})"),
                None => fret,
            };
            if n.effects.grace.is_some() {
                label = format!("{}{label}", n.effects.grace.map(|g| g.fret).unwrap_or(0));
            }
            let w = fonts().text_width(FontKind::Text, &label, size);
            page.rect(
                Rect::new(
                    cx - w / 2.0 - 0.15 * sp,
                    y - 0.7 * sp,
                    cx + w / 2.0 + 0.15 * sp,
                    y + 0.7 * sp,
                ),
                Color::WHITE,
            );
            let color = if n.tie { Color::GREY } else { Color::BLACK };
            page.items.push(Item::Text {
                font: FontKind::Text,
                text: label,
                at: Point::new(cx, y + 0.65 * sp),
                size,
                align: Align::Center,
                color,
            });
            let entry = placed
                .entry((pos.0, pos.1, pos.2, n.string))
                .or_insert(Placed {
                    page: 0,
                    system: 0,
                    x,
                    y: 0.0,
                    tab_y: None,
                    above: true,
                });
            entry.tab_y = Some(y);
            entry.x = x;
            if let Some(b) = &n.effects.bend {
                // Arrow from the note up above the staff, labelled with the bend amount.
                let peak = b.points.iter().map(|p| p.value).max().unwrap_or(0);
                let top = tab_top - 2.0 * sp;
                let mut path = BezPath::new();
                path.move_to((cx + w / 2.0 + 0.2 * sp, y));
                path.quad_to((cx + 1.8 * sp, y), (cx + 1.8 * sp, top));
                page.items.push(Item::Path {
                    path,
                    color: Color::BLACK,
                    stroke: Some(0.12 * sp),
                });
                let amount = match peak {
                    4 => "full".to_string(),
                    2 => "½".to_string(),
                    v => format!("{}", v as f64 / 4.0),
                };
                page.text(
                    FontKind::Text,
                    amount,
                    (cx + 1.8 * sp, top - 0.4 * sp),
                    1.5 * sp,
                    Align::Center,
                );
            }
            if n.effects.palm_mute {
                above_marks.push("P.M.".into());
            }
            if n.effects.let_ring {
                above_marks.push("let ring".into());
            }
            if n.effects.hammer_pull {
                above_marks.push("H/P".into());
            }
            if !n.effects.slides.is_empty() {
                above_marks.push("sl.".into());
            }
            if n.effects.vibrato || beat.effects.vibrato {
                above_marks.push("~".into());
            }
        }
        above_marks.dedup();
        // Sustained marks (let ring, palm mute) are shown where they start.
        let previous = pos
            .2
            .checked_sub(1)
            .and_then(|i| self.track.bars.get(pos.0)?.voices.get(pos.1)?.beats.get(i));
        let sustained =
            |b: &Beat, f: fn(&NoteEffects) -> bool| b.notes.iter().any(|n| f(&n.effects));
        if let Some(prev) = previous {
            if sustained(prev, |e| e.let_ring) {
                above_marks.retain(|m| m != "let ring");
            }
            if sustained(prev, |e| e.palm_mute) {
                above_marks.retain(|m| m != "P.M.");
            }
        }
        if !above_marks.is_empty() {
            page.text(
                FontKind::Italic,
                above_marks.join(" "),
                (cx, tab_top - 0.9 * sp),
                1.5 * sp,
                Align::Center,
            );
        }
        // Rhythm stems below the tab when there is no standard staff.
        if !self.standard && !beat.notes.is_empty() && beat.duration.value != DurationValue::Whole {
            let bottom = tab_top + (self.track.strings.len().max(1) - 1) as f64 * 1.5 * sp;
            let (a, b) = (bottom + 1.0 * sp, bottom + 3.5 * sp);
            page.line((cx, a), (cx, b), 0.12 * sp);
            for k in 0..beam_count(beat.duration.value) {
                let y = b - k as f64 * 0.6 * sp;
                page.line((cx, y), (cx + 1.0 * sp, y), 0.3 * sp);
            }
        }
    }

    /// Ties between consecutive notes on the same string, split across systems when needed.
    fn draw_ties(
        &self,
        layout: &mut Layout,
        placed: &HashMap<(usize, usize, usize, u8), Placed>,
        systems: &[(usize, f64, f64)],
    ) {
        let sp = self.sp;
        let voices = self
            .track
            .bars
            .iter()
            .map(|b| b.voices.len())
            .max()
            .unwrap_or(0);
        for v in 0..voices {
            let mut last: HashMap<u8, (usize, usize, usize)> = HashMap::new();
            for (bi, bar) in self.track.bars.iter().enumerate() {
                let Some(voice) = bar.voices.get(v) else {
                    continue;
                };
                for (i, beat) in voice.beats.iter().enumerate() {
                    for n in &beat.notes {
                        if n.tie {
                            if let Some(&from) = last.get(&n.string) {
                                let (Some(a), Some(b)) = (
                                    placed.get(&(from.0, from.1, from.2, n.string)),
                                    placed.get(&(bi, v, i, n.string)),
                                ) else {
                                    continue;
                                };
                                if a.y == 0.0 || !self.standard {
                                    continue;
                                }
                                let dy = if a.above { -0.6 * sp } else { 0.6 * sp };
                                let arc = |page: &mut Page, x0: f64, x1: f64, y: f64| {
                                    let mut p = BezPath::new();
                                    let mid = (x0 + x1) / 2.0;
                                    p.move_to((x0, y + dy * 0.4));
                                    p.quad_to((mid, y + dy * 2.2), (x1, y + dy * 0.4));
                                    p.quad_to((mid, y + dy * 1.8), (x0, y + dy * 0.4));
                                    page.items.push(Item::Path {
                                        path: p,
                                        color: Color::BLACK,
                                        stroke: None,
                                    });
                                };
                                if a.system == b.system {
                                    arc(&mut layout.pages[a.page], a.x + self.nh_width, b.x, a.y);
                                } else if let (Some(sa), Some(sb)) =
                                    (systems.get(a.system), systems.get(b.system))
                                {
                                    arc(&mut layout.pages[a.page], a.x + self.nh_width, sa.2, a.y);
                                    arc(
                                        &mut layout.pages[b.page],
                                        sb.1 + self.header_width() - sp,
                                        b.x,
                                        b.y,
                                    );
                                }
                            }
                        }
                        last.insert(n.string, (bi, v, i));
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spelling_and_keys() {
        assert_eq!(spell(61, 0), (0, 1, 4)); // C#4
        assert_eq!(spell(61, -2), (1, -1, 4)); // Db4
        assert_eq!(key_alters(2), [1, 0, 0, 1, 0, 0, 0]); // F#, C#
        assert_eq!(key_alters(-1), [0, 0, 0, 0, 0, 0, -1]); // Bb
    }

    #[test]
    fn lays_out_a_small_score() {
        let beat = |fret| Beat {
            duration: Duration::new(DurationValue::Eighth),
            notes: vec![Note::new(1, fret)],
            ..Beat::default()
        };
        let bar = Bar {
            voices: vec![Voice {
                beats: (0..8).map(|i| beat(i % 5)).collect(),
            }],
        };
        let track = Track {
            name: "Gtr".into(),
            strings: vec![64, 59, 55, 50, 45, 40],
            bars: vec![bar; 12],
            ..Track::default()
        };
        let score = Score {
            tempo: 100,
            master_bars: vec![MasterBar::default(); 12],
            tracks: vec![track],
            ..Score::default()
        };
        let layout = engrave(&score, 0, &Options::default());
        assert!(!layout.pages.is_empty());
        assert!(layout.pages.iter().map(|p| p.items.len()).sum::<usize>() > 500);
        assert_eq!(layout.beats.len(), 96);
        // Systems wrap within the page width.
        assert!(
            layout
                .bars
                .iter()
                .all(|&(_, a, b)| a >= 40.0 && b <= 595.0 - 39.0)
        );
    }
}
