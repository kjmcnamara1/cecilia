//! The monophonic melody a chant staff shows for an ordinary track, and the neumes it is
//! grouped into.
//!
//! Chant is monophonic, so a track with chords has one note picked from each ([`ChordPolicy`]).
//! The staff is only a view: nothing here changes the track, and playback never looks at it.
//! Both the chant staff of the engraver and the modern-to-chant conversion build on
//! [`ChantView`], so they show the same syllables, neumes and divisions.

use std::collections::HashMap;
use std::fmt;

use crate::chant::{ChantClef, ChantNote, ClefKind, group_neumes};
use crate::{ChantMarks, Score, TICKS_PER_QUARTER, Track};

/// Which note of a chord the chant staff shows.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ChordPolicy {
    /// The highest note of each chord (the "skyline").
    #[default]
    Top,
    /// The lowest note.
    Bottom,
    /// No chant staff: a chord is an error.
    Error,
}

impl ChordPolicy {
    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "top" => Some(Self::Top),
            "bottom" => Some(Self::Bottom),
            "error" => Some(Self::Error),
            _ => None,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::Top => "top",
            Self::Bottom => "bottom",
            Self::Error => "error",
        }
    }
}

/// A chord the policy refuses. Bar and beat are zero-based.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChordError {
    pub bar: usize,
    pub beat: usize,
}

impl fmt::Display for ChordError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "Chant staff: chord at bar {} beat {} — chant is monophonic",
            self.bar + 1,
            self.beat + 1
        )
    }
}

/// One sung note of the melody: a beat's chosen note, with the tied beats that go on sounding it.
#[derive(Debug, Clone, PartialEq)]
pub struct MelodyEvent {
    /// Position of the beat (first voice) and of the beats tied on to it.
    pub bar: usize,
    pub beat: usize,
    pub continued: Vec<(usize, usize)>,
    /// MIDI pitch of the chosen note and its string (the note's identity within the beat).
    pub pitch: u8,
    pub string: u8,
    pub ticks: u32,
    pub lyric: Option<String>,
    /// Quarter-note group, used to form syllables when there are no lyrics.
    pub quarter: u32,
    /// Division after the note: `,` `;` `:` `::`.
    pub division: Option<String>,
    /// Chant signs kept with the note.
    pub marks: Option<ChantMarks>,
}

impl MelodyEvent {
    /// The last beat this note sounds on.
    pub fn last_beat(&self) -> (usize, usize) {
        self.continued
            .last()
            .copied()
            .unwrap_or((self.bar, self.beat))
    }
}

/// Index of the note of a beat the chant staff shows (the lowest under [`ChordPolicy::Bottom`],
/// else the highest; the first note of a chord when the policy would refuse it).
pub fn pick(track: &Track, beat: &crate::Beat, policy: ChordPolicy) -> Option<usize> {
    let pitch = |i: &usize| track.pitch_of(&beat.notes[*i]);
    let all = 0..beat.notes.len();
    match policy {
        ChordPolicy::Bottom => all.min_by_key(pitch),
        ChordPolicy::Top => all.max_by_key(pitch),
        ChordPolicy::Error => (!beat.notes.is_empty()).then_some(0),
    }
}

/// The notes of a track's first voice as a single line: a rest closes a phrase (a division of a
/// comma for a short rest, a colon from a quarter on), so does a double bar (a colon), a beat's
/// own chant division wins over those, and the end of the piece is a final division.
pub fn melody(
    score: &Score,
    track: &Track,
    policy: ChordPolicy,
) -> Result<Vec<MelodyEvent>, ChordError> {
    let mut events: Vec<MelodyEvent> = Vec::new();
    let mut tick_total = 0u32;
    for (bi, bar) in track.bars.iter().enumerate() {
        let mut tick = 0;
        let beats = bar.voices.first().map(|v| v.beats.as_slice());
        for (k, beat) in beats.unwrap_or_default().iter().enumerate() {
            if beat.empty {
                continue;
            }
            if beat.notes.len() > 1 && policy == ChordPolicy::Error {
                return Err(ChordError { bar: bi, beat: k });
            }
            let ticks = beat.ticks();
            if beat.notes.is_empty() {
                // A rest closes a phrase.
                if let Some(last) = events.last_mut() {
                    let d = if ticks >= TICKS_PER_QUARTER { ":" } else { "," };
                    if last.division.as_deref().is_none_or(|x| x == ",") {
                        last.division = Some(d.into());
                    }
                }
            } else if beat.notes.iter().all(|n| n.tie) && !events.is_empty() {
                let last = events.last_mut().unwrap();
                last.ticks += ticks;
                last.continued.push((bi, k));
            } else {
                let chosen = &beat.notes[pick(track, beat, policy).expect("a beat with notes")];
                events.push(MelodyEvent {
                    bar: bi,
                    beat: k,
                    continued: Vec::new(),
                    pitch: track.pitch_of(chosen),
                    string: chosen.string,
                    ticks,
                    lyric: beat
                        .lyrics
                        .first()
                        .filter(|l| !l.trim().is_empty())
                        .cloned(),
                    quarter: (tick_total + tick) / TICKS_PER_QUARTER,
                    division: None,
                    marks: chosen.chant.as_deref().cloned(),
                });
            }
            if let (Some(d), Some(last)) = (&beat.chant_division, events.last_mut()) {
                last.division = Some(d.clone());
            }
            tick += ticks;
        }
        tick_total += score.master_bars.get(bi).map_or(tick, |m| m.ticks());
        if score.master_bars.get(bi).is_some_and(|m| m.double_bar) {
            if let Some(last) = events.last_mut() {
                last.division = Some(":".into());
            }
        }
    }
    if let Some(last) = events.last_mut() {
        last.division = Some("::".into());
    }
    Ok(events)
}

/// Whether each note starts a syllable: at each lyric, or without lyrics at each quarter note.
pub fn syllable_starts(events: &[MelodyEvent]) -> Vec<bool> {
    let has_lyrics = events.iter().any(|e| e.lyric.is_some());
    let mut last_group = None;
    events
        .iter()
        .enumerate()
        .map(|(i, e)| {
            let starts = if has_lyrics {
                e.lyric.is_some() || i == 0
            } else {
                last_group != Some(e.quarter)
            };
            last_group = Some(e.quarter);
            starts
        })
        .collect()
}

const DIATONIC: [i32; 7] = [0, 2, 4, 5, 7, 9, 11];

/// The clef and shift (in semitones) that put a melody on the white-key scale (B♭ allowed) and
/// keep it on the four-line staff.
pub fn fit(pitches: &[u8], family: Option<ClefKind>) -> (ChantClef, i32) {
    let cost = |shift: i32| -> f64 {
        pitches
            .iter()
            .map(|&p| {
                let pc = (p as i32 + shift).rem_euclid(12);
                if DIATONIC.contains(&pc) {
                    0.0
                } else if pc == 10 {
                    0.3
                } else {
                    1.0
                }
            })
            .sum::<f64>()
            + shift.abs() as f64 * 0.01
    };
    let shift = (-6..=5)
        .min_by(|a, b| cost(*a).total_cmp(&cost(*b)))
        .unwrap_or(0);
    let (clef, octave) = ChantClef::fitting(
        &pitches
            .iter()
            .map(|&p| (p as i32 + shift).clamp(0, 127) as u8)
            .collect::<Vec<_>>(),
        family,
    );
    (clef, shift + octave)
}

/// A melody laid out as chant: the clef, each note's staff position and its neume.
#[derive(Debug, Clone)]
pub struct ChantView {
    pub clef: ChantClef,
    /// Semitones the melody is moved to fit the scale and staff.
    pub shift: i32,
    pub events: Vec<MelodyEvent>,
    /// Each event as a chant note (staff position, signs, a mora on long notes).
    pub notes: Vec<ChantNote>,
    /// Alteration of each event's position: -1 flat, 1 a chromatic note placed below its pitch.
    pub alters: Vec<i32>,
    /// Neumes as indices of events. Neumes never span a syllable, division or `neume_start`.
    pub neumes: Vec<Vec<usize>>,
    /// Syllables (as indices of their first event) each event belongs to.
    pub syllable_of: Vec<usize>,
    by_beat: HashMap<(usize, usize), (usize, bool)>,
}

impl ChantView {
    pub fn build(score: &Score, track: &Track, policy: ChordPolicy) -> Result<Self, ChordError> {
        let events = melody(score, track, policy)?;
        Ok(Self::of(events, track.chant_family()))
    }

    /// A view of `events` on a clef of the given family (the best fitting one if `None`).
    pub fn of(events: Vec<MelodyEvent>, family: Option<ClefKind>) -> Self {
        let pitches: Vec<u8> = events.iter().map(|e| e.pitch).collect();
        let (clef, shift) = fit(&pitches, family);
        // A typical note length: longer notes get a mora.
        let mut lengths: Vec<u32> = events.iter().map(|e| e.ticks).collect();
        lengths.sort_unstable();
        let base = lengths.get(lengths.len() / 2).copied().unwrap_or(1).max(1);
        let mut notes = Vec::new();
        let mut alters = Vec::new();
        for e in &events {
            let (pos, alter) = clef.place(e.pitch, shift);
            let mut note = ChantNote::new(pos.clamp(0, 13) as u8);
            if let Some(m) = &e.marks {
                note.inclinatum = m.inclinatum;
                note.initio_debilis = m.initio_debilis;
                note.mods = m.mods.clone();
            }
            if e.ticks * 4 >= base * 7 && !note.mods.iter().any(|m| m.starts_with('.')) {
                note.mods.push(".".into());
            }
            notes.push(note);
            alters.push(if alter == 1 { 0 } else { alter });
        }
        // Runs of notes between syllable starts and divisions, cut into neumes.
        let starts = syllable_starts(&events);
        let has_lyrics = events.iter().any(|e| e.lyric.is_some());
        let mut neumes: Vec<Vec<usize>> = Vec::new();
        let mut syllable_of = Vec::new();
        let mut run: Vec<usize> = Vec::new();
        let mut syllable = 0;
        let mut climaci: Vec<Vec<usize>> = Vec::new();
        let mut flush = |run: &mut Vec<usize>, neumes: &mut Vec<Vec<usize>>| {
            let mut part: Vec<usize> = Vec::new();
            let mut parts: Vec<Vec<usize>> = Vec::new();
            for &i in run.iter() {
                let forced = events[i]
                    .marks
                    .as_ref()
                    .is_some_and(|m| m.neume_start || (m.punctum && !has_lyrics));
                if forced && !part.is_empty() {
                    parts.push(std::mem::take(&mut part));
                }
                part.push(i);
            }
            parts.extend((!part.is_empty()).then_some(part));
            for p in parts {
                // Falling stretches of three or more plain notes are climaci; the rest group
                // by contour.
                let plain = |i: usize| {
                    !notes[i].inclinatum && notes[i].mods.iter().all(|m| m.starts_with('.'))
                };
                let mut rest: Vec<usize> = Vec::new();
                let mut k = 0;
                let drain = |rest: &mut Vec<usize>, neumes: &mut Vec<Vec<usize>>| {
                    let notes_of: Vec<(usize, &ChantNote)> =
                        rest.iter().map(|&i| (i, &notes[i])).collect();
                    for g in group_neumes(&notes_of) {
                        neumes.push(g.iter().map(|&(i, _)| i).collect());
                    }
                    rest.clear();
                };
                while k < p.len() {
                    let mut e = k;
                    while e + 1 < p.len()
                        && plain(p[e])
                        && plain(p[e + 1])
                        && notes[p[e + 1]].pitch < notes[p[e]].pitch
                    {
                        e += 1;
                    }
                    if e - k >= 2 {
                        drain(&mut rest, neumes);
                        neumes.push(p[k..=e].to_vec());
                        climaci.push(p[k..=e].to_vec());
                        k = e + 1;
                    } else {
                        rest.push(p[k]);
                        k += 1;
                    }
                }
                drain(&mut rest, neumes);
            }
            run.clear();
        };
        for (i, e) in events.iter().enumerate() {
            if starts[i] {
                flush(&mut run, &mut neumes);
                syllable = i;
            }
            syllable_of.push(syllable);
            run.push(i);
            if e.division.is_some() {
                flush(&mut run, &mut neumes);
            }
        }
        flush(&mut run, &mut neumes);
        // A climacus: a virga and then puncta inclinata, as they are written.
        for g in &climaci {
            notes[g[0]].mods.insert(0, "v".into());
            for &i in &g[1..] {
                notes[i].inclinatum = true;
            }
        }
        let mut by_beat = HashMap::new();
        for (i, e) in events.iter().enumerate() {
            by_beat.insert((e.bar, e.beat), (i, true));
            for &c in &e.continued {
                by_beat.insert(c, (i, false));
            }
        }
        Self {
            clef,
            shift,
            events,
            notes,
            alters,
            neumes,
            syllable_of,
            by_beat,
        }
    }

    /// The event a beat belongs to, and whether the beat is where it starts (the rest of its
    /// tied beats only sound it on).
    pub fn at(&self, bar: usize, beat: usize) -> Option<(usize, bool)> {
        self.by_beat.get(&(bar, beat)).copied()
    }

    /// Position of an event in its neume: `(neume, index, length)`.
    pub fn neume_of(&self, event: usize) -> Option<(usize, usize, usize)> {
        self.neumes
            .iter()
            .enumerate()
            .find_map(|(n, g)| g.iter().position(|&e| e == event).map(|i| (n, i, g.len())))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Beat, Duration, DurationValue, Note, Track};

    fn track(bars: Vec<Vec<Vec<u8>>>) -> (Score, usize) {
        let mut score = Score::default();
        let mut t = Track {
            kind: crate::TrackKind::Pitched,
            ..Track::default()
        };
        for bar in bars {
            let mut b = crate::Bar::default();
            let mut v = crate::Voice::default();
            for notes in bar {
                let mut beat = Beat::rest(Duration::new(DurationValue::Quarter));
                for p in notes {
                    beat.notes.push(Note::new(0, p));
                }
                v.beats.push(beat);
            }
            b.voices.push(v);
            t.bars.push(b);
            score.master_bars.push(crate::MasterBar::default());
        }
        score.tracks.push(t);
        (score, 0)
    }

    #[test]
    fn chords_follow_the_policy() {
        let (s, i) = track(vec![vec![vec![60, 64, 67], vec![62]]]);
        let t = &s.tracks[i];
        assert_eq!(melody(&s, t, ChordPolicy::Top).unwrap()[0].pitch, 67);
        assert_eq!(melody(&s, t, ChordPolicy::Bottom).unwrap()[0].pitch, 60);
        let e = melody(&s, t, ChordPolicy::Error).unwrap_err();
        assert_eq!((e.bar, e.beat), (0, 0));
        assert!(e.to_string().contains("bar 1 beat 1"));
    }

    #[test]
    fn rests_and_bars_make_divisions() {
        let (mut s, i) = track(vec![vec![vec![60], vec![], vec![62]], vec![vec![64]]]);
        s.master_bars[0].double_bar = true;
        let ev = melody(&s, &s.tracks[i], ChordPolicy::Top).unwrap();
        // A quarter rest is a colon; the double bar after the last note of the bar too.
        assert_eq!(ev[0].division.as_deref(), Some(":"));
        assert_eq!(ev[1].division.as_deref(), Some(":"));
        assert_eq!(ev[2].division.as_deref(), Some("::"));
        s.tracks[i].bars[0].voices[0].beats[0].chant_division = Some(";".into());
        s.master_bars[0].double_bar = false;
        let ev = melody(&s, &s.tracks[i], ChordPolicy::Top).unwrap();
        assert_eq!(ev[0].division.as_deref(), Some(";"));
    }

    #[test]
    fn neumes_group_by_contour() {
        // One quarter note per syllable group: all four notes in one beat group is not
        // possible, so use eighths.
        let (mut s, i) = track(vec![vec![vec![72], vec![74], vec![72], vec![79]]]);
        for b in &mut s.tracks[i].bars[0].voices[0].beats {
            b.duration = Duration::new(DurationValue::Eighth);
        }
        let v = ChantView::build(&s, &s.tracks[i], ChordPolicy::Top).unwrap();
        // Two quarter beats: two syllables.
        assert_eq!(v.syllable_of, vec![0, 0, 2, 2]);
        assert_eq!(v.neumes.iter().map(Vec::len).sum::<usize>(), 4);
        s.tracks[i].bars[0].voices[0].beats[1].notes[0].chant = Some(Box::new(ChantMarks {
            neume_start: true,
            ..Default::default()
        }));
        let w = ChantView::build(&s, &s.tracks[i], ChordPolicy::Top).unwrap();
        assert!(w.neumes.iter().any(|n| n[0] == 1));
    }

    /// Punctums entered one by one are separate neumes until lyrics exist; then each syllable
    /// condenses its notes by contour (pes, clivis, torculus), whatever the flags say.
    #[test]
    fn lyrics_condense_punctums_into_neumes() {
        let shapes: [(&[u8], usize); 4] = [
            (&[72, 74], 2),
            (&[74, 72], 2),
            (&[72, 74, 72], 3),
            (&[72, 74, 72, 74], 4),
        ];
        for (pitches, first) in shapes {
            let rows: Vec<Vec<u8>> = pitches.iter().map(|&p| vec![p]).collect();
            let n = pitches.len();
            let (mut s2, i2) = track(vec![rows]);
            for b in &mut s2.tracks[i2].bars[0].voices[0].beats {
                b.duration = Duration::new(DurationValue::Eighth);
                b.notes[0].chant = Some(Box::new(ChantMarks {
                    punctum: true,
                    ..Default::default()
                }));
            }
            // No lyrics: a neume per punctum.
            let v = ChantView::build(&s2, &s2.tracks[i2], ChordPolicy::Top).unwrap();
            assert_eq!(v.neumes.len(), n, "{pitches:?}");
            // One syllable over the lot: the neumes follow the contour.
            s2.tracks[i2].bars[0].voices[0].beats[0].lyrics = vec!["Ky".into()];
            let v = ChantView::build(&s2, &s2.tracks[i2], ChordPolicy::Top).unwrap();
            assert_eq!(v.neumes[0].len(), first, "{pitches:?}: {:?}", v.neumes);
            // An explicit neume start still splits.
            s2.tracks[i2].bars[0].voices[0].beats[1].notes[0]
                .chant
                .as_mut()
                .unwrap()
                .neume_start = true;
            let v = ChantView::build(&s2, &s2.tracks[i2], ChordPolicy::Top).unwrap();
            assert_eq!(v.neumes[0].len(), 1, "{pitches:?}");
        }
    }
}
