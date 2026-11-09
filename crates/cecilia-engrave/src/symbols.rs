//! The music-font symbols the editing palette's icons use: the same SMuFL glyphs the engraver
//! draws for each symbol, so an icon looks like what the tool puts in the score.

use kurbo::{BezPath, Point, Rect};

use crate::display::{Align, Color, FontKind, Item, Page};

/// How a palette tool is drawn as an icon.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Icon {
    /// Glyphs of the music font side by side.
    Row(&'static [char]),
    /// Glyphs one over the other (a time signature).
    Stack(&'static [char]),
    /// A pictogram the frontend draws itself, because the engraver draws it from lines and
    /// curves rather than a glyph (ties, slides, hairpins' wedges, beams …).
    Drawn,
    /// No symbol: the tool's short text label.
    Text,
}

const NOTE: [char; 7] = [
    '\u{E1D2}', '\u{E1D3}', '\u{E1D5}', '\u{E1D7}', '\u{E1D9}', '\u{E1DB}', '\u{E1DD}',
];
const DOT: char = '\u{E1E7}';
const DYN_P: char = '\u{E520}';
const DYN_M: char = '\u{E521}';
const DYN_F: char = '\u{E522}';

/// The icon of a palette command (`duration 8`, `toggle fermata` …); every command of the
/// palette has one.
pub fn palette_icon(command: &str) -> Icon {
    let (name, arg) = command
        .trim_end()
        .split_once(' ')
        .unwrap_or((command.trim_end(), ""));
    match (name, arg) {
        ("duration", n) => match n {
            "1" => Icon::Row(&NOTE[0..1]),
            "2" => Icon::Row(&NOTE[1..2]),
            "4" => Icon::Row(&NOTE[2..3]),
            "8" => Icon::Row(&NOTE[3..4]),
            "16" => Icon::Row(&NOTE[4..5]),
            "32" => Icon::Row(&NOTE[5..6]),
            _ => Icon::Row(&NOTE[6..7]),
        },
        ("dot", _) => Icon::Row(&['\u{E1D5}', DOT]),
        ("double-dot", _) => Icon::Row(&['\u{E1D5}', DOT, DOT]),
        ("tuplet", _) => Icon::Row(&['\u{E883}']),
        ("rest", _) => Icon::Row(&['\u{E4E5}']),
        ("accidental", "sharp") => Icon::Row(&['\u{E262}']),
        ("accidental", "flat") => Icon::Row(&['\u{E260}']),
        ("accidental", "natural") => Icon::Row(&['\u{E261}']),
        // The chant palette's flat and natural are the same signs; its other tools show their
        // short labels (the chant font has no icon-sized glyph for them).
        ("chant-alter", "flat") => Icon::Row(&['\u{E260}']),
        ("chant-alter", "natural") => Icon::Row(&['\u{E261}']),
        ("accidental", "double-sharp") => Icon::Row(&['\u{E263}']),
        ("accidental", "double-flat") => Icon::Row(&['\u{E264}']),
        ("octave-up" | "octave-down", _) => Icon::Drawn,
        ("dynamic", d) => Icon::Row(match d {
            "ppp" => &[DYN_P, DYN_P, DYN_P],
            "pp" => &[DYN_P, DYN_P],
            "p" => &[DYN_P],
            "mp" => &[DYN_M, DYN_P],
            "mf" => &[DYN_M, DYN_F],
            "f" => &[DYN_F],
            "ff" => &[DYN_F, DYN_F],
            _ => &[DYN_F, DYN_F, DYN_F],
        }),
        ("toggle", "crescendo" | "decrescendo") => Icon::Drawn,
        ("toggle", "fade-in" | "fade-out" | "volume-swell") => Icon::Drawn,
        ("toggle", "tie" | "hammer" | "slide" | "slide-legato") => Icon::Drawn,
        ("toggle", "slide-in-above" | "slide-in-below") => Icon::Drawn,
        ("toggle", "slide-out-down" | "slide-out-up") => Icon::Drawn,
        ("toggle", "dead") => Icon::Row(&['\u{E0A9}']),
        ("toggle", "ghost") => Icon::Row(&['\u{E0F5}', '\u{E0A4}', '\u{E0F6}']),
        ("toggle", "accent") => Icon::Row(&['\u{E4A0}']),
        ("toggle", "heavy-accent") => Icon::Row(&['\u{E4AC}']),
        ("toggle", "staccato") => Icon::Row(&['\u{E4A2}']),
        ("toggle", "fermata") => Icon::Row(&['\u{E4C0}']),
        ("toggle", "vibrato") => Icon::Row(&['\u{EAB3}']),
        ("toggle", "vibrato-wide") => Icon::Row(&['\u{EAB3}', '\u{EAB3}']),
        ("toggle", "harmonic") => Icon::Drawn,
        ("toggle", "tremolo") => Icon::Row(&['\u{E222}']),
        ("toggle", "bend" | "half-bend") => Icon::Drawn,
        ("toggle", "grace-before") => Icon::Row(&['\u{E560}']),
        ("toggle", "grace-on") => Icon::Row(&['\u{E562}']),
        ("toggle", "pick-down") => Icon::Row(&['\u{E610}']),
        ("toggle", "pick-up") => Icon::Row(&['\u{E612}']),
        ("toggle", "stroke-down" | "stroke-up") => Icon::Drawn,
        ("toggle", "arpeggio-down" | "arpeggio-up") => Icon::Drawn,
        ("toggle", "rasgueado") => Icon::Text,
        ("toggle", "let-ring" | "palm-mute" | "artificial-harmonic") => Icon::Text,
        ("toggle", "tap" | "slap" | "pop" | "left-hand-tap") => Icon::Text,
        ("beams", _) | ("stems", _) => Icon::Drawn,
        ("clef", _) => Icon::Row(&['\u{E050}']),
        ("key", _) => Icon::Row(&['\u{E260}', '\u{E262}']),
        ("time", _) => Icon::Stack(&['\u{E084}', '\u{E084}']),
        ("free-time", _) => Icon::Text,
        ("double-bar", _) => Icon::Row(&['\u{E031}']),
        ("bar-repeat", "1") => Icon::Row(&['\u{E500}']),
        ("bar-repeat", _) => Icon::Row(&['\u{E501}']),
        ("repeat-start", _) => Icon::Row(&['\u{E040}']),
        ("repeat-end", _) => Icon::Row(&['\u{E041}']),
        ("direction", _) => Icon::Row(&['\u{E047}']),
        ("ending" | "section", _) => Icon::Text,
        _ => Icon::Text,
    }
}

/// What a chant palette tool puts in the score, drawn the way the chant engraver draws it, as
/// items in font units (y down; the gap between two staff lines is [`ICON_GAP`]), or `None` for
/// the tools that have no symbol of their own (pitch, text, the flat and natural). The items are
/// black; the frontend scales them to the icon and recolours them.
pub fn chant_icon(command: &str) -> Option<Vec<Item>> {
    use crate::neume::{HEAD, HEAD_H, Kind, N, Typeset};
    use cecilia_core::chant::{ChantClef, ClefKind, Division};

    let (name, arg) = command
        .trim_end()
        .split_once(' ')
        .unwrap_or((command.trim_end(), ""));
    let mut page = Page::default();
    let mut ts = Typeset {
        page: &mut page,
        bottom: 0.0,
        g: ICON_GAP,
    };
    // A four-line staff behind a symbol whose place on it matters.
    let staff = |ts: &mut Typeset, from: f64, to: f64| {
        for k in 0..4 {
            let y = -ICON_GAP * k as f64;
            ts.page.items.push(Item::Line {
                from: Point::new(from, y),
                to: Point::new(to, y),
                width: 14.0,
                color: STAFF,
            });
        }
    };
    // A head on a line of the staff, which is where the engraver sets one that has no pitch.
    let pos = 5;
    let head = |ts: &mut Typeset, kind: Kind, count: u8| ts.single(0.0, N { pos, kind }, count).0;
    match name {
        "chant-clef" => {
            let clef = ChantClef::parse(arg)?;
            let fa = clef.kind == ClefKind::Fa;
            staff(&mut ts, -60.0, 340.0);
            ts.clef(0.0, fa, clef.position());
            if clef.flat {
                let mut ti = clef.position();
                while clef.degree(ti) != 6 {
                    ti -= 1;
                }
                ts.accidental(300.0, ti, true);
            }
        }
        "chant-division" => {
            staff(&mut ts, -200.0, 200.0);
            // Drawn bold (the font's own strokes are too thin at icon size), at the heights
            // and with the weights the engraver gives each division; the staff is
            // [`ICON_GAP`] a line, 0 to -3 gaps.
            let g = ICON_GAP;
            let top = -3.0 * g;
            let bar = |ts: &mut Typeset, x: f64, a: f64, b: f64, w: f64| {
                ts.page.line((x, a), (x, b), w);
            };
            let w = 0.32 * g;
            match Division::of(arg) {
                Division::Virgula => {
                    let mut p = BezPath::new();
                    p.move_to((0.0, top - 0.9 * g));
                    p.quad_to((0.25 * g, top - 0.5 * g), (0.05 * g, top - 0.1 * g));
                    ts.page.items.push(Item::Path {
                        path: p,
                        color: Color::BLACK,
                        stroke: Some(w),
                    });
                }
                Division::Minima => bar(&mut ts, 0.0, top - 0.5 * g, top + 0.5 * g, w),
                Division::Minor => bar(&mut ts, 0.0, -0.7 * g, -2.3 * g, w),
                Division::Maior => bar(&mut ts, 0.0, 0.0, top, w),
                Division::Finalis => {
                    bar(&mut ts, -0.3 * g, 0.0, top, w);
                    bar(&mut ts, 0.3 * g, 0.0, top, 2.0 * w);
                }
            }
            // The variants of the minor division are told apart by their number.
            if let Some(n) = arg.strip_prefix(';').filter(|n| !n.is_empty()) {
                ts.page.items.push(Item::Text {
                    font: FontKind::Text,
                    text: n.into(),
                    at: Point::new(130.0, 420.0),
                    size: 260.0,
                    align: Align::Left,
                    color: Color::BLACK,
                });
            }
        }
        "chant-shape" => match arg {
            "punctum" | "quadratum" => {
                head(&mut ts, Kind::Punctum, 1);
            }
            "inclinatum" => {
                head(&mut ts, Kind::Inclinatum, 1);
            }
            "virga" | "virga-reversa" => {
                head(&mut ts, Kind::Virga, 1);
            }
            "bivirga" => {
                head(&mut ts, Kind::Virga, 2);
            }
            "trivirga" => {
                head(&mut ts, Kind::Virga, 3);
            }
            "stropha" => {
                head(&mut ts, Kind::Stropha, 1);
            }
            "bistropha" => {
                head(&mut ts, Kind::Stropha, 2);
            }
            "tristropha" => {
                head(&mut ts, Kind::Stropha, 3);
            }
            "quilisma" => {
                head(&mut ts, Kind::Quilisma, 1);
            }
            "oriscus" | "oriscus-stem" => {
                head(&mut ts, Kind::Oriscus(true), 1);
            }
            _ => return None,
        },
        "chant-liquescence" => match arg {
            // A clivis whose last note is the small one.
            "deminutus" => {
                ts.neume(
                    0.0,
                    &[
                        N {
                            pos: pos + 1,
                            kind: Kind::Punctum,
                        },
                        N {
                            pos,
                            kind: Kind::Small,
                        },
                    ],
                );
            }
            "ascendens" | "descendens" => {
                head(&mut ts, Kind::Punctum, 1);
                let x = HEAD / 2.0 - 0.03 * ICON_GAP;
                let y = ts.y(crate::neume::digit(pos));
                let dy = if arg == "ascendens" { -0.8 } else { 0.8 } * ICON_GAP;
                ts.page.line((x, y), (x, y + dy), 0.07 * ICON_GAP);
            }
            _ => return None,
        },
        // An initio debilis: a small note before the next one.
        "chant-mod" if arg == "-" => {
            ts.neume(
                0.0,
                &[
                    N {
                        pos,
                        kind: Kind::Small,
                    },
                    N {
                        pos: pos + 1,
                        kind: Kind::Punctum,
                    },
                ],
            );
        }
        // Letters with a text caret after them: what lyric mode does.
        "lyric-mode" => {
            let fonts = crate::fonts::fonts();
            let size = 760.0;
            let mut x = 0.0;
            for ch in ['L', 'y'] {
                ts.page.items.push(Item::Glyph {
                    font: FontKind::Bold,
                    ch,
                    at: Point::new(x, 0.0),
                    size,
                    color: Color::BLACK,
                });
                x += fonts.advance(FontKind::Bold, ch, size);
            }
            ts.page.line((x + 70.0, -560.0), (x + 70.0, 170.0), 60.0);
        }
        // The signs of the text, as the engraver draws them: the barred A in the text font, the
        // versicle and response in the chant font's glyphs.
        "chant-symbol" if matches!(arg, "Ⱥ" | "℣" | "℟") => {
            let ch = arg.chars().next()?;
            let (font, size) = if arg == "Ⱥ" {
                (FontKind::Text, 760.0)
            } else {
                (FontKind::Chant, 760.0 * 0.9)
            };
            ts.page.items.push(Item::Glyph {
                font,
                ch,
                at: Point::new(0.0, 0.0),
                size,
                color: Color::BLACK,
            });
        }
        // The style of the text each markup tool gives, on a letter.
        "chant-markup" => {
            let fonts = crate::fonts::fonts();
            let size = 760.0;
            let glyph = |ts: &mut Typeset, font: FontKind, ch: char, x: f64, size: f64| {
                ts.page.items.push(Item::Glyph {
                    font,
                    ch,
                    at: Point::new(x, 0.0),
                    size,
                    color: Color::BLACK,
                });
                x + fonts.advance(font, ch, size)
            };
            match arg {
                "i" => {
                    glyph(&mut ts, FontKind::Italic, 'I', 0.0, size);
                    glyph(&mut ts, FontKind::Italic, 'i', 330.0, size);
                }
                "b" => {
                    let x = glyph(&mut ts, FontKind::Bold, 'B', 0.0, size);
                    glyph(&mut ts, FontKind::Bold, 'b', x, size);
                }
                "ul" => {
                    let x = glyph(&mut ts, FontKind::Text, 'U', 0.0, size);
                    ts.page.line((0.0, 120.0), (x, 120.0), 55.0);
                }
                "sc" => {
                    let x = glyph(&mut ts, FontKind::Text, 'S', 0.0, size);
                    glyph(&mut ts, FontKind::Text, 'C', x, size * 0.78);
                }
                "c" => {
                    let x = glyph(&mut ts, FontKind::Text, 'A', 0.0, size);
                    ts.page.items.push(Item::Line {
                        from: Point::new(0.0, 190.0),
                        to: Point::new(x, 190.0),
                        width: 110.0,
                        color: Color::RED,
                    });
                }
                "tt" => {
                    glyph(&mut ts, FontKind::Text, 't', 0.0, size);
                    glyph(&mut ts, FontKind::Text, 't', 480.0, size);
                }
                "e" => {
                    // A small letter under a tie.
                    let x = glyph(&mut ts, FontKind::Text, 'e', 0.0, size * 0.8);
                    let mut p = BezPath::new();
                    p.move_to((0.0, 80.0));
                    p.quad_to((x / 2.0, 330.0), (x, 80.0));
                    ts.page.items.push(Item::Path {
                        path: p,
                        color: Color::BLACK,
                        stroke: Some(50.0),
                    });
                }
                "alt" => {
                    staff(&mut ts, -60.0, 600.0);
                    glyph(&mut ts, FontKind::Text, 'A', 140.0, size * 0.8);
                    // The letter is above the staff.
                    let n = ts.page.items.len();
                    if let Item::Glyph { at, .. } = &mut ts.page.items[n - 1] {
                        at.y = -ICON_GAP * 3.0 - 150.0;
                    }
                }
                _ => return None,
            }
        }
        "chant-mora" => {
            head(&mut ts, Kind::Punctum, 1);
            ts.mora(HEAD, pos);
        }
        "chant-episema" => {
            head(&mut ts, Kind::Punctum, 1);
            let y = ts.y(crate::neume::digit(pos)) - HEAD_H / 2.0 - 0.3 * ICON_GAP;
            ts.page.line((0.0, y), (HEAD, y), 0.12 * ICON_GAP);
        }
        "chant-ictus" => {
            head(&mut ts, Kind::Punctum, 1);
            ts.ictus(HEAD / 2.0, crate::neume::digit(pos), &[]);
        }
        _ => return None,
    }
    Some(page.items)
}

/// The faint colour of the staff lines behind a symbol in [`chant_icon`]'s items (the frontend
/// takes only its opacity).
pub const STAFF: Color = Color {
    r: 0,
    g: 0,
    b: 0,
    a: 55,
};

/// The gap between two staff lines in [`chant_icon`]'s items.
pub const ICON_GAP: f64 = 250.0;

/// The box the items cover (None when they cover nothing).
pub fn icon_bounds(items: &[Item]) -> Option<Rect> {
    use kurbo::Shape as _;
    items
        .iter()
        .filter_map(|item| match item {
            Item::Glyph {
                font, ch, at, size, ..
            } => {
                let b = crate::fonts::fonts().bbox(*font, *ch, *size);
                (b.width() > 0.0).then(|| b + at.to_vec2())
            }
            Item::Line {
                from, to, width, ..
            } => Some(Rect::from_points(*from, *to).inflate(width / 2.0, width / 2.0)),
            Item::Path { path, stroke, .. } => Some(
                path.bounding_box()
                    .inflate(stroke.unwrap_or(0.0) / 2.0, stroke.unwrap_or(0.0) / 2.0),
            ),
            Item::Rect { rect, .. } => Some(*rect),
            Item::Text { .. } => None,
        })
        .reduce(|a, b| a.union(b))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::display::FontKind;
    use crate::fonts::fonts;

    #[test]
    fn every_icon_glyph_is_in_the_music_font() {
        let commands = [
            "duration 1",
            "duration 2",
            "duration 4",
            "duration 8",
            "duration 16",
            "duration 32",
            "duration 64",
            "dot",
            "double-dot",
            "tuplet 3",
            "rest",
            "accidental sharp",
            "accidental flat",
            "accidental natural",
            "accidental double-sharp",
            "accidental double-flat",
            "dynamic ppp",
            "dynamic mf",
            "dynamic fff",
            "toggle dead",
            "toggle ghost",
            "toggle accent",
            "toggle heavy-accent",
            "toggle staccato",
            "toggle fermata",
            "toggle vibrato",
            "toggle vibrato-wide",
            "toggle tremolo",
            "toggle grace-before",
            "toggle grace-on",
            "toggle pick-down",
            "toggle pick-up",
            "clef ",
            "key ",
            "time ",
            "double-bar",
            "bar-repeat 1",
            "bar-repeat 2",
            "repeat-start",
            "repeat-end",
            "direction ",
        ];
        for c in commands {
            match palette_icon(c) {
                Icon::Row(g) | Icon::Stack(g) => {
                    for ch in g {
                        assert!(fonts().has(FontKind::Music, *ch), "{c}: U+{:X}", *ch as u32);
                    }
                }
                other => panic!("{c} should be a glyph, is {other:?}"),
            }
        }
    }

    #[test]
    fn the_chant_palettes_symbols_are_drawn_as_the_engraver_draws_them() {
        for c in [
            "chant-clef c1",
            "chant-clef c4",
            "chant-clef cb3",
            "chant-clef f3",
            "chant-division ,",
            "chant-division ;",
            "chant-division :",
            "chant-division ::",
            "chant-division ;3",
            "chant-shape punctum",
            "chant-shape inclinatum",
            "chant-shape virga",
            "chant-shape bivirga",
            "chant-shape trivirga",
            "chant-shape stropha",
            "chant-shape bistropha",
            "chant-shape tristropha",
            "chant-shape quilisma",
            "chant-shape oriscus",
            "chant-shape quadratum",
            "chant-liquescence deminutus",
            "chant-liquescence ascendens",
            "chant-liquescence descendens",
            "chant-mod -",
            "chant-symbol Ⱥ",
            "chant-symbol ℣",
            "chant-symbol ℟",
            "chant-mora",
            "chant-episema",
            "chant-ictus",
        ] {
            let items = chant_icon(c).unwrap_or_else(|| panic!("{c} has no icon"));
            let b = icon_bounds(&items).unwrap_or_else(|| panic!("{c} draws nothing"));
            assert!(b.width() > 0.0 && b.height() > 0.0, "{c}");
        }
        // The text and pitch tools, flat and natural keep their own icons.
        for c in ["pitch-up", "chant-symbol *", "chant-alter flat"] {
            assert!(chant_icon(c).is_none(), "{c}");
        }
    }
}
