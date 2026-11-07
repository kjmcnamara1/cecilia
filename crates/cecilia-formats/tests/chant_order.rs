//! Pins the definition of a chant's "note index": the order of `Chant::resolved`, which is the
//! order `transcribe` makes beats in, on the GABC files of `test-corpus/` (a local, gitignored
//! directory; without it the test has nothing to check and passes).

mod common;

use cecilia_core::chant::{NeumeItem, transcribe};

#[test]
fn note_order_is_the_order_of_the_beats() {
    for path in common::corpus_files(&["gabc"]) {
        let name = path.display().to_string();
        let text = std::fs::read_to_string(&path).unwrap();
        let Ok(chant) = cecilia_formats::gabc::parse(&text) else {
            continue;
        };
        let resolved = chant.resolved();
        let t = transcribe(&chant);

        // One note per written note, in the order the syllables list them.
        let written: Vec<(usize, usize)> = chant
            .syllables
            .iter()
            .enumerate()
            .flat_map(|(si, s)| {
                s.items
                    .iter()
                    .enumerate()
                    .filter(|(_, i)| matches!(i, NeumeItem::Note(_)))
                    .map(move |(ii, _)| (si, ii))
            })
            .collect();
        assert_eq!(chant.note_count(), written.len(), "{name}");
        assert_eq!(resolved.len(), written.len(), "{name}");
        assert_eq!(t.positions.len(), written.len(), "{name}");
        for (n, r) in resolved.iter().enumerate() {
            assert_eq!((r.syllable, r.item), written[n], "{name}: note {n}");
            assert_eq!(chant.note_place(n), Some(written[n]), "{name}");
            assert_eq!(chant.note_index(r.syllable, r.item), Some(n), "{name}");
        }

        // The beats follow: note n starts at positions[n], in increasing order, and lasts
        // lengths[n] beats; together they cover every beat of every bar exactly once.
        assert!(t.positions.windows(2).all(|w| w[0] < w[1]), "{name}");
        // The cells (notes, empty groups, divisions of their own) cover every beat once.
        let mut next = (0, 0);
        for (k, c) in t.cells.iter().enumerate() {
            let in_bar = |b: usize| t.bars[b].voices[0].beats.len();
            if (c.bar, c.beat) != next {
                // A new bar starts where the previous one ended.
                assert_eq!((c.bar, c.beat), (next.0 + 1, 0), "{name}: cell {k}");
                assert_eq!(next.1, in_bar(next.0), "{name}: cell {k}");
            }
            assert_eq!(t.cell_at(c.bar, c.beat), Some(k), "{name}: cell {k}");
            next = (c.bar, c.beat + c.len);
        }
        for (n, r) in resolved.iter().enumerate() {
            let (bar, beat) = t.place_of(n).unwrap();
            for k in 0..t.lengths[n] as usize {
                assert_eq!(t.note_at(bar, beat + k), Some(n), "{name}: note {n}");
            }

            // The beat's pitch is the note's resolved pitch, its lyric the syllable's text.
            let first = &t.bars[bar].voices[0].beats[beat];
            assert_eq!(first.notes[0].fret, r.midi, "{name}: note {n}");
            let syl = &chant.syllables[r.syllable];
            let first_in_syllable = syl.notes().next().is_some()
                && chant.note_index(r.syllable, r.item)
                    == chant.note_index(
                        r.syllable,
                        syl.items
                            .iter()
                            .position(|i| matches!(i, NeumeItem::Note(_)))
                            .unwrap(),
                    );
            let text = syl.plain_text();
            if first_in_syllable && !text.trim().is_empty() {
                assert!(
                    first
                        .lyrics
                        .first()
                        .is_some_and(|l| l.starts_with(text.trim())),
                    "{name}: note {n}"
                );
            }
        }
        let total: usize = t.bars.iter().map(|b| b.voices[0].beats.len()).sum();
        let cells: usize = t.cells.iter().map(|c| c.len).sum();
        assert_eq!(total, cells, "{name}");
        let sounded: usize = t.lengths.iter().map(|&l| l as usize).sum();
        let silent = t
            .bars
            .iter()
            .flat_map(|b| &b.voices[0].beats)
            .filter(|b| b.empty)
            .count();
        assert_eq!(total, sounded + silent, "{name}");
    }
}
