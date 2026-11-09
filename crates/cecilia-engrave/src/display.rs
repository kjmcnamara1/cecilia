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
    /// Bold, slanted.
    BoldItalic,
    /// Noto Serif, and its bold; slanted for italic.
    Serif,
    SerifBold,
    SerifItalic,
    SerifBoldItalic,
}

/// The family of a chant score's lyrics (`ui.chant_text_font`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TextFamily {
    /// Noto Serif.
    #[default]
    Serif,
    /// Source Sans 3, the app's text font.
    Sans,
}

impl TextFamily {
    /// The family a config name stands for (`noto-serif`, `source-sans`).
    pub fn parse(name: &str) -> Option<Self> {
        match name.trim().to_ascii_lowercase().as_str() {
            "noto-serif" | "serif" => Some(Self::Serif),
            "source-sans" | "sans" => Some(Self::Sans),
            _ => None,
        }
    }

    /// The face of this family for a weight and slant.
    pub fn kind(self, bold: bool, italic: bool) -> FontKind {
        match (self, bold, italic) {
            (Self::Serif, false, false) => FontKind::Serif,
            (Self::Serif, true, false) => FontKind::SerifBold,
            (Self::Serif, false, true) => FontKind::SerifItalic,
            (Self::Serif, true, true) => FontKind::SerifBoldItalic,
            (Self::Sans, false, false) => FontKind::Text,
            (Self::Sans, true, false) => FontKind::Bold,
            (Self::Sans, false, true) => FontKind::Italic,
            (Self::Sans, true, true) => FontKind::BoldItalic,
        }
    }
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

    /// A line of chant text in runs of style (see [`cecilia_core::chant::text_runs`]): italic
    /// (slanted), bold, underlined, small capitals, red, monospaced, small (elision), in the
    /// font `family`. The versicle ℣, response ℟, dagger † and barred Ⱥ are always the serif
    /// font's (the sans font lacks some of them). The asterisk is drawn as a six-pointed star.
    /// Only `<c>` colors anything.
    pub fn rich_text(
        &mut self,
        runs: &[cecilia_core::chant::TextRun],
        at: Point,
        size: f64,
        align: Align,
        family: TextFamily,
    ) {
        let fonts = crate::fonts::fonts();
        let total: f64 = runs.iter().map(|r| run_width(r, size, family)).sum();
        let mut x = match align {
            Align::Left => at.x,
            Align::Center => at.x - total / 2.0,
            Align::Right => at.x - total,
        };
        for run in runs {
            let rs = run_size(run, size);
            let width = run_width(run, size, family);
            let color = if run.colored {
                Color::RED
            } else {
                Color::BLACK
            };
            let mut piece = String::new();
            let mut piece_size = rs;
            let mut piece_font = char_font(run, family, ' ');
            let mut px = x;
            let flush =
                |page: &mut Page, piece: &mut String, px: &mut f64, ps: f64, pf: FontKind| {
                    if piece.is_empty() {
                        return;
                    }
                    page.items.push(Item::Text {
                        font: pf,
                        text: piece.clone(),
                        at: Point::new(*px, at.y),
                        size: ps,
                        align: Align::Left,
                        color,
                    });
                    *px += fonts.text_width(pf, piece, ps);
                    piece.clear();
                };
            // The advance and font of the last character drawn, for a combining mark to centre
            // on.
            let (mut last_adv, mut last_font) = (0.0, piece_font);
            for c in run.text.chars() {
                let (ch, cs) = char_style(run, c, rs);
                let font = char_font(run, family, c);
                if ('\u{300}'..='\u{36F}').contains(&c) {
                    // A combining mark goes over the letter before it.
                    flush(self, &mut piece, &mut px, piece_size, piece_font);
                    let b = fonts.bbox(last_font, c, cs);
                    self.items.push(Item::Text {
                        font: last_font,
                        text: c.to_string(),
                        at: Point::new(px - last_adv / 2.0 - (b.x0 + b.x1) / 2.0, at.y),
                        size: cs,
                        align: Align::Left,
                        color,
                    });
                    continue;
                }
                let star = matches!(c, '*' | '✱');
                if star || cs != piece_size || font != piece_font || run.mono {
                    flush(self, &mut piece, &mut px, piece_size, piece_font);
                    piece_size = cs;
                    piece_font = font;
                }
                let advance = char_advance(run, font, c, cs);
                (last_adv, last_font) = (advance, font);
                if run.mono && !star {
                    self.items.push(Item::Text {
                        font,
                        text: ch.to_string(),
                        at: Point::new(
                            px + (advance - fonts.text_width(font, &ch.to_string(), cs)) / 2.0,
                            at.y,
                        ),
                        size: cs,
                        align: Align::Left,
                        color,
                    });
                    px += advance;
                    continue;
                }
                if !star {
                    piece.push(ch);
                    continue;
                }
                // The six-pointed asterisk, as the lyrics' ✻: three spokes.
                let (cx, cy) = (px + advance / 2.0, at.y - 0.447 * cs);
                for a in [90.0f64, 30.0, 150.0] {
                    let (dx, dy) = (
                        a.to_radians().cos() * 0.263 * cs,
                        a.to_radians().sin() * 0.263 * cs,
                    );
                    self.items.push(Item::Line {
                        from: Point::new(cx - dx, cy - dy),
                        to: Point::new(cx + dx, cy + dy),
                        width: 0.105 * cs,
                        color,
                    });
                }
                px += advance;
            }
            flush(self, &mut piece, &mut px, piece_size, piece_font);
            if run.underline {
                self.items.push(Item::Line {
                    from: Point::new(x, at.y + 0.14 * size),
                    to: Point::new(x + width, at.y + 0.14 * size),
                    width: 0.06 * size,
                    color,
                });
            }
            x += width;
        }
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

/// The font of a character of a run: the family's face for its weight and slant, but the
/// serif font for the signs the sans font lacks.
fn char_font(run: &cecilia_core::chant::TextRun, family: TextFamily, c: char) -> FontKind {
    let family = if matches!(c, '℟' | '℣' | '†' | 'Ⱥ') {
        TextFamily::Serif
    } else {
        family
    };
    family.kind(run.bold, run.italic)
}

/// The em size of a run: an elision is set smaller.
fn run_size(run: &cecilia_core::chant::TextRun, size: f64) -> f64 {
    if run.elision { size * 0.78 } else { size }
}

/// A character as a run shows it, and the size it is drawn at (small capitals draw lower case
/// as smaller capitals).
fn char_style(run: &cecilia_core::chant::TextRun, c: char, size: f64) -> (char, f64) {
    if run.small_caps && c.is_lowercase() {
        (c.to_uppercase().next().unwrap_or(c), size * 0.8)
    } else {
        (c, size)
    }
}

/// The advance of a character in a run (the asterisk as the star's width).
fn char_advance(run: &cecilia_core::chant::TextRun, font: FontKind, c: char, size: f64) -> f64 {
    if run.mono {
        return 0.6 * size;
    }
    let shown = if c == '✱' { '*' } else { c };
    crate::fonts::fonts().text_width(font, &shown.to_string(), size)
}

fn run_width(run: &cecilia_core::chant::TextRun, size: f64, family: TextFamily) -> f64 {
    let rs = run_size(run, size);
    run.text
        .chars()
        .map(|c| {
            let (_, cs) = char_style(run, c, rs);
            char_advance(run, char_font(run, family, c), c, cs)
        })
        .sum()
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

    #[test]
    fn the_asterisk_is_black_unless_colored_and_a_combining_mark_sits_on_its_letter() {
        use cecilia_core::chant::text_runs;
        let spokes = |text: &str, color: Color| {
            let (runs, _) = text_runs(text);
            let mut page = Page::default();
            page.rich_text(
                &runs,
                Point::new(0.0, 0.0),
                10.0,
                Align::Left,
                TextFamily::Sans,
            );
            page.items
                .iter()
                .filter(|i| matches!(i, Item::Line { color: c, .. } if *c == color))
                .count()
        };
        assert_eq!(spokes("*", Color::BLACK), 3);
        assert_eq!(spokes("<c>*</c>", Color::RED), 3);
        // œ́: the acute is drawn over the œ, not after it.
        let (runs, _) = text_runs("<sp>'oe</sp>");
        let mut page = Page::default();
        page.rich_text(
            &runs,
            Point::new(0.0, 0.0),
            10.0,
            Align::Left,
            TextFamily::Sans,
        );
        let xs: Vec<f64> = page
            .items
            .iter()
            .filter_map(|i| match i {
                Item::Text { at, .. } => Some(at.x),
                _ => None,
            })
            .collect();
        let w = crate::fonts::fonts().text_width(FontKind::Text, "œ", 10.0);
        assert_eq!(xs.len(), 2);
        assert!(xs[1] < xs[0] + w, "{xs:?} {w}");
    }

    #[test]
    fn rich_text_styles_runs_and_draws_the_signs_the_font_lacks() {
        use cecilia_core::chant::text_runs;
        let (runs, _) = text_runs("<b>B</b><ul>u</ul><c>c</c>†℟℣");
        let mut page = Page::default();
        page.rich_text(
            &runs,
            Point::new(0.0, 0.0),
            10.0,
            Align::Left,
            TextFamily::Sans,
        );
        let fonts_used = |k: FontKind| {
            page.items
                .iter()
                .any(|i| matches!(i, Item::Text { font, .. } if *font == k))
        };
        assert!(fonts_used(FontKind::Bold));
        // Bold and italic together are both: the bold face, slanted.
        let (both, _) = text_runs("<b><i>x</i></b>");
        let mut page2 = Page::default();
        page2.rich_text(
            &both,
            Point::new(0.0, 0.0),
            10.0,
            Align::Left,
            TextFamily::Sans,
        );
        assert!(page2.items.iter().any(|i| matches!(
            i,
            Item::Text {
                font: FontKind::BoldItalic,
                ..
            }
        )));
        // Nothing is colored but what `<c>` colors: no red line, the colored run is red text.
        assert!(
            !page
                .items
                .iter()
                .any(|i| matches!(i, Item::Line { color, .. } if *color == Color::RED))
        );
        assert!(page.items.iter().any(
            |i| matches!(i, Item::Text { text, color, .. } if text == "c" && *color == Color::RED)
        ));
        // The underline; and ℟ ℣ † are Noto Serif's, even with the sans family.
        let black_lines = page
            .items
            .iter()
            .filter(|i| matches!(i, Item::Line { color, .. } if *color == Color::BLACK))
            .count();
        assert_eq!(black_lines, 1);
        assert!(page.items.iter().any(|i| matches!(
            i,
            Item::Text { font: FontKind::Serif, text, .. } if text.contains('℟') && text.contains('℣')
        )));
        // The serif family draws everything in it, the bold as its bold.
        let (runs, _) = text_runs("a<b>b</b>");
        let mut serif = Page::default();
        serif.rich_text(
            &runs,
            Point::new(0.0, 0.0),
            10.0,
            Align::Left,
            TextFamily::Serif,
        );
        let used: Vec<FontKind> = serif
            .items
            .iter()
            .filter_map(|i| match i {
                Item::Text { font, .. } => Some(*font),
                _ => None,
            })
            .collect();
        assert_eq!(used, [FontKind::Serif, FontKind::SerifBold]);
    }
}
