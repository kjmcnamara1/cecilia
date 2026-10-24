//! Notation conversions for muse.
//!
//! Currently: assigning strings and frets to pitched notes. This is a simple greedy fingering
//! that keeps the hand near its previous position; the cost-based fingering in the spec
//! (milestone 4) will replace it.

use muse_core::{Note, Track, TrackKind};

/// Highest fret the fingering will use.
const MAX_FRET: i32 = 24;

/// Chooses (string, fret) for each MIDI pitch of a chord, using each string at most once.
/// `strings` are open-string pitches with index 0 = highest string. `position` is the fret the
/// hand is near (updated to the chord's average fret). Pitches that cannot be placed get `None`.
pub fn fret_chord(
    pitches: &[u8],
    strings: &[u8],
    capo: u8,
    position: &mut i32,
) -> Vec<Option<(u8, u8)>> {
    let mut used = vec![false; strings.len()];
    let mut out = vec![None; pitches.len()];
    // Place the highest pitches first, on the highest strings available.
    let mut order: Vec<usize> = (0..pitches.len()).collect();
    order.sort_by_key(|&i| std::cmp::Reverse(pitches[i]));
    for i in order {
        let key = pitches[i] as i32 - capo as i32;
        let best = strings
            .iter()
            .enumerate()
            .filter(|(s, _)| !used[*s])
            .map(|(s, &open)| (s, key - open as i32))
            .filter(|(_, f)| (0..=MAX_FRET).contains(f))
            .min_by_key(|(_, f)| ((f - *position).abs(), *f));
        if let Some((s, f)) = best {
            used[s] = true;
            out[i] = Some((s as u8, f as u8));
        }
    }
    let frets: Vec<i32> = out
        .iter()
        .flatten()
        .map(|&(_, f)| f as i32)
        .filter(|&f| f > 0)
        .collect();
    if !frets.is_empty() {
        *position = frets.iter().sum::<i32>() / frets.len() as i32;
    }
    out
}

/// A six-string tuning in fourths (with a major third) whose lowest string is at or below
/// `lowest`, used to put pitched parts on a fretboard.
pub fn tuning_for_range(lowest: u8) -> Vec<u8> {
    let base = lowest.min(40);
    [24, 19, 15, 10, 5, 0].iter().map(|o| base + o).collect()
}

/// Converts a pitched (non-fretted) track into a fretted one: notes get strings and frets,
/// notes that do not fit are dropped. Returns the number of dropped notes.
pub fn pitched_to_fretted(track: &mut Track) -> usize {
    if track.kind != TrackKind::Pitched {
        return 0;
    }
    let lowest = track
        .bars
        .iter()
        .flat_map(|b| &b.voices)
        .flat_map(|v| &v.beats)
        .flat_map(|b| &b.notes)
        .map(|n| n.fret)
        .min()
        .unwrap_or(40);
    track.strings = tuning_for_range(lowest);
    track.kind = TrackKind::Fretted;
    track.capo = 0;
    let strings = track.strings.clone();
    let mut dropped = 0;
    let mut position = 0;
    for beat in track
        .bars
        .iter_mut()
        .flat_map(|b| &mut b.voices)
        .flat_map(|v| &mut v.beats)
    {
        let pitches: Vec<u8> = beat.notes.iter().map(|n| n.fret).collect();
        let placed = fret_chord(&pitches, &strings, 0, &mut position);
        let old = std::mem::take(&mut beat.notes);
        for (note, place) in old.into_iter().zip(placed) {
            match place {
                Some((string, fret)) => beat.notes.push(Note {
                    string,
                    fret,
                    ..note
                }),
                None => dropped += 1,
            }
        }
    }
    dropped
}

#[cfg(test)]
mod tests {
    use super::*;

    const STANDARD: [u8; 6] = [64, 59, 55, 50, 45, 40];

    #[test]
    fn chord_uses_distinct_strings() {
        let mut pos = 0;
        // Open E major: E2 B2 E3 G#3 B3 E4
        let placed = fret_chord(&[40, 47, 52, 56, 59, 64], &STANDARD, 0, &mut pos);
        let strings: Vec<u8> = placed.iter().map(|p| p.unwrap().0).collect();
        let mut unique = strings.clone();
        unique.sort();
        unique.dedup();
        assert_eq!(unique.len(), 6);
        assert_eq!(placed[0], Some((5, 0)));
    }

    #[test]
    fn stays_near_position_and_drops_out_of_range() {
        let mut pos = 7;
        assert_eq!(
            fret_chord(&[64], &STANDARD, 0, &mut pos),
            vec![Some((1, 5))]
        );
        let mut pos = 0;
        assert_eq!(fret_chord(&[30], &STANDARD, 0, &mut pos), vec![None]);
    }
}
