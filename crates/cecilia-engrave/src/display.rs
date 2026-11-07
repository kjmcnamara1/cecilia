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
    /// Caeciliae, the chant font: glyphs are picked by code, one em is four staff-line gaps.
    Chant,
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
    /// Which items belong to which voice of a bar with several (for dimming the inactive ones).
    pub voice_spans: Vec<VoiceSpan>,
}

/// The items `start..end` of a page draw one voice of a bar that has several.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VoiceSpan {
    pub voice: usize,
    pub start: usize,
    pub end: usize,
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

    /// This page with every voice but `active` faded (`alpha` out of 255 of its ink), in the
    /// bars that have several voices. `None` when there are none to fade.
    pub fn with_voices_dimmed(&self, active: usize, alpha: u8) -> Option<Page> {
        if self.voice_spans.iter().all(|s| s.voice == active) {
            return None;
        }
        let mut page = self.clone();
        for s in self.voice_spans.iter().filter(|s| s.voice != active) {
            for item in &mut page.items[s.start..s.end] {
                let (Item::Line { color, .. }
                | Item::Rect { color, .. }
                | Item::Path { color, .. }
                | Item::Glyph { color, .. }
                | Item::Text { color, .. }) = item;
                color.a = (color.a as u16 * alpha as u16 / 255) as u8;
            }
        }
        Some(page)
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

    /// A line of text in which every `*` is red (the asterisk of chant and of lyrics).
    pub fn text_red_stars(
        &mut self,
        font: FontKind,
        text: &str,
        at: Point,
        size: f64,
        align: Align,
    ) {
        if !text.contains('*') {
            return self.text(font, text, at, size, align);
        }
        let fonts = crate::fonts::fonts();
        let total = fonts.text_width(font, text, size);
        let mut x = match align {
            Align::Left => at.x,
            Align::Center => at.x - total / 2.0,
            Align::Right => at.x - total,
        };
        let mut piece = String::new();
        let flush = |page: &mut Page, piece: &mut String, x: &mut f64, red: bool| {
            if piece.is_empty() {
                return;
            }
            page.items.push(Item::Text {
                font,
                text: piece.clone(),
                at: Point::new(*x, at.y),
                size,
                align: Align::Left,
                color: if red { Color::RED } else { Color::BLACK },
            });
            *x += fonts.text_width(font, piece, size);
            piece.clear();
        };
        for c in text.chars() {
            if c == '*' {
                flush(self, &mut piece, &mut x, false);
                // The six-pointed asterisk, as the lyrics' ✻: three spokes through a centre a
                // little above the middle of the letters, in the advance of a `*`.
                let advance = fonts.text_width(font, "*", size);
                let (cx, cy) = (x + advance / 2.0, at.y - 0.447 * size);
                for a in [90.0f64, 30.0, 150.0] {
                    let (dx, dy) = (
                        a.to_radians().cos() * 0.263 * size,
                        a.to_radians().sin() * 0.263 * size,
                    );
                    self.items.push(Item::Line {
                        from: Point::new(cx - dx, cy - dy),
                        to: Point::new(cx + dx, cy + dy),
                        width: 0.105 * size,
                        color: Color::RED,
                    });
                }
                x += advance;
            } else {
                piece.push(c);
            }
        }
        flush(self, &mut piece, &mut x, false);
    }
}

#[cfg(test)]
mod star_tests {
    use super::*;

    #[test]
    fn asterisks_are_drawn_red_and_the_rest_black() {
        let mut page = Page::default();
        page.text_red_stars(
            FontKind::Text,
            "a * b",
            Point::new(0.0, 0.0),
            10.0,
            Align::Left,
        );
        // The star is three red spokes, not a glyph; the text around it stays black.
        let spokes = page
            .items
            .iter()
            .filter(|i| matches!(i, Item::Line { color, .. } if *color == Color::RED))
            .count();
        assert_eq!(spokes, 3);
        assert!(page.items.iter().all(|i| match i {
            Item::Text { text, .. } => !text.contains('*'),
            _ => true,
        }));
    }
}
