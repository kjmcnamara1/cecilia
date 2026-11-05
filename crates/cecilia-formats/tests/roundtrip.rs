//! Round-trips corpus files through the writers and compares the models.

mod common;

use cecilia_core::*;
use common::corpus_files;

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

/// Removes differences GPIF cannot represent: page setup, per-note dynamics within a beat, note duration
/// percentages, fingering, barres, MIDI volume/pan, marker colors, whammy bar, trill speed.
fn normalize_gpif(s: &mut Score) {
    s.extras.clear();
    s.audio = None;
    // Page setup is only kept by Guitar Pro 5 files.
    s.page = PageSetup::default();
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
        let mut original = cecilia_formats::read_file(&f).unwrap();
        let bytes = cecilia_formats::gp_write::write_gp5(&original).unwrap();
        let mut back = match cecilia_formats::gp::read_with_trailing(&bytes) {
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
        let original = cecilia_formats::read_file(&f).unwrap();
        let bytes = cecilia_formats::gp_write::write_gp5(&original).unwrap();
        match cecilia_formats::gp::read_with_trailing(&bytes) {
            Ok((back, 0)) => {
                // GP5 has a track per staff: a grand staff comes back as two tracks.
                if back.master_bars.len() != original.master_bars.len()
                    || back.tracks.len() != original.split_grand().tracks.len()
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
        let original = cecilia_formats::read_file(&f).unwrap();
        let bytes = cecilia_formats::gpif_write::write_gp7(&original).unwrap();
        let back = match cecilia_formats::gpif::read_gp7(&bytes) {
            Ok(s) => s,
            Err(e) => {
                failures.push(format!("{}: {e}", f.display()));
                continue;
            }
        };
        // Grand staves compare as their two staves (hands split at the grand staff's split).
        let (mut original, mut back) = (original.split_grand(), back.split_grand());
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
        let original = cecilia_formats::read_file(&f).unwrap();
        let bytes = cecilia_formats::gpif_write::write_gpx(&original).unwrap();
        let back = match cecilia_formats::gpif::read_gpx(&bytes) {
            Ok(s) => s,
            Err(e) => {
                failures.push(format!("{}: {e}", f.display()));
                continue;
            }
        };
        // Grand staves compare as their two staves (hands split at the grand staff's split).
        let (mut original, mut back) = (original.split_grand(), back.split_grand());
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

/// MIDI keeps sounding notes, not notation: compare (onset, pitch) of the performances.
#[test]
fn corpus_survives_midi_round_trip() {
    use std::collections::HashMap;
    let mut failures = Vec::new();
    let (mut total, mut matched) = (0usize, 0usize);
    for f in corpus_files(&["gp3", "gp4", "gp5", "gpx", "gp"]) {
        let original = cecilia_formats::read_file(&f).unwrap();
        let bytes = cecilia_formats::midi::write_midi(&original).unwrap();
        let back = match cecilia_formats::midi::read_midi(&bytes) {
            Ok(s) => s,
            Err(e) => {
                failures.push(format!("{}: {e}", f.display()));
                continue;
            }
        };
        let notes = |s: &Score| {
            let perf = cecilia_core::Performance::build(s, &vec![false; s.tracks.len()]);
            let mut m: HashMap<(u64, u8), usize> = HashMap::new();
            // Compare on the import grid: strums spread chord notes by a few ticks.
            let grid = |t: u64| {
                let snap = |g: u64| (t + g / 2) / g * g;
                let (a, b) = (snap(120), snap(160));
                if a.abs_diff(t) <= b.abs_diff(t) { a } else { b }
            };
            for n in perf.notes {
                // Unisons in two voices sound as one note.
                m.insert((grid(n.start), n.key), 1);
            }
            m
        };
        let (a, b) = (notes(&original), notes(&back));
        let n: usize = a.values().sum();
        let same: usize = a
            .iter()
            .map(|(k, v)| (*v).min(b.get(k).copied().unwrap_or(0)))
            .sum();
        total += n;
        matched += same;
        if n > 0 && (same as f64) < 0.9 * n as f64 {
            failures.push(format!("{}: only {same} of {n} notes survive", f.display()));
        }
    }
    eprintln!(
        "MIDI round trip: {matched} of {total} notes ({:.2}%)",
        matched as f64 * 100.0 / total.max(1) as f64
    );
    assert!(
        failures.is_empty(),
        "{} failures:\n{}",
        failures.len(),
        failures.join("\n")
    );
}

/// What MusicXML keeps of a bar: per voice, each beat's onset, length and notes.
type MxlBar = Vec<Vec<(u32, u32, Vec<(u8, u8, bool, bool)>)>>;

fn mxl_view(s: &Score) -> Vec<Vec<MxlBar>> {
    s.tracks
        .iter()
        .map(|t| {
            t.bars
                .iter()
                .zip(&s.master_bars)
                .map(|(bar, mb)| {
                    bar.voices
                        .iter()
                        .map(|v| {
                            let mut out = Vec::new();
                            let mut tick = 0;
                            for b in &v.beats {
                                if b.empty {
                                    continue;
                                }
                                // Notes past the bar line are trimmed on export.
                                if !b.notes.is_empty() && tick < mb.ticks() {
                                    let len = b.ticks().min(mb.ticks() - tick);
                                    let mut notes: Vec<_> = b
                                        .notes
                                        .iter()
                                        .map(|n| match t.kind {
                                            TrackKind::Drums => (0, n.fret, n.tie, n.dead),
                                            _ => (n.string, n.fret, n.tie, n.dead),
                                        })
                                        .collect();
                                    notes.sort();
                                    notes.dedup();
                                    out.push((tick, len, notes));
                                }
                                tick += b.ticks();
                            }
                            out
                        })
                        .filter(|v| !v.is_empty())
                        .collect()
                })
                .collect()
        })
        .collect()
}

#[test]
fn corpus_survives_musicxml_round_trip() {
    let mut failures = Vec::new();
    for f in corpus_files(&["gp3", "gp4", "gp5", "gpx", "gp"]) {
        let original = cecilia_formats::read_file(&f).unwrap();
        let bytes = cecilia_formats::musicxml::write_musicxml(&original).unwrap();
        let back = match cecilia_formats::musicxml::read_musicxml(&bytes) {
            Ok(s) => s,
            Err(e) => {
                failures.push(format!("{}: {e}", f.display()));
                continue;
            }
        };
        // Grand staves compare as their two staves (hands split at the grand staff's split).
        let (original, back) = (original.split_grand(), back.split_grand());
        let structure = |s: &Score| -> Vec<_> {
            s.master_bars
                .iter()
                .map(|m| {
                    (
                        m.time_signature,
                        m.key_signature,
                        m.repeat_start,
                        m.repeat_end,
                        m.alternate_endings,
                        m.marker.as_ref().map(|x| x.title.clone()),
                    )
                })
                .collect()
        };
        let tracks = |s: &Score| -> Vec<_> {
            s.tracks
                .iter()
                .map(|t| {
                    (
                        t.name.clone(),
                        t.kind,
                        t.strings.clone(),
                        t.capo,
                        t.midi.program,
                    )
                })
                .collect()
        };
        if structure(&original) != structure(&back) {
            let (x, y) = (structure(&original), structure(&back));
            let i = x
                .iter()
                .zip(&y)
                .position(|(a, b)| a != b)
                .unwrap_or(x.len().min(y.len()));
            failures.push(format!(
                "{}: bar {i} differs: {:?} vs {:?} ({} vs {} bars)",
                f.display(),
                x.get(i),
                y.get(i),
                x.len(),
                y.len()
            ));
        } else if tracks(&original) != tracks(&back) {
            failures.push(format!(
                "{}: tracks differ: {:?} vs {:?}",
                f.display(),
                tracks(&original),
                tracks(&back)
            ));
        } else if (original.meta.title.as_str(), original.tempo)
            != (back.meta.title.as_str(), back.tempo)
        {
            failures.push(format!("{}: title/tempo differ", f.display()));
        } else {
            let (a, b) = (mxl_view(&original), mxl_view(&back));
            'outer: for (ti, (x, y)) in a.iter().zip(&b).enumerate() {
                for (bi, (p, q)) in x.iter().zip(y).enumerate() {
                    if p != q {
                        failures.push(format!(
                            "{}: track {ti} bar {bi}:\n  {p:?}\n  {q:?}",
                            f.display()
                        ));
                        break 'outer;
                    }
                }
            }
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
fn corpus_survives_native_round_trip_exactly() {
    for f in corpus_files(&["gp3", "gp4", "gp5", "gpx", "gp"]) {
        let original = cecilia_formats::read_file(&f).unwrap();
        let bytes = cecilia_formats::native::write_cecilia(&original).unwrap();
        let back = cecilia_formats::read_bytes(&bytes, None).unwrap();
        assert!(original == back, "{}", f.display());
    }
}

#[test]
fn corpus_survives_gabc_round_trip() {
    for f in corpus_files(&["gabc"]) {
        let original = cecilia_formats::read_file(&f).unwrap();
        let bytes = cecilia_formats::write_bytes(&original, "gabc").unwrap();
        let back = cecilia_formats::read_bytes(&bytes, None).unwrap();
        assert!(original == back, "{}", f.display());
        // The chant also survives `.cc`, and its transcription is consistent.
        let native = cecilia_formats::native::write_cecilia(&original).unwrap();
        assert!(cecilia_formats::read_bytes(&native, None).unwrap() == original);
        let t = &original.tracks[0];
        assert_eq!(t.bars.len(), original.master_bars.len(), "{}", f.display());
        for (mb, bar) in original.master_bars.iter().zip(&t.bars) {
            let ticks: u32 = bar.voices[0].beats.iter().map(Beat::ticks).sum();
            assert_eq!(mb.ticks(), ticks, "{}", f.display());
        }
    }
}

/// A one-bar guitar score whose four voices play different frets and lengths.
fn four_voices() -> Score {
    let voice = |fret: u8, value: DurationValue| {
        let count = (4 * TICKS_PER_QUARTER / value.ticks()) as usize;
        Voice {
            beats: vec![
                Beat {
                    duration: Duration::new(value),
                    notes: vec![Note::new(fret as u8 % 6, fret)],
                    ..Beat::default()
                };
                count
            ],
        }
    };
    let track = Track {
        name: "Gtr".into(),
        strings: vec![64, 59, 55, 50, 45, 40],
        fret_count: 24,
        bars: vec![Bar {
            voices: vec![
                voice(1, DurationValue::Eighth),
                voice(2, DurationValue::Quarter),
                voice(3, DurationValue::Half),
                voice(4, DurationValue::Whole),
            ],
            ..Default::default()
        }],
        ..Track::default()
    };
    Score {
        tempo: 100,
        master_bars: vec![MasterBar::default()],
        tracks: vec![track],
        ..Score::default()
    }
}

fn voice_shape(s: &Score) -> Vec<(usize, u8)> {
    s.tracks[0].bars[0]
        .voices
        .iter()
        .map(|v| (v.beats.len(), v.beats[0].notes[0].fret))
        .collect()
}

#[test]
fn four_voices_survive_gpx_gp_and_native_and_musicxml() {
    let score = four_voices();
    let want = vec![(8, 1), (4, 2), (2, 3), (1, 4)];
    for ext in ["gpx", "gp", "cc", "musicxml"] {
        let bytes = cecilia_formats::write_bytes(&score, ext).unwrap();
        let name = format!("x.{ext}");
        let back = cecilia_formats::read_bytes(&bytes, Some(std::path::Path::new(&name))).unwrap();
        assert_eq!(voice_shape(&back), want, "{ext}");
    }
}

#[test]
fn gp5_keeps_two_voices_and_the_loss_is_reported() {
    let score = four_voices();
    let bytes = cecilia_formats::write_bytes(&score, "gp5").unwrap();
    let back = cecilia_formats::read_bytes(&bytes, Some(std::path::Path::new("x.gp5"))).unwrap();
    assert_eq!(voice_shape(&back), vec![(8, 1), (4, 2)]);
    assert_eq!(cecilia_formats::voices_lost(&score, "gp5"), 2);
    assert_eq!(cecilia_formats::voices_lost(&score, "gpx"), 0);
    assert_eq!(cecilia_formats::voices_lost(&score, "cc"), 0);
}

#[test]
fn zz_dump_four_voices() {
    let mut s = four_voices();
    s.master_bars.push(MasterBar::default());
    let b = s.tracks[0].bars[0].clone();
    s.tracks[0].bars.push(b);
    std::fs::write("/tmp/four.cc", cecilia_formats::write_bytes(&s, "cc").unwrap()).unwrap();
}
