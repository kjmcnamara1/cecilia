//! Backend-neutral drawing primitives, in points (1/72 inch), y pointing down.

use kurbo::{BezPath, Point, Rect};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl Color {
    pub const BLACK: Color = Color::rgb(0, 0, 0);
    pub const WHITE: Color = Color::rgb(255, 255, 255);
    pub const GREY: Color = Color::rgb(110, 110, 110);
    pub const RED: Color = Color::rgb(200, 40, 40);

    pub const fn rgb(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b, a: 255 }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FontKind {
    /// SMuFL music font (Bravura); one em equals four staff spaces.
    Music,
    Text,
    Bold,
    Italic,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Align {
    Left,
    Center,
    Right,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Item {
    Line {
        from: Point,
        to: Point,
        width: f64,
        color: Color,
    },
    Rect {
        rect: Rect,
        color: Color,
    },
    /// A filled (or stroked when `stroke` is set) path.
    Path {
        path: BezPath,
        color: Color,
        stroke: Option<f64>,
    },
    /// One character of `font` with its baseline origin at `at`; `size` is the em size.
    Glyph {
        font: FontKind,
        ch: char,
        at: Point,
        size: f64,
        color: Color,
    },
    /// A line of text; `at` is the baseline point the alignment refers to.
    Text {
        font: FontKind,
        text: String,
        at: Point,
        size: f64,
        align: Align,
        color: Color,
    },
}

/// What a region of the layout refers to, for hit-testing and cursor display.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct BeatRef {
    pub track: usize,
    pub bar: usize,
    pub voice: usize,
    pub beat: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Hit {
    pub rect: Rect,
    pub beat: BeatRef,
    /// String (tab line) under this rect, for tablature hits.
    pub string: Option<u8>,
}

/// One page (or the single canvas of a continuous layout).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Page {
    pub width: f64,
    pub height: f64,
    pub items: Vec<Item>,
    pub hits: Vec<Hit>,
}

impl Page {
    pub fn line(&mut self, from: impl Into<Point>, to: impl Into<Point>, width: f64) {
        self.items.push(Item::Line {
            from: from.into(),
            to: to.into(),
            width,
            color: Color::BLACK,
        });
    }

    pub fn rect(&mut self, rect: Rect, color: Color) {
        self.items.push(Item::Rect { rect, color });
    }

    pub fn glyph(&mut self, ch: char, at: impl Into<Point>, size: f64) {
        self.items.push(Item::Glyph {
            font: FontKind::Music,
            ch,
            at: at.into(),
            size,
            color: Color::BLACK,
        });
    }

    pub fn text(
        &mut self,
        font: FontKind,
        text: impl Into<String>,
        at: impl Into<Point>,
        size: f64,
        align: Align,
    ) {
        self.items.push(Item::Text {
            font,
            text: text.into(),
            at: at.into(),
            size,
            align,
            color: Color::BLACK,
        });
    }
}
