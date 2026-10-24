//! Chant ↔ modern notation. Chant to modern is the transcription in `muse_core::chant`; this
//! module turns a modern melody into chant (always lossy) and carries edits made to a chant
//! track's transcription back into its chant notation.

use muse_core::chant::{Chant, ChantClef, ChantNote, ClefKind, NeumeItem, Syllable, transcribe};
use muse_core::{Bar, Score, TrackKind};

/// The result of [`modern_to_chant`].
#[derive(Debug, Clone)]
pub struct ToChant {
    pub chant: Chant,
    /// Semitones the melody was moved to fit the chant scale and staff.
    pub transpose: i32,
    /// Notes that were not in the scale and were rounded to a neighbour.
    pub rounded: usize,
    /// Chords reduced to their top note.
    pub chords: usize,
}

/// One sounded note of the melody.
struct Event {
    pitch: u8,
    ticks: u32,
    lyric: Option<String>,
    /// Group used to form syllables when there are no lyrics (one per quarter beat).
    beat: u32,
    /// Division to place after this note.
    division: Option<&'static str>,
}

fn melody(score: &Score, track: usize) -> (Vec<Event>, usize) {
    let t = &score.tracks[track];
    let mut events: Vec<Event> = Vec::new();
    let mut chords = 0;
    let mut tick_total = 0u32;
    for (bi, bar) in t.bars.iter().enumerate() {
        let mut tick = 0;
        for beat in bar
            .voices
            .first()
            .map(|v| v.beats.as_slice())
            .unwrap_or_default()
        {
            if beat.empty {
                continue;
            }
            let ticks = beat.ticks();
            if beat.notes.is_empty() {
                // A rest closes a phrase.
                if let Some(last) = events.last_mut() {
                    let d = if ticks >= muse_core::TICKS_PER_QUARTER {
                        ":"
                    } else {
                        ","
                    };
                    if last.division.is_none_or(|x| x == ",") {
                        last.division = Some(d);
                    }
                }
            } else if beat.notes.iter().all(|n| n.tie) && !events.is_empty() {
                events.last_mut().unwrap().ticks += ticks;
            } else {
                if beat.notes.len() > 1 {
                    chords += 1;
                }
                let pitch = beat.notes.iter().map(|n| t.pitch_of(n)).max().unwrap_or(60);
                let lyric = beat
                    .lyrics
                    .first()
                    .filter(|l| !l.trim().is_empty())
                    .cloned();
                events.push(Event {
                    pitch,
                    ticks,
                    lyric,
                    beat: (tick_total + tick) / muse_core::TICKS_PER_QUARTER,
                    division: None,
                });
            }
            tick += ticks;
        }
        tick_total += score.master_bars.get(bi).map_or(tick, |m| m.ticks());
        if score.master_bars.get(bi).is_some_and(|m| m.double_bar) {
            if let Some(last) = events.last_mut() {
                last.division = Some(":");
            }
        }
    }
    if let Some(last) = events.last_mut() {
        last.division = Some("::");
    }
    (events, chords)
}

const DIATONIC: [i32; 7] = [0, 2, 4, 5, 7, 9, 11];

/// Converts a track's melody to chant: syllables follow its lyrics (or quarter-note groups
/// without lyrics), the melody moves to the white-key scale with B♭, and a clef is chosen to
/// keep it on the staff.
pub fn modern_to_chant(score: &Score, track: usize) -> Result<ToChant, String> {
    let t = score.tracks.get(track).ok_or("no such track")?;
    if t.kind == TrackKind::Drums {
        return Err("drum tracks cannot become chant".into());
    }
    let (events, chords) = melody(score, track);
    if events.is_empty() {
        return Err("the track has no notes".into());
    }
    // Transposition that puts most notes on the scale (B♭ allowed).
    let cost = |shift: i32| -> f64 {
        events
            .iter()
            .map(|e| {
                let pc = (e.pitch as i32 + shift).rem_euclid(12);
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
    // Clef and octave that keep the melody on the staff.
    let clefs = [
        ("c4", ClefKind::Do, 4),
        ("c3", ClefKind::Do, 3),
        ("f3", ClefKind::Fa, 3),
        ("c2", ClefKind::Do, 2),
        ("f4", ClefKind::Fa, 4),
        ("c1", ClefKind::Do, 1),
    ];
    let mut best: Option<(f64, ChantClef, i32)> = None;
    for (order, &(_, kind, line)) in clefs.iter().enumerate() {
        let clef = ChantClef {
            kind,
            line,
            flat: false,
        };
        for octave in [-24, -12, 0, 12, 24] {
            let c: f64 = events
                .iter()
                .map(|e| {
                    let p = (e.pitch as i32 + shift + octave).clamp(0, 127) as u8;
                    let (pos, _) = clef.position_of(p, true);
                    if !(0..=12).contains(&pos) {
                        10.0
                    } else if !(2..=10).contains(&pos) {
                        1.0
                    } else {
                        0.0
                    }
                })
                .sum::<f64>()
                + octave.abs() as f64 * 0.02
                + order as f64 * 0.01;
            if best.as_ref().is_none_or(|b| c < b.0) {
                best = Some((c, clef, octave));
            }
        }
    }
    let (_, clef, octave) = best.unwrap();
    let transpose = shift + octave;

    // Typical note length: longer notes get a mora.
    let mut lengths: Vec<u32> = events.iter().map(|e| e.ticks).collect();
    lengths.sort_unstable();
    let base = lengths[lengths.len() / 2].max(1);

    let has_lyrics = events.iter().any(|e| e.lyric.is_some());
    let mut chant = Chant::default();
    if !score.meta.title.is_empty() {
        chant
            .headers
            .push(("name".into(), score.meta.title.clone()));
    }
    chant.syllables.push(Syllable {
        text: String::new(),
        word_start: true,
        items: vec![NeumeItem::Clef(clef)],
    });
    let mut rounded = 0;
    let mut flats: Vec<u8> = Vec::new();
    let mut continues = false;
    let mut last_group = None;
    for e in &events {
        let starts = if has_lyrics {
            e.lyric.is_some() || chant.syllables.len() == 1
        } else {
            last_group != Some(e.beat)
        };
        last_group = Some(e.beat);
        if starts {
            let (text, cont) = match &e.lyric {
                Some(l) => {
                    let l = l.trim();
                    let cont = l.ends_with('-');
                    (l.trim_end_matches('-').trim().to_string(), cont)
                }
                None => (String::new(), false),
            };
            let word_start = !continues;
            if word_start {
                flats.clear();
            }
            chant.syllables.push(Syllable {
                text: escape(&text),
                word_start,
                items: Vec::new(),
            });
            continues = cont;
        }
        let pitch = (e.pitch as i32 + transpose).clamp(0, 127) as u8;
        let (pos, alter) = clef.position_of(pitch, true);
        if clef.midi(pos, alter) != pitch {
            rounded += 1;
        }
        let pos = pos.clamp(0, 13) as u8;
        let items = &mut chant.syllables.last_mut().unwrap().items;
        if alter == -1 && !flats.contains(&pos) {
            items.push(NeumeItem::Alteration {
                pitch: pos,
                sign: "x".into(),
            });
            flats.push(pos);
        } else if alter == 0 && flats.contains(&pos) {
            items.push(NeumeItem::Alteration {
                pitch: pos,
                sign: "y".into(),
            });
            flats.retain(|&p| p != pos);
        }
        let mut note = ChantNote::new(pos);
        if e.ticks * 4 >= base * 7 {
            note.mods.push(".".into());
        }
        items.push(NeumeItem::Note(note));
        if let Some(d) = e.division {
            chant.syllables.push(Syllable {
                text: String::new(),
                word_start: true,
                items: vec![NeumeItem::Bar(d.into())],
            });
            flats.clear();
            continues = false;
        }
    }
    Ok(ToChant {
        chant,
        transpose,
        rounded,
        chords,
    })
}

/// Escapes characters that are syntax in GABC text.
fn escape(text: &str) -> String {
    text.chars()
        .flat_map(|c| match c {
            '(' | ')' | '%' | '<' | '>' | '{' | '}' | '$' => vec!['$', c],
            c => vec![c],
        })
        .collect()
}

/// The first sounded pitch of each beat in a track's bars.
fn beat_pitches(bars: &[Bar]) -> Vec<u8> {
    bars.iter()
        .flat_map(|b| {
            b.voices
                .first()
                .map(|v| v.beats.as_slice())
                .unwrap_or_default()
        })
        .filter(|b| !b.empty && !b.notes.is_empty())
        .map(|b| b.notes.iter().map(|n| n.fret).max().unwrap_or(0))
        .collect()
}

/// Carries edits made to a chant track's bars (its transcription) back into the chant: changed
/// pitches move notes on the staff (adding flats or naturals as needed), deleted notes are
/// removed and new notes are added as puncta to the syllable before them.
pub fn reconcile(chant: &mut Chant, bars: &[Bar]) {
    let old_t = transcribe(chant);
    let old = beat_pitches(&old_t.bars);
    let new = beat_pitches(bars);
    if old == new {
        return;
    }
    let resolved = chant.resolved();
    // The chant note behind each old beat.
    let mut owner = Vec::new();
    for (k, r) in resolved.iter().enumerate() {
        let repeats = match &chant.syllables[r.syllable].items[r.item] {
            NeumeItem::Note(n) => n.repeats(),
            _ => 1,
        };
        for _ in 0..repeats {
            owner.push(k);
        }
    }
    owner.truncate(old.len());
    let ops = diff(&old, &new);
    enum Change {
        Pitch(usize, u8, bool),
        Delete(usize),
        Insert(Option<usize>, u8),
    }
    let mut changes = Vec::new();
    let mut last_old: Option<usize> = None;
    let (mut i, mut j) = (0, 0);
    for op in ops {
        match op {
            Op::Keep => {
                last_old = Some(owner[i]);
                i += 1;
                j += 1;
            }
            Op::Replace => {
                changes.push(Change::Pitch(owner[i], new[j], new[j] > old[i]));
                last_old = Some(owner[i]);
                i += 1;
                j += 1;
            }
            Op::Delete => {
                changes.push(Change::Delete(owner[i]));
                i += 1;
            }
            Op::Insert => {
                changes.push(Change::Insert(last_old, new[j]));
                j += 1;
            }
        }
    }
    // Apply from the end so item indices stay valid.
    changes.sort_by_key(|c| {
        std::cmp::Reverse(match c {
            Change::Pitch(k, ..) | Change::Delete(k) => (*k as i64) * 2,
            Change::Insert(k, _) => k.map_or(-1, |k| k as i64 * 2 + 1),
        })
    });
    let mut deleted = std::collections::HashSet::new();
    for c in changes {
        match c {
            Change::Pitch(k, midi, up) => {
                if deleted.contains(&k) {
                    continue;
                }
                let r = resolved[k];
                set_pitch(chant, r.syllable, r.item, r.clef, midi, up);
            }
            Change::Delete(k) => {
                if deleted.insert(k) {
                    let r = resolved[k];
                    chant.syllables[r.syllable].items.remove(r.item);
                }
            }
            Change::Insert(after, midi) => {
                let (si, ii, clef) = match after {
                    Some(k) => (resolved[k].syllable, resolved[k].item + 1, resolved[k].clef),
                    None => {
                        let si = chant
                            .syllables
                            .iter()
                            .position(|s| s.notes().next().is_some())
                            .unwrap_or(chant.syllables.len().saturating_sub(1));
                        let ii = chant.syllables.get(si).map_or(0, |s| {
                            s.items
                                .iter()
                                .position(|i| matches!(i, NeumeItem::Note(_)))
                                .unwrap_or(s.items.len())
                        });
                        (si, ii, chant.initial_clef())
                    }
                };
                if chant.syllables.is_empty() {
                    chant.syllables.push(Syllable {
                        word_start: true,
                        ..Syllable::default()
                    });
                }
                let (pos, _) = clef.position_of(midi, true);
                chant.syllables[si]
                    .items
                    .insert(ii, NeumeItem::Note(ChantNote::new(pos.clamp(0, 13) as u8)));
                set_pitch(chant, si, ii, clef, midi, true);
            }
        }
    }
}

/// Moves a note to the staff position of a MIDI pitch, adding a flat or natural before it when
/// the alteration in force differs.
fn set_pitch(chant: &mut Chant, si: usize, ii: usize, clef: ChantClef, midi: u8, up: bool) {
    let (pos, alter) = clef.position_of(midi, up);
    let pos = pos.clamp(0, 13) as u8;
    if let NeumeItem::Note(n) = &mut chant.syllables[si].items[ii] {
        n.pitch = pos;
    }
    // What the note now sounds like with the alterations already in force.
    let sounding = chant
        .resolved()
        .into_iter()
        .find(|r| r.syllable == si && r.item == ii)
        .map(|r| r.midi);
    let natural = clef.midi(pos as i32, 0);
    let wanted = clef.midi(pos as i32, alter);
    if sounding != Some(wanted) {
        let sign = if wanted < natural {
            "x"
        } else if wanted > natural {
            "#"
        } else {
            "y"
        };
        chant.syllables[si].items.insert(
            ii,
            NeumeItem::Alteration {
                pitch: pos,
                sign: sign.into(),
            },
        );
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Op {
    Keep,
    Replace,
    Delete,
    Insert,
}

/// Edit script from `a` to `b` (Levenshtein, preferring replacements).
fn diff(a: &[u8], b: &[u8]) -> Vec<Op> {
    let (n, m) = (a.len(), b.len());
    let mut d = vec![vec![0u32; m + 1]; n + 1];
    for (i, row) in d.iter_mut().enumerate() {
        row[0] = i as u32;
    }
    for (j, cell) in d[0].iter_mut().enumerate() {
        *cell = j as u32;
    }
    for i in 1..=n {
        for j in 1..=m {
            let sub = d[i - 1][j - 1] + u32::from(a[i - 1] != b[j - 1]);
            d[i][j] = sub.min(d[i - 1][j] + 1).min(d[i][j - 1] + 1);
        }
    }
    let mut ops = Vec::new();
    let (mut i, mut j) = (n, m);
    while i > 0 || j > 0 {
        if i > 0 && j > 0 && d[i][j] == d[i - 1][j - 1] + u32::from(a[i - 1] != b[j - 1]) {
            ops.push(if a[i - 1] == b[j - 1] {
                Op::Keep
            } else {
                Op::Replace
            });
            i -= 1;
            j -= 1;
        } else if i > 0 && d[i][j] == d[i - 1][j] + 1 {
            ops.push(Op::Delete);
            i -= 1;
        } else {
            ops.push(Op::Insert);
            j -= 1;
        }
    }
    ops.reverse();
    ops
}

#[cfg(test)]
mod tests {
    use super::*;
    use muse_core::*;

    fn chant() -> Chant {
        let mut c = Chant::default();
        c.syllables.push(Syllable {
            text: String::new(),
            word_start: true,
            items: vec![NeumeItem::Clef(ChantClef::default())],
        });
        c.syllables.push(Syllable {
            text: "Al".into(),
            word_start: true,
            items: vec![
                NeumeItem::Note(ChantNote::new(5)),
                NeumeItem::Note(ChantNote::new(6)),
            ],
        });
        c.syllables.push(Syllable {
            text: "le".into(),
            word_start: false,
            items: vec![NeumeItem::Note(ChantNote::new(7))],
        });
        c
    }

    fn pitches(c: &Chant) -> Vec<u8> {
        c.resolved().iter().map(|r| r.midi).collect()
    }

    #[test]
    fn edits_flow_back_into_chant() {
        let mut c = chant();
        assert_eq!(pitches(&c), [65, 67, 69]);
        // Raise the second note a semitone: G → A♭ is not in the scale; up rounds to A.
        let mut bars = transcribe(&c).bars;
        bars[0].voices[0].beats[1].notes[0].fret = 70;
        reconcile(&mut c, &bars);
        assert_eq!(pitches(&c), [65, 70, 69]); // B♭ via a flat on i
        assert!(matches!(
            c.syllables[1].items[1],
            NeumeItem::Alteration { pitch: 8, .. }
        ));
        // Delete the first note, insert one after the last.
        let mut bars = transcribe(&c).bars;
        let beats = &mut bars[0].voices[0].beats;
        beats.remove(0);
        let mut extra = beats[1].clone();
        extra.notes[0].fret = 72;
        extra.lyrics.clear();
        beats.push(extra);
        reconcile(&mut c, &bars);
        assert_eq!(pitches(&c), [70, 69, 72]);
        assert_eq!(c.syllables[2].notes().count(), 2);
    }

    #[test]
    fn melody_becomes_chant() {
        let mut score = Score {
            tempo: 90,
            ..Score::default()
        };
        score.master_bars.push(MasterBar::default());
        let mut beats = Vec::new();
        // F♯ G♯ A B sung to "Ky-ri-e": the melody is moved onto the chant scale unchanged in
        // shape.
        for (p, lyric) in [(66, "Ky-"), (68, ""), (69, "ri-"), (71, "e")] {
            beats.push(Beat {
                duration: Duration::new(DurationValue::Quarter),
                notes: vec![Note::new(0, p)],
                lyrics: if lyric.is_empty() {
                    vec![]
                } else {
                    vec![lyric.into()]
                },
                ..Beat::default()
            });
        }
        score.tracks.push(Track {
            kind: TrackKind::Pitched,
            bars: vec![Bar {
                voices: vec![Voice { beats }],
            }],
            ..Track::default()
        });
        let r = modern_to_chant(&score, 0).unwrap();
        assert_eq!(r.rounded, 0);
        let texts: Vec<_> = r
            .chant
            .syllables
            .iter()
            .map(|s| (s.text.as_str(), s.word_start))
            .collect();
        assert_eq!(
            texts,
            [
                ("", true),
                ("Ky", true),
                ("ri", false),
                ("e", false),
                ("", true)
            ]
        );
        let p = pitches(&r.chant);
        let original = [66, 68, 69, 71];
        for (a, b) in p.iter().zip(original) {
            assert_eq!(*a as i32 - b, r.transpose);
        }
        assert_eq!(r.chant.syllables[1].notes().count(), 2);
        assert_eq!(
            r.chant.syllables.last().unwrap().division(),
            Some(muse_core::chant::Division::Finalis)
        );
    }
}
