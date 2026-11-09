//! The native `.cc` format: a zip archive holding a versioned manifest and the full score
//! model as JSON, so nothing is lost when saving.

use std::io::{Read, Write};

use cecilia_core::Score;

use crate::{Error, Result};

/// Version of the score model stored in `score.json`; bump when its schema changes and add a
/// migration in [`read_cecilia`].
///
/// - 2: modern lyrics text in the current syntax (`(bar:beat)` anchors, markup tags, no GABC
///   groups): version 1 text is converted on load ([`cecilia_core::gabcplus::from_v1`]).
pub const FORMAT_VERSION: u32 = 2;

fn err(message: impl Into<String>) -> Error {
    Error::Parse {
        offset: 0,
        location: Some(".cc".into()),
        message: message.into(),
    }
}

pub fn write_cecilia(score: &Score) -> Result<Vec<u8>> {
    // A chant score's bar sync points are derived from its note points: only those are saved.
    let derived;
    let model = if score.is_chant() && score.audio.as_ref().is_some_and(|a| !a.points.is_empty()) {
        let mut s = score.clone();
        if let Some(a) = s.audio.as_mut() {
            a.points.clear();
        }
        derived = s;
        &derived
    } else {
        score
    };
    let json = serde_json::to_vec(model).map_err(|e| err(e.to_string()))?;
    let manifest = format!(
        "format = \"cecilia\"\nformat_version = {FORMAT_VERSION}\napp_version = \"{}\"\n",
        env!("CARGO_PKG_VERSION")
    );
    let mut zip = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    let zerr = |e: zip::result::ZipError| err(e.to_string());
    let options = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);
    zip.start_file("manifest.toml", options).map_err(zerr)?;
    zip.write_all(manifest.as_bytes())?;
    zip.start_file("score.json", options).map_err(zerr)?;
    zip.write_all(&json)?;
    if let Some(audio) = &score.audio {
        if let Some(data) = &audio.embedded {
            let name = audio.file.as_deref().unwrap_or("audio");
            let base = name.rsplit('/').next().unwrap_or(name);
            let stored = zip::write::SimpleFileOptions::default()
                .compression_method(zip::CompressionMethod::Stored);
            zip.start_file(format!("audio/{base}"), stored)
                .map_err(zerr)?;
            zip.write_all(data)?;
        }
    }
    Ok(zip.finish().map_err(zerr)?.into_inner())
}

pub fn read_cecilia(bytes: &[u8]) -> Result<Score> {
    let mut zip =
        zip::ZipArchive::new(std::io::Cursor::new(bytes)).map_err(|e| err(e.to_string()))?;
    let mut manifest = String::new();
    zip.by_name("manifest.toml")
        .map_err(|e| err(e.to_string()))?
        .read_to_string(&mut manifest)?;
    let version = manifest
        .lines()
        .find_map(|l| l.trim().strip_prefix("format_version"))
        .and_then(|v| v.trim().trim_start_matches('=').trim().parse::<u32>().ok())
        .ok_or_else(|| err("manifest has no format_version"))?;
    if version > FORMAT_VERSION {
        return Err(err(format!(
            "file format version {version} is newer than this cecilia (supports {FORMAT_VERSION})"
        )));
    }
    let mut json = Vec::new();
    zip.by_name("score.json")
        .map_err(|e| err(e.to_string()))?
        .read_to_end(&mut json)?;
    let mut value: serde_json::Value =
        serde_json::from_slice(&json).map_err(|e| err(e.to_string()))?;
    drop_chant_staves(&mut value);
    let mut score: Score = serde_json::from_value(value).map_err(|e| err(e.to_string()))?;
    if let Some(audio) = score.audio.as_mut() {
        let base = audio
            .file
            .as_deref()
            .map(|f| f.rsplit('/').next().unwrap_or(f).to_string());
        if let Some(base) = base {
            if let Ok(mut entry) = zip.by_name(&format!("audio/{base}")) {
                let mut data = Vec::new();
                entry.read_to_end(&mut data)?;
                audio.embedded = Some(std::sync::Arc::new(data));
            }
        }
    }
    if version < 2 {
        upgrade_lyrics(&mut score);
    }
    // A score is chant or modern, not both: old files could hold a chant track beside others.
    // It loads as a modern score (the chant's bars and syllables), as `:chant-to-modern` makes.
    score.one_kind();
    score.normalize_dynamics();
    if score.is_chant() {
        // Notes get their ids (older files have none); the bar points of a chant score are
        // derived from its note points, and bar points an older file kept become note points.
        let t = score.tracks.iter().position(|t| t.chant.is_some()).unwrap();
        if let Some(c) = score.tracks[t].chant.as_deref_mut() {
            c.assign_ids();
            // The kept GABC text comes back with its map (a text that no longer reads as the
            // chant is dropped).
            crate::gabc::refresh_source(c);
        }
        if score
            .audio
            .as_ref()
            .is_some_and(|a| a.chant_points.is_empty() && !a.points.is_empty())
        {
            score.bar_points_to_notes();
        } else {
            score.refresh_chant_sync();
        }
    } else {
        // A modern score's bar points and note timing come from its beat points.
        score.refresh_beat_sync();
    }
    Ok(score)
}

/// Converts the lyrics text of the tracks of a version 1 file to the current syntax (once: the
/// file is saved as version 2). Chant tracks keep their GABC.
fn upgrade_lyrics(score: &mut Score) {
    for t in 0..score.tracks.len() {
        let track = &score.tracks[t];
        if track.chant.is_some() || track.lyrics_text.trim().is_empty() {
            continue;
        }
        let text = cecilia_core::gabcplus::from_v1(score, t, &track.lyrics_text);
        score.tracks[t].lyrics_text = text;
    }
}

/// Files written before chant staves were removed from ordinary tracks may name one
/// (`"Chant"`, `"ChantFa"`) among a track's staves, with a clef line (`chant_line`). The staff is
/// dropped (with the lyrics attached to it); the track keeps its other staves, or the default
/// ones if it had no other. Chant tracks (`chant` set) have no staves of their own to keep.
fn drop_chant_staves(score: &mut serde_json::Value) {
    let Some(tracks) = score.get_mut("tracks").and_then(|t| t.as_array_mut()) else {
        return;
    };
    for track in tracks {
        let Some(track) = track.as_object_mut() else {
            continue;
        };
        track.remove("chant_line");
        let Some(staves) = track.get("staves").and_then(|s| s.as_array()) else {
            continue;
        };
        let dropped: Vec<usize> = staves
            .iter()
            .enumerate()
            .filter(|(_, s)| matches!(s.as_str(), Some("Chant" | "ChantFa")))
            .map(|(i, _)| i)
            .collect();
        for &i in dropped.iter().rev() {
            if let Some(a) = track.get_mut("staves").and_then(|s| s.as_array_mut()) {
                a.remove(i);
            }
            if let Some(a) = track.get_mut("staff_lyrics").and_then(|s| s.as_array_mut()) {
                if i < a.len() {
                    a.remove(i);
                }
            }
        }
    }
}

/// Whether bytes look like a `.cc` archive (a zip with a cecilia manifest).
pub fn is_cecilia(bytes: &[u8]) -> bool {
    bytes.starts_with(b"PK")
        && zip::ZipArchive::new(std::io::Cursor::new(bytes))
            .is_ok_and(|z| z.index_for_name("manifest.toml").is_some())
}

#[cfg(test)]
mod tests {
    use super::*;
    use cecilia_core::*;

    #[test]
    fn round_trips_exactly() {
        let score = Score {
            tempo: 133,
            meta: Metadata {
                title: "Ünïcödé".into(),
                ..Metadata::default()
            },
            master_bars: vec![MasterBar {
                tempo_changes: vec![TempoChange {
                    tick: 480,
                    bpm: 99.5,
                }],
                ..MasterBar::default()
            }],
            tracks: vec![Track {
                name: "t".into(),
                bars: vec![Bar::default()],
                ..Track::default()
            }],
            ..Score::default()
        };
        let bytes = write_cecilia(&score).unwrap();
        assert!(is_cecilia(&bytes));
        assert_eq!(read_cecilia(&bytes).unwrap(), score);
    }

    #[test]
    fn a_chant_staff_of_an_ordinary_track_in_an_old_file_is_dropped() {
        let track = |staves: Vec<Staff>| Track {
            name: "t".into(),
            staves,
            staff_lyrics: vec![None, None],
            bars: vec![Bar::default()],
            ..Track::default()
        };
        let score = Score {
            master_bars: vec![MasterBar::default()],
            tracks: vec![
                track(vec![Staff::Tab, Staff::Slash]),
                track(vec![Staff::Slash]),
            ],
            ..Score::default()
        };
        // What an old version wrote: a chant staff among the staves, and its clef line.
        let mut json = serde_json::to_value(&score).unwrap();
        let t = json["tracks"][0].as_object_mut().unwrap();
        t["staves"] = serde_json::json!(["Chant", "Tab", "ChantFa"]);
        t["staff_lyrics"] = serde_json::json!([null, null, {"track": null}]);
        t.insert("chant_line".into(), serde_json::json!(3));
        json["tracks"][1]["staves"] = serde_json::json!(["ChantFa"]);
        drop_chant_staves(&mut json);
        let back: Score = serde_json::from_value(json).unwrap();
        assert_eq!(back.tracks[0].staves, [Staff::Tab]);
        assert_eq!(
            back.tracks[0].staff_lyrics,
            [None],
            "the lyrics of a dropped staff go"
        );
        // Only a chant staff: the track gets its default staves again.
        assert!(back.tracks[1].staves.is_empty());
        assert!(!back.tracks[1].staves().is_empty());
    }

    #[test]
    fn a_file_mixing_chant_and_other_tracks_loads_as_a_modern_score() {
        let chant =
            crate::gabc::parse("name: t;\n%%\n(c4) Al(fg)le(h) lu(g)ia.(f.) (::)\n").unwrap();
        let chant_only = crate::gabc::chant_score(chant);
        // A chant score stays one: it round-trips as it is.
        let back = read_cecilia(&write_cecilia(&chant_only).unwrap()).unwrap();
        assert!(back.is_chant());
        assert_eq!(back, chant_only);
        // An old file could hold another track beside the chant.
        let mut mixed = chant_only.clone();
        mixed.tracks.push(Track {
            name: "Organ".into(),
            kind: TrackKind::Pitched,
            bars: vec![Bar::default(); mixed.master_bars.len()],
            ..Track::default()
        });
        assert!(!mixed.is_chant());
        let back = read_cecilia(&write_cecilia(&mixed).unwrap()).unwrap();
        assert!(!back.is_chant());
        assert_eq!(back.tracks.len(), 2);
        assert!(back.tracks.iter().all(|t| t.chant.is_none()));
        assert_eq!(back.tracks[0].kind, TrackKind::Pitched);
        // (The chant's division beat takes no time and is not a beat of a modern score.)
        let mut sounding = chant_only.tracks[0].bars.clone();
        for v in sounding.iter_mut().flat_map(|b| &mut b.voices) {
            v.beats.retain(|b| !b.empty);
        }
        assert_eq!(back.tracks[0].bars, sounding);
        assert_eq!(back.tracks[0].lyrics_text, "Al- _ le lu-ia.");
        assert_eq!(back.tracks[1].name, "Organ");
    }

    #[test]
    fn lyrics_of_a_version_1_file_are_converted_once() {
        let bar = Bar {
            voices: vec![Voice {
                beats: (0..4)
                    .map(|_| Beat {
                        notes: vec![Note::new(0, 60)],
                        ..Beat::default()
                    })
                    .collect(),
            }],
            ..Bar::default()
        };
        let mut score = Score {
            master_bars: vec![MasterBar::default(); 2],
            tracks: vec![Track {
                kind: TrackKind::Pitched,
                bars: vec![bar.clone(), bar],
                lyrics_text: "(c4) <2:2> Ky(f)ri(gh)e(h) _e-le-i-son a-_men".into(),
                ..Track::default()
            }],
            ..Score::default()
        };
        let write_as = |score: &Score, version: u32| {
            let bytes = write_cecilia(score).unwrap();
            let mut zip = zip::ZipArchive::new(std::io::Cursor::new(bytes)).unwrap();
            let mut out = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
            let opts = zip::write::SimpleFileOptions::default();
            for name in ["manifest.toml", "score.json"] {
                let mut data = Vec::new();
                zip.by_name(name).unwrap().read_to_end(&mut data).unwrap();
                if name == "manifest.toml" {
                    data =
                        format!("format = \"cecilia\"\nformat_version = {version}\n").into_bytes();
                }
                out.start_file(name, opts).unwrap();
                out.write_all(&data).unwrap();
            }
            out.finish().unwrap().into_inner()
        };
        let old = read_cecilia(&write_as(&score, 1)).unwrap();
        assert_eq!(
            old.tracks[0].lyrics_text,
            "(2:2) Ky-ri-e <i>e</i>-le-i-son a-<i>men</i>"
        );
        // A file saved by this version is not converted again.
        score.tracks[0].lyrics_text = "(2:2) <i>a</i>-_b".into();
        let back = read_cecilia(&write_cecilia(&score).unwrap()).unwrap();
        assert_eq!(back.tracks[0].lyrics_text, "(2:2) <i>a</i>-_b");
    }

    #[test]
    fn rejects_newer_versions() {
        let mut zip = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
        zip.start_file("manifest.toml", zip::write::SimpleFileOptions::default())
            .unwrap();
        zip.write_all(b"format_version = 999\n").unwrap();
        let bytes = zip.finish().unwrap().into_inner();
        assert!(read_cecilia(&bytes).is_err());
    }

    fn chant_with_audio() -> Score {
        let chant =
            crate::gabc::parse("name: t;\n%%\n(c4) Al(fg)le(h) (;) lu(g)ia.(f.) (::)\n").unwrap();
        let mut score = crate::gabc::chant_score(chant);
        score.audio = Some(cecilia_core::AudioSync {
            file: Some("a.mp3".into()),
            ..Default::default()
        });
        score.set_chant_sync_point(1, 10_000);
        score.set_chant_sync_point(4, 60_000);
        score.refresh_chant_sync();
        score
    }

    #[test]
    fn a_chant_score_keeps_its_note_ids_and_points() {
        let score = chant_with_audio();
        let ids = score.tracks[0].chant.as_deref().unwrap().note_ids();
        assert_eq!(ids, [1, 2, 3, 4, 5]);
        assert_eq!(score.audio.as_ref().unwrap().points.len(), 2);
        let bytes = write_cecilia(&score).unwrap();
        // Only the note points are saved: the bar points are derived.
        let mut zip = zip::ZipArchive::new(std::io::Cursor::new(&bytes)).unwrap();
        let mut json = String::new();
        zip.by_name("score.json")
            .unwrap()
            .read_to_string(&mut json)
            .unwrap();
        let value: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(value["audio"]["points"].as_array().unwrap().len(), 0);
        assert_eq!(value["audio"]["chant_points"].as_array().unwrap().len(), 2);
        let back = read_cecilia(&bytes).unwrap();
        assert_eq!(back, score);
        let chant = back.tracks[0].chant.as_deref().unwrap();
        assert_eq!(chant.note_ids(), ids);
        let a = back.audio.as_ref().unwrap();
        assert_eq!(a.chant_points, score.audio.as_ref().unwrap().chant_points);
        assert_eq!(a.points, score.audio.as_ref().unwrap().points);
        // The points are on the same notes still after a note is added before them.
        let mut edited = back.clone();
        edited.tracks[0]
            .chant
            .as_deref_mut()
            .unwrap()
            .insert_note(0, false, cecilia_core::chant_edit::Join::NewSyllable, 5)
            .unwrap();
        cecilia_core::chant::retranscribe(&mut edited, 0);
        let c = edited.tracks[0].chant.as_deref().unwrap();
        assert_eq!(c.index_of_id(4), Some(4));
        assert_eq!(edited.audio.as_ref().unwrap().chant_points.len(), 2);
    }

    #[test]
    fn a_modern_score_keeps_its_beat_ids_and_points() {
        use cecilia_core::*;
        let beat = || Beat {
            duration: Duration::new(DurationValue::Quarter),
            notes: vec![Note::new(0, 0)],
            ..Beat::default()
        };
        let mut score = Score {
            tempo: 120,
            master_bars: vec![MasterBar::default(); 1],
            tracks: vec![Track {
                strings: vec![64, 59, 55, 50, 45, 40],
                bars: vec![Bar {
                    voices: vec![Voice {
                        beats: (0..4).map(|_| beat()).collect(),
                    }],
                    ..Default::default()
                }],
                ..Track::default()
            }],
            ..Score::default()
        };
        score.audio = Some(AudioSync::default());
        score.assign_beat_ids();
        let ids: Vec<u32> = score.tracks[0].bars[0].voices[0]
            .beats
            .iter()
            .map(|b| b.id)
            .collect();
        score.set_beat_sync_point(ids[1], 44_100);
        score.set_beat_sync_point(ids[3], 3 * 44_100);
        score.refresh_beat_sync();
        let back = read_cecilia(&write_cecilia(&score).unwrap()).unwrap();
        let back_ids: Vec<u32> = back.tracks[0].bars[0].voices[0]
            .beats
            .iter()
            .map(|b| b.id)
            .collect();
        assert_eq!(back_ids, ids);
        let a = back.audio.as_ref().unwrap();
        assert_eq!(a.beat_points, score.audio.as_ref().unwrap().beat_points);
        // The timing of the notes is derived again on loading.
        assert_eq!(a.note_anchors, score.audio.as_ref().unwrap().note_anchors);
        assert!(!a.note_anchors.is_empty());
    }

    #[test]
    fn an_old_chant_file_without_ids_gets_them() {
        let mut score = chant_with_audio();
        score.audio = None;
        for item in score.tracks[0]
            .chant
            .as_deref_mut()
            .unwrap()
            .syllables
            .iter_mut()
            .flat_map(|s| &mut s.items)
        {
            if let cecilia_core::chant::NeumeItem::Note(n) = item {
                n.id = 0;
            }
        }
        let back = read_cecilia(&write_cecilia(&score).unwrap()).unwrap();
        let ids = back.tracks[0].chant.as_deref().unwrap().note_ids();
        assert!(ids.iter().all(|&i| i != 0));
        assert_eq!(ids.len(), 5);
    }
}
