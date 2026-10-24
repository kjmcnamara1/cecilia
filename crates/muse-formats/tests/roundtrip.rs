//! Round-trips corpus files through the writers and compares the models.

mod common;

use common::corpus_files;
use muse_core::*;

/// Removes differences that GP5 cannot represent or that do not matter.
fn normalize_gp5(s: &mut Score) {
    s.extras.clear();
    if s.lyrics.as_ref().is_some_and(|l| l.lines.is_empty()) {
        s.lyrics = None;
    }
    let q = |v: u8| ((v as u32 + 4) / 8 * 8).min(127) as u8;
    for t in &mut s.tracks {
        t.extras.clear();
        t.midi.channel = 0;
        t.midi.effect_channel = 0;
        t.midi.volume = q(t.midi.volume);
        t.midi.pan = q(t.midi.pan);
        let strings = t.strings.len();
        for bar in &mut t.bars {
            bar.voices.truncate(2);
            while bar
                .voices
                .last()
                .is_some_and(|v| v.beats.iter().all(|b| b.empty))
            {
                bar.voices.pop();
            }
            for beat in bar.voices.iter_mut().flat_map(|v| &mut v.beats) {
                if let Some(c) = &mut beat.chord {
                    c.frets = (0..strings)
                        .map(|i| c.frets.get(i).copied().flatten())
                        .collect();
                }
                for n in &mut beat.notes {
                    if let Some(Harmonic::Artificial { semitones }) = &mut n.effects.harmonic {
                        *semitones = if *semitones >= 24 { 24 } else { 12 };
                    }
                }
            }
        }
    }
}

fn first_difference(a: &Score, b: &Score) -> String {
    if a.meta != b.meta {
        return format!("metadata: {:?} vs {:?}", a.meta, b.meta);
    }
    for (i, (x, y)) in a.master_bars.iter().zip(&b.master_bars).enumerate() {
        if x != y {
            return format!("master bar {i}: {x:?}\n  vs {y:?}");
        }
    }
    for (ti, (x, y)) in a.tracks.iter().zip(&b.tracks).enumerate() {
        for (bi, (p, q)) in x.bars.iter().zip(&y.bars).enumerate() {
            if p != q {
                return format!("track {ti} bar {bi}:\n  {p:?}\n  vs {q:?}");
            }
        }
        let (mut x, mut y) = (x.clone(), y.clone());
        x.bars.clear();
        y.bars.clear();
        if x != y {
            return format!("track {ti}: {x:?}\n  vs {y:?}");
        }
    }
    format!(
        "tempo {} vs {}, lyrics {:?} vs {:?}",
        a.tempo, b.tempo, a.lyrics, b.lyrics
    )
}

#[test]
fn gp3_to_gp5_survive_gp5_round_trip() {
    let mut failures = Vec::new();
    for f in corpus_files(&["gp3", "gp4", "gp5"]) {
        let mut original = muse_formats::read_file(&f).unwrap();
        let bytes = muse_formats::gp_write::write_gp5(&original).unwrap();
        let mut back = match muse_formats::gp::read_with_trailing(&bytes) {
            Ok((s, 0)) => s,
            Ok((_, n)) => {
                failures.push(format!("{}: {n} trailing bytes after writing", f.display()));
                continue;
            }
            Err(e) => {
                failures.push(format!("{}: {e}", f.display()));
                continue;
            }
        };
        normalize_gp5(&mut original);
        normalize_gp5(&mut back);
        if original != back {
            failures.push(format!(
                "{}: {}",
                f.display(),
                first_difference(&original, &back)
            ));
        }
    }
    assert!(
        failures.is_empty(),
        "{} failures:\n{}",
        failures.len(),
        failures.join("\n")
    );
}

#[test]
fn gpif_files_convert_to_gp5() {
    let mut failures = Vec::new();
    for f in corpus_files(&["gpx", "gp"]) {
        let original = muse_formats::read_file(&f).unwrap();
        let bytes = muse_formats::gp_write::write_gp5(&original).unwrap();
        match muse_formats::gp::read_with_trailing(&bytes) {
            Ok((back, 0)) => {
                if back.master_bars.len() != original.master_bars.len()
                    || back.tracks.len() != original.tracks.len()
                {
                    failures.push(format!("{}: structure changed", f.display()));
                }
            }
            Ok((_, n)) => failures.push(format!("{}: {n} trailing bytes", f.display())),
            Err(e) => failures.push(format!("{}: {e}", f.display())),
        }
    }
    assert!(
        failures.is_empty(),
        "{} failures:\n{}",
        failures.len(),
        failures.join("\n")
    );
}
