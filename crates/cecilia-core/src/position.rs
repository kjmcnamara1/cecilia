//! Positions in a score as `bar:beat.fraction`: the bar (from 1) and the beat in it (from 1) with
//! the part of the beat after the point, so the eighth note on the upbeat of the third beat of
//! the fifth bar is `5:3.5`. A beat is the unit of the time signature (a quarter note in 4/4, an
//! eighth in 6/8). A bar holding more than its count goes on counting (`2:6.0` in 4/4).

use crate::{Score, TICKS_PER_QUARTER, Track};

/// Ticks in one beat of bar `bar`.
pub fn beat_ticks(score: &Score, bar: usize) -> u32 {
    let den = score
        .master_bars
        .get(bar)
        .map_or(4, |m| m.time_signature.denominator.max(1)) as u32;
    (TICKS_PER_QUARTER * 4 / den).max(1)
}

/// The ticks between the start of bar `bar` and beat `beat` of voice `voice` of the track.
pub fn offset_ticks(track: &Track, bar: usize, voice: usize, beat: usize) -> u32 {
    track
        .bars
        .get(bar)
        .and_then(|b| b.voices.get(voice))
        .map_or(0, |v| v.beats.iter().take(beat).map(|b| b.ticks()).sum())
}

/// The beat number (from 1, with its fraction) of beat `beat` of voice `voice` in bar `bar`.
pub fn beat_position(score: &Score, track: &Track, bar: usize, voice: usize, beat: usize) -> f64 {
    1.0 + offset_ticks(track, bar, voice, beat) as f64 / beat_ticks(score, bar) as f64
}

/// `bar:beat.fraction` for a bar (from 0) and a beat number (from 1). With `short` a whole beat
/// has no fraction (`5:3`), else it has `.0`.
pub fn format_position(bar: usize, pos: f64, short: bool) -> String {
    let mut s = format!("{:.3}", pos);
    while s.ends_with('0') && !s.ends_with(".0") {
        s.pop();
    }
    if short && s.ends_with(".0") {
        s.truncate(s.len() - 2);
    }
    format!("{}:{}", bar + 1, s)
}

/// Reads `bar:beat` or `bar:beat.fraction`: the bar from 0 and the beat number from 1.
pub fn parse_position(s: &str) -> Option<(usize, f64)> {
    let (a, b) = s.split_once(':')?;
    let bar: usize = a.trim().parse().ok()?;
    let b = b.trim();
    if b.is_empty()
        || !b.chars().all(|c| c.is_ascii_digit() || c == '.')
        || b.matches('.').count() > 1
    {
        return None;
    }
    let pos: f64 = b.parse().ok()?;
    (bar > 0 && pos >= 1.0).then(|| (bar - 1, pos))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn positions_read_and_write() {
        assert_eq!(format_position(4, 3.5, false), "5:3.5");
        assert_eq!(format_position(1, 6.0, false), "2:6.0");
        assert_eq!(format_position(1, 6.0, true), "2:6");
        assert_eq!(format_position(0, 1.0 + 1.0 / 3.0, false), "1:1.333");
        assert_eq!(parse_position("5:3.5"), Some((4, 3.5)));
        assert_eq!(parse_position("2:6"), Some((1, 6.0)));
        assert_eq!(parse_position("0:1"), None);
        assert_eq!(parse_position("1:0.5"), None);
        assert_eq!(parse_position("1:x"), None);
        assert_eq!(parse_position("1:2.3.4"), None);
    }
}
