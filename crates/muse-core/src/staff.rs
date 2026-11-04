//! Staff positions: spelling pitches in a key, and the diatonic numbers (octave * 7 + step,
//! step 0 = C) that place notes on a five-line staff.

use crate::score::StaffClef;

/// Semitones above C of each step letter (C D E F G A B).
const STEP_SEMITONES: [i32; 7] = [0, 2, 4, 5, 7, 9, 11];

/// Step (0 = C), alteration and octave of a pitch, spelled for a key.
pub fn spell(midi: i32, key: i8) -> (i32, i32, i32) {
    const SHARPS: [(i32, i32); 12] = [
        (0, 0),
        (0, 1),
        (1, 0),
        (1, 1),
        (2, 0),
        (3, 0),
        (3, 1),
        (4, 0),
        (4, 1),
        (5, 0),
        (5, 1),
        (6, 0),
    ];
    const FLATS: [(i32, i32); 12] = [
        (0, 0),
        (1, -1),
        (1, 0),
        (2, -1),
        (2, 0),
        (3, 0),
        (4, -1),
        (4, 0),
        (5, -1),
        (5, 0),
        (6, -1),
        (6, 0),
    ];
    let pc = midi.rem_euclid(12) as usize;
    let octave = midi.div_euclid(12) - 1;
    // Flat keys use flats; in C, B♭ is far commoner than A♯.
    let (step, alter) = if key < 0 || (key == 0 && pc == 10) {
        FLATS[pc]
    } else {
        SHARPS[pc]
    };
    (step, alter, octave)
}

/// As [`spell`], but with the alteration a note was written with (a double sharp or flat)
/// when it has one.
pub fn spell_with(midi: i32, key: i8, alter: Option<i8>) -> (i32, i32, i32) {
    if let Some(a) = alter.map(i32::from) {
        let natural = midi - a;
        let pc = natural.rem_euclid(12);
        if let Some(step) = STEP_SEMITONES.iter().position(|s| *s == pc) {
            return (step as i32, a, natural.div_euclid(12) - 1);
        }
    }
    spell(midi, key)
}

/// Diatonic number of a pitch written with an alteration (see [`spell_with`]).
pub fn diatonic_with(midi: i32, key: i8, alter: Option<i8>) -> i32 {
    let (step, _, octave) = spell_with(midi, key, alter);
    octave * 7 + step
}

/// Alteration each step letter gets from a key signature.
pub fn key_alters(key: i8) -> [i32; 7] {
    let mut out = [0; 7];
    const SHARP_ORDER: [usize; 7] = [3, 0, 4, 1, 5, 2, 6];
    const FLAT_ORDER: [usize; 7] = [6, 2, 5, 1, 4, 0, 3];
    for i in 0..key.unsigned_abs().min(7) as usize {
        if key > 0 {
            out[SHARP_ORDER[i]] = 1;
        } else {
            out[FLAT_ORDER[i]] = -1;
        }
    }
    out
}

/// Diatonic number of a (written) pitch as spelled in a key.
pub fn diatonic(midi: i32, key: i8) -> i32 {
    let (step, _, octave) = spell(midi, key);
    octave * 7 + step
}

/// The (written) pitch at a diatonic number, with the key signature's alteration.
pub fn letter_midi(d: i32, key: i8) -> i32 {
    let step = d.rem_euclid(7) as usize;
    (d.div_euclid(7) + 1) * 12 + STEP_SEMITONES[step] + key_alters(key)[step]
}

/// Diatonic number of the step letter (0 = C) nearest to `from`.
pub fn nearest(step: i32, from: i32) -> i32 {
    let below = from - (from - step).rem_euclid(7);
    if from - below <= 3 { below } else { below + 7 }
}

/// Step number of a note letter (`c` = 0 … `b` = 6).
pub fn letter_step(c: char) -> Option<i32> {
    "cdefgab".find(c.to_ascii_lowercase()).map(|i| i as i32)
}

impl StaffClef {
    /// Diatonic number of the bottom staff line.
    pub fn bottom_line(self) -> i32 {
        if self.is_bass() { 2 * 7 + 4 } else { 4 * 7 + 2 }
    }

    /// Semitones added to the sounding pitch to get the written pitch.
    pub fn transpose(self) -> i32 {
        match self {
            StaffClef::Treble8vb | StaffClef::Bass8vb => 12,
            _ => 0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn written_alterations() {
        // C double sharp sounds as D; it is still on C's line.
        assert_eq!(spell_with(62, 0, Some(2)), (0, 2, 4));
        assert_eq!(spell_with(60, 0, Some(-2)), (1, -2, 4));
        assert_eq!(spell_with(61, 0, None), spell(61, 0));
        // An alteration no letter can carry falls back to the key's spelling.
        assert_eq!(spell_with(61, 0, Some(2)), spell(61, 0));
        assert_eq!(diatonic_with(62, 0, Some(2)), 4 * 7);
    }

    #[test]
    fn letters_and_keys() {
        // G4 in C, F#4 in G, Bb3 in F.
        assert_eq!(letter_midi(diatonic(67, 0), 0), 67);
        assert_eq!(letter_midi(4 * 7 + 3, 1), 66);
        assert_eq!(letter_midi(3 * 7 + 6, -1), 58);
        for key in -7..=7 {
            for midi in 30..90 {
                let (_, alter, _) = spell(midi, key);
                assert_eq!(letter_midi(diatonic(midi, key), 0) + alter, midi);
            }
        }
        // From G4 (diatonic 32): A is above, C is above (C5), E below (E4).
        assert_eq!(nearest(5, 32), 33);
        assert_eq!(nearest(0, 32), 35);
        assert_eq!(nearest(2, 32), 30);
        assert_eq!(letter_step('g'), Some(4));
    }
}
