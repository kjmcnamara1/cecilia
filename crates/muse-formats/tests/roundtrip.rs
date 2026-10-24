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

/// Removes differences GPIF cannot represent: per-note dynamics within a beat, note duration
/// percentages, fingering, barres, MIDI volume/pan, marker colours, whammy bar, trill speed.
fn normalize_gpif(s: &mut Score) {
    s.extras.clear();
    s.audio = None;
    if s.lyrics.as_ref().is_some_and(|l| l.lines.is_empty()) {
        s.lyrics = None;
    }
    if let Some(l) = &mut s.lyrics {
        l.track = None;
    }
    for mb in &mut s.master_bars {
        if let Some(m) = &mut mb.marker {
            m.color = Color::default();
        }
        mb.tempo_changes.retain(|c| c.tick > 0 || c.bpm > 0.0);
    }
    for t in &mut s.tracks {
        t.extras.clear();
        t.midi.channel = 0;
        t.midi.effect_channel = 0;
        t.midi.volume = 0;
        t.midi.pan = 0;
        let kind = t.kind;
        if kind == TrackKind::Drums {
            t.strings = vec![0; 6];
        }
        if kind != TrackKind::Fretted {
            t.capo = 0;
        }
        let strings = t.strings.len();
        for bar in &mut t.bars {
            for v in &mut bar.voices {
                if v.beats.iter().all(|b| b.empty) {
                    v.beats = vec![Beat {
                        empty: true,
                        ..Beat::default()
                    }];
                }
            }
            while bar
                .voices
                .last()
                .is_some_and(|v| v.beats.iter().all(|b| b.empty))
            {
                bar.voices.pop();
            }
            bar.voices.truncate(4);
            for beat in bar.voices.iter_mut().flat_map(|v| &mut v.beats) {
                beat.effects.tremolo_bar = None;
                if let Some(st) = &mut beat.effects.stroke {
                    st.duration = DurationValue::Sixteenth;
                }
                if let Some(c) = &mut beat.chord {
                    c.frets = (0..strings)
                        .map(|i| c.frets.get(i).copied().flatten())
                        .collect();
                    c.barres.clear();
                }
                let tremolo = beat.notes.iter().find_map(|n| n.effects.tremolo_picking);
                let velocity = beat.notes.first().map(|n| n.velocity);
                for n in &mut beat.notes {
                    if let Some(b) = &mut n.effects.bend {
                        *b = gpif_bend(b);
                    }
                    n.velocity = velocity.unwrap();
                    n.velocity = match n.velocity {
                        0..=22 => 15,
                        23..=38 => 31,
                        39..=54 => 47,
                        55..=70 => 63,
                        71..=86 => 79,
                        87..=102 => 95,
                        103..=118 => 111,
                        _ => 127,
                    };
                    n.duration_percent = 1.0;
                    n.effects.left_finger = None;
                    n.effects.right_finger = None;
                    n.effects.tremolo_picking = tremolo;
                    if kind == TrackKind::Drums {
                        n.string = 0;
                    }
                    if let Some(tr) = &mut n.effects.trill {
                        tr.speed = DurationValue::Sixteenth;
                    }
                    if kind == TrackKind::Drums {
                        // Which drum note a grace attaches to is ambiguous in GPIF.
                        n.effects.grace = None;
                    }
                    if let Some(g) = &mut n.effects.grace {
                        if g.transition == GraceTransition::Bend {
                            g.transition = GraceTransition::None;
                        }
                        g.velocity = velocity.unwrap_or(95);
                    }
                }
                if kind == TrackKind::Drums {
                    beat.notes.sort_by_key(|n| n.fret);
                }
            }
        }
    }
}

/// The shape a bend takes in GPIF: origin, up to two middle points sharing one value, and
/// destination, without vibrato flags.
fn gpif_bend(b: &Bend) -> Bend {
    let pts = &b.points;
    let point = |position, value| BendPoint {
        position,
        value,
        vibrato: false,
    };
    let (Some(first), Some(last)) = (pts.first(), pts.last()) else {
        return b.clone();
    };
    let mut points = vec![point(0, first.value)];
    if pts.len() > 2 {
        let m = &pts[1..pts.len() - 1];
        points.push(point(m[0].position, m[0].value));
        if m[m.len() - 1].position != m[0].position {
            points.push(point(m[m.len() - 1].position, m[0].value));
        }
    }
    if pts.len() > 1 {
        points.push(point(last.position, last.value));
    }
    Bend { points }
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
            if p == q {
                continue;
            }
            if p.voices.len() != q.voices.len() {
                return format!(
                    "track {ti} bar {bi}: {} vs {} voices",
                    p.voices.len(),
                    q.voices.len()
                );
            }
            for (vi, (u, w)) in p.voices.iter().zip(&q.voices).enumerate() {
                if u.beats.len() != w.beats.len() {
                    return format!(
                        "track {ti} bar {bi} voice {vi}: {} vs {} beats",
                        u.beats.len(),
                        w.beats.len()
                    );
                }
                for (i, (c, d)) in u.beats.iter().zip(&w.beats).enumerate() {
                    if c != d {
                        let (c, d) = (format!("{c:?}"), format!("{d:?}"));
                        let at = c
                            .chars()
                            .zip(d.chars())
                            .position(|(x, y)| x != y)
                            .unwrap_or(0);
                        let from = at.saturating_sub(100);
                        let snip = |s: &str| s.chars().skip(from).take(200).collect::<String>();
                        return format!(
                            "track {ti} bar {bi} voice {vi} beat {i}:\n  …{}\n  …{}",
                            snip(&c),
                            snip(&d)
                        );
                    }
                }
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

#[test]
fn corpus_survives_gp7_round_trip() {
    let mut failures = Vec::new();
    for f in corpus_files(&["gp3", "gp4", "gp5", "gpx", "gp"]) {
        let mut original = muse_formats::read_file(&f).unwrap();
        let bytes = muse_formats::gpif_write::write_gp7(&original).unwrap();
        let mut back = match muse_formats::gpif::read_gp7(&bytes) {
            Ok(s) => s,
            Err(e) => {
                failures.push(format!("{}: {e}", f.display()));
                continue;
            }
        };
        normalize_gpif(&mut original);
        normalize_gpif(&mut back);
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
fn corpus_survives_gpx_round_trip() {
    let mut failures = Vec::new();
    for f in corpus_files(&["gp3", "gp4", "gp5", "gpx", "gp"]) {
        let mut original = muse_formats::read_file(&f).unwrap();
        let bytes = muse_formats::gpif_write::write_gpx(&original).unwrap();
        let mut back = match muse_formats::gpif::read_gpx(&bytes) {
            Ok(s) => s,
            Err(e) => {
                failures.push(format!("{}: {e}", f.display()));
                continue;
            }
        };
        normalize_gpif(&mut original);
        normalize_gpif(&mut back);
        // GP6 has no sound bank selection.
        for t in original.tracks.iter_mut().chain(back.tracks.iter_mut()) {
            t.midi.bank = 0;
        }
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
