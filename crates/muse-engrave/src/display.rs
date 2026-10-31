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
    /// Which of the track's staves the rect belongs to.
    pub staff: usize,
}

/// One page (or the single canvas of a continuous layout).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Page {
    pub width: f64,
    pub height: f64,
    pub items: Vec<Item>,
    pub hits: Vec<Hit>,
    /// The items that draw a note's body or tab number, to recolour one note on every staff.
    pub marks: Vec<NoteMark>,
}

/// What a marked item of a page is for a note.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MarkKind {
    /// The body of a note on a staff of lines: a notehead, slash or square note.
    Body,
    /// A fret number on a tab staff.
    Number,
}

/// An item of a page that draws one note (`string` of a beat) on one staff of its track.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NoteMark {
    /// Index into `Page::items`.
    pub item: usize,
    pub kind: MarkKind,
    pub beat: BeatRef,
    pub string: u8,
    pub staff: usize,
}

impl Page {
    /// Marks the last item as drawing the note on `string` of `beat` on `staff`.
    pub(crate) fn mark_last(&mut self, kind: MarkKind, beat: BeatRef, string: u8, staff: usize) {
        if let Some(item) = self.items.len().checked_sub(1) {
            self.marks.push(NoteMark {
                item,
                kind,
                beat,
                string,
                staff,
            });
        }
    }

    /// This page with the note on `string` of `beat` drawn in `color` on every staff but
    /// `except`: its body on staves of lines, its number (bold) on tabs. `None` when the page
    /// shows the note nowhere else.
    pub fn with_note_color(
        &self,
        beat: BeatRef,
        string: u8,
        except: usize,
        color: Color,
    ) -> Option<Page> {
        let mut page = None::<Page>;
        for m in self
            .marks
            .iter()
            .filter(|m| m.beat == beat && m.string == string && m.staff != except)
        {
            let page = page.get_or_insert_with(|| self.clone());
            match (&mut page.items[m.item], m.kind) {
                (Item::Glyph { color: c, .. } | Item::Rect { color: c, .. }, MarkKind::Body)
                | (Item::Path { color: c, .. }, MarkKind::Body) => *c = color,
                (Item::Text { color: c, font, .. }, MarkKind::Number) => {
                    *c = color;
                    *font = FontKind::Bold;
                }
                _ => {}
            }
        }
        page
    }
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
