//! The data behind the palette's tools survives the formats that can hold it: everything in
//! `.cc`; fades, left-hand taps and repeat signs in Guitar Pro 7; stems, fermatas and
//! double accidentals in MusicXML.

use cecilia_core::*;

/// A fretted track with one bar of four quarter notes and a second, empty-ish bar, every
/// beat carrying something the palette can set.
fn sample() -> Score {
    let note = |fret| Note::new(0, fret);
    let mut beats: Vec<Beat> = (0..4)
        .map(|i| Beat {
            duration: Duration::new(DurationValue::Quarter),
            notes: vec![note(i + 3)],
            ..Beat::default()
        })
        .collect();
    beats[0].beam = BeamMode::Force;
    beats[0].stem = StemDir::Up;
    beats[0].effects.fermata = true;
    beats[1].beam = BeamMode::Break;
    beats[1].stem = StemDir::Down;
    beats[1].effects.fade_out = true;
    beats[2].beam = BeamMode::BreakSecondary;
    beats[2].effects.volume_swell = true;
    beats[2].notes[0].effects.left_hand_tap = true;
    // F double sharp sounds as G (E string, 3rd fret).
    beats[3].notes[0].fret = 3;
    beats[3].notes[0].alter = Some(2);
    let bar = Bar {
        voices: vec![Voice { beats }],
        ..Default::default()
    };
    let mut repeated = bar.clone();
    repeated.simile = Some(Simile::OneBar);
    let mut master = vec![MasterBar::default(), MasterBar::default()];
    master[0].directions = vec![Direction::DaCapoAlCoda];
    master[0].free_time = true;
    master[0].double_bar = true;
    master[1].alternate_endings = 0b11;
    Score {
        tempo: 100,
        master_bars: master,
        tracks: vec![Track {
            name: "Guitar".into(),
            kind: TrackKind::Fretted,
            strings: vec![64, 59, 55, 50, 45, 40],
            fret_count: 24,
            bars: vec![bar, repeated],
            ..Track::default()
        }],
        ..Score::default()
    }
}

fn beats(s: &Score, bar: usize) -> &[Beat] {
    &s.tracks[0].bars[bar].voices[0].beats
}

#[test]
fn native_keeps_everything() {
    let score = sample();
    let bytes = cecilia_formats::native::write_cecilia(&score).unwrap();
    let back = cecilia_formats::native::read_cecilia(&bytes).unwrap();
    assert_eq!(back, score);
    assert_eq!(back.tracks[0].bars[1].simile, Some(Simile::OneBar));
    assert_eq!(beats(&back, 0)[3].notes[0].alter, Some(2));
}

#[test]
fn gp7_keeps_fades_left_hand_taps_and_repeat_signs() {
    let score = sample();
    let bytes = cecilia_formats::gpif_write::write_gp7(&score).unwrap();
    let back = cecilia_formats::gpif::read_gp7(&bytes).unwrap();
    let b = beats(&back, 0);
    assert!(b[1].effects.fade_out && !b[1].effects.fade_in);
    assert!(b[2].effects.volume_swell);
    assert!(b[2].notes[0].effects.left_hand_tap);
    assert_eq!(back.tracks[0].bars[1].simile, Some(Simile::OneBar));
    assert_eq!(
        back.master_bars[0].directions,
        vec![Direction::DaCapoAlCoda]
    );
    assert!(back.master_bars[0].double_bar && back.master_bars[0].free_time);
    assert_eq!(back.master_bars[1].alternate_endings, 0b11);
    // A repeat sign without notes of its own plays the bar it repeats.
    assert_eq!(beats(&back, 1).len(), 4);
}

#[test]
fn musicxml_keeps_stems_fermatas_and_double_accidentals() {
    let score = sample();
    let bytes = cecilia_formats::musicxml::write_musicxml(&score).unwrap();
    let text = String::from_utf8_lossy(&bytes);
    assert!(text.contains("<alter>2</alter>"));
    let back = cecilia_formats::musicxml::read_musicxml(&bytes).unwrap();
    let b = beats(&back, 0);
    assert_eq!(b[0].stem, StemDir::Up);
    assert_eq!(b[1].stem, StemDir::Down);
    assert_eq!(b[2].stem, StemDir::Auto);
    assert!(b[0].effects.fermata && !b[1].effects.fermata);
    // The double sharp keeps its pitch and its spelling.
    let want = &beats(&score, 0)[3].notes[0];
    let got = &b[3].notes[0];
    assert_eq!(got.alter, Some(2));
    assert_eq!(back.tracks[0].pitch_of(got), score.tracks[0].pitch_of(want));
}

#[test]
fn files_written_before_the_new_fields_still_read() {
    // The sample without the new fields serialises to JSON a version-1 reader wrote.
    let mut plain = sample();
    for bar in &mut plain.tracks[0].bars {
        bar.simile = None;
        for b in &mut bar.voices[0].beats {
            b.beam = BeamMode::Auto;
            b.stem = StemDir::Auto;
            b.effects = BeatEffects::default();
            b.notes[0].alter = None;
            b.notes[0].effects.left_hand_tap = false;
        }
    }
    let json = serde_json::to_string(&plain).unwrap();
    for field in [
        "simile",
        "\"beam\"",
        "\"stem\"",
        "fermata",
        "fade_out",
        "left_hand_tap",
        "\"alter\":",
    ] {
        assert!(!json.contains(field), "{field} is not written when unset");
    }
    let back: Score = serde_json::from_str(&json).unwrap();
    assert_eq!(back, plain);
}

#[test]
fn dynamics_are_per_beat_in_every_format() {
    // A chord whose notes disagree: the beat takes its first note's velocity.
    let mut score = sample();
    {
        let beat = &mut score.tracks[0].bars[0].voices[0].beats[0];
        beat.notes[0].velocity = 63;
        let mut other = Note::new(1, 5);
        other.velocity = 111;
        beat.notes.push(other);
    }
    // Old `.cc` files have no beat dynamic: the first note's velocity becomes it.
    let json = serde_json::to_string(&score).unwrap();
    let old: Score = serde_json::from_str(&json).unwrap();
    assert_eq!(beats(&old, 0)[0].dynamic, None);
    let bytes = cecilia_formats::native::write_cecilia(&score).unwrap();
    let back = cecilia_formats::native::read_cecilia(&bytes).unwrap();
    let b = &beats(&back, 0)[0];
    assert_eq!(b.dynamic, Some(63));
    assert!(b.notes.iter().all(|n| n.velocity == 63));
    // Guitar Pro 7 stores a dynamic per beat.
    let bytes = cecilia_formats::gpif_write::write_gp7(&score).unwrap();
    let back = cecilia_formats::gpif::read_gp7(&bytes).unwrap();
    let b = &beats(&back, 0)[0];
    assert_eq!(b.dynamic_velocity(), 63);
    assert!(b.notes.iter().all(|n| n.velocity == 63));
    // A beat dynamic is written for all its notes.
    let mut set = sample();
    set.tracks[0].bars[0].voices[0].beats[1].dynamic = Some(31);
    let bytes = cecilia_formats::write_bytes(&set, "gp").unwrap();
    let back = cecilia_formats::gpif::read_gp7(&bytes).unwrap();
    assert_eq!(beats(&back, 0)[1].dynamic, Some(31));
}
