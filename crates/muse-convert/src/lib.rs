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
/// hand is near (updated to the chord's average fret). Places as many notes as possible, then
/// prefers frets close to `position`; notes that cannot be placed get `None`.
pub fn fret_chord(
    pitches: &[u8],
    strings: &[u8],
    capo: u8,
    position: &mut i32,
) -> Vec<Option<(u8, u8)>> {
    let candidates: Vec<Vec<(usize, i32)>> = pitches
        .iter()
        .map(|&p| {
            let key = p as i32 - capo as i32;
            let mut c: Vec<(usize, i32)> = strings
                .iter()
                .enumerate()
                .map(|(s, &open)| (s, key - open as i32))
                .filter(|(_, f)| (0..=MAX_FRET).contains(f))
                .collect();
            c.sort_by_key(|(_, f)| ((f - *position).abs(), *f));
            c
        })
        .collect();
    let mut best = Search {
        placed: 0,
        cost: i32::MAX,
        choice: vec![None; pitches.len()],
    };
    let mut current = vec![None; pitches.len()];
    let mut used = vec![false; strings.len()];
    search(
        &candidates,
        *position,
        0,
        0,
        0,
        &mut used,
        &mut current,
        &mut best,
    );
    let out: Vec<Option<(u8, u8)>> = best
        .choice
        .iter()
        .map(|c| c.map(|(s, f)| (s as u8, f as u8)))
        .collect();
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

struct Search {
    placed: usize,
    cost: i32,
    choice: Vec<Option<(usize, i32)>>,
}

/// Depth-first search over string assignments (chords have at most a handful of notes).
#[allow(clippy::too_many_arguments)]
fn search(
    candidates: &[Vec<(usize, i32)>],
    position: i32,
    i: usize,
    placed: usize,
    cost: i32,
    used: &mut [bool],
    current: &mut [Option<(usize, i32)>],
    best: &mut Search,
) {
    // Even placing every remaining note cannot beat the best: prune.
    if placed + (candidates.len() - i) < best.placed {
        return;
    }
    if i == candidates.len() {
        if placed > best.placed || (placed == best.placed && cost < best.cost) {
            best.placed = placed;
            best.cost = cost;
            best.choice = current.to_vec();
        }
        return;
    }
    for &(s, f) in &candidates[i] {
        if used[s] {
            continue;
        }
        used[s] = true;
        current[i] = Some((s, f));
        search(
            candidates,
            position,
            i + 1,
            placed + 1,
            cost + (f - position).abs(),
            used,
            current,
            best,
        );
        used[s] = false;
        current[i] = None;
    }
    search(
        candidates,
        position,
        i + 1,
        placed,
        cost,
        used,
        current,
        best,
    );
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
    fn keeps_the_low_string_free_for_the_lowest_note() {
        // A2 alone would sit at fret 5 on the low E near position 7, but E2 needs that string.
        let mut pos = 7;
        let placed = fret_chord(&[45, 40], &STANDARD, 0, &mut pos);
        assert_eq!(placed, vec![Some((4, 0)), Some((5, 0))]);
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
