//! The rhythm of a tab+ staff in terminal characters: a stem hanging below each beat's tab
//! column, beams joining the stems of a beat group, dots and rests.
//!
//! ```text
//! e|--3---5---3-5-|
//! B|--------------|
//!    │   │   │ │ ·
//!    └───┘   └─┘     (eighths beamed in pairs)
//! ```
//!
//! Everything here is pure: [`rows`] allocates the rows, [`render_bar`] builds the character
//! grid of one bar from beat columns, and [`stem_glyph`] / [`beam_level`] pick the glyphs.

use cecilia_core::{Beat, DurationValue, Staff, TICKS_PER_QUARTER, Track};

/// Rows below the strings for the track's rhythm: 0 without a tab+ staff, otherwise a stem row
/// and a beam row, plus a second beam row when anything is shorter than an eighth.
pub fn rows(track: &Track) -> u16 {
    if !track.staves().contains(&Staff::TabRhythm) {
        return 0;
    }
    let sixteenths = track
        .bars
        .iter()
        .flat_map(|b| &b.voices)
        .flat_map(|v| &v.beats)
        .any(|b| beam_level(b.duration.value) >= 2);
    2 + sixteenths as u16
}

/// Number of beams (or flags) a value carries: 0 up to a quarter, 1 for an eighth, 2 for a
/// sixteenth and so on.
pub fn beam_level(value: DurationValue) -> u8 {
    match value {
        DurationValue::Whole | DurationValue::Half | DurationValue::Quarter => 0,
        DurationValue::Eighth => 1,
        DurationValue::Sixteenth => 2,
        DurationValue::ThirtySecond => 3,
        DurationValue::SixtyFourth => 4,
        DurationValue::HundredTwentyEighth => 5,
    }
}

/// The glyph on the stem row for a beat: a stem (hollow for half notes, none for a whole note,
/// which gets a ring) or, for a rest, a mark of its value.
pub fn stem_glyph(value: DurationValue, rest: bool) -> char {
    match (rest, value) {
        (true, DurationValue::Whole) => '▀',
        (true, DurationValue::Half) => '▄',
        (true, _) => 'z',
        (false, DurationValue::Whole) => 'o',
        (false, DurationValue::Half) => '¦',
        (false, _) => '│',
    }
}

/// One bar's rhythm as `height` rows of `width` characters (spaces where nothing is drawn).
/// `cols` is the column of each beat's stem (index = beat); beats are beamed per quarter note
/// of the bar, the way the engraver groups them. Rests and long notes break a beam.
pub fn render_bar(beats: &[Beat], cols: &[u16], width: u16, height: u16) -> Vec<Vec<char>> {
    let mut grid = vec![vec![' '; width as usize]; height as usize];
    if height == 0 {
        return grid;
    }
    let mut put = |row: u16, x: u16, c: char| {
        if row < height
            && let Some(cell) = grid[row as usize].get_mut(x as usize)
        {
            *cell = c;
        }
    };
    // Beams: runs of beamed beats in one quarter-note group.
    let mut tick = 0;
    let mut groups: Vec<Vec<(u16, u8)>> = vec![];
    let mut open: Option<u32> = None;
    for (beat, &x) in beats.iter().zip(cols) {
        let level = beam_level(beat.duration.value);
        let glyph = stem_glyph(beat.duration.value, beat.is_rest());
        if !beat.empty {
            put(0, x, glyph);
            for d in 0..beat.duration.dots as u16 {
                put(0, x + 1 + d, '·');
            }
        }
        let group = tick / TICKS_PER_QUARTER;
        if level > 0 && !beat.is_rest() && !beat.empty {
            if open != Some(group) {
                groups.push(vec![]);
                open = Some(group);
            }
            if let Some(g) = groups.last_mut() {
                g.push((x, level));
            }
        } else {
            open = None;
        }
        tick += beat.ticks();
    }
    for g in &groups {
        beam_group(g, height, &mut put);
    }
    grid
}

/// Draws one run of `(stem column, level)` stems: a primary beam on row 1 and, for each level
/// above one, a partial beam on the rows below (a stub towards the neighbour when it has
/// fewer beams). A lone stem gets a flag.
fn beam_group(group: &[(u16, u8)], height: u16, put: &mut impl FnMut(u16, u16, char)) {
    let (Some(&(first, _)), Some(&(last, _))) = (group.first(), group.last()) else {
        return;
    };
    if group.len() == 1 {
        put(1, first, '╰');
        for row in 2..height.min(1 + group[0].1 as u16) {
            put(row, first, '╰');
        }
        return;
    }
    for x in first..=last {
        put(1, x, '─');
    }
    for (i, &(x, _)) in group.iter().enumerate() {
        let c = match i {
            0 => '└',
            n if n == group.len() - 1 => '┘',
            _ => '┴',
        };
        put(1, x, c);
    }
    // Secondary beams: row `r` joins neighbouring stems that both have level > r.
    for row in 2..height {
        let level = row as u8 - 1;
        for (i, &(x, l)) in group.iter().enumerate() {
            if l <= level {
                continue;
            }
            let next = group.get(i + 1).filter(|n| n.1 > level);
            let prev = i
                .checked_sub(1)
                .and_then(|p| group.get(p))
                .filter(|p| p.1 > level);
            match next {
                Some(&(nx, _)) => {
                    put(row, x, '═');
                    for k in x + 1..nx {
                        put(row, k, '═');
                    }
                }
                None if prev.is_some() => put(row, x, '═'),
                // A stub towards the neighbour on the side that has room.
                None => put(row, x, if i + 1 < group.len() { '╒' } else { '╕' }),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cecilia_core::{Bar, Duration, Note, Voice};

    fn beat(value: DurationValue, rest: bool) -> Beat {
        Beat {
            duration: Duration::new(value),
            notes: if rest { vec![] } else { vec![Note::new(0, 3)] },
            ..Beat::default()
        }
    }

    fn text(grid: &[Vec<char>]) -> Vec<String> {
        grid.iter().map(|r| r.iter().collect()).collect()
    }

    #[test]
    fn rows_need_a_tab_rhythm_staff() {
        let mut t = Track {
            strings: vec![64; 6],
            bars: vec![Bar {
                voices: vec![Voice {
                    beats: vec![beat(DurationValue::Eighth, false)],
                }],
                ..Bar::default()
            }],
            ..Track::default()
        };
        assert_eq!(rows(&t), 0);
        t.staves = vec![Staff::Tab, Staff::TabRhythm];
        assert_eq!(rows(&t), 2);
        t.bars[0].voices[0]
            .beats
            .push(beat(DurationValue::Sixteenth, false));
        assert_eq!(rows(&t), 3);
    }

    #[test]
    fn glyphs_by_value() {
        assert_eq!(stem_glyph(DurationValue::Quarter, false), '│');
        assert_eq!(stem_glyph(DurationValue::Half, false), '¦');
        assert_eq!(stem_glyph(DurationValue::Whole, false), 'o');
        assert_eq!(stem_glyph(DurationValue::Eighth, true), 'z');
        assert_eq!(beam_level(DurationValue::Quarter), 0);
        assert_eq!(beam_level(DurationValue::ThirtySecond), 3);
    }

    #[test]
    fn eighths_are_beamed_per_quarter() {
        let beats = vec![beat(DurationValue::Eighth, false); 4];
        let g = render_bar(&beats, &[0, 2, 4, 6], 8, 2);
        assert_eq!(text(&g), ["│ │ │ │ ", "└─┘ └─┘ "]);
    }

    #[test]
    fn rests_break_beams_and_lone_notes_flag() {
        let beats = vec![
            beat(DurationValue::Eighth, false),
            beat(DurationValue::Eighth, true),
            beat(DurationValue::Quarter, false),
        ];
        let g = render_bar(&beats, &[0, 2, 4], 6, 2);
        assert_eq!(text(&g), ["│ z │ ", "╰     "]);
    }

    #[test]
    fn sixteenths_get_a_second_beam_and_dots_follow_stems() {
        let mut dotted = beat(DurationValue::Eighth, false);
        dotted.duration.dots = 1;
        let beats = vec![
            dotted,
            beat(DurationValue::Sixteenth, false),
            beat(DurationValue::Quarter, false),
        ];
        let g = render_bar(&beats, &[0, 3, 6], 8, 3);
        assert_eq!(text(&g), ["│· │  │ ", "└──┘    ", "   ╕    "]);
    }
}
