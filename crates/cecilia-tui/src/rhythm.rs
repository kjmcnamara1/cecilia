//! The rhythm of a tab+ staff in terminal characters: one glyph per beat, in a row under the
//! strings and aligned with the beat's fret column, chosen only from the beat's value, dots and
//! whether it is a rest.

use cecilia_core::{Beat, DurationValue, Staff, Track};

/// Rows below the strings for the track's rhythm: one with a tab+ staff, else none.
pub fn rows(track: &Track) -> u16 {
    track.staves().contains(&Staff::TabRhythm) as u16
}

/// The glyph of a beat's rhythm in the `tabline.icons` style: `nerd` (Material Design music
/// notes and rests), `unicode` (note symbols; rests are letters) or `none` (letters: `w h q e s`
/// for notes, `W H Q E S` for rests, a `.` per dot). Values shorter than a sixteenth use the
/// sixteenth's glyph. An empty beat has none.
pub fn glyph(beat: &Beat, style: &str) -> String {
    if beat.empty {
        return String::new();
    }
    let value = beat.duration.value;
    let dots = beat.duration.dots as usize;
    let rest = beat.is_rest();
    let v = value
        .min(DurationValue::Sixteenth)
        .max(DurationValue::Whole);
    // Index into the tables below: whole, half, quarter, eighth, sixteenth.
    let i = v as usize;
    let letter = ["w", "h", "q", "e", "s"][i];
    match style {
        "nerd" => {
            // A rest has no dotted glyph: its dots follow it.
            let (plain, dotted) = if rest {
                (REST[i], None)
            } else {
                (NOTE[i], Some(NOTE_DOTTED[i]))
            };
            match dotted.filter(|_| dots > 0) {
                Some(d) => format!("{d}{}", "·".repeat(dots - 1)),
                None => format!("{plain}{}", "·".repeat(dots)),
            }
        }
        "unicode" if !rest => format!("{}{}", ["○", "◐", "♩", "♪", "♬"][i], "·".repeat(dots)),
        _ => {
            let l = if rest {
                letter.to_uppercase()
            } else {
                letter.into()
            };
            format!("{l}{}", ".".repeat(dots))
        }
    }
}

/// Notes (whole … sixteenth).
const NOTE: [char; 5] = [
    '\u{f038d}', // nf-md-music_note_whole
    '\u{f0389}', // nf-md-music_note_half
    '\u{f038b}', // nf-md-music_note_quarter
    '\u{f0387}', // nf-md-music_note (an eighth note: Material has no plain `_eighth`)
    '\u{f038c}', // nf-md-music_note_sixteenth
];

const NOTE_DOTTED: [char; 5] = [
    '\u{f0f77}', // nf-md-music_note_whole_dotted
    '\u{f0f72}', // nf-md-music_note_half_dotted
    '\u{f0f75}', // nf-md-music_note_quarter_dotted
    '\u{f0f71}', // nf-md-music_note_eighth_dotted
    '\u{f0f76}', // nf-md-music_note_sixteenth_dotted
];

const REST: [char; 5] = [
    '\u{f0f7c}', // nf-md-music_rest_whole
    '\u{f0f79}', // nf-md-music_rest_half
    '\u{f0f7a}', // nf-md-music_rest_quarter
    '\u{f0f78}', // nf-md-music_rest_eighth
    '\u{f0f7b}', // nf-md-music_rest_sixteenth
];

#[cfg(test)]
mod tests {
    use super::*;
    use cecilia_core::{Duration, Note};

    fn beat(value: DurationValue, dots: u8, rest: bool) -> Beat {
        let mut duration = Duration::new(value);
        duration.dots = dots;
        Beat {
            duration,
            notes: if rest { vec![] } else { vec![Note::new(0, 3)] },
            ..Beat::default()
        }
    }

    #[test]
    fn rows_need_a_tab_rhythm_staff() {
        let mut t = Track {
            strings: vec![64; 6],
            ..Track::default()
        };
        assert_eq!(rows(&t), 0);
        t.staves = vec![Staff::Tab, Staff::TabRhythm];
        assert_eq!(rows(&t), 1);
    }

    #[test]
    fn nerd_glyphs_by_value() {
        let g = |v, d, r| glyph(&beat(v, d, r), "nerd");
        assert_eq!(g(DurationValue::Whole, 0, false), "\u{f038d}");
        assert_eq!(g(DurationValue::Quarter, 0, false), "\u{f038b}");
        assert_eq!(g(DurationValue::Eighth, 0, false), "\u{f0387}");
        assert_eq!(g(DurationValue::Half, 1, false), "\u{f0f72}");
        assert_eq!(g(DurationValue::Quarter, 2, false), "\u{f0f75}·");
        assert_eq!(g(DurationValue::Quarter, 0, true), "\u{f0f7a}");
        assert_eq!(g(DurationValue::Half, 1, true), "\u{f0f79}·");
        // Shorter than a sixteenth: the sixteenth's glyph.
        assert_eq!(g(DurationValue::SixtyFourth, 0, false), "\u{f038c}");
        assert_eq!(g(DurationValue::ThirtySecond, 0, true), "\u{f0f7b}");
    }

    #[test]
    fn fallbacks_without_nerd_fonts() {
        let g = |s, v, d, r| glyph(&beat(v, d, r), s);
        assert_eq!(g("unicode", DurationValue::Eighth, 0, false), "♪");
        assert_eq!(g("unicode", DurationValue::Quarter, 1, false), "♩·");
        assert_eq!(g("unicode", DurationValue::Quarter, 0, true), "Q");
        assert_eq!(g("none", DurationValue::Half, 1, false), "h.");
        assert_eq!(g("none", DurationValue::Sixteenth, 0, true), "S");
        let empty = Beat {
            empty: true,
            ..Beat::default()
        };
        assert_eq!(glyph(&empty, "nerd"), "");
    }
}
