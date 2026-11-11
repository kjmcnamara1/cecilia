//! The pitch of a recording as a series of sung notes, and the alignment of a melody with them
//! (`:sync-detect` for chant): the recording's pitch is tracked with the tuner's YIN method,
//! cut into steady notes, and the melody's notes are laid along them, in order, allowing for a
//! transposition and for notes the singing runs together or leaves out.

use crate::AudioClip;
use crate::tuner::detect_pitch;

/// A steady note found in a recording.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Segment {
    /// Seconds into the recording where it starts and ends.
    pub start: f64,
    pub end: f64,
    /// Its pitch, as a (fractional) MIDI note number.
    pub midi: f32,
}

/// The rate the audio is analysed at.
const RATE: u32 = 11_025;
/// Samples of a pitch reading, and the seconds between readings.
const WINDOW: usize = 1024;
const HOP: f64 = 0.025;
/// How periodic a reading must be.
const CLARITY: f32 = 0.8;
/// Semitones a note may wander and still be the same note.
const STEADY: f32 = 0.7;
/// The fewest readings in a note.
const MIN_READINGS: usize = 3;

/// The steady notes of a recording, in time order.
pub fn segments(clip: &AudioClip) -> Vec<Segment> {
    let factor = (clip.rate / RATE).max(1) as usize;
    let rate = clip.rate as f32 / factor as f32;
    let mono: Vec<f32> = clip
        .samples
        .chunks(2 * factor)
        .map(|c| c.iter().sum::<f32>() / c.len().max(1) as f32)
        .collect();
    let hop = ((HOP * rate as f64) as usize).max(1);
    let mut out = Vec::new();
    // The note being followed: its readings' mean and count, and where it started.
    let mut cur: Option<(f32, usize, f64)> = None;
    let mut at = 0;
    let close = |cur: &mut Option<(f32, usize, f64)>, end: f64, out: &mut Vec<Segment>| {
        if let Some((mean, n, start)) = cur.take()
            && n >= MIN_READINGS
        {
            out.push(Segment {
                start,
                end,
                midi: mean,
            });
        }
    };
    while at + WINDOW <= mono.len() {
        let t = (at + WINDOW / 2) as f64 / rate as f64;
        let midi = detect_pitch(&mono[at..at + WINDOW], rate)
            .filter(|r| r.clarity >= CLARITY)
            .map(|r| 69.0 + 12.0 * (r.freq / 440.0).log2());
        match (midi, &mut cur) {
            (Some(m), Some((mean, n, _))) if (m - *mean).abs() <= STEADY => {
                *mean = (*mean * *n as f32 + m) / (*n + 1) as f32;
                *n += 1;
            }
            (Some(m), _) => {
                close(&mut cur, t, &mut out);
                cur = Some((m, 1, t));
            }
            (None, _) => close(&mut cur, t, &mut out),
        }
        at += hop;
    }
    close(&mut cur, mono.len() as f64 / rate as f64, &mut out);
    out
}

/// How far (semitones, 0–6) two pitches are apart modulo an octave, which the pitch tracker
/// is not sure of.
fn distance(a: f32, b: f32) -> f32 {
    let d = (a - b).rem_euclid(12.0);
    d.min(12.0 - d)
}

/// The most cells (melody notes times sung notes) [`align`] works on, about 160 MB of scores.
pub const MAX_ALIGN_CELLS: usize = 40_000_000;

/// Whether a melody of `notes` against `sung` notes is too big for [`align`], which then
/// matches nothing; callers should say so rather than report a failed alignment.
pub fn align_too_large(notes: usize, sung: usize) -> bool {
    (notes + 1).saturating_mul(sung + 1) > MAX_ALIGN_CELLS
}

/// Lays a melody (MIDI notes) along the notes sung: for each note, the index of the sung note
/// it is, in order and with some left out on either side; the best of the transpositions in
/// half semitones. The singing may start before and go on after the melody.
pub fn align(melody: &[f32], sung: &[Segment]) -> Vec<Option<usize>> {
    const SKIP_NOTE: f32 = -0.6;
    const SKIP_SUNG: f32 = -0.2;
    const TOLERANCE: f32 = 0.75;
    let (n, m) = (melody.len(), sung.len());
    let mut best: (f32, Vec<Option<usize>>) = (f32::MIN, vec![None; n]);
    if n == 0 || m == 0 {
        return best.1;
    }
    if align_too_large(n, m) {
        return best.1;
    }
    let w = m + 1;
    let mut dp = vec![0f32; (n + 1) * w];
    for shift in (0..24).map(|k| k as f32 * 0.5) {
        let score = |i: usize, j: usize| {
            let d = distance(sung[j].midi - melody[i], shift);
            if d <= TOLERANCE { 2.0 - d } else { -1.5 }
        };
        for i in 1..=n {
            dp[i * w] = dp[(i - 1) * w] + SKIP_NOTE;
            for j in 1..=m {
                dp[i * w + j] = (dp[(i - 1) * w + j - 1] + score(i - 1, j - 1))
                    .max(dp[(i - 1) * w + j] + SKIP_NOTE)
                    .max(dp[i * w + j - 1] + SKIP_SUNG);
            }
        }
        let jend = (0..=m)
            .max_by(|&a, &b| dp[n * w + a].total_cmp(&dp[n * w + b]))
            .unwrap_or(0);
        if dp[n * w + jend] <= best.0 {
            continue;
        }
        let mut out = vec![None; n];
        let (mut i, mut j) = (n, jend);
        while i > 0 && j > 0 {
            let here = dp[i * w + j];
            if (here - (dp[(i - 1) * w + j - 1] + score(i - 1, j - 1))).abs() < 1e-4
                && score(i - 1, j - 1) > 0.0
            {
                out[i - 1] = Some(j - 1);
                i -= 1;
                j -= 1;
            } else if (here - (dp[(i - 1) * w + j] + SKIP_NOTE)).abs() < 1e-4 {
                i -= 1;
            } else {
                j -= 1;
            }
        }
        best = (dp[n * w + jend], out);
    }
    best.1
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A clip (stereo, 22.05 kHz) of sine notes `(Hz, seconds)`; Hz 0 is silence.
    fn clip(notes: &[(f32, f32)]) -> AudioClip {
        let rate = 22_050u32;
        let mut samples = Vec::new();
        for &(hz, secs) in notes {
            for i in 0..(secs * rate as f32) as usize {
                let v = if hz == 0.0 {
                    0.0
                } else {
                    0.5 * (std::f32::consts::TAU * hz * i as f32 / rate as f32).sin()
                };
                samples.push(v);
                samples.push(v);
            }
        }
        AudioClip { rate, samples }
    }

    #[test]
    fn sung_notes_are_found_with_their_times() {
        // A, B, C# with a pause before and between.
        let c = clip(&[
            (0.0, 0.5),
            (220.0, 0.6),
            (0.0, 0.1),
            (246.94, 0.6),
            (277.18, 0.6),
        ]);
        let s = segments(&c);
        assert_eq!(s.len(), 3, "{s:?}");
        let midi: Vec<i32> = s.iter().map(|x| x.midi.round() as i32).collect();
        assert_eq!(midi, [57, 59, 61]);
        for (seg, want) in s.iter().zip([0.5, 1.2, 1.8]) {
            assert!((seg.start - want).abs() < 0.08, "{s:?}");
        }
    }

    #[test]
    fn a_melody_is_laid_along_the_notes_sung_transposed_and_with_gaps() {
        let seg = |start: f64, midi: f32| Segment {
            start,
            end: start + 0.4,
            midi,
        };
        // Sung a tone higher than written, with a leading note not in the melody and one note
        // of the melody (the F) left out.
        let sung = [
            seg(0.0, 50.0),
            seg(1.0, 62.0),
            seg(2.0, 64.0),
            seg(3.0, 60.0),
            seg(4.0, 62.0),
        ];
        let melody = [60.0, 62.0, 65.0, 58.0, 60.0];
        let a = align(&melody, &sung);
        assert_eq!(a[0], Some(1));
        assert_eq!(a[1], Some(2));
        assert_eq!(a[2], None);
        assert_eq!(a[3], Some(3));
        assert_eq!(a[4], Some(4));
        assert!(align(&[], &sung).is_empty());
        assert_eq!(align(&melody, &[]), vec![None; 5]);
    }
}
