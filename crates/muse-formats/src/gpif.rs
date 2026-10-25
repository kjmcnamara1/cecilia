//! GPIF, the XML score format inside Guitar Pro 6 (`.gpx`) and Guitar Pro 7/8 (`.gp`) files.

use std::collections::HashMap;

use muse_core::*;
use roxmltree::{Document, Node};

use crate::{Error, Result};

fn err(message: impl Into<String>) -> Error {
    Error::Parse {
        offset: 0,
        location: Some("score.gpif".into()),
        message: message.into(),
    }
}

/// Reads a `.gp` (GP7/8) zip archive.
pub fn read_gp7(bytes: &[u8]) -> Result<Score> {
    let mut zip =
        zip::ZipArchive::new(std::io::Cursor::new(bytes)).map_err(|e| err(e.to_string()))?;
    let mut file = zip
        .by_name("Content/score.gpif")
        .map_err(|e| err(e.to_string()))?;
    let mut xml = String::new();
    std::io::Read::read_to_string(&mut file, &mut xml)?;
    drop(file);
    let mut score = read_gpif(&xml)?;
    // The backing track lives in the archive (Content/Assets/...).
    if let Some(audio) = score.audio.as_mut() {
        if let Some(name) = audio.file.clone() {
            if let Ok(mut entry) = zip.by_name(&name) {
                let mut data = Vec::new();
                std::io::Read::read_to_end(&mut entry, &mut data)?;
                audio.embedded = Some(std::sync::Arc::new(data));
            }
        }
    }
    Ok(score)
}

/// Reads a `.gpx` (GP6) file.
pub fn read_gpx(bytes: &[u8]) -> Result<Score> {
    let files = crate::gpx::read_container(bytes)?;
    let (_, data) = files
        .iter()
        .find(|(name, _)| name == "score.gpif")
        .ok_or_else(|| err("GPX file has no score.gpif"))?;
    read_gpif(&String::from_utf8_lossy(data))
}

fn child<'a>(n: Node<'a, 'a>, name: &str) -> Option<Node<'a, 'a>> {
    n.children().find(|c| c.has_tag_name(name))
}

fn path<'a>(n: Node<'a, 'a>, names: &[&str]) -> Option<Node<'a, 'a>> {
    names.iter().try_fold(n, |n, name| child(n, name))
}

fn text<'a>(n: Node<'a, 'a>, name: &str) -> Option<&'a str> {
    child(n, name).and_then(|c| c.text()).map(str::trim)
}

/// Text content for user-visible strings: keeps spaces, drops layout newlines and tabs.
fn text_or_empty(n: Node, name: &str) -> String {
    child(n, name)
        .and_then(|c| c.text())
        .unwrap_or_default()
        .trim_matches(['\n', '\r', '\t'])
        .to_string()
}

fn num<T: std::str::FromStr>(s: Option<&str>) -> Option<T> {
    s?.trim().parse().ok()
}

fn ids(s: Option<&str>) -> Vec<&str> {
    s.map(|s| s.split_whitespace().collect())
        .unwrap_or_default()
}

/// `<Properties><Property name="X">…</Property></Properties>` as a map.
fn props<'a>(n: Node<'a, 'a>) -> HashMap<&'a str, Node<'a, 'a>> {
    child(n, "Properties")
        .map(|p| {
            p.children()
                .filter(|c| c.has_tag_name("Property"))
                .filter_map(|c| Some((c.attribute("name")?, c)))
                .collect()
        })
        .unwrap_or_default()
}

fn index_by_id<'a>(root: Node<'a, 'a>, section: &str) -> HashMap<&'a str, Node<'a, 'a>> {
    child(root, section)
        .map(|s| {
            s.children()
                .filter(|c| c.is_element())
                .filter_map(|c| Some((c.attribute("id")?, c)))
                .collect()
        })
        .unwrap_or_default()
}

struct TrackInfo {
    /// Open-string pitches from lowest to highest, as GPIF lists them.
    tuning: Vec<u8>,
    /// GP7 drum kits: output MIDI note per instrument articulation index.
    articulations: Vec<u8>,
    /// Notation sounds an octave below where it is written (`<Transpose><Octave>-1`).
    octave_down: bool,
    diagrams: HashMap<String, Chord>,
}

pub fn read_gpif(xml: &str) -> Result<Score> {
    let doc = Document::parse(xml).map_err(|e| err(e.to_string()))?;
    let root = doc.root_element();
    if !root.has_tag_name("GPIF") {
        return Err(err("not a GPIF document"));
    }
    let mut score = Score {
        tempo: 120,
        ..Score::default()
    };
    if let Some(v) = text(root, "GPVersion").or(text(root, "GPRevision")) {
        score.extras.insert("gpif.version".into(), v.to_string());
    }
    if let Some(s) = child(root, "Score") {
        let m = &mut score.meta;
        m.title = text_or_empty(s, "Title");
        m.subtitle = text_or_empty(s, "SubTitle");
        m.artist = text_or_empty(s, "Artist");
        m.album = text_or_empty(s, "Album");
        m.words = text_or_empty(s, "Words");
        m.music = text_or_empty(s, "Music");
        m.copyright = text_or_empty(s, "Copyright");
        m.tab_author = text_or_empty(s, "Tabber");
        m.instructions = text_or_empty(s, "Instructions");
        m.notices = child(s, "Notices")
            .and_then(|n| n.text())
            .filter(|n| !n.is_empty())
            .map(|n| n.split('\n').map(String::from).collect())
            .unwrap_or_default();
    }

    let rhythms: HashMap<&str, Duration> = index_by_id(root, "Rhythms")
        .into_iter()
        .map(|(id, n)| (id, rhythm(n)))
        .collect();
    let notes = index_by_id(root, "Notes");
    let beats = index_by_id(root, "Beats");
    let voices = index_by_id(root, "Voices");
    let bars = index_by_id(root, "Bars");

    let mut infos = Vec::new();
    for t in child(root, "Tracks")
        .into_iter()
        .flat_map(|t| t.children().filter(|c| c.has_tag_name("Track")))
    {
        let (track, info) = read_track(t);
        if score.lyrics.is_none() {
            let lines: Vec<LyricLine> = path(t, &["Lyrics"])
                .into_iter()
                .flat_map(|l| l.children().filter(|c| c.has_tag_name("Line")))
                .filter_map(|l| {
                    let line = child(l, "Text")?.text()?.to_string();
                    let start_bar = num(text(l, "Offset")).unwrap_or(0);
                    (!line.is_empty()).then_some(LyricLine {
                        start_bar,
                        text: line,
                    })
                })
                .collect();
            if !lines.is_empty() {
                score.lyrics = Some(Lyrics {
                    track: Some(score.tracks.len()),
                    lines,
                });
            }
        }
        // Multi-staff tracks (e.g. a piano grand staff) list one bar per staff; each further
        // staff becomes a track marked as part of the same instrument.
        let staves = staff_count(t);
        for staff in 1..staves {
            let mut extra = track.clone();
            extra.name = format!("{} ({})", track.name.trim(), staff + 1);
            extra.extra_staff = true;
            score.tracks.push(extra);
            infos.push(TrackInfo {
                tuning: info.tuning.clone(),
                articulations: info.articulations.clone(),
                octave_down: info.octave_down,
                diagrams: info.diagrams.clone(),
            });
        }
        score.tracks.insert(score.tracks.len() + 1 - staves, track);
        infos.insert(infos.len() + 1 - staves, info);
    }

    for mb_node in child(root, "MasterBars")
        .into_iter()
        .flat_map(|m| m.children().filter(|c| c.has_tag_name("MasterBar")))
    {
        let mb = read_master_bar(mb_node, score.master_bars.last());
        let bar_ids = ids(text(mb_node, "Bars"));
        for (ti, track) in score.tracks.iter_mut().enumerate() {
            let node = bar_ids.get(ti).and_then(|id| bars.get(id));
            if track.bars.is_empty() && track.kind != TrackKind::Drums {
                // Kept for now; reduced to an override of the default once notes are read.
                let down = infos[ti].octave_down;
                track.clef = match node.and_then(|b| text(*b, "Clef")) {
                    Some("F4") => Some(StaffClef::new(true, down)),
                    Some("G2") => Some(StaffClef::new(false, down)),
                    _ => None,
                };
            }
            let bar = node
                .map(|b| read_bar(*b, track, &infos[ti], &voices, &beats, &notes, &rhythms))
                .unwrap_or_else(empty_bar);
            track.bars.push(bar);
        }
        score.master_bars.push(mb);
    }
    if score.master_bars.is_empty() {
        return Err(err("score has no bars"));
    }

    // Keep a track's clef only where it differs from the one muse would choose anyway.
    for t in &mut score.tracks {
        if let Some(c) = t.clef.take() {
            if t.staff_clef() != c {
                t.clef = Some(c);
            }
        }
    }
    score.anacrusis = path(root, &["MasterTrack", "Anacrusis"]).is_some();
    read_automations(root, &mut score);
    Ok(score)
}

/// Number of staves of a track: GP7 lists them; GP6 marks grand-staff instruments "-gs".
fn staff_count(t: Node) -> usize {
    let listed = path(t, &["Staves"]).map_or(0, |s| {
        s.children().filter(|c| c.has_tag_name("Staff")).count()
    });
    if listed > 0 {
        return listed;
    }
    match child(t, "Instrument").and_then(|i| i.attribute("ref")) {
        Some(r) if r.ends_with("-gs") => 2,
        _ => 1,
    }
}

fn rhythm(n: Node) -> Duration {
    let value = match text(n, "NoteValue").unwrap_or("Quarter") {
        "Whole" => DurationValue::Whole,
        "Half" => DurationValue::Half,
        "Eighth" => DurationValue::Eighth,
        "16th" => DurationValue::Sixteenth,
        "32nd" => DurationValue::ThirtySecond,
        "64th" => DurationValue::SixtyFourth,
        "128th" => DurationValue::HundredTwentyEighth,
        _ => DurationValue::Quarter,
    };
    let dots = child(n, "AugmentationDot")
        .and_then(|d| num(d.attribute("count")))
        .unwrap_or(0);
    let tuplet = child(n, "PrimaryTuplet").and_then(|t| {
        let enters: u8 = num(t.attribute("num"))?;
        let times: u8 = num(t.attribute("den"))?;
        (enters > 0 && times > 0 && enters != times).then_some(Tuplet { enters, times })
    });
    Duration {
        value,
        dots,
        tuplet,
    }
}

fn read_track(t: Node) -> (Track, TrackInfo) {
    let mut track = Track {
        name: text_or_empty(t, "Name"),
        fret_count: 24,
        ..Track::default()
    };
    if let Some(c) = text(t, "Color") {
        let v: Vec<u8> = c
            .split_whitespace()
            .filter_map(|x| x.parse().ok())
            .collect();
        if v.len() == 3 {
            track.color = Color {
                r: v[0],
                g: v[1],
                b: v[2],
            };
        }
    }
    // GP6 keeps tuning on the track, GP7 on the first staff.
    let staff = path(t, &["Staves", "Staff"]);
    let p = staff
        .map(props)
        .filter(|p| !p.is_empty())
        .unwrap_or_else(|| props(t));
    let tuning: Vec<u8> = p
        .get("Tuning")
        .and_then(|n| text(*n, "Pitches"))
        .map(|s| {
            s.split_whitespace()
                .filter_map(|x| x.parse().ok())
                .collect()
        })
        .unwrap_or_default();
    track.capo = p
        .get("CapoFret")
        .and_then(|n| num(text(*n, "Fret")))
        .unwrap_or(0);
    if let Some(n) = p
        .get("FretCount")
        .and_then(|n| num(text(*n, "Number").or(text(*n, "Fret"))))
    {
        track.fret_count = n;
    }

    let midi = child(t, "GeneralMidi").or_else(|| child(t, "MidiConnection"));
    if let Some(m) = midi {
        track.midi.channel = num(text(m, "PrimaryChannel")).unwrap_or(0u8).min(15);
        track.midi.effect_channel = num(text(m, "SecondaryChannel")).unwrap_or(1u8).min(15);
        if let Some(p) = num(text(m, "Program")) {
            track.midi.program = p;
        }
    }
    if let Some(sound) = path(t, &["Sounds", "Sound", "MIDI"]) {
        track.midi.program = num(text(sound, "Program")).unwrap_or(track.midi.program);
        track.midi.bank = num(text(sound, "MSB")).unwrap_or(0);
    }
    match text(t, "PlaybackState") {
        Some("Mute") => track.mute = true,
        Some("Solo") => track.solo = true,
        _ => {}
    }

    let drum = child(t, "Instrument").and_then(|i| i.attribute("ref")) == Some("drmkt")
        || child(t, "GeneralMidi").and_then(|g| g.attribute("table")) == Some("Percussion")
        || path(t, &["InstrumentSet", "Type"]).and_then(|n| n.text()) == Some("drumKit")
        || track.midi.channel == 9;
    let articulations: Vec<u8> = path(t, &["InstrumentSet", "Elements"])
        .into_iter()
        .flat_map(|e| e.children().filter(|c| c.has_tag_name("Element")))
        .flat_map(|e| {
            child(e, "Articulations")
                .into_iter()
                .flat_map(|a| a.children().filter(|c| c.has_tag_name("Articulation")))
        })
        .map(|a| num(text(a, "OutputMidiNumber")).unwrap_or(0))
        .collect();
    track.kind = if drum {
        TrackKind::Drums
    } else if tuning.is_empty() {
        TrackKind::Pitched
    } else {
        TrackKind::Fretted
    };
    track.strings = match track.kind {
        TrackKind::Fretted => tuning.iter().rev().copied().collect(),
        TrackKind::Drums => vec![0; DRUM_ROWS],
        _ => Vec::new(),
    };

    let mut diagrams = HashMap::new();
    if let Some(items) = p.get("DiagramCollection").and_then(|d| child(*d, "Items")) {
        let n = tuning.len();
        for item in items.children().filter(|c| c.has_tag_name("Item")) {
            let (Some(id), Some(d)) = (item.attribute("id"), child(item, "Diagram")) else {
                continue;
            };
            let mut chord = Chord {
                name: item.attribute("name").unwrap_or_default().to_string(),
                first_fret: num(d.attribute("baseFret")).unwrap_or(0u8),
                frets: vec![None; n],
                barres: vec![],
            };
            for f in d.children().filter(|c| c.has_tag_name("Fret")) {
                let (Some(s), Some(fret)) = (
                    num::<usize>(f.attribute("string")),
                    num::<u8>(f.attribute("fret")),
                ) else {
                    continue;
                };
                if s < n {
                    chord.frets[n - 1 - s] = Some(fret);
                }
            }
            diagrams.insert(id.to_string(), chord);
        }
    }
    let fretted = track.kind == TrackKind::Fretted;
    (
        track,
        TrackInfo {
            tuning,
            articulations,
            diagrams,
            // GP7 states the notation octave; GP6 writes fretted instruments an octave up.
            octave_down: match (
                num::<i32>(path(t, &["Transpose", "Octave"]).and_then(|n| n.text())),
                num::<i32>(path(t, &["PartSounding", "TranspositionPitch"]).and_then(|n| n.text())),
            ) {
                (Some(o), _) => o < 0,
                (None, Some(p)) => p <= -12,
                (None, None) => fretted,
            },
        },
    )
}

fn read_master_bar(n: Node, previous: Option<&MasterBar>) -> MasterBar {
    let mut mb = MasterBar {
        time_signature: previous.map(|p| p.time_signature).unwrap_or_default(),
        ..MasterBar::default()
    };
    if let Some((a, b)) = text(n, "Time").and_then(|t| t.split_once('/')) {
        if let (Ok(num), Ok(den)) = (a.parse::<u8>(), b.parse::<u8>()) {
            if num > 0 && den > 0 {
                mb.time_signature = TimeSignature {
                    numerator: num,
                    denominator: den,
                };
            }
        }
    }
    if let Some(k) = child(n, "Key") {
        mb.key_signature = KeySignature {
            accidentals: num(text(k, "AccidentalCount")).unwrap_or(0),
            minor: text(k, "Mode") == Some("Minor"),
        };
    }
    if let Some(r) = child(n, "Repeat") {
        mb.repeat_start = r.attribute("start") == Some("true");
        if r.attribute("end") == Some("true") {
            mb.repeat_end = Some(num(r.attribute("count")).unwrap_or(2u8).max(1));
        }
    }
    for e in ids(text(n, "AlternateEndings")) {
        if let Ok(i @ 1..=8) = e.parse::<u8>() {
            mb.alternate_endings |= 1 << (i - 1);
        }
    }
    if let Some(s) = child(n, "Section") {
        let text = text_or_empty(s, "Text");
        let title = if text.is_empty() {
            text_or_empty(s, "Letter")
        } else {
            text
        };
        mb.marker = Some(Marker {
            title,
            color: Color { r: 255, g: 0, b: 0 },
        });
    }
    if let Some(d) = child(n, "Directions") {
        for c in d.children().filter(|c| c.is_element()) {
            if let Some(dir) = c.text().and_then(direction_from_name) {
                mb.directions.push(dir);
            }
        }
    }
    mb.double_bar = child(n, "DoubleBar").is_some();
    mb.free_time = child(n, "FreeTime").is_some();
    mb.triplet_feel = match text(n, "TripletFeel") {
        Some("Triplet8th") => TripletFeel::Eighth,
        Some("Triplet16th") => TripletFeel::Sixteenth,
        Some("Dotted8th") => TripletFeel::DottedEighth,
        Some("Dotted16th") => TripletFeel::DottedSixteenth,
        Some("Scottish8th") => TripletFeel::ScottishEighth,
        Some("Scottish16th") => TripletFeel::ScottishSixteenth,
        _ => TripletFeel::None,
    };
    mb
}

pub(crate) const DIRECTION_NAMES: [(Direction, &str); 19] = [
    (Direction::Coda, "Coda"),
    (Direction::DoubleCoda, "DoubleCoda"),
    (Direction::Segno, "Segno"),
    (Direction::SegnoSegno, "SegnoSegno"),
    (Direction::Fine, "Fine"),
    (Direction::DaCapo, "DaCapo"),
    (Direction::DaCapoAlCoda, "DaCapoAlCoda"),
    (Direction::DaCapoAlDoubleCoda, "DaCapoAlDoubleCoda"),
    (Direction::DaCapoAlFine, "DaCapoAlFine"),
    (Direction::DaSegno, "DaSegno"),
    (Direction::DaSegnoAlCoda, "DaSegnoAlCoda"),
    (Direction::DaSegnoAlDoubleCoda, "DaSegnoAlDoubleCoda"),
    (Direction::DaSegnoAlFine, "DaSegnoAlFine"),
    (Direction::DaSegnoSegno, "DaSegnoSegno"),
    (Direction::DaSegnoSegnoAlCoda, "DaSegnoSegnoAlCoda"),
    (
        Direction::DaSegnoSegnoAlDoubleCoda,
        "DaSegnoSegnoAlDoubleCoda",
    ),
    (Direction::DaSegnoSegnoAlFine, "DaSegnoSegnoAlFine"),
    (Direction::DaCoda, "DaCoda"),
    (Direction::DaDoubleCoda, "DaDoubleCoda"),
];

fn direction_from_name(s: &str) -> Option<Direction> {
    DIRECTION_NAMES
        .iter()
        .find(|(_, n)| *n == s.trim())
        .map(|(d, _)| *d)
}

fn empty_bar() -> Bar {
    Bar {
        voices: vec![Voice {
            beats: vec![Beat {
                empty: true,
                ..Beat::rest(Duration::new(DurationValue::Whole))
            }],
        }],
    }
}

fn read_bar(
    bar: Node,
    track: &Track,
    info: &TrackInfo,
    voices: &HashMap<&str, Node>,
    beats: &HashMap<&str, Node>,
    notes: &HashMap<&str, Node>,
    rhythms: &HashMap<&str, Duration>,
) -> Bar {
    let voice_ids = ids(text(bar, "Voices"));
    let last_used = voice_ids.iter().rposition(|v| *v != "-1");
    let Some(last_used) = last_used else {
        return Bar {
            voices: vec![Voice {
                beats: vec![Beat {
                    empty: true,
                    ..Beat::default()
                }],
            }],
        };
    };
    let mut out = Bar::default();
    for vid in &voice_ids[..=last_used] {
        let mut voice = Voice::default();
        if let Some(v) = voices.get(vid) {
            // Grace notes with their duration and on-beat flag, waiting for the next beat.
            let mut pending_grace: Vec<(Note, DurationValue, bool)> = Vec::new();
            for bid in ids(text(*v, "Beats")) {
                let Some(b) = beats.get(bid) else { continue };
                let mut beat = read_beat(*b, track, info, notes, rhythms);
                if let Some(kind) = text(*b, "GraceNotes") {
                    let on_beat = kind == "OnBeat";
                    let duration = beat.duration.value;
                    pending_grace.extend(beat.notes.into_iter().map(|n| (n, duration, on_beat)));
                    continue;
                }
                if !pending_grace.is_empty() && !beat.notes.is_empty() {
                    for (g, grace_duration, grace_on_beat) in pending_grace.drain(..) {
                        let target = beat
                            .notes
                            .iter()
                            .position(|n| n.string == g.string)
                            .unwrap_or(0);
                        beat.notes[target].effects.grace = Some(Grace {
                            fret: g.fret,
                            velocity: g.velocity,
                            duration: grace_duration,
                            dead: g.dead,
                            on_beat: grace_on_beat,
                            transition: if g.effects.hammer_pull {
                                GraceTransition::Hammer
                            } else if !g.effects.slides.is_empty() {
                                GraceTransition::Slide
                            } else {
                                GraceTransition::None
                            },
                        });
                    }
                }
                voice.beats.push(beat);
            }
        }
        if voice.beats.is_empty() {
            voice.beats.push(Beat {
                empty: true,
                ..Beat::default()
            });
        }
        out.voices.push(voice);
    }
    out
}

fn velocity_of(dynamic: Option<&str>) -> u8 {
    match dynamic {
        Some("PPP") => 15,
        Some("PP") => 31,
        Some("P") => 47,
        Some("MP") => 63,
        Some("MF") => 79,
        Some("FF") => 111,
        Some("FFF") => 127,
        _ => 95,
    }
}

fn read_beat(
    b: Node,
    track: &Track,
    info: &TrackInfo,
    notes: &HashMap<&str, Node>,
    rhythms: &HashMap<&str, Duration>,
) -> Beat {
    let mut beat = Beat {
        duration: child(b, "Rhythm")
            .and_then(|r| r.attribute("ref"))
            .and_then(|r| rhythms.get(r))
            .copied()
            .unwrap_or_default(),
        ..Beat::default()
    };
    let velocity = velocity_of(text(b, "Dynamic"));
    let tremolo = match text(b, "Tremolo") {
        Some("1/8") => Some(DurationValue::Eighth),
        Some("1/16") => Some(DurationValue::Sixteenth),
        Some("1/32") => Some(DurationValue::ThirtySecond),
        _ => None,
    };
    for nid in ids(text(b, "Notes")) {
        if let Some(n) = notes.get(nid) {
            if let Some(mut note) = read_note(*n, track, info, velocity, &mut beat.effects) {
                note.effects.tremolo_picking = tremolo;
                beat.notes.push(note);
            }
        }
    }
    beat.text = child(b, "FreeText").map(|_| text_or_empty(b, "FreeText"));
    beat.chord = text(b, "Chord").and_then(|c| info.diagrams.get(c)).cloned();
    beat.lyrics = child(b, "Lyrics")
        .map(|l| {
            l.children()
                .filter(|c| c.has_tag_name("Line"))
                .map(|c| c.text().unwrap_or_default().trim().to_string())
                .collect()
        })
        .unwrap_or_default();
    if beat.lyrics.iter().all(String::is_empty) {
        beat.lyrics.clear();
    }
    let p = props(b);
    let direction = |name: &str| match p.get(name).and_then(|n| text(*n, "Direction")) {
        Some("Up") => StrokeDirection::Up,
        Some("Down") => StrokeDirection::Down,
        _ => StrokeDirection::None,
    };
    let brush = match direction("Brush") {
        StrokeDirection::None => match text(b, "Arpeggio") {
            Some("Up") => StrokeDirection::Up,
            Some("Down") => StrokeDirection::Down,
            _ => StrokeDirection::None,
        },
        d => d,
    };
    if brush != StrokeDirection::None {
        beat.effects.stroke = Some(Stroke {
            direction: brush,
            duration: DurationValue::Sixteenth,
        });
    }
    beat.effects.pick_stroke = direction("PickStroke");
    beat.effects.rasgueado = p.contains_key("Rasgueado");
    if p.contains_key("Slapped") {
        beat.effects.slap = Slap::Slap;
    } else if p.contains_key("Popped") {
        beat.effects.slap = Slap::Pop;
    }
    beat.effects.vibrato = p.contains_key("VibratoWTremBar");
    beat.effects.fade_in = text(b, "Fadding") == Some("FadeIn");
    beat.effects.ottava = match text(b, "Ottavia") {
        Some("8va") => Ottava::Ottava,
        Some("8vb") => Ottava::OttavaBassa,
        Some("15ma") => Ottava::Quindicesima,
        Some("15mb") => Ottava::QuindicesimaBassa,
        _ => Ottava::None,
    };
    if beat.notes.is_empty() && beat.duration.ticks() == 0 {
        beat.empty = true;
    }
    beat
}

const DRUM_ROWS: usize = 6;

/// Display row of a drum note in tablature (0 = top: cymbals, 5 = bottom: kick).
pub(crate) fn drum_row(key: u8) -> u8 {
    muse_core::drums::lane(key)
}

/// GP6 drum kit element and variation to General MIDI percussion key. Element order follows
/// the GP6 "drmkt" instrument; this mapping is approximate.
fn gp6_drum_key(element: usize, variation: usize) -> u8 {
    GP6_DRUM_KEYS
        .get(element)
        .map_or(38, |v| v[variation.min(2)])
}

/// Inverse of [`gp6_drum_key`]: the element and variation that produce a MIDI key.
pub(crate) fn gp6_drum_element(key: u8) -> Option<(usize, usize)> {
    GP6_DRUM_KEYS
        .iter()
        .enumerate()
        .find_map(|(e, v)| v.iter().position(|k| *k == key).map(|var| (e, var)))
}

const GP6_DRUM_KEYS: [[u8; 3]; 17] = [
    [36, 35, 36], // kick
    [38, 40, 37], // snare: hit, rim shot, side stick
    [42, 44, 46], // hi-hat: closed, pedal, open
    [41, 41, 41], // low floor tom
    [43, 43, 43], // high floor tom
    [45, 45, 45], // low tom
    [47, 47, 47], // low-mid tom
    [48, 48, 48], // high-mid tom
    [50, 50, 50], // high tom
    [44, 44, 44], // pedal hi-hat
    [51, 59, 53], // ride: middle, edge, bell
    [49, 49, 49], // crash medium
    [57, 57, 57], // crash high
    [52, 52, 52], // china
    [55, 55, 55], // splash
    [56, 56, 56], // cowbell
    [54, 54, 54], // tambourine
];

fn note_name_to_midi(pitch: Node) -> Option<u8> {
    let step = match text(pitch, "Step")? {
        "C" => 0,
        "D" => 2,
        "E" => 4,
        "F" => 5,
        "G" => 7,
        "A" => 9,
        "B" => 11,
        _ => return None,
    };
    let acc = match text(pitch, "Accidental").unwrap_or("") {
        "#" => 1,
        "##" | "x" => 2,
        "b" => -1,
        "bb" => -2,
        _ => 0,
    };
    let octave: i32 = num(text(pitch, "Octave"))?;
    u8::try_from((octave + 1) * 12 + step + acc).ok()
}

fn harmonic_semitones(fret: f32) -> u8 {
    match fret.round() as i32 {
        12 => 12,
        7 | 19 => 19,
        5 | 24 => 24,
        4 | 9 | 16 => 28,
        3 => 31,
        f => f.clamp(0, 36) as u8,
    }
}

fn read_note(
    n: Node,
    track: &Track,
    info: &TrackInfo,
    velocity: u8,
    beat_fx: &mut BeatEffects,
) -> Option<Note> {
    let p = props(n);
    let pnum = |name: &str, field: &str| p.get(name).and_then(|x| num::<i32>(text(*x, field)));
    let midi = pnum("Midi", "Number")
        .or_else(|| Some(pnum("Octave", "Number")? * 12 + pnum("Tone", "Step")?))
        .or_else(|| {
            p.get("ConcertPitch")
                .and_then(|c| child(*c, "Pitch"))
                .and_then(note_name_to_midi)
                .map(i32::from)
        })
        .and_then(|m| u8::try_from(m).ok());
    let string_fret = pnum("String", "String").zip(pnum("Fret", "Fret"));
    let mut note = Note::new(0, 0);
    note.velocity = velocity;
    match track.kind {
        TrackKind::Fretted => {
            let count = track.strings.len() as i32;
            if let Some((s, f)) = string_fret {
                note.string = (count - 1 - s).clamp(0, count - 1) as u8;
                note.fret = f.clamp(0, 99) as u8;
            } else {
                // Pitch only: put it on the lowest-fret string that can play it.
                let key = midi? as i32 - track.capo as i32;
                let (s, f) = track
                    .strings
                    .iter()
                    .enumerate()
                    .filter(|(_, open)| key >= **open as i32)
                    .map(|(i, open)| (i, key - *open as i32))
                    .min_by_key(|(_, f)| *f)?;
                note.string = s as u8;
                note.fret = f.min(99) as u8;
            }
        }
        TrackKind::Drums => {
            let key = midi
                .or_else(|| {
                    let art: usize = num(text(n, "InstrumentArticulation"))?;
                    info.articulations.get(art).copied().filter(|k| *k > 0)
                })
                .or_else(|| {
                    let e = pnum("Element", "Element")?;
                    Some(gp6_drum_key(
                        e as usize,
                        pnum("Variation", "Variation").unwrap_or(0) as usize,
                    ))
                })
                .or_else(|| {
                    let (s, f) = string_fret?;
                    let open = info.tuning.get(s as usize).copied().unwrap_or(0) as i32;
                    u8::try_from(open + f).ok()
                })?;
            note.fret = key.min(127);
            note.string = drum_row(key);
        }
        _ => note.fret = midi?,
    }

    let tie = child(n, "Tie");
    note.tie = tie.and_then(|t| t.attribute("destination")) == Some("true");
    note.dead = p.contains_key("Muted");
    note.ghost = child(n, "AntiAccent").is_some();
    let accent: u32 = num(text(n, "Accent")).unwrap_or(0);
    note.accent = if accent & 0x04 != 0 {
        Accent::Heavy
    } else if accent & 0x08 != 0 {
        Accent::Normal
    } else {
        Accent::None
    };
    let fx = &mut note.effects;
    fx.staccato = accent & 0x01 != 0;
    fx.palm_mute = p.contains_key("PalmMuted");
    fx.let_ring = child(n, "LetRing").is_some();
    fx.vibrato = child(n, "Vibrato").is_some();
    fx.hammer_pull = p.contains_key("HopoOrigin");
    if let Some(flags) = pnum("Slide", "Flags") {
        let kinds = [
            SlideKind::Shift,
            SlideKind::Legato,
            SlideKind::OutDown,
            SlideKind::OutUp,
            SlideKind::IntoFromBelow,
            SlideKind::IntoFromAbove,
        ];
        fx.slides = kinds
            .into_iter()
            .enumerate()
            .filter(|(i, _)| flags & (1 << i) != 0)
            .map(|(_, k)| k)
            .collect();
    }
    if let Some(kind) = p.get("HarmonicType").and_then(|h| text(*h, "HType")) {
        let hfret: f32 = p
            .get("HarmonicFret")
            .and_then(|h| num(text(*h, "HFret")))
            .unwrap_or(12.0);
        fx.harmonic = match kind {
            "Natural" => Some(Harmonic::Natural),
            "Artificial" => Some(Harmonic::Artificial {
                semitones: harmonic_semitones(hfret),
            }),
            "Pinch" => Some(Harmonic::Pinch),
            "Tap" => Some(Harmonic::Tapped {
                fret: note.fret.saturating_add(hfret.round() as u8),
            }),
            "Semi" | "Feedback" => Some(Harmonic::Semi),
            _ => None,
        };
    }
    if p.contains_key("Bended") {
        let pf = |name: &str| p.get(name).and_then(|x| num::<f32>(text(*x, "Float")));
        let pos =
            |pct: f32| (pct.clamp(0.0, 100.0) * Bend::MAX_POSITION as f32 / 100.0).round() as u8;
        let val = |v: f32| (v / 25.0).round() as i16;
        let mut points = vec![BendPoint {
            position: 0,
            value: val(pf("BendOriginValue").unwrap_or(0.0)),
            vibrato: false,
        }];
        if let Some(m) = pf("BendMiddleValue") {
            points.push(BendPoint {
                position: pos(pf("BendMiddleOffset1").unwrap_or(50.0)),
                value: val(m),
                vibrato: false,
            });
            let second = pos(pf("BendMiddleOffset2").unwrap_or(50.0));
            if second != points[1].position {
                points.push(BendPoint {
                    position: second,
                    value: val(m),
                    vibrato: false,
                });
            }
        }
        points.push(BendPoint {
            position: pos(pf("BendDestinationOffset").unwrap_or(100.0)),
            value: val(pf("BendDestinationValue").unwrap_or(0.0)),
            vibrato: false,
        });
        fx.bend = Some(Bend { points });
    }
    if let Some(t) = num::<i32>(text(n, "Trill")) {
        let open = track
            .strings
            .get(note.string as usize)
            .copied()
            .unwrap_or(0) as i32
            + track.capo as i32;
        let fret = if track.kind == TrackKind::Fretted {
            t - open
        } else {
            t
        };
        fx.trill = Some(Trill {
            fret: fret.clamp(0, 99) as u8,
            speed: DurationValue::Sixteenth,
        });
    }
    if p.contains_key("Tapped") {
        beat_fx.slap = Slap::Tap;
    }
    Some(note)
}

fn read_automations(root: Node, score: &mut Score) {
    let bar_ticks: Vec<u32> = score.master_bars.iter().map(MasterBar::ticks).collect();
    let mut sync = Vec::new();
    for a in path(root, &["MasterTrack", "Automations"])
        .into_iter()
        .flat_map(|a| a.children().filter(|c| c.has_tag_name("Automation")))
    {
        let bar: usize = num(text(a, "Bar")).unwrap_or(0);
        let position: f64 = num(text(a, "Position")).unwrap_or(0.0);
        match text(a, "Type") {
            Some("Tempo") => {
                let mut parts = text(a, "Value").unwrap_or("120").split_whitespace();
                let bpm: f64 = num(parts.next()).unwrap_or(120.0);
                let factor = match parts.next() {
                    Some("1") => 0.5,
                    Some("3") => 1.5,
                    Some("4") => 2.0,
                    Some("5") => 3.0,
                    _ => 1.0,
                };
                let bpm = bpm * factor;
                let Some(&ticks) = bar_ticks.get(bar) else {
                    continue;
                };
                let tick = (position.clamp(0.0, 1.0) * ticks as f64).round() as u32;
                if bar == 0 && tick == 0 {
                    score.tempo = bpm.round().max(1.0) as u32;
                } else {
                    let changes = &mut score.master_bars[bar].tempo_changes;
                    changes.retain(|c| c.tick != tick);
                    changes.push(TempoChange { tick, bpm });
                    changes.sort_by_key(|c| c.tick);
                }
            }
            Some("SyncPoint") => {
                let Some(v) = child(a, "Value") else { continue };
                sync.push(SyncPoint {
                    bar: num(text(v, "BarIndex")).unwrap_or(bar),
                    occurrence: num(text(v, "BarOccurrence")).unwrap_or(0),
                    frame: num(text(v, "FrameOffset")).unwrap_or(0),
                    tempo: num(text(v, "ModifiedTempo"))
                        .or(num(text(v, "OriginalTempo")))
                        .unwrap_or(score.tempo as f64),
                });
            }
            _ => {}
        }
    }
    let backing = child(root, "BackingTrack");
    let asset = path(root, &["Assets", "Asset", "EmbeddedFilePath"])
        .and_then(|n| n.text())
        .map(|s| s.trim().to_string());
    if !sync.is_empty() || asset.is_some() {
        score.audio = Some(AudioSync {
            file: asset,
            frame_padding: backing
                .and_then(|b| num(text(b, "FramePadding")))
                .unwrap_or(0),
            points: sync,
            embedded: None,
        });
    }
}
