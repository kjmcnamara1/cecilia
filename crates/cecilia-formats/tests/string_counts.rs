//! Tracks with 4, 5 and 7 strings keep their tuning and their notes' strings through every
//! format that stores tablature.

use cecilia_core::*;
use cecilia_formats::{read_bytes, write_bytes};

fn track(name: &str, strings: &[u8]) -> Track {
    let n = strings.len() as u8;
    let beat = |string, fret| Beat {
        duration: Duration::new(DurationValue::Quarter),
        notes: vec![Note::new(string, fret)],
        ..Beat::default()
    };
    Track {
        name: name.into(),
        strings: strings.to_vec(),
        fret_count: 24,
        bars: vec![Bar {
            voices: vec![Voice {
                // The first and the last string, and one between.
                beats: vec![beat(0, 3), beat(n / 2, 5), beat(n - 1, 0), beat(n - 1, 7)],
            }],
            ..Default::default()
        }],
        ..Track::default()
    }
}

fn score(strings: &[u8]) -> Score {
    Score {
        tempo: 100,
        master_bars: vec![MasterBar::default()],
        tracks: vec![track("T", strings)],
        ..Score::default()
    }
}

fn notes(t: &Track) -> Vec<(u8, u8)> {
    t.bars[0].voices[0]
        .beats
        .iter()
        .flat_map(|b| &b.notes)
        .map(|n| (n.string, n.fret))
        .collect()
}

#[test]
fn four_five_and_seven_strings_survive_the_formats() {
    let tunings: [&[u8]; 3] = [
        &[43, 38, 33, 28],
        &[43, 38, 33, 28, 23],
        &[64, 59, 55, 50, 45, 40, 35],
    ];
    for extension in ["gp5", "gp", "gpx", "musicxml", "mxl", "ccc"] {
        for strings in tunings {
            let original = score(strings);
            let bytes = write_bytes(&original, extension).unwrap();
            let back = read_bytes(
                &bytes,
                Some(std::path::Path::new(&format!("x.{extension}"))),
            )
            .unwrap_or_else(|e| panic!("{extension} {}: {e}", strings.len()));
            let t = &back.tracks[0];
            let what = format!(".{extension}, {} strings", strings.len());
            assert_eq!(t.strings, strings, "{what}");
            assert_eq!(notes(t), notes(&original.tracks[0]), "{what}");
        }
    }
}
