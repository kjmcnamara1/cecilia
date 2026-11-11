//! Notation conversions for cecilia: assigning strings and frets to pitched notes
//! ([`fret_chord`], [`pitched_to_fretted`]) and modern ⇄ chant ([`chant`]).
//!
//! The fingering is a bounded search that places as many notes of a chord as it can and keeps
//! the hand near its previous position.

pub mod chant;

use cecilia_core::{Note, Track, TrackKind};

/// Highest fret the fingering will use.
const MAX_FRET: i32 = 24;

/// Most notes of a chord the fingering searches over (the highest ones; no instrument has
/// more strings, and the search grows with the chord).
const MAX_CHORD: usize = 12;

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
    let mut candidates: Vec<Vec<(usize, i32)>> = pitches
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
    if pitches.len() > MAX_CHORD {
        // Notes that fit the instrument come first, the highest of them before the rest; the
        // others are left unplaced.
        let mut order: Vec<usize> = (0..pitches.len()).collect();
        order.sort_by_key(|&i| (candidates[i].is_empty(), std::cmp::Reverse(pitches[i])));
        for &i in &order[MAX_CHORD..] {
            candidates[i].clear();
        }
    }
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
    // Costs only grow: a branch that cannot place more notes than the best, at no less cost,
    // cannot replace it.
    if cost >= best.cost && placed + (candidates.len() - i) <= best.placed {
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
    fn huge_chords_are_searched_quickly() {
        let mut pos = 0;
        let pitches: Vec<u8> = (40..90).collect();
        let placed = fret_chord(&pitches, &STANDARD, 0, &mut pos);
        let n = placed.iter().flatten().count();
        assert!((3..=6).contains(&n), "{n} placed");
    }

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

/// Per-bar, per-voice, per-beat note pitches.
type Pitches = Vec<Vec<Vec<Vec<u8>>>>;
/// A chord fingering: (string, fret) per note, None when it cannot be placed.
type Shape = Vec<Option<(u8, u8)>>;

/// Cost of one chord fingering: (dropped notes, stretch, height) folded into one number.
fn shape_cost(placed: &[Option<(u8, u8)>]) -> f64 {
    let dropped = placed.iter().filter(|p| p.is_none()).count() as f64;
    let fretted: Vec<i32> = placed
        .iter()
        .flatten()
        .map(|&(_, f)| f as i32)
        .filter(|&f| f > 0)
        .collect();
    let span = match (fretted.iter().min(), fretted.iter().max()) {
        (Some(a), Some(b)) => (b - a) as f64,
        _ => 0.0,
    };
    let height: f64 = fretted.iter().map(|&f| f as f64).sum::<f64>() / fretted.len().max(1) as f64;
    dropped * 100.0 + span.max(3.0) * 2.0 - 6.0
        + height * 0.1
        + if span > 4.0 { (span - 4.0) * 10.0 } else { 0.0 }
}

/// Hand position of a fingering (mean fretted fret), or None for all-open chords.
fn centre(placed: &[Option<(u8, u8)>]) -> Option<f64> {
    let fretted: Vec<f64> = placed
        .iter()
        .flatten()
        .map(|&(_, f)| f as f64)
        .filter(|&f| f > 0.0)
        .collect();
    (!fretted.is_empty()).then(|| fretted.iter().sum::<f64>() / fretted.len() as f64)
}

/// Chooses string/fret for a sequence of chords (MIDI pitches) so the hand moves as little as
/// possible: a Viterbi search over candidate shapes per chord.
pub fn finger_sequence(chords: &[Vec<u8>], strings: &[u8], capo: u8) -> Vec<Vec<Option<(u8, u8)>>> {
    const SEEDS: [i32; 9] = [0, 2, 3, 5, 7, 9, 10, 12, 14];
    let candidates: Vec<Vec<Shape>> = chords
        .iter()
        .map(|pitches| {
            let mut c: Vec<Vec<Option<(u8, u8)>>> = Vec::new();
            for seed in SEEDS {
                let mut pos = seed;
                let shape = fret_chord(pitches, strings, capo, &mut pos);
                if !c.contains(&shape) {
                    c.push(shape);
                }
            }
            c
        })
        .collect();
    if chords.is_empty() {
        return vec![];
    }
    // cost[i][k], back[i][k]
    let mut cost: Vec<Vec<f64>> = Vec::with_capacity(chords.len());
    let mut back: Vec<Vec<usize>> = Vec::with_capacity(chords.len());
    // Where the hand is after each candidate (open chords keep the previous position).
    let mut hand: Vec<Vec<Option<f64>>> = Vec::with_capacity(chords.len());
    for (i, cands) in candidates.iter().enumerate() {
        let mut row = Vec::with_capacity(cands.len());
        let mut brow = Vec::with_capacity(cands.len());
        let mut hrow = Vec::with_capacity(cands.len());
        for shape in cands {
            let own = shape_cost(shape);
            let c = centre(shape);
            if i == 0 {
                row.push(own + c.unwrap_or(0.0) * 0.2);
                brow.push(0);
                hrow.push(c);
                continue;
            }
            let mut best = (f64::MAX, 0, None);
            for (k, prev) in cost[i - 1].iter().enumerate() {
                let from = hand[i - 1][k];
                let moved = match (from, c) {
                    (Some(a), Some(b)) => (a - b).abs(),
                    _ => 0.0,
                };
                let total = prev + own + moved * 1.5;
                if total < best.0 {
                    best = (total, k, c.or(from));
                }
            }
            row.push(best.0);
            brow.push(best.1);
            hrow.push(best.2);
        }
        cost.push(row);
        back.push(brow);
        hand.push(hrow);
    }
    let last = cost.len() - 1;
    let mut k = (0..cost[last].len())
        .min_by(|&a, &b| cost[last][a].total_cmp(&cost[last][b]))
        .unwrap_or(0);
    let mut out = vec![Vec::new(); chords.len()];
    for i in (0..chords.len()).rev() {
        out[i] = candidates[i][k].clone();
        k = back[i][k];
    }
    out
}

/// Re-fingers every voice of a fretted track in `bars` from the notes' pitches. Returns the
/// number of notes that could not be placed (they are removed).
pub fn refinger(track: &mut Track, bars: std::ops::Range<usize>) -> usize {
    let pitches: Pitches = track.bars[bars.clone()]
        .iter()
        .map(|b| {
            b.voices
                .iter()
                .map(|v| {
                    v.beats
                        .iter()
                        .map(|beat| beat.notes.iter().map(|n| track.pitch_of(n)).collect())
                        .collect()
                })
                .collect()
        })
        .collect();
    let (strings, capo) = (track.strings.clone(), track.capo);
    apply_fingering(track, bars, &pitches, &strings, capo)
}

fn apply_fingering(
    track: &mut Track,
    bars: std::ops::Range<usize>,
    pitches: &[Vec<Vec<Vec<u8>>>],
    strings: &[u8],
    capo: u8,
) -> usize {
    let voices = pitches.iter().map(|b| b.len()).max().unwrap_or(0);
    let mut dropped = 0;
    for v in 0..voices {
        let chords: Vec<Vec<u8>> = pitches
            .iter()
            .filter_map(|b| b.get(v))
            .flatten()
            .filter(|c| !c.is_empty())
            .cloned()
            .collect();
        let mut shapes = finger_sequence(&chords, strings, capo).into_iter();
        for bar in &mut track.bars[bars.clone()] {
            let Some(voice) = bar.voices.get_mut(v) else {
                continue;
            };
            for beat in &mut voice.beats {
                if beat.notes.is_empty() {
                    continue;
                }
                let Some(shape) = shapes.next() else { break };
                let old = std::mem::take(&mut beat.notes);
                for (note, place) in old.into_iter().zip(shape) {
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
        }
    }
    dropped
}

/// Changes a fretted track's tuning and capo, keeping every note's pitch (moving notes out of
/// range by octaves) and re-fingering. Returns (octave-shifted notes, dropped notes).
pub fn retune(track: &mut Track, strings: Vec<u8>, capo: u8) -> (usize, usize) {
    let low = strings.iter().copied().min().unwrap_or(40) as i32 + capo as i32;
    let high = strings.iter().copied().max().unwrap_or(64) as i32 + MAX_FRET;
    let mut shifted = 0;
    let pitches: Pitches = track
        .bars
        .iter()
        .map(|b| {
            b.voices
                .iter()
                .map(|v| {
                    v.beats
                        .iter()
                        .map(|beat| {
                            beat.notes
                                .iter()
                                .map(|n| {
                                    let mut p = track.pitch_of(n) as i32;
                                    let original = p;
                                    while p < low {
                                        p += 12;
                                    }
                                    while p > high {
                                        p -= 12;
                                    }
                                    if p != original {
                                        shifted += 1;
                                    }
                                    p.clamp(0, 127) as u8
                                })
                                .collect()
                        })
                        .collect()
                })
                .collect()
        })
        .collect();
    track.strings = strings.clone();
    track.capo = capo;
    track.kind = TrackKind::Fretted;
    let n = track.bars.len();
    let dropped = apply_fingering(track, 0..n, &pitches, &strings, capo);
    (shifted, dropped)
}

/// Standard tunings by name (index 0 = highest string).
pub fn tuning_preset(name: &str) -> Option<Vec<u8>> {
    Some(match name.to_ascii_lowercase().as_str() {
        "standard" | "guitar" | "e" => vec![64, 59, 55, 50, 45, 40],
        "drop-d" | "dropd" => vec![64, 59, 55, 50, 45, 38],
        "eb" | "half-step-down" => vec![63, 58, 54, 49, 44, 39],
        "d" | "whole-step-down" => vec![62, 57, 53, 48, 43, 38],
        "dadgad" => vec![62, 57, 55, 50, 45, 38],
        "open-g" => vec![62, 59, 55, 50, 43, 38],
        "open-d" => vec![62, 57, 54, 50, 45, 38],
        "open-e" => vec![64, 59, 56, 52, 47, 40],
        "7-string" => vec![64, 59, 55, 50, 45, 40, 35],
        "bass" | "bass4" => vec![43, 38, 33, 28],
        "bass5" => vec![43, 38, 33, 28, 23],
        "ukulele" => vec![69, 64, 60, 67],
        _ => return None,
    })
}

#[cfg(test)]
mod sequence_tests {
    use super::*;
    use cecilia_core::*;

    #[test]
    fn sequence_stays_in_position() {
        // A scale fragment around the 5th fret should not jump to open strings.
        let chords: Vec<Vec<u8>> = [57, 59, 60, 62, 64, 65].iter().map(|&p| vec![p]).collect();
        let shapes = finger_sequence(&chords, &[64, 59, 55, 50, 45, 40], 0);
        let frets: Vec<u8> = shapes.iter().map(|s| s[0].unwrap().1).collect();
        let (lo, hi) = (*frets.iter().min().unwrap(), *frets.iter().max().unwrap());
        assert!(hi - lo <= 5, "{frets:?}");
    }

    #[test]
    fn retune_keeps_pitches() {
        let beat = |s, f| Beat {
            notes: vec![Note::new(s, f)],
            ..Beat::default()
        };
        let mut track = Track {
            strings: vec![64, 59, 55, 50, 45, 40],
            bars: vec![Bar {
                voices: vec![Voice {
                    beats: vec![beat(5, 0), beat(4, 2), beat(0, 3)],
                }],
                ..Default::default()
            }],
            ..Track::default()
        };
        let before: Vec<u8> = track.bars[0].voices[0]
            .beats
            .iter()
            .map(|b| track.pitch_of(&b.notes[0]))
            .collect();
        let (shifted, dropped) = retune(&mut track, tuning_preset("drop-d").unwrap(), 0);
        let after: Vec<u8> = track.bars[0].voices[0]
            .beats
            .iter()
            .map(|b| track.pitch_of(&b.notes[0]))
            .collect();
        assert_eq!((shifted, dropped), (0, 0));
        assert_eq!(before, after);
        // Low E is now fret 2 on the low D string.
        assert_eq!(
            (
                track.bars[0].voices[0].beats[0].notes[0].string,
                track.bars[0].voices[0].beats[0].notes[0].fret
            ),
            (5, 2)
        );
    }

    #[test]
    fn retune_between_string_counts() {
        let beat = |s, f| Beat {
            notes: vec![Note::new(s, f)],
            ..Beat::default()
        };
        let mut track = Track {
            strings: vec![64, 59, 55, 50, 45, 40],
            bars: vec![Bar {
                voices: vec![Voice {
                    beats: vec![beat(5, 0), beat(4, 2), beat(0, 3), beat(0, 22)],
                }],
                ..Default::default()
            }],
            ..Track::default()
        };
        let pitches = |t: &Track| -> Vec<u8> {
            t.bars[0].voices[0]
                .beats
                .iter()
                .map(|b| t.pitch_of(&b.notes[0]))
                .collect()
        };
        let before = pitches(&track);
        // More strings: nothing moves, and a note on the new low string is possible.
        let (shifted, dropped) = retune(&mut track, tuning_preset("7-string").unwrap(), 0);
        assert_eq!((shifted, dropped), (0, 0));
        assert_eq!((track.strings.len(), pitches(&track)), (7, before.clone()));
        // Fewer strings: a note above the four strings' range moves down an octave (reported),
        // and every note is still on a string of the track.
        let (shifted, dropped) = retune(&mut track, tuning_preset("bass").unwrap(), 0);
        assert_eq!(track.strings.len(), 4);
        assert!(shifted > 0 && dropped == 0, "{shifted} {dropped}");
        for b in &track.bars[0].voices[0].beats {
            assert!((b.notes[0].string as usize) < 4);
        }
    }
}
