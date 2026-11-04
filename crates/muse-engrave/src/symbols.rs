//! The music-font symbols the editing palette's icons use: the same SMuFL glyphs the engraver
//! draws for each symbol, so an icon looks like what the tool puts in the score.

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
}
