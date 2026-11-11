//! The rhythm row of a tab in terminal characters: one glyph per beat, in a row under the
//! strings and aligned with the beat's fret column, chosen only from the beat's value, dots and
//! whether it is a rest, and below it a row of beams joining eighths and shorter notes.

use cecilia_core::{Beat, DurationValue, TICKS_PER_QUARTER, TabRhythm, Track};

/// Rows below the strings for the track's rhythm: the glyph row and the beam row when the
/// `ui.tab_rhythm` option shows it for the track, else none.
pub fn rows(track: &Track, mode: TabRhythm) -> u16 {
    if mode.shows(track) { 2 } else { 0 }
}

/// The beam row of one bar, `width` characters: under runs of eighths and shorter notes in one
/// quarter-note group, a line from the first to the last glyph column (`─`, or `═` where both
/// neighbours are sixteenths or shorter; `-` and `=` unless `style` is `nerd`). `cols[i]` is the
/// column of beat `i`'s glyph. Rests, longer notes and a lone note break a run.
pub fn beams(beats: &[Beat], cols: &[u16], width: u16, style: &str) -> String {
    let (single, double) = if style == "nerd" {
        ('─', '═')
    } else {
        ('-', '=')
    };
    let mut row = vec![' '; width as usize];
    let mut tick = 0;
    // The open run: its quarter-note group and its (column, shorter than an eighth) beats.
    let mut run: (u32, Vec<(u16, bool)>) = (u32::MAX, vec![]);
    let mut runs = vec![];
    for (beat, &col) in beats.iter().zip(cols) {
        let level = beat.duration.value;
        let beamed = !beat.empty && !beat.is_rest() && level >= DurationValue::Eighth;
        let group = tick / TICKS_PER_QUARTER;
        if !beamed || run.0 != group {
            runs.push(std::mem::replace(&mut run, (u32::MAX, vec![])).1);
        }
        if beamed {
            run.0 = group;
            run.1.push((col, level >= DurationValue::Sixteenth));
        }
        tick += beat.ticks();
    }
    runs.push(run.1);
    for r in runs.iter().filter(|r| r.len() > 1) {
        for pair in r.windows(2) {
            let ((a, da), (b, db)) = (pair[0], pair[1]);
            for x in a..b {
                if let Some(c) = row.get_mut(x as usize) {
                    *c = if da && db { double } else { single };
                }
            }
        }
        if let Some(&(x, d)) = r.last()
            && let Some(c) = row.get_mut(x as usize)
        {
            *c = if d { double } else { single };
        }
    }
    row.into_iter().collect()
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
    fn rows_follow_the_tab_rhythm_option() {
        use cecilia_core::Staff;
        let mut t = Track {
            strings: vec![64; 6],
            staves: vec![Staff::Tab],
            ..Track::default()
        };
        assert_eq!(rows(&t, TabRhythm::Auto), 2);
        t.staves = vec![Staff::Standard(cecilia_core::Clef::Treble), Staff::Tab];
        assert_eq!(rows(&t, TabRhythm::Auto), 0);
        assert_eq!(rows(&t, TabRhythm::On), 2);
        t.staves = vec![Staff::Tab];
        assert_eq!(rows(&t, TabRhythm::Off), 0);
    }

    #[test]
    fn beams_join_eighths_per_quarter() {
        let e = || beat(DurationValue::Eighth, 0, false);
        let s = || beat(DurationValue::Sixteenth, 0, false);
        let q = || beat(DurationValue::Quarter, 0, false);
        let r = || beat(DurationValue::Eighth, 0, true);
        // Two pairs of eighths: one beam per quarter, not across.
        let bs = [e(), e(), e(), e()];
        assert_eq!(beams(&bs, &[0, 3, 6, 9], 10, "nerd"), "────  ────");
        // Sixteenths double the beam between them; a mix is single.
        let bs = [s(), s(), s(), s(), e(), e()];
        assert_eq!(beams(&bs, &[0, 2, 4, 6, 8, 10], 12, "nerd"), "═══════ ─── ");
        assert_eq!(beams(&[e(), s()], &[0, 2], 4, "nerd"), "──═ ");
        // Rests, longer notes and lone notes break or get no beam.
        let bs = [e(), r(), e(), q(), e()];
        assert_eq!(beams(&bs, &[0, 2, 4, 6, 8], 9, "nerd"), " ".repeat(9));
        assert_eq!(beams(&[e(), e()], &[0, 2], 4, "none"), "--- ");
        assert_eq!(beams(&[s(), s()], &[0, 2], 4, "unicode"), "=== ");
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
