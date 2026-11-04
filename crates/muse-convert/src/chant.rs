//! Chant ↔ modern notation. Chant to modern is the transcription in `muse_core::chant`; this
//! module turns a modern melody into chant (always lossy) and carries edits made to a chant
//! track's transcription back into its chant notation.

use muse_core::chant::{Chant, ChantClef, ChantNote, NeumeItem, Syllable, transcribe};
use muse_core::melody::{ChantView, ChordPolicy, syllable_starts};
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

/// Converts a track's melody to chant: syllables follow its lyrics (or quarter-note groups
/// without lyrics), the melody moves to the white-key scale with B♭, and a clef is chosen to
/// keep it on the staff. Chords are reduced as `policy` says (an error under
/// [`ChordPolicy::Error`]). The chant staff of the engraver shows the same view
/// ([`ChantView`]).
pub fn modern_to_chant(
    score: &Score,
    track: usize,
    policy: ChordPolicy,
) -> Result<ToChant, String> {
    let t = score.tracks.get(track).ok_or("no such track")?;
    if t.kind == TrackKind::Drums {
        return Err("drum tracks cannot become chant".into());
    }
    let view = ChantView::build(score, t, policy).map_err(|e| e.to_string())?;
    if view.events.is_empty() {
        return Err("the track has no notes".into());
    }
    let chords = t
        .bars
        .iter()
        .filter_map(|b| b.voices.first())
        .flat_map(|v| &v.beats)
        .filter(|b| !b.empty && b.notes.len() > 1)
        .count();
    let (clef, transpose) = (view.clef, view.shift);
    let starts = syllable_starts(&view.events);

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
    for (k, e) in view.events.iter().enumerate() {
        if starts[k] {
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
        let note = view.notes[k].clone();
        let pos = note.pitch;
        let alter = view.alters[k];
        let (_, exact) = clef.place((e.pitch as i32 + transpose).clamp(0, 127) as u8, 0);
        if exact == 1 {
            rounded += 1;
        }
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
        items.push(NeumeItem::Note(note));
        if let Some(d) = &e.division {
            chant.syllables.push(Syllable {
                text: String::new(),
                word_start: true,
                items: vec![NeumeItem::Bar(d.clone())],
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
                ..Default::default()
            }],
            ..Track::default()
        });
        let r = modern_to_chant(&score, 0, ChordPolicy::Top).unwrap();
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

    /// The chant staff of an ordinary track and modern-to-chant group notes into the same neumes
    /// (pes, clivis, torculus, porrectus, climacus, scandicus, a stropha's repeated notes).
    #[test]
    fn staff_view_and_conversion_share_neumes() {
        // Syllables of eighth notes, a quarter beat (two notes) each, or longer runs with
        // lyrics on their first note.
        let runs: [&[u8]; 7] = [
            &[62, 65],                 // pes
            &[65, 62],                 // clivis
            &[62, 65, 62],             // torculus
            &[65, 62, 65],             // porrectus
            &[62, 64, 67, 69],         // scandicus
            &[64, 64, 64],             // repeated notes
            &[67, 64, 65, 67, 64, 62], // a longer melisma
        ];
        let mut track = Track {
            kind: TrackKind::Pitched,
            ..Track::default()
        };
        let mut lyric = 0;
        for notes in runs {
            let mut voice = Voice::default();
            for (k, &p) in notes.iter().enumerate() {
                let mut beat = Beat::rest(Duration::new(DurationValue::Eighth));
                beat.notes.push(Note::new(0, p));
                if k == 0 {
                    lyric += 1;
                    beat.lyrics.push(format!("s{lyric}"));
                }
                voice.beats.push(beat);
            }
            track.bars.push(Bar {
                voices: vec![voice],
                ..Default::default()
            });
        }
        let score = Score {
            master_bars: vec![MasterBar::default(); runs.len()],
            tracks: vec![track],
            ..Score::default()
        };
        let view = ChantView::build(&score, &score.tracks[0], ChordPolicy::Top).unwrap();
        let converted = modern_to_chant(&score, 0, ChordPolicy::Top).unwrap();
        // The converted chant's neumes: its runs of notes between separators, grouped as the
        // engraver does.
        let mut sizes = Vec::new();
        let mut positions = Vec::new();
        for syllable in &converted.chant.syllables {
            let mut run: Vec<(usize, &ChantNote)> = Vec::new();
            let flush = |run: &mut Vec<(usize, &ChantNote)>, sizes: &mut Vec<usize>| {
                // A climacus (a virga then inclinati) is one neume, drawn from separate notes.
                let mut open = false;
                for g in muse_core::chant::group_neumes(run) {
                    let n = g[0].1;
                    if open && g.len() == 1 && n.inclinatum {
                        *sizes.last_mut().unwrap() += 1;
                    } else {
                        sizes.push(g.len());
                        open = g.len() == 1 && n.mods.iter().any(|m| m == "v");
                    }
                }
                run.clear();
            };
            for (k, item) in syllable.items.iter().enumerate() {
                match item {
                    NeumeItem::Note(n) => {
                        positions.push(n.pitch);
                        run.push((k, n));
                    }
                    _ => flush(&mut run, &mut sizes),
                }
            }
            flush(&mut run, &mut sizes);
        }
        let staff: Vec<usize> = view.neumes.iter().map(Vec::len).collect();
        assert_eq!(sizes, staff);
        assert_eq!(
            positions,
            view.notes.iter().map(|n| n.pitch).collect::<Vec<_>>()
        );
        // Not trivially single notes: the shapes above do group.
        assert!(staff.iter().any(|&n| n >= 3) && staff.contains(&2));
    }

    /// Three or more falling notes in a neume become a climacus: a virga, then inclinati.
    #[test]
    fn falling_runs_become_a_climacus() {
        let mut track = Track {
            kind: TrackKind::Pitched,
            ..Track::default()
        };
        let mut voice = Voice::default();
        for (k, p) in [72u8, 69, 67, 65].into_iter().enumerate() {
            let mut beat = Beat::rest(Duration::new(DurationValue::Eighth));
            beat.notes.push(Note::new(0, p));
            if k == 0 {
                beat.lyrics.push("a".into());
            }
            voice.beats.push(beat);
        }
        track.bars.push(Bar {
            voices: vec![voice],
            ..Default::default()
        });
        let score = Score {
            master_bars: vec![MasterBar::default()],
            tracks: vec![track],
            ..Score::default()
        };
        let view = ChantView::build(&score, &score.tracks[0], ChordPolicy::Top).unwrap();
        assert_eq!(view.neumes, vec![vec![0, 1, 2, 3]]);
        assert!(view.notes[0].mods.contains(&"v".to_string()));
        assert!(view.notes[1..].iter().all(|n| n.inclinatum));
        let gabc = modern_to_chant(&score, 0, ChordPolicy::Top)
            .unwrap()
            .chant
            .syllables
            .iter()
            .map(|s| s.gabc_notes())
            .collect::<String>();
        assert!(
            gabc.contains("v") && gabc.chars().filter(|c| c.is_ascii_uppercase()).count() == 3,
            "{gabc}"
        );
    }
}
