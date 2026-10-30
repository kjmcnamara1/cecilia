//! Square-notation engraving of chant tracks: a four-line staff, neumes drawn as ligatures,
//! clefs, divisions, custodes, rhythmic signs and the text underlay with a drop-cap initial.
//!
//! Each chant note is mapped to the beat that transcribes it, so cursors, playback and
//! hit-testing work on chant exactly as on modern notation.

use std::collections::HashMap;

use kurbo::{BezPath, Circle, Point, Rect, Shape as _};
use muse_core::chant::{
    Chant, ChantClef, ChantNote, ClefKind, Division, Liquescence, NeumeItem, Shape, Syllable,
    transcribe,
};
use muse_core::{Score, Track};

use crate::display::{Align, BeatRef, Color, FontKind, Hit, Item, Page};
use crate::fonts::fonts;
use crate::layout::{BeatPos, Layout, Mode, Options, StaffPos};

const FLAT: char = '\u{E260}';
const NATURAL: char = '\u{E261}';
const SHARP: char = '\u{E262}';

/// Chant staff spaces are larger than modern ones.
const SCALE: f64 = 1.6;

struct Ctx<'a> {
    score: &'a Score,
    track_index: usize,
    chant: &'a Chant,
    o: Options,
    /// Staff space.
    sp: f64,
    /// Note head width and height.
    w: f64,
    h: f64,
    text_size: f64,
    /// Beat (bar, beat) of each note, keyed by (syllable, item).
    beats: HashMap<(usize, usize), (usize, usize, u8)>,
}

/// What a syllable's notation needs to be placed and drawn.
struct Measure {
    /// Width of the notation.
    width: f64,
    /// Offset of the first note's centre (where the text's vowel goes), if any notes.
    anchor: Option<f64>,
    /// Lowest and highest staff positions used.
    low: i32,
    high: i32,
    /// First pitch (for the custos of the previous line).
    first_pitch: Option<u8>,
}

/// A syllable placed on a line: x of its notation and of its text.
struct Placed {
    index: usize,
    x: f64,
    text_x: f64,
    text_w: f64,
}

pub fn engrave_chant(
    score: &Score,
    track_index: usize,
    track: &Track,
    chant: &Chant,
    options: &Options,
) -> Layout {
    let sp = options.sp * SCALE;
    let t = transcribe(chant);
    let mut beats = HashMap::new();
    for (k, r) in chant.resolved().iter().enumerate() {
        if let Some(&(bar, beat)) = t.positions.get(k) {
            let repeats = match &chant.syllables[r.syllable].items[r.item] {
                NeumeItem::Note(n) => n.repeats(),
                _ => 1,
            };
            beats.insert((r.syllable, r.item), (bar, beat, repeats));
        }
    }
    let _ = track;
    let ctx = Ctx {
        score,
        track_index,
        chant,
        o: *options,
        sp,
        w: 0.95 * sp,
        h: 0.8 * sp,
        text_size: 1.55 * sp,
        beats,
    };
    ctx.run()
}

/// Text as the bundled font can show it.
fn displayable(text: &str) -> String {
    fonts().display(FontKind::Text, text).into_owned()
}

fn is_vowel(c: char) -> bool {
    "aeiouyæœáéíóúýǽàèìòùâêîôûäëïöüÿAEIOUYÆŒÁÉÍÓÚÝǼ".contains(c)
}

/// Byte range of the vowel group the notes are centred on.
fn vowel_range(text: &str) -> Option<(usize, usize)> {
    let start = text.char_indices().find(|(_, c)| is_vowel(*c))?.0;
    let end = text[start..]
        .char_indices()
        .find(|(_, c)| !is_vowel(*c))
        .map_or(text.len(), |(i, _)| start + i);
    Some((start, end))
}

impl Ctx<'_> {
    fn font(&self, s: &Syllable) -> FontKind {
        if s.italic() {
            FontKind::Italic
        } else {
            FontKind::Text
        }
    }

    fn text_width(&self, s: &Syllable, text: &str) -> f64 {
        fonts().text_width(self.font(s), text, self.text_size)
    }

    /// Lines of the staff as staff positions (bottom line first).
    fn lines(&self) -> Vec<i32> {
        (0..self.chant.staff_lines.clamp(2, 5) as i32)
            .map(|i| 3 + 2 * i)
            .collect()
    }

    fn top_position(&self) -> i32 {
        *self.lines().last().unwrap()
    }

    fn run(&self) -> Layout {
        let sp = self.sp;
        let margin = self.o.margin;
        let syllables = &self.chant.syllables;
        let measures: Vec<Measure> = syllables
            .iter()
            .enumerate()
            .map(|(i, s)| self.draw_syllable(i, s, 0.0, &|_| 0.0, None))
            .collect();

        // The initial: the first letter of the first syllable with text.
        let initial_style = self
            .chant
            .header("initial-style")
            .and_then(|v| v.trim().parse::<u32>().ok())
            .unwrap_or(1);
        let initial_syllable = syllables.iter().position(|s| {
            !displayable(&s.plain_text()).trim().is_empty() && s.notes().next().is_some()
        });
        let initial: Option<(usize, String)> = initial_syllable
            .filter(|_| initial_style > 0)
            .and_then(|i| {
                let text = displayable(&syllables[i].plain_text());
                let c = text.trim().chars().next()?;
                Some((i, c.to_uppercase().collect()))
            });
        let initial_size = 4.4 * sp;
        let initial_w = initial.as_ref().map_or(0.0, |(_, c)| {
            fonts().text_width(FontKind::Text, c, initial_size) + 0.6 * sp
        });

        let width = match self.o.mode {
            Mode::Page { width, .. } | Mode::Vertical { width } => width,
            Mode::Horizontal => {
                margin * 2.0
                    + initial_w
                    + 4.0 * sp
                    + measures.iter().map(|m| m.width + 2.0 * sp).sum::<f64>()
                    + syllables
                        .iter()
                        .map(|s| self.text_width(s, &displayable(&s.plain_text())) + sp)
                        .sum::<f64>()
            }
        };
        let clef_w = 1.3 * sp;

        // Break into lines.
        let mut lines: Vec<Vec<Placed>> = Vec::new();
        let mut line: Vec<Placed> = Vec::new();
        let line_start =
            |n: usize| margin + if n == 0 { initial_w } else { 0.0 } + clef_w + 0.8 * sp;
        let right = width - margin - 1.2 * sp;
        let (mut notes_end, mut text_end) = (line_start(0), line_start(0));
        let mut prev_word_end = true;
        let mut i = 0;
        while i < syllables.len() {
            let s = &syllables[i];
            let m = &measures[i];
            // A clef alone at the very start is drawn as the line clef.
            if i == 0
                && s.notes().next().is_none()
                && s.items
                    .iter()
                    .all(|it| matches!(it, NeumeItem::Clef(_) | NeumeItem::Space(_)))
            {
                i += 1;
                continue;
            }
            let mut text = displayable(&s.plain_text());
            if initial.as_ref().is_some_and(|(k, _)| *k == i) {
                text = text.trim_start().chars().skip(1).collect();
            }
            let text = text.trim_end().to_string();
            let tw = self.text_width(s, &text);
            let vowel = vowel_range(&text).map(|(a, b)| {
                self.text_width(s, &text[..a]) + self.text_width(s, &text[a..b]) / 2.0
            });
            let anchor = m.anchor.unwrap_or(m.width / 2.0);
            let center = vowel.unwrap_or(tw / 2.0);
            let gap_text = if s.word_start && !prev_word_end || s.word_start {
                0.45 * self.text_size
            } else {
                0.08 * self.text_size
            };
            let gap_notes = if m.width > 0.0 { 1.0 * sp } else { 0.0 };
            let mut x = (notes_end + gap_notes).max(text_end + gap_text + center - anchor);
            if line.is_empty() {
                x = x.max(line_start(lines.len()) + (center - anchor).max(0.0));
            }
            let extent = (x + m.width).max(x + anchor - center + tw);
            let only_division = s.notes().next().is_none() && s.division().is_some();
            if !line.is_empty()
                && extent > right
                && !only_division
                && self.o.mode != Mode::Horizontal
            {
                lines.push(std::mem::take(&mut line));
                notes_end = line_start(lines.len());
                text_end = notes_end;
                continue;
            }
            line.push(Placed {
                index: i,
                x,
                text_x: x + anchor - center,
                text_w: tw,
            });
            notes_end = x + m.width;
            if !text.is_empty() {
                text_end = x + anchor - center + tw;
            }
            prev_word_end = syllables.get(i + 1).is_none_or(|n| n.word_start);
            i += 1;
        }
        if !line.is_empty() {
            lines.push(line);
        }

        // Justify all but the last line.
        let count = lines.len();
        for (li, line) in lines.iter_mut().enumerate() {
            if li + 1 == count || line.len() < 2 || self.o.mode == Mode::Horizontal {
                continue;
            }
            let end = line
                .iter()
                .map(|p| (p.x + measures[p.index].width).max(p.text_x + p.text_w))
                .fold(0.0, f64::max);
            let extra = (right - end).max(0.0);
            let n = line.len() - 1;
            for (k, p) in line.iter_mut().enumerate() {
                let dx = extra * k as f64 / n as f64;
                p.x += dx;
                p.text_x += dx;
            }
        }

        // Draw.
        let mut layout = Layout {
            pages: Vec::new(),
            beats: HashMap::new(),
            bars: vec![(0, f64::MAX, 0.0); self.score.master_bars.len()],
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
        let top = self.top_position();
        let mut clef = self.chant.initial_clef();
        for (li, line) in lines.iter().enumerate() {
            let high = line
                .iter()
                .map(|p| measures[p.index].high)
                .max()
                .unwrap_or(top)
                .max(top);
            let low = line
                .iter()
                .map(|p| measures[p.index].low)
                .min()
                .unwrap_or(3)
                .min(3);
            let above = (high - top) as f64 * sp / 2.0 + 1.2 * sp;
            let staff_h = (top - 3) as f64 * sp / 2.0;
            let below = ((3 - low) as f64 * sp / 2.0).max(0.0);
            let annotation = li == 0 && initial.is_some();
            let extra_top = if annotation { 1.8 * sp } else { 0.0 };
            let line_h = extra_top + above + staff_h + below + 2.6 * sp + self.text_size;
            if let Mode::Page { height, .. } = self.o.mode {
                if y + line_h > height - margin && y > margin + 1.0 {
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
            let bottom_line = y + extra_top + above + staff_h;
            let ymap = move |p: i32| bottom_line - (p - 3) as f64 * sp / 2.0;
            let text_y = (bottom_line + 2.2 * sp).max(ymap(low) + 1.0 * sp + 0.75 * self.text_size);
            let staff_x0 = margin + if li == 0 { initial_w } else { 0.0 };
            let staff_x1 = width - margin;
            for l in self.lines() {
                page.items.push(Item::Line {
                    from: Point::new(staff_x0, ymap(l)),
                    to: Point::new(staff_x1, ymap(l)),
                    width: 0.07 * sp,
                    color: Color::BLACK,
                });
            }
            self.draw_clef(&mut page, staff_x0 + 0.4 * sp, clef, &ymap);
            if li == 0 {
                if let Some((_, c)) = &initial {
                    page.text(
                        FontKind::Text,
                        c.clone(),
                        (margin, text_y),
                        initial_size,
                        Align::Left,
                    );
                    let annotation: Vec<String> = {
                        let a: Vec<String> = self
                            .chant
                            .headers
                            .iter()
                            .filter(|(k, _)| k == "annotation")
                            .map(|(_, v)| v.clone())
                            .collect();
                        if a.is_empty() {
                            // As printed in chant books: the office part, abbreviated, then
                            // the mode.
                            let office = self.chant.header("office-part").unwrap_or_default();
                            let abbreviation = match office.trim().to_lowercase().as_str() {
                                "" => String::new(),
                                "antiphona" => "Ant.".into(),
                                "hymnus" => "Hymn.".into(),
                                "introitus" => "Intr.".into(),
                                "graduale" => "Grad.".into(),
                                "offertorium" => "Offert.".into(),
                                "communio" => "Comm.".into(),
                                "alleluia" => "Allel.".into(),
                                "responsorium" | "responsorium breve" => "Resp.".into(),
                                "sequentia" => "Seq.".into(),
                                "tractus" => "Tract.".into(),
                                "kyriale" | "kyrie" => "Kyrie".into(),
                                other => other.chars().take(5).collect::<String>() + ".",
                            };
                            let mode = self
                                .chant
                                .header("mode")
                                .map(str::trim)
                                .filter(|m| !m.is_empty())
                                .map(|m| match m.parse::<u32>() {
                                    Ok(n) => format!("{n}."),
                                    Err(_) => m.to_string(),
                                });
                            [Some(abbreviation).filter(|a| !a.is_empty()), mode]
                                .into_iter()
                                .flatten()
                                .collect()
                        } else {
                            a
                        }
                    };
                    for (k, a) in annotation.iter().enumerate() {
                        page.text(
                            FontKind::Bold,
                            a.clone(),
                            (
                                margin + (initial_w - 0.6 * sp) / 2.0,
                                y + 0.6 * sp + k as f64 * 1.35 * sp,
                            ),
                            1.3 * sp,
                            Align::Center,
                        );
                    }
                }
            }
            let page_index = layout.pages.len();
            for (k, p) in line.iter().enumerate() {
                let s = &self.chant.syllables[p.index];
                let mut hits = Vec::new();
                self.draw_syllable(p.index, s, p.x, &ymap, Some((&mut page, &mut hits)));
                for (beat, rect) in hits {
                    let pos = BeatPos {
                        page: page_index,
                        x: rect.x0,
                        width: rect.width(),
                        top: ymap(top) - 1.5 * sp,
                        bottom: text_y + 0.4 * self.text_size,
                        staves: vec![StaffPos::Lines {
                            staff: 0,
                            y0: ymap(0),
                            step: sp / 2.0,
                            range: (i32::MIN, i32::MAX),
                        }],
                    };
                    if let Some(bar) = layout.bars.get_mut(beat.bar) {
                        *bar = (page_index, bar.1.min(rect.x0), bar.2.max(rect.x1));
                    }
                    page.hits.push(Hit {
                        rect: Rect::new(rect.x0, pos.top, rect.x1, pos.bottom),
                        beat,
                        string: None,
                        staff: 0,
                    });
                    layout.beats.insert(beat, pos);
                }
                // Text and hyphens.
                let mut text = displayable(&s.plain_text());
                if initial.as_ref().is_some_and(|(i, _)| *i == p.index) {
                    text = text.trim_start().chars().skip(1).collect();
                }
                let text = text.trim_end();
                if !text.is_empty() {
                    page.text(
                        self.font(s),
                        text.to_string(),
                        (p.text_x, text_y),
                        self.text_size,
                        Align::Left,
                    );
                }
                let next_same_word = self.chant.syllables.get(p.index + 1).is_some_and(|n| {
                    !n.word_start && !displayable(&n.plain_text()).trim().is_empty()
                });
                if next_same_word && !text.is_empty() {
                    let end = p.text_x + p.text_w;
                    let hy = fonts().text_width(FontKind::Text, "-", self.text_size);
                    match line.get(k + 1) {
                        Some(n) if n.text_x - end > hy * 1.15 => {
                            page.text(
                                FontKind::Text,
                                "-",
                                ((end + n.text_x) / 2.0, text_y),
                                self.text_size,
                                Align::Center,
                            );
                        }
                        None => {
                            page.text(
                                FontKind::Text,
                                "-",
                                (end + 0.15 * sp, text_y),
                                self.text_size,
                                Align::Left,
                            );
                        }
                        _ => {}
                    }
                }
                // Track the clef for the next line.
                for item in &s.items {
                    if let NeumeItem::Clef(c) = item {
                        clef = *c;
                    }
                }
            }
            // Custos: the first note of the next line.
            if let Some(next) = lines.get(li + 1) {
                if let Some(pitch) = next.iter().find_map(|p| measures[p.index].first_pitch) {
                    self.draw_custos(&mut page, staff_x1 - 0.7 * sp, pitch as i32, &ymap);
                }
            }
            y += line_h;
        }
        if let Mode::Page { height, .. } = self.o.mode {
            page.height = height;
        } else {
            page.height = y + margin;
        }
        layout.pages.push(page);
        for b in &mut layout.bars {
            if b.1 == f64::MAX {
                *b = (b.0, 0.0, 0.0);
            }
        }
        layout
    }

    fn draw_header(&self, page: &mut Page, width: f64, y: f64) -> f64 {
        let sp = self.o.sp;
        let m = &self.score.meta;
        let mut y = y;
        if !m.title.is_empty() {
            y += 5.0 * sp;
            page.text(
                FontKind::Bold,
                m.title.clone(),
                (width / 2.0, y),
                4.6 * sp,
                Align::Center,
            );
        }
        let office = self.chant.header("office-part").unwrap_or_default();
        let commentary = self.chant.header("commentary").unwrap_or_default();
        if !office.is_empty() || !commentary.is_empty() {
            y += 3.4 * sp;
            if !office.is_empty() {
                page.text(
                    FontKind::Italic,
                    office.to_string(),
                    (self.o.margin, y),
                    2.6 * sp,
                    Align::Left,
                );
            }
            if !commentary.is_empty() {
                page.text(
                    FontKind::Italic,
                    commentary.to_string(),
                    (width - self.o.margin, y),
                    2.4 * sp,
                    Align::Right,
                );
            }
        }
        y + 2.0 * sp
    }

    /// Lays out (and with `page`, draws) a syllable's notation starting at `x`.
    fn draw_syllable(
        &self,
        si: usize,
        s: &Syllable,
        x0: f64,
        ymap: &dyn Fn(i32) -> f64,
        mut out: Option<(&mut Page, &mut Vec<(BeatRef, Rect)>)>,
    ) -> Measure {
        let sp = self.sp;
        let mut x = x0;
        let mut m = Measure {
            width: 0.0,
            anchor: None,
            low: i32::MAX,
            high: i32::MIN,
            first_pitch: None,
        };
        let mut scratch = Page::default();
        let mut scratch_hits = Vec::new();
        let (page, hits): (&mut Page, &mut Vec<(BeatRef, Rect)>) = match out.as_mut() {
            Some((p, h)) => (p, h),
            None => (&mut scratch, &mut scratch_hits),
        };
        // Split into runs of notes between separators.
        let items = &s.items;
        let mut i = 0;
        let mut pending_gap = 0.0;
        let mut started = false;
        while i < items.len() {
            match &items[i] {
                NeumeItem::Note(_) => {
                    let start = i;
                    while i < items.len() && matches!(items[i], NeumeItem::Note(_)) {
                        i += 1;
                    }
                    let run: Vec<(usize, &ChantNote)> = (start..i)
                        .map(|k| match &items[k] {
                            NeumeItem::Note(n) => (k, n),
                            _ => unreachable!(),
                        })
                        .collect();
                    if started {
                        x += pending_gap;
                    }
                    for (gi, glyph) in group(&run).iter().enumerate() {
                        if gi > 0 {
                            let prev = run_glyph_last(glyph);
                            x += if prev { -0.2 * self.w } else { 0.22 * sp };
                        }
                        let (wd, centers) = self.draw_glyph(page, x, glyph, ymap);
                        for ((k, n), cx) in glyph.iter().zip(&centers) {
                            m.first_pitch.get_or_insert(n.pitch);
                            m.anchor.get_or_insert(cx - x0);
                            m.low = m.low.min(n.pitch as i32);
                            m.high = m.high.max(n.pitch as i32);
                            if let Some(&(bar, beat, repeats)) = self.beats.get(&(si, *k)) {
                                let rect = Rect::new(
                                    cx - self.w / 2.0,
                                    ymap(n.pitch as i32) - self.h,
                                    cx + self.w / 2.0,
                                    ymap(n.pitch as i32) + self.h,
                                );
                                for r in 0..repeats as usize {
                                    hits.push((
                                        BeatRef {
                                            track: self.track_index,
                                            bar,
                                            voice: 0,
                                            beat: beat + r,
                                        },
                                        rect,
                                    ));
                                }
                            }
                        }
                        x += wd;
                    }
                    started = true;
                    pending_gap = 0.25 * sp;
                    continue;
                }
                NeumeItem::Space(t) => {
                    pending_gap = match t.trim_start_matches('!') {
                        "" => 0.02 * sp,
                        "/0" => 0.3 * sp,
                        "/" => 0.55 * sp,
                        "//" => 0.95 * sp,
                        " " => 1.25 * sp,
                        "/!" => 0.1 * sp,
                        t if t.starts_with("/[") => {
                            t[2..t.len() - 1].parse::<f64>().unwrap_or(1.0) * 0.5 * sp
                        }
                        _ => 0.55 * sp,
                    };
                }
                NeumeItem::Alteration { pitch, sign } => {
                    if started {
                        x += pending_gap.min(0.3 * sp);
                    }
                    let ch = match sign.chars().next() {
                        Some('x' | 'X') => FLAT,
                        Some('#') => SHARP,
                        _ => NATURAL,
                    };
                    let size = 3.0 * sp;
                    page.glyph(ch, (x, ymap(*pitch as i32)), size);
                    x += fonts()
                        .bbox(crate::display::FontKind::Music, ch, size)
                        .width()
                        + 0.3 * sp;
                    m.low = m.low.min(*pitch as i32);
                    m.high = m.high.max(*pitch as i32 + 1);
                    started = true;
                    pending_gap = 0.0;
                }
                NeumeItem::Clef(c) => {
                    if started {
                        x += 0.8 * sp;
                    }
                    if si > 0 || started {
                        self.draw_clef(page, x, *c, ymap);
                        x += 1.3 * sp;
                    }
                    started = true;
                    pending_gap = 0.4 * sp;
                }
                NeumeItem::Bar(b) => {
                    if started {
                        x += 0.9 * sp;
                    }
                    self.draw_division(page, x, Division::of(b), ymap);
                    let w = if Division::of(b) == Division::Finalis {
                        0.7 * sp
                    } else {
                        0.15 * sp
                    };
                    m.anchor.get_or_insert(x - x0 + w / 2.0);
                    x += w;
                    started = true;
                    pending_gap = 0.9 * sp;
                }
                _ => {}
            }
            i += 1;
        }
        m.width = x - x0;
        if m.low == i32::MAX {
            m.low = 3;
            m.high = 3;
        }
        m
    }

    /// Draws one ligature; returns its width and each note's centre x.
    fn draw_glyph(
        &self,
        page: &mut Page,
        x: f64,
        notes: &[(usize, &ChantNote)],
        ymap: &dyn Fn(i32) -> f64,
    ) -> (f64, Vec<f64>) {
        let (w, sp) = (self.w, self.sp);
        let y = |n: &ChantNote| ymap(n.pitch as i32);
        let small = |k: usize, n: &ChantNote| {
            (k + 1 == notes.len() && n.liquescence() == Liquescence::Deminutus && notes.len() > 1)
                || (k == 0 && n.initio_debilis && notes.len() > 1)
        };
        let ups: Vec<bool> = notes
            .windows(2)
            .map(|p| p[1].1.pitch > p[0].1.pitch)
            .collect();
        let stroke = 0.09 * sp;
        let vline = |page: &mut Page, x: f64, a: f64, b: f64| {
            page.items.push(Item::Line {
                from: Point::new(x, a.min(b) - self.h * 0.35),
                to: Point::new(x, a.max(b) + self.h * 0.35),
                width: stroke,
                color: Color::BLACK,
            });
        };
        let n = notes.len();
        if n == 1 {
            let note = notes[0].1;
            let wd = self.draw_note(page, x, y(note), note, false);
            self.draw_signs(page, x, wd, note, ymap);
            return (wd, vec![x + wd / 2.0]);
        }
        // Pes: stacked.
        if n == 2 && ups[0] {
            let (a, b) = (notes[0].1, notes[1].1);
            self.draw_note(page, x, y(a), a, small(0, a));
            self.draw_note(page, x, y(b), b, small(1, b));
            vline(page, x + w - stroke / 2.0, y(a), y(b));
            self.draw_signs(page, x, w, a, ymap);
            self.draw_signs(page, x, w, b, ymap);
            return (w, vec![x + w / 2.0, x + w / 2.0]);
        }
        // Porrectus (and flexus): an oblique stroke for the first two notes.
        let oblique_at = if ups.first() == Some(&false) && ups.get(1) == Some(&true) {
            Some(0)
        } else if n >= 4 && ups[0] && !ups[1] && ups[2] {
            Some(1)
        } else {
            None
        };
        let mut centers = Vec::new();
        let mut cx = x;
        let mut k = 0;
        while k < n {
            let note = notes[k].1;
            if Some(k) == oblique_at {
                let next = notes[k + 1].1;
                let len = 1.8 * w;
                let (y1, y2) = (y(note), y(next));
                let mut p = BezPath::new();
                p.move_to((cx, y1 - self.h / 2.0));
                p.line_to((cx + len, y2 - self.h / 2.0));
                p.line_to((cx + len, y2 + self.h / 2.0));
                p.line_to((cx, y1 + self.h / 2.0));
                p.close_path();
                page.items.push(Item::Path {
                    path: p,
                    color: Color::BLACK,
                    stroke: None,
                });
                if k == 0 {
                    vline(page, cx + stroke / 2.0, y1, y1 + 1.2 * sp);
                } else {
                    vline(page, cx, y(notes[k - 1].1), y1);
                }
                centers.push(cx + 0.3 * w);
                centers.push(cx + len - 0.3 * w);
                self.draw_signs(page, cx, 0.6 * w, note, ymap);
                // The note after the oblique sits above its end.
                let third = notes[k + 2].1;
                let tx = cx + len - w;
                self.draw_note(page, tx, y(third), third, small(k + 2, third));
                vline(page, cx + len - stroke / 2.0, y2, y(third));
                centers.push(tx + w / 2.0);
                self.draw_signs(page, tx, w, third, ymap);
                cx += len;
                k += 3;
                continue;
            }
            if k > 0 {
                vline(page, cx, y(notes[k - 1].1), y(note));
            } else if !ups[0] {
                // Clivis-style first note: stem on the left, down past the next note.
                let next = y(notes[1].1);
                vline(
                    page,
                    cx + stroke / 2.0,
                    y(note),
                    next.max(y(note) + 0.9 * sp),
                );
            }
            let wd = self.draw_note(page, cx, y(note), note, small(k, note));
            self.draw_signs(page, cx, wd, note, ymap);
            centers.push(cx + wd / 2.0);
            cx += wd;
            k += 1;
        }
        (cx - x, centers)
    }

    /// Draws a note head; returns its width.
    fn draw_note(&self, page: &mut Page, x: f64, y: f64, n: &ChantNote, small: bool) -> f64 {
        let sp = self.sp;
        let (w, h) = if small {
            (self.w * 0.6, self.h * 0.6)
        } else {
            (self.w, self.h)
        };
        let x = if small { x + (self.w - w) / 2.0 } else { x };
        let black = |page: &mut Page, p: BezPath| {
            page.items.push(Item::Path {
                path: p,
                color: Color::BLACK,
                stroke: None,
            })
        };
        let stem = |page: &mut Page, x: f64, from: f64, to: f64| {
            page.items.push(Item::Line {
                from: Point::new(x, from),
                to: Point::new(x, to),
                width: 0.09 * sp,
                color: Color::BLACK,
            })
        };
        let punctum = |x: f64, w: f64, h: f64| {
            // Slightly concave top and bottom.
            let d = 0.08 * h;
            let mut p = BezPath::new();
            p.move_to((x, y - h / 2.0));
            p.quad_to((x + w / 2.0, y - h / 2.0 + d), (x + w, y - h / 2.0));
            p.line_to((x + w, y + h / 2.0));
            p.quad_to((x + w / 2.0, y + h / 2.0 + d), (x, y + h / 2.0));
            p.close_path();
            p
        };
        match n.shape() {
            Shape::Inclinatum => {
                let (hw, hh) = (w * 0.5, h * 0.55);
                let mut p = BezPath::new();
                p.move_to((x + hw, y - hh));
                p.line_to((x + 2.0 * hw, y));
                p.line_to((x + hw, y + hh));
                p.line_to((x, y));
                p.close_path();
                black(page, p);
                return w;
            }
            Shape::Quilisma => {
                let mut p = BezPath::new();
                p.move_to((x, y + 0.45 * h));
                p.line_to((x + w, y + 0.25 * h));
                p.line_to((x + w, y - 0.5 * h));
                for (i, t) in [0.8, 0.6, 0.4, 0.2, 0.0].iter().enumerate() {
                    let up = if i % 2 == 0 { -0.1 } else { -0.55 };
                    p.line_to((x + t * w, y + up * h));
                }
                p.close_path();
                black(page, p);
                return w;
            }
            Shape::Oriscus(stem_down) => {
                let mut p = BezPath::new();
                p.move_to((x, y - 0.1 * h));
                p.curve_to(
                    (x + 0.35 * w, y - 0.75 * h),
                    (x + 0.6 * w, y + 0.05 * h),
                    (x + w, y - 0.55 * h),
                );
                p.line_to((x + w, y + 0.15 * h));
                p.curve_to(
                    (x + 0.6 * w, y + 0.75 * h),
                    (x + 0.35 * w, y - 0.05 * h),
                    (x, y + 0.55 * h),
                );
                p.close_path();
                black(page, p);
                if stem_down {
                    stem(page, x + w - 0.05 * sp, y, y + 1.6 * sp);
                }
                return w;
            }
            Shape::Stropha(count) => {
                let sw = w * 0.75;
                for i in 0..count as usize {
                    let sx = x + i as f64 * (sw + 0.25 * sp);
                    let mut p = BezPath::new();
                    p.move_to((sx, y - 0.4 * h));
                    p.line_to((sx + sw, y - 0.4 * h));
                    p.line_to((sx + sw, y + 0.15 * h));
                    p.line_to((sx + 0.45 * sw, y + 0.55 * h));
                    p.line_to((sx, y + 0.3 * h));
                    p.close_path();
                    black(page, p);
                }
                return count as f64 * sw + (count as f64 - 1.0) * 0.25 * sp;
            }
            Shape::Bivirga(count) => {
                for i in 0..count as usize {
                    let vx = x + i as f64 * (w + 0.3 * sp);
                    black(page, punctum(vx, w, h));
                    stem(page, vx + w - 0.045 * sp, y, y + 1.9 * sp);
                }
                return count as f64 * w + (count as f64 - 1.0) * 0.3 * sp;
            }
            _ => {}
        }
        if n.cavum() {
            page.items.push(Item::Path {
                path: punctum(x + 0.05 * sp, w - 0.1 * sp, h - 0.1 * sp),
                color: Color::BLACK,
                stroke: Some(0.1 * sp),
            });
        } else {
            black(page, punctum(x, w, h));
        }
        if n.linea() {
            stem(page, x - 0.2 * sp, y - 0.8 * sp, y + 0.8 * sp);
            stem(page, x + w + 0.2 * sp, y - 0.8 * sp, y + 0.8 * sp);
        }
        match n.shape() {
            Shape::Virga => stem(page, x + w - 0.045 * sp, y, y + 1.9 * sp),
            Shape::VirgaReversa => stem(page, x + 0.045 * sp, y, y + 1.9 * sp),
            _ => {}
        }
        match n.liquescence() {
            Liquescence::AuctusAscendens => stem(page, x + w - 0.045 * sp, y, y - 0.9 * sp),
            Liquescence::AuctusDescendens => stem(page, x + w - 0.045 * sp, y, y + 0.9 * sp),
            _ => {}
        }
        w
    }

    /// Mora, episemata.
    fn draw_signs(
        &self,
        page: &mut Page,
        x: f64,
        w: f64,
        n: &ChantNote,
        ymap: &dyn Fn(i32) -> f64,
    ) {
        let sp = self.sp;
        let p = n.pitch as i32;
        let y = ymap(p);
        let on_line = self.lines().contains(&p)
            || (p % 2 == 1 && p < 3)
            || (p % 2 == 1 && p > self.top_position());
        for d in 0..n.mora() {
            let dy = if on_line { y - sp / 2.0 } else { y };
            let c = Circle::new((x + w + 0.35 * sp + d as f64 * 0.45 * sp, dy), 0.15 * sp);
            page.items.push(Item::Path {
                path: c.to_path(0.05),
                color: Color::BLACK,
                stroke: None,
            });
        }
        if n.episema() {
            let below = n.mods.iter().any(|m| {
                m.starts_with('_') && m.contains('0') || m.contains('1') && m.starts_with('_')
            });
            let ey = if below {
                y + self.h / 2.0 + 0.3 * sp
            } else {
                y - self.h / 2.0 - 0.3 * sp
            };
            page.items.push(Item::Line {
                from: Point::new(x, ey),
                to: Point::new(x + w, ey),
                width: 0.12 * sp,
                color: Color::BLACK,
            });
        }
        if n.ictus() {
            let iy = y + self.h / 2.0 + 0.2 * sp;
            page.items.push(Item::Line {
                from: Point::new(x + w / 2.0, iy),
                to: Point::new(x + w / 2.0, iy + 0.5 * sp),
                width: 0.1 * sp,
                color: Color::BLACK,
            });
        }
    }

    fn draw_clef(&self, page: &mut Page, x: f64, clef: ChantClef, ymap: &dyn Fn(i32) -> f64) {
        let sp = self.sp;
        let ly = ymap(clef.position());
        let block = |page: &mut Page, r: Rect| {
            page.items.push(Item::Rect {
                rect: r,
                color: Color::BLACK,
            })
        };
        match clef.kind {
            ClefKind::Do => {
                block(
                    page,
                    Rect::new(x, ly - 0.95 * sp, x + 0.22 * sp, ly + 0.95 * sp),
                );
                block(
                    page,
                    Rect::new(x, ly - 0.95 * sp, x + 0.75 * sp, ly - 0.2 * sp),
                );
                block(
                    page,
                    Rect::new(x, ly + 0.2 * sp, x + 0.75 * sp, ly + 0.95 * sp),
                );
            }
            ClefKind::Fa => {
                block(
                    page,
                    Rect::new(x, ly - 0.95 * sp, x + 0.22 * sp, ly + 1.4 * sp),
                );
                block(
                    page,
                    Rect::new(x, ly - 0.95 * sp, x + 0.7 * sp, ly - 0.2 * sp),
                );
                block(
                    page,
                    Rect::new(x, ly + 0.2 * sp, x + 0.7 * sp, ly + 0.95 * sp),
                );
                block(
                    page,
                    Rect::new(x + 0.95 * sp, ly - 0.75 * sp, x + 1.55 * sp, ly - 0.15 * sp),
                );
                block(
                    page,
                    Rect::new(x + 0.95 * sp, ly + 0.15 * sp, x + 1.55 * sp, ly + 0.75 * sp),
                );
            }
        }
        if clef.flat {
            // The flat sits on ti, the step below do.
            let mut ti = clef.position();
            while clef.degree(ti) != 6 {
                ti -= 1;
            }
            page.glyph(FLAT, (x + 1.0 * sp, ymap(ti)), 3.0 * sp);
        }
    }

    fn draw_division(&self, page: &mut Page, x: f64, d: Division, ymap: &dyn Fn(i32) -> f64) {
        let sp = self.sp;
        let lines = self.lines();
        let (bottom, top) = (ymap(lines[0]), ymap(*lines.last().unwrap()));
        let line = |page: &mut Page, x: f64, a: f64, b: f64, w: f64| {
            page.items.push(Item::Line {
                from: Point::new(x, a),
                to: Point::new(x, b),
                width: w,
                color: Color::BLACK,
            })
        };
        let thin = 0.1 * sp;
        match d {
            Division::Virgula => {
                let mut p = BezPath::new();
                p.move_to((x, top - 0.9 * sp));
                p.quad_to(
                    (x + 0.25 * sp, top - 0.5 * sp),
                    (x + 0.05 * sp, top - 0.1 * sp),
                );
                page.items.push(Item::Path {
                    path: p,
                    color: Color::BLACK,
                    stroke: Some(thin),
                });
            }
            Division::Minima => line(page, x, top - 0.5 * sp, top + 0.5 * sp, thin),
            Division::Minor => {
                let inner = if lines.len() >= 4 {
                    (ymap(lines[1]), ymap(lines[lines.len() - 2]))
                } else {
                    (bottom - 0.3 * sp, top + 0.3 * sp)
                };
                line(page, x, inner.0 + 0.3 * sp, inner.1 - 0.3 * sp, thin);
            }
            Division::Maior => line(page, x, top, bottom, thin),
            Division::Finalis => {
                line(page, x, top, bottom, thin);
                line(page, x + 0.55 * sp, top, bottom, 0.25 * sp);
            }
        }
    }

    fn draw_custos(&self, page: &mut Page, x: f64, pitch: i32, ymap: &dyn Fn(i32) -> f64) {
        let sp = self.sp;
        let y = ymap(pitch);
        page.items.push(Item::Rect {
            rect: Rect::new(x, y - 0.22 * sp, x + 0.35 * sp, y + 0.22 * sp),
            color: Color::BLACK,
        });
        let middle = (3 + self.top_position()) / 2;
        let (a, b) = if pitch <= middle {
            (y, y - 1.1 * sp)
        } else {
            (y, y + 1.1 * sp)
        };
        page.items.push(Item::Line {
            from: Point::new(x + 0.33 * sp, a),
            to: Point::new(x + 0.33 * sp, b),
            width: 0.07 * sp,
            color: Color::BLACK,
        });
    }
}

/// True when a glyph is a single diamond (puncta inclinata pack closely).
fn run_glyph_last(glyph: &[(usize, &ChantNote)]) -> bool {
    glyph.len() == 1 && glyph[0].1.shape() == Shape::Inclinatum
}

/// Groups a run of notes (no separators) into ligatures.
fn group<'a>(run: &[(usize, &'a ChantNote)]) -> Vec<Vec<(usize, &'a ChantNote)>> {
    let ligates = |n: &ChantNote| matches!(n.shape(), Shape::Punctum) && !n.cavum() && !n.linea();
    let mut out: Vec<Vec<(usize, &ChantNote)>> = Vec::new();
    let mut cur: Vec<(usize, &ChantNote)> = Vec::new();
    for (i, &(k, n)) in run.iter().enumerate() {
        let extend = match cur.last() {
            None => false,
            Some(&(_, prev)) => {
                let starts_ok = match cur[0].1.shape() {
                    Shape::Punctum => ligates(cur[0].1) || cur[0].1.initio_debilis,
                    // A quilisma or square note joins the next, higher note.
                    Shape::Quilisma | Shape::Quadratum => cur.len() == 1 && n.pitch > prev.pitch,
                    _ => false,
                };
                let dirs: Vec<bool> = cur
                    .windows(2)
                    .map(|p| p[1].1.pitch > p[0].1.pitch)
                    .chain([n.pitch > prev.pitch])
                    .collect();
                let pattern_ok = matches!(
                    dirs.as_slice(),
                    [_] | [true, false]
                        | [false, true]
                        | [true, false, true]
                        | [false, true, false]
                );
                // Scandicus: three rising notes are a punctum and a pes.
                let scandicus = cur.len() == 1
                    && n.pitch > prev.pitch
                    && run
                        .get(i + 1)
                        .is_some_and(|&(_, nx)| nx.pitch > n.pitch && ligates(nx));
                starts_ok
                    && ligates(n)
                    && n.pitch != prev.pitch
                    && pattern_ok
                    && !scandicus
                    && (prev.liquescence() == Liquescence::None)
            }
        };
        if !extend && !cur.is_empty() {
            out.push(std::mem::take(&mut cur));
        }
        cur.push((k, n));
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    out
}
