//! Chant ↔ modern notation. Chant to modern is the transcription in `cecilia_core::chant`; this
//! module turns a modern melody into chant (always lossy). A chant is edited through the
//! operations on `cecilia_core::chant::Chant`, never through its transcription.

use cecilia_core::chant::{Chant, NeumeItem, Syllable};
use cecilia_core::melody::{ChantView, ChordPolicy, syllable_starts};
use cecilia_core::{Score, TrackKind};

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
    /// For every bar of the modern score, the chant note (reading order) of its first sounded
    /// note, or none for the bars after the last note.
    pub bar_notes: Vec<Option<usize>>,
}

impl ToChant {
    /// The audio sync of a modern score as points on chant notes: `(note, frame)` for each
    /// first-time bar point, on the first note sounded at or after that bar. Later points on
    /// the same note are dropped.
    pub fn note_points(&self, points: &[cecilia_core::SyncPoint]) -> Vec<(usize, i64)> {
        let mut out: Vec<(usize, i64)> = Vec::new();
        for p in points.iter().filter(|p| p.occurrence == 0) {
            if let Some(Some(k)) = self.bar_notes.get(p.bar) {
                if !out.iter().any(|o| o.0 == *k) {
                    out.push((*k, p.frame));
                }
            }
        }
        out
    }
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
    let bar_notes = (0..score.master_bars.len())
        .map(|b| view.events.iter().position(|e| e.bar >= b))
        .collect();
    Ok(ToChant {
        chant,
        transpose,
        rounded,
        chords,
        bar_notes,
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

#[cfg(test)]
mod tests {
    use super::*;
    use cecilia_core::chant::ChantNote;
    use cecilia_core::*;

    fn pitches(c: &Chant) -> Vec<u8> {
        c.resolved().iter().map(|r| r.midi).collect()
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
            Some(cecilia_core::chant::Division::Finalis)
        );
    }

    /// Modern-to-chant groups notes into neumes
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
                for g in cecilia_core::chant::group_neumes(run) {
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
