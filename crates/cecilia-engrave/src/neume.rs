//! Square notation set with the Caeciliae font (Oblates of St. Joseph, SIL OFL).
//!
//! Caeciliae keeps a glyph for every note head at every staff height, so a glyph is picked by
//! the element and the height it stands at. Heights are Caeciliae's digits: 1 is the bottom
//! line, 2 the space above it, 3 the next line and so on (GABC position minus 2); 125 font
//! units are one step and the gap between two lines is 250 units, so one em is four gaps. Neumes
//! are put together the way the font's tutorial does: heads one after the other, joined by its
//! connecting lines, with its ready-made podatus, clivis and porrectus glyphs where they exist.
//!
//! What the font lacks is drawn by hand where it is needed: the horizontal episema, the sharp,
//! and the large mora dot.

use kurbo::{BezPath, Circle, Point, Shape as _};

use crate::display::{Color, FontKind, Item, Page};

/// Advance of a head, in font units.
pub const HEAD: f64 = 128.0;
/// Height of a head, in font units.
pub const HEAD_H: f64 = 158.0;

/// What a note looks like.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Punctum,
    Virga,
    Inclinatum,
    Quilisma,
    /// Oriscus; `true` ascending.
    Oriscus(bool),
    Stropha,
    Hollow,
    /// A small note: a liquescent's or an initio debilis.
    Small,
}

/// One note of a neume: a GABC staff position (3 = bottom line) and its shape.
#[derive(Debug, Clone, Copy)]
pub struct N {
    pub pos: i32,
    pub kind: Kind,
}

/// Fills a path black.
pub fn fill(page: &mut Page, path: BezPath) {
    page.items.push(Item::Path {
        path,
        color: Color::BLACK,
        stroke: None,
    });
}

/// Caeciliae's height digit of a GABC position.
pub fn digit(pos: i32) -> i32 {
    (pos - 2).clamp(-2, 10)
}

/// A page to set glyphs on, over a staff whose bottom line is at `bottom` and whose line gap is
/// `g`.
pub struct Typeset<'a> {
    pub page: &'a mut Page,
    pub bottom: f64,
    pub g: f64,
}

impl Typeset<'_> {
    /// Page units per font unit.
    pub fn u(&self) -> f64 {
        self.g / 250.0
    }

    /// Page y of the centre of a height digit.
    pub fn y(&self, n: i32) -> f64 {
        self.bottom - (n - 1) as f64 * self.g / 2.0
    }

    /// Draws a glyph by code with its pen at `x`, raised by `dy` font units.
    pub fn glyph(&mut self, code: i32, x: f64, dy: f64) {
        let Some(ch) = u32::try_from(code).ok().and_then(char::from_u32) else {
            return;
        };
        // The bottom line is at font y -8.
        let baseline = self.bottom - 8.0 * self.u();
        self.page.items.push(Item::Glyph {
            font: FontKind::Chant,
            ch,
            at: Point::new(x, baseline - dy * self.u()),
            size: 4.0 * self.g,
            color: Color::BLACK,
        });
    }

    fn head(&mut self, kind: Kind, n: i32, x: f64) -> f64 {
        let base = match kind {
            Kind::Punctum => 0xE105,
            Kind::Virga => 0xE125,
            Kind::Inclinatum | Kind::Small => 0xE1D5,
            Kind::Quilisma => 0xE145,
            Kind::Oriscus(true) => 0xE1B5,
            Kind::Oriscus(false) => 0xE1C5,
            Kind::Stropha => 0xE285,
            Kind::Hollow => 0xE275,
        };
        self.glyph(base + n, x, 0.0);
        match kind {
            Kind::Inclinatum | Kind::Small => 96.0,
            _ => HEAD,
        }
    }

    /// A connecting line between heights `lo` and `lo + k` (k ≥ 2) drawn at `x`.
    fn line(&mut self, x: f64, lo: i32, k: i32) {
        if k >= 2 {
            self.glyph(0xE700 + 0x10 * (k - 2) + 5 + lo, x, 0.0);
        }
    }

    /// A neume set from `x`: returns its width and the centre x of each head.
    pub fn neume(&mut self, x: f64, notes: &[N]) -> (f64, Vec<f64>) {
        let u = self.u();
        let d: Vec<i32> = notes.iter().map(|n| digit(n.pos)).collect();
        let mut centers = Vec::new();
        let mut pen = 0.0;
        let all_punctum = notes.iter().all(|n| n.kind == Kind::Punctum);
        if all_punctum && notes.len() == 2 && d[1] > d[0] {
            let k = d[1] - d[0];
            if k <= 4 {
                // A podatus glyph holds both notes.
                self.glyph(0xE170 + 0x10 * (k - 1) + 5 + d[0], x, 0.0);
            } else {
                self.head(Kind::Punctum, d[0], x);
                self.line(x, d[0], k);
                self.head(Kind::Punctum, d[1], x);
            }
            return (HEAD * u, vec![x + HEAD / 2.0 * u; 2]);
        }
        if all_punctum && notes.len() == 2 && d[1] < d[0] {
            // Clivis: for one step a glyph with the stem, else the stem, the heads and a line.
            let k = d[0] - d[1];
            if k == 1 {
                self.glyph(0xE130 + 5 + d[0], x, 0.0);
                self.head(Kind::Punctum, d[1], x + HEAD * u);
                return (2.0 * HEAD * u, vec![x + 64.0 * u, x + 192.0 * u]);
            }
            self.glyph(0xE740 + 0x10 * (k - 1) + 5 + d[1], x, 0.0);
            pen += 11.0;
            self.head(Kind::Punctum, d[0], x + pen * u);
            centers.push(x + (pen + 64.0) * u);
            pen += HEAD;
            self.line(x + pen * u, d[1], k);
            pen += 11.0;
            self.head(Kind::Punctum, d[1], x + pen * u);
            centers.push(x + (pen + 64.0) * u);
            return ((pen + HEAD) * u, centers);
        }
        if all_punctum && notes.len() == 3 && d[1] < d[0] && d[2] > d[1] && d[0] - d[1] <= 4 {
            // Porrectus: the oblique stroke glyph, then the last note joined to its end.
            let k = d[0] - d[1];
            self.glyph(0xE220 + 0x10 * (k - 1) + 5 + d[0], x, 0.0);
            let adv = if k == 1 { 386.0 } else { 483.0 };
            self.glyph(0xE160 + 5 + d[2], x + adv * u, 0.0);
            return (
                adv * u,
                vec![x + 64.0 * u, x + (adv - 190.0) * u, x + (adv - 59.0) * u],
            );
        }
        for (i, note) in notes.iter().enumerate() {
            let adv = self.head(note.kind, d[i], x + pen * u);
            centers.push(x + (pen + adv / 2.0) * u);
            pen += adv;
            let Some(next) = notes.get(i + 1) else {
                continue;
            };
            let k = (d[i + 1] - d[i]).abs();
            if all_punctum {
                if k >= 2 {
                    self.line(x + pen * u, d[i].min(d[i + 1]), k);
                    pen += 11.0;
                }
            } else if note.kind == Kind::Virga
                || (note.kind == Kind::Inclinatum && next.kind == Kind::Inclinatum && k > 1)
            {
                // A little room after a virga and between inclinata that span a step.
                pen += 32.0;
            }
        }
        (pen * u, centers)
    }

    /// A single note; returns its width (several for a stropha or bivirga).
    pub fn single(&mut self, x: f64, note: N, count: u8) -> (f64, Vec<f64>) {
        let u = self.u();
        let n = digit(note.pos);
        let mut pen = 0.0;
        let mut centers = Vec::new();
        for i in 0..count.max(1) {
            let adv = self.head(note.kind, n, x + pen * u);
            centers.push(x + (pen + adv / 2.0) * u);
            pen += adv;
            if i + 1 < count {
                pen += if note.kind == Kind::Stropha {
                    64.0
                } else {
                    40.0
                };
            }
        }
        (pen * u, centers)
    }

    /// The C or F clef on the line at GABC position `line_pos`; returns its width.
    pub fn clef(&mut self, x: f64, fa: bool, line_pos: i32) -> f64 {
        let n = line_pos - 2;
        if fa {
            self.glyph('f' as i32, x, (n - 5) as f64 * 125.0);
            248.0 * self.u()
        } else {
            self.glyph('d' as i32, x, (n - 3) as f64 * 125.0);
            HEAD * self.u()
        }
    }

    /// A custos with its left edge at `x`, at GABC position `pos`.
    pub fn custos(&mut self, x: f64, pos: i32) {
        self.glyph(0xE2A5 + digit(pos), x, 0.0);
    }

    /// The mora dot after a head ending at `x`: a circle a little larger than the font's own.
    /// On a line (an odd height digit) it sits high, in the space above the head's middle; in a
    /// space it sits low, so it never lies on a line.
    pub fn mora(&mut self, x: f64, pos: i32) {
        let n = pos - 2;
        let up = if n.rem_euclid(2) == 1 { 90.0 } else { -45.0 };
        let c = Circle::new(
            (x + 92.0 * self.u(), self.y(n) - up * self.u()),
            0.12 * self.g,
        );
        fill(self.page, c.to_path(0.05));
    }

    /// An ictus (a short vertical tick) under the head at height digit `n` and centre `cx`,
    /// that never crosses a staff line: it sits in the space just below the head, or, when
    /// that space is too narrow for it, in the one below the next line. `lines` are the page
    /// y of the staff lines.
    pub fn ictus(&mut self, cx: f64, n: i32, lines: &[f64]) {
        let (g, u) = (self.g, self.u());
        let w = 0.07 * g;
        let margin = 0.1 * g + w / 2.0;
        let start = self.y(n) + 0.5 * HEAD_H * u + 0.08 * g;
        let len = 0.3 * g;
        // The first line below the head's bottom edge bounds the space the tick starts in.
        let below = lines
            .iter()
            .copied()
            .filter(|&y| y >= start - margin)
            .fold(f64::INFINITY, f64::min);
        let (a, b) = if below.is_finite() && below - margin - start < 0.15 * g {
            // No room: the space under that line.
            (below + margin, below + margin + len)
        } else if below.is_finite() {
            let a = start;
            (a, (a + len).min(below - margin))
        } else {
            (start, start + len)
        };
        self.page.line((cx, a), (cx, b), w);
    }

    /// A flat or natural at height `pos`, its right edge at `x`; returns false for a sharp.
    pub fn accidental(&mut self, x: f64, pos: i32, flat: bool) {
        let n = pos - 2;
        let e = n.div_euclid(2);
        let shift = (n.rem_euclid(2)) as f64 * 125.0;
        let base = if flat { 0xE0F2 } else { 0xE0FA };
        self.glyph(base + e, x - HEAD * self.u(), shift);
    }

    /// A division centred on `x` for a four-line staff.
    pub fn division(&mut self, x: f64, kind: Division) {
        let u = self.u();
        match kind {
            Division::Virgula => self.glyph(',' as i32, x - 95.0 * u, 0.0),
            Division::Minima => self.glyph('+' as i32, x - 64.0 * u, 0.0),
            Division::Minor => self.glyph(';' as i32, x - 64.0 * u, 0.0),
            Division::Maior => self.glyph('|' as i32, x - 90.0 * u, 0.0),
            Division::Finalis => {
                self.glyph('|' as i32, x - 148.0 * u, 0.0);
                self.glyph('|' as i32, x - 32.0 * u, 0.0);
            }
        }
    }
}

/// The divisions the font has glyphs for.
pub type Division = cecilia_core::chant::Division;
