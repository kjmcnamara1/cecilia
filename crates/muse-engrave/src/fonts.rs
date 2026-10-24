//! Bundled fonts and glyph outlines. Everything is drawn as outlines, so output never depends
//! on installed fonts.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

use kurbo::{Affine, BezPath, Rect};
use ttf_parser::{Face, GlyphId, OutlineBuilder};

use crate::display::{Align, FontKind};

static MUSIC: &[u8] = include_bytes!("../../../assets/fonts/Bravura.otf");
static TEXT: &[u8] = include_bytes!("../../../assets/fonts/SourceSans3-Regular.otf");
static BOLD: &[u8] = include_bytes!("../../../assets/fonts/SourceSans3-Bold.otf");

pub struct Fonts {
    faces: [Face<'static>; 3],
    /// Unit-space outline per (font, glyph).
    cache: Mutex<HashMap<(usize, u16), Option<BezPath>>>,
}

/// The process-wide font set.
pub fn fonts() -> &'static Fonts {
    static FONTS: OnceLock<Fonts> = OnceLock::new();
    FONTS.get_or_init(|| Fonts {
        faces: [
            Face::parse(MUSIC, 0).expect("bundled music font"),
            Face::parse(TEXT, 0).expect("bundled text font"),
            Face::parse(BOLD, 0).expect("bundled bold font"),
        ],
        cache: Mutex::new(HashMap::new()),
    })
}

struct Builder(BezPath);

impl OutlineBuilder for Builder {
    fn move_to(&mut self, x: f32, y: f32) {
        self.0.move_to((x as f64, y as f64));
    }
    fn line_to(&mut self, x: f32, y: f32) {
        self.0.line_to((x as f64, y as f64));
    }
    fn quad_to(&mut self, x1: f32, y1: f32, x: f32, y: f32) {
        self.0.quad_to((x1 as f64, y1 as f64), (x as f64, y as f64));
    }
    fn curve_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32) {
        self.0.curve_to(
            (x1 as f64, y1 as f64),
            (x2 as f64, y2 as f64),
            (x as f64, y as f64),
        );
    }
    fn close(&mut self) {
        self.0.close_path();
    }
}

impl Fonts {
    fn index(kind: FontKind) -> usize {
        match kind {
            FontKind::Music => 0,
            FontKind::Text | FontKind::Italic => 1,
            FontKind::Bold => 2,
        }
    }

    fn face(&self, kind: FontKind) -> &Face<'static> {
        &self.faces[Self::index(kind)]
    }

    fn glyph(&self, kind: FontKind, ch: char) -> Option<GlyphId> {
        self.face(kind).glyph_index(ch)
    }

    fn scale(&self, kind: FontKind, size: f64) -> f64 {
        size / self.face(kind).units_per_em() as f64
    }

    /// Advance width of a character at an em size.
    pub fn advance(&self, kind: FontKind, ch: char, size: f64) -> f64 {
        let face = self.face(kind);
        let adv = self
            .glyph(kind, ch)
            .and_then(|g| face.glyph_hor_advance(g))
            .unwrap_or(0);
        adv as f64 * self.scale(kind, size)
    }

    pub fn text_width(&self, kind: FontKind, text: &str, size: f64) -> f64 {
        text.chars().map(|c| self.advance(kind, c, size)).sum()
    }

    /// Bounding box of a glyph drawn at the origin (y down).
    pub fn bbox(&self, kind: FontKind, ch: char, size: f64) -> Rect {
        let s = self.scale(kind, size);
        match self
            .glyph(kind, ch)
            .and_then(|g| self.face(kind).glyph_bounding_box(g))
        {
            Some(b) => Rect::new(
                b.x_min as f64 * s,
                -b.y_max as f64 * s,
                b.x_max as f64 * s,
                -b.y_min as f64 * s,
            ),
            None => Rect::ZERO,
        }
    }

    /// Outline of a character with its baseline origin at (x, y), y down.
    pub fn glyph_path(
        &self,
        kind: FontKind,
        ch: char,
        x: f64,
        y: f64,
        size: f64,
    ) -> Option<BezPath> {
        let gid = self.glyph(kind, ch)?;
        let key = (Self::index(kind), gid.0);
        let unit = {
            let mut cache = self.cache.lock().unwrap();
            cache
                .entry(key)
                .or_insert_with(|| {
                    let mut b = Builder(BezPath::new());
                    self.face(kind).outline_glyph(gid, &mut b)?;
                    Some(b.0)
                })
                .clone()?
        };
        let s = self.scale(kind, size);
        let skew = if kind == FontKind::Italic {
            Affine::skew(-0.2, 0.0)
        } else {
            Affine::IDENTITY
        };
        Some(Affine::translate((x, y)) * skew * Affine::scale_non_uniform(s, -s) * unit)
    }

    /// Outlines of a text line positioned by its alignment point.
    pub fn text_path(
        &self,
        kind: FontKind,
        text: &str,
        x: f64,
        y: f64,
        size: f64,
        align: Align,
    ) -> BezPath {
        let width = self.text_width(kind, text, size);
        let mut pen = match align {
            Align::Left => x,
            Align::Center => x - width / 2.0,
            Align::Right => x - width,
        };
        let mut out = BezPath::new();
        for c in text.chars() {
            if let Some(p) = self.glyph_path(kind, c, pen, y, size) {
                out.extend(p);
            }
            pen += self.advance(kind, c, size);
        }
        out
    }
}
