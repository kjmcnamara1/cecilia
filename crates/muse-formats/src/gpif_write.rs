//! Guitar Pro 7 (`.gp`) writer: GPIF XML plus the small binary side files GP expects.

use std::collections::HashMap;
use std::fmt::Write as _;
use std::io::Write as _;

use muse_core::*;

use crate::gpif::DIRECTION_NAMES;
use crate::{Error, Result};

/// Encodes a score as a GP7 `.gp` archive.
pub fn write_gp7(score: &Score) -> Result<Vec<u8>> {
    let xml = write_gpif(score);
    let mut zip = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    let options = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);
    let zerr = |e: zip::result::ZipError| Error::Unsupported(e.to_string());
    let tracks = score.parts().len();
    let files: [(&str, Vec<u8>); 6] = [
        ("VERSION", b"7.0".to_vec()),
        ("Content/score.gpif", xml.into_bytes()),
        ("Content/BinaryStylesheet", 0u32.to_be_bytes().to_vec()),
        ("Content/PartConfiguration", part_configuration(score)),
        ("Content/LayoutConfiguration", layout_configuration(tracks)),
        ("Content/Preferences.json", b"{}".to_vec()),
    ];
    for (name, data) in files {
        zip.start_file(name, options).map_err(zerr)?;
        zip.write_all(&data)?;
    }
    // The backing track, stored as is (it is already compressed).
    if let Some((name, data)) = score
        .audio
        .as_ref()
        .and_then(|a| Some((asset_path(a)?, a.embedded.as_ref()?)))
    {
        let stored = options.compression_method(zip::CompressionMethod::Stored);
        zip.start_file(name, stored).map_err(zerr)?;
        zip.write_all(data)?;
    }
    Ok(zip.finish().map_err(zerr)?.into_inner())
}

/// The notated tempo at the start of a bar.
fn original_tempo(score: &Score, bar: usize) -> f64 {
    let mut bpm = score.tempo as f64;
    for (b, mb) in score.master_bars.iter().enumerate() {
        for c in &mb.tempo_changes {
            if b < bar || (b == bar && c.tick == 0) {
                bpm = c.bpm;
            }
        }
    }
    bpm
}

/// Where an embedded backing track lives in the archive.
fn asset_path(audio: &AudioSync) -> Option<String> {
    let file = audio.file.as_deref()?;
    if file.starts_with("Content/Assets/") {
        return Some(file.to_string());
    }
    let name = std::path::Path::new(file).file_name()?.to_string_lossy();
    Some(format!("Content/Assets/{name}"))
}

/// Encodes a score as a GP6 `.gpx` file.
pub fn write_gpx(score: &Score) -> Result<Vec<u8>> {
    let xml = write_gpif_flavour(score, true);
    let part = part_configuration(score);
    let layout = layout_configuration(score.parts().len());
    let empty_style = 0u32.to_be_bytes();
    crate::gpx::write_container(&[
        ("score.gpif", xml.as_bytes()),
        ("BinaryStylesheet", &empty_style),
        ("PartConfiguration", &part),
        ("LayoutConfiguration", &layout),
    ])
}

/// One score view for the whole score plus one per track; each lists staff flags per track
/// (1 = standard notation, 2 = tablature).
fn part_configuration(score: &Score) -> Vec<u8> {
    let flags: Vec<u8> = score
        .parts()
        .iter()
        .map(|p| &score.tracks[p.start])
        .map(|t| if t.kind == TrackKind::Fretted { 3 } else { 1 })
        .collect();
    let mut out = Vec::new();
    out.extend((flags.len() as u32 + 1).to_be_bytes());
    out.push(0);
    out.extend((flags.len() as u32).to_be_bytes());
    out.extend(&flags);
    for f in &flags {
        out.push(0);
        out.extend(1u32.to_be_bytes());
        out.push(*f);
    }
    out.extend(1u32.to_be_bytes());
    out
}

fn layout_configuration(tracks: usize) -> Vec<u8> {
    let mut out = 4u32.to_be_bytes().to_vec();
    out.extend([1, 0]);
    for _ in 0..tracks {
        out.extend([0xFF, 0xFF]);
    }
    out.extend(0u32.to_be_bytes());
    out
}

/// Text content: CDATA, or escaped text when it contains carriage returns (which XML parsers
/// would otherwise normalise away).
fn cdata(s: &str) -> String {
    if s.contains('\r') {
        return s
            .replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('>', "&gt;")
            .replace('\r', "&#13;");
    }
    format!("<![CDATA[{}]]>", s.replace("]]>", "]]]]><![CDATA[>"))
}

const STEPS: [(&str, &str); 12] = [
    ("C", ""),
    ("C", "#"),
    ("D", ""),
    ("D", "#"),
    ("E", ""),
    ("F", ""),
    ("F", "#"),
    ("G", ""),
    ("G", "#"),
    ("A", ""),
    ("A", "#"),
    ("B", ""),
];

fn pitch_xml(midi: u8) -> String {
    let (step, acc) = STEPS[midi as usize % 12];
    format!(
        "<Pitch><Step>{step}</Step><Accidental>{acc}</Accidental><Octave>{}</Octave></Pitch>",
        midi as i32 / 12 - 1
    )
}

fn dynamic(velocity: u8) -> &'static str {
    match velocity {
        0..=22 => "PPP",
        23..=38 => "PP",
        39..=54 => "P",
        55..=70 => "MP",
        71..=86 => "MF",
        87..=102 => "F",
        103..=118 => "FF",
        _ => "FFF",
    }
}

fn note_value(v: DurationValue) -> &'static str {
    match v {
        DurationValue::Whole => "Whole",
        DurationValue::Half => "Half",
        DurationValue::Quarter => "Quarter",
        DurationValue::Eighth => "Eighth",
        DurationValue::Sixteenth => "16th",
        DurationValue::ThirtySecond => "32nd",
        DurationValue::SixtyFourth => "64th",
        DurationValue::HundredTwentyEighth => "128th",
    }
}

/// Inverse of the reader's artificial-harmonic mapping: semitones above the note to the fret
/// where the harmonic is touched (relative to the fretted note).
fn harmonic_fret(semitones: u8) -> u8 {
    match semitones {
        19 => 7,
        24 => 5,
        28 => 4,
        31 => 3,
        s => s,
    }
}

fn instrument_type(t: &Track) -> (&'static str, &'static str) {
    match t.kind {
        TrackKind::Drums => ("drumKit", "Drums"),
        TrackKind::Fretted => match t.midi.program {
            24 => ("nylonGuitar", "Guitar"),
            25 => ("steelGuitar", "Guitar"),
            32..=39 => ("electricBass", "Bass"),
            _ if t.strings.len() <= 5 && t.strings.iter().min().is_some_and(|&p| p < 40) => {
                ("electricBass", "Bass")
            }
            _ => ("electricGuitar", "Guitar"),
        },
        _ => match t.midi.program {
            16..=23 => ("electricOrgan", "Organ"),
            40..=47 => ("violin", "Strings"),
            48..=55 => ("voice", "Voice"),
            _ => ("acousticPiano", "Piano"),
        },
    }
}

#[derive(Default)]
struct Ids {
    /// Write Guitar Pro 6 conventions instead of Guitar Pro 7.
    gp6: bool,
    /// Chord diagrams and drum keys of the track being written.
    chords: Vec<Chord>,
    drum_keys: Vec<u8>,
    bars: String,
    voices: String,
    beats: String,
    notes: String,
    rhythms: Vec<Duration>,
    next_bar: usize,
    next_voice: usize,
    /// Identical notes and beats share one id (as Guitar Pro does), keyed by their XML body.
    note_ids: HashMap<String, usize>,
    beat_ids: HashMap<String, usize>,
}

impl Ids {
    fn add_note(&mut self, body: String) -> usize {
        let next = self.note_ids.len();
        let id = *self.note_ids.entry(body.clone()).or_insert(next);
        if id == next {
            let _ = writeln!(self.notes, "<Note id=\"{id}\">{body}</Note>");
        }
        id
    }

    fn add_beat(&mut self, body: String) -> usize {
        let next = self.beat_ids.len();
        let id = *self.beat_ids.entry(body.clone()).or_insert(next);
        if id == next {
            let _ = writeln!(self.beats, "<Beat id=\"{id}\">{body}</Beat>");
        }
        id
    }

    fn rhythm(&mut self, d: Duration) -> usize {
        match self.rhythms.iter().position(|r| *r == d) {
            Some(i) => i,
            None => {
                self.rhythms.push(d);
                self.rhythms.len() - 1
            }
        }
    }
}

/// Serialises a score as GPIF (Guitar Pro 7 flavour).
pub fn write_gpif(score: &Score) -> String {
    write_gpif_flavour(score, false)
}

/// Serialises a score as GPIF; `gp6` selects Guitar Pro 6 conventions (tuning on the track,
/// instrument references, drum elements) for `.gpx` files.
pub fn write_gpif_flavour(score: &Score, gp6: bool) -> String {
    let mut x = String::with_capacity(1 << 16);
    x.push_str("<?xml version=\"1.0\" encoding=\"utf-8\"?>\n<GPIF>\n");
    if gp6 {
        x.push_str("<GPRevision>7840</GPRevision>\n");
    } else {
        x.push_str("<GPVersion>7</GPVersion>\n");
        x.push_str("<GPRevision required=\"12021\" recommended=\"12023\">12024</GPRevision>\n");
        x.push_str("<Encoding><EncodingDescription>GP7</EncodingDescription></Encoding>\n");
    }
    write_score_info(&mut x, score);

    // Master track: track list and tempo automation.
    // Guitar Pro tracks are parts; a multi-staff part spans several model tracks.
    let parts = score.parts();
    let list: Vec<String> = (0..parts.len()).map(|i| i.to_string()).collect();
    let _ = write!(x, "<MasterTrack>\n<Tracks>{}</Tracks>\n", list.join(" "));
    if score.anacrusis {
        x.push_str("<Anacrusis />\n");
    }
    x.push_str("<Automations>\n");
    let tempo = |x: &mut String, bar: usize, pos: f64, bpm: f64| {
        let _ = writeln!(
            x,
            "<Automation><Type>Tempo</Type><Linear>false</Linear><Bar>{bar}</Bar><Position>{pos}</Position><Visible>true</Visible><Value>{bpm} 2</Value></Automation>"
        );
    };
    tempo(&mut x, 0, 0.0, score.tempo as f64);
    for (b, mb) in score.master_bars.iter().enumerate() {
        for c in &mb.tempo_changes {
            tempo(&mut x, b, c.tick as f64 / mb.ticks().max(1) as f64, c.bpm);
        }
    }
    let audio = score.audio.as_ref().filter(|_| !gp6);
    for p in audio.map(|a| a.points.as_slice()).unwrap_or_default() {
        let _ = writeln!(
            x,
            "<Automation><Type>SyncPoint</Type><Linear>false</Linear><Bar>{bar}</Bar><Position>0</Position><Visible>true</Visible><Value><BarIndex>{bar}</BarIndex><BarOccurrence>{}</BarOccurrence><ModifiedTempo>{}</ModifiedTempo><OriginalTempo>{}</OriginalTempo><FrameOffset>{}</FrameOffset></Value></Automation>",
            p.occurrence,
            p.tempo,
            original_tempo(score, p.bar),
            p.frame,
            bar = p.bar,
        );
    }
    x.push_str("</Automations>\n</MasterTrack>\n");
    let asset = audio.filter(|a| a.embedded.is_some()).and_then(asset_path);
    if let Some(a) = audio {
        let _ = writeln!(
            x,
            "<BackingTrack><Name><![CDATA[Audio Track]]></Name><ShortName><![CDATA[a.track]]></ShortName><PlaybackState>Default</PlaybackState><Enabled>true</Enabled><Source>Local</Source><AssetId>{}</AssetId><YouTubeVideoUrl></YouTubeVideoUrl><Filter>6</Filter><FramesPerPixel>300</FramesPerPixel><FramePadding>{}</FramePadding><Semitones>0</Semitones><Cents>0</Cents></BackingTrack>",
            if asset.is_some() { "0" } else { "" },
            a.frame_padding,
        );
    }

    x.push_str("<Tracks>\n");
    for (i, part) in parts.iter().enumerate() {
        let t = &score.tracks[part.start];
        if gp6 {
            write_track_gp6(&mut x, score, i, part.clone(), t);
        } else {
            write_track(&mut x, score, i, part.clone(), t);
        }
    }
    x.push_str("</Tracks>\n");

    let mut ids = Ids {
        gp6,
        ..Ids::default()
    };
    let mut master = String::new();
    for (b, mb) in score.master_bars.iter().enumerate() {
        let mut bar_ids = Vec::new();
        for t in &score.tracks {
            bar_ids.push(ids.next_bar);
            ids.chords = chords(t).into_iter().cloned().collect();
            ids.drum_keys = drum_keys(t);
            write_bar(&mut ids, t, t.bars.get(b));
        }
        write_master_bar(&mut master, mb, &bar_ids);
    }
    let _ = write!(x, "<MasterBars>\n{master}</MasterBars>\n");
    let _ = write!(x, "<Bars>\n{}</Bars>\n", ids.bars);
    let _ = write!(x, "<Voices>\n{}</Voices>\n", ids.voices);
    let _ = write!(x, "<Beats>\n{}</Beats>\n", ids.beats);
    let _ = write!(x, "<Notes>\n{}</Notes>\n", ids.notes);
    x.push_str("<Rhythms>\n");
    for (i, d) in ids.rhythms.iter().enumerate() {
        let _ = write!(
            x,
            "<Rhythm id=\"{i}\"><NoteValue>{}</NoteValue>",
            note_value(d.value)
        );
        if d.dots > 0 {
            let _ = write!(x, "<AugmentationDot count=\"{}\" />", d.dots);
        }
        if let Some(tp) = d.tuplet {
            let _ = write!(
                x,
                "<PrimaryTuplet num=\"{}\" den=\"{}\" />",
                tp.enters, tp.times
            );
        }
        x.push_str("</Rhythm>\n");
    }
    x.push_str("</Rhythms>\n");
    if let Some(path) = asset {
        let original = score
            .audio
            .as_ref()
            .and_then(|a| a.file.as_deref())
            .unwrap_or_default();
        let _ = writeln!(
            x,
            "<Assets><Asset id=\"0\"><OriginalFilePath><![CDATA[{original}]]></OriginalFilePath><EmbeddedFilePath><![CDATA[{path}]]></EmbeddedFilePath></Asset></Assets>"
        );
    }
    x.push_str("</GPIF>\n");
    x
}

fn write_score_info(x: &mut String, score: &Score) {
    let m = &score.meta;
    x.push_str("<Score>\n");
    for (tag, value) in [
        ("Title", &m.title),
        ("SubTitle", &m.subtitle),
        ("Artist", &m.artist),
        ("Album", &m.album),
        ("Words", &m.words),
        ("Music", &m.music),
        ("Copyright", &m.copyright),
        ("Tabber", &m.tab_author),
        ("Instructions", &m.instructions),
    ] {
        let _ = writeln!(x, "<{tag}>{}</{tag}>", cdata(value));
    }
    let _ = writeln!(x, "<WordsAndMusic>{}</WordsAndMusic>", cdata(""));
    let _ = writeln!(x, "<Notices>{}</Notices>", cdata(&m.notices.join("\n")));
    x.push_str("<ScoreSystemsDefaultLayout>4</ScoreSystemsDefaultLayout>\n<ScoreSystemsLayout>4</ScoreSystemsLayout>\n");
    x.push_str("</Score>\n");
}

/// GP6 instrument reference for a track.
fn gp6_instrument(t: &Track) -> &'static str {
    match t.kind {
        TrackKind::Drums => "drmkt",
        TrackKind::Fretted => {
            let bass = instrument_type(t).0 == "electricBass";
            match (bass, t.strings.len()) {
                (true, 5) => "e-bass5",
                (true, 6) => "e-bass6",
                (true, _) => "e-bass4",
                (false, 7) => "e-gtr7",
                _ => match t.midi.program {
                    24 => "n-gtr6",
                    25 => "s-gtr6",
                    _ => "e-gtr6",
                },
            }
        }
        _ => match t.midi.program {
            16..=23 => "em-organ-ss",
            48..=55 => "sprn-s",
            _ => "a-piano-ss",
        },
    }
}

fn write_track_gp6(
    x: &mut String,
    score: &Score,
    index: usize,
    part: std::ops::Range<usize>,
    t: &Track,
) {
    let _ = writeln!(x, "<Track id=\"{index}\">");
    let _ = writeln!(x, "<Name>{}</Name>", cdata(&t.name));
    let short: String = t.name.chars().take(8).collect();
    let _ = writeln!(x, "<ShortName>{}</ShortName>", cdata(&short));
    let _ = writeln!(
        x,
        "<Color>{} {} {}</Color>",
        t.color.r, t.color.g, t.color.b
    );
    x.push_str("<SystemsDefautLayout>3</SystemsDefautLayout>\n<SystemsLayout></SystemsLayout>\n<PlayingStyle>Default</PlayingStyle>\n");
    let instrument = gp6_instrument(t);
    // GP6 marks two-staff instruments with "-gs"; other kinds use the piano's.
    let instrument = match instrument.strip_suffix("-ss") {
        Some(base) if part.len() > 1 => format!("{base}-gs"),
        _ if part.len() > 1 => "a-piano-gs".to_string(),
        _ => instrument.to_string(),
    };
    let _ = writeln!(x, "<Instrument ref=\"{instrument}\" />");
    let transposition = if t.kind == TrackKind::Fretted { -12 } else { 0 };
    let _ = writeln!(
        x,
        "<PartSounding><NominalKey>{}</NominalKey><TranspositionPitch>{transposition}</TranspositionPitch></PartSounding>",
        if t.is_percussion() { "Neutral" } else { "C" }
    );
    let (primary, secondary) = if t.is_percussion() {
        (9, 9)
    } else {
        (t.midi.channel.min(15), t.midi.effect_channel.min(15))
    };
    let _ = writeln!(
        x,
        "<GeneralMidi table=\"{}\"><Program>{}</Program><Port>0</Port><PrimaryChannel>{primary}</PrimaryChannel><SecondaryChannel>{secondary}</SecondaryChannel><ForeOneChannelPerString>false</ForeOneChannelPerString></GeneralMidi>",
        if t.is_percussion() {
            "Percussion"
        } else {
            "Instrument"
        },
        t.midi.program
    );
    let state = if t.solo {
        "Solo"
    } else if t.mute {
        "Mute"
    } else {
        "Default"
    };
    let _ = writeln!(x, "<PlaybackState>{state}</PlaybackState>");
    write_track_lyrics(x, score, part.start);
    x.push_str("<Properties>\n");
    let _ = writeln!(
        x,
        "<Property name=\"CapoFret\"><Fret>{}</Fret></Property>",
        t.capo
    );
    let _ = writeln!(
        x,
        "<Property name=\"FretCount\"><Number>{}</Number></Property>",
        t.fret_count.max(1)
    );
    if t.kind == TrackKind::Fretted {
        let pitches: Vec<String> = t.strings.iter().rev().map(|p| p.to_string()).collect();
        let _ = writeln!(
            x,
            "<Property name=\"Tuning\"><Pitches>{}</Pitches></Property>",
            pitches.join(" ")
        );
        write_diagrams(x, t);
    }
    x.push_str("</Properties>\n</Track>\n");
}

fn write_track_lyrics(x: &mut String, score: &Score, index: usize) {
    x.push_str("<Lyrics dispatched=\"true\">\n");
    let lyrics = score
        .lyrics
        .as_ref()
        .filter(|l| l.track.unwrap_or(0) == index);
    for i in 0..5 {
        let line = lyrics.and_then(|l| l.lines.get(i));
        let _ = writeln!(
            x,
            "<Line><Text>{}</Text><Offset>{}</Offset></Line>",
            cdata(line.map_or("", |l| l.text.as_str())),
            line.map_or(0, |l| l.start_bar)
        );
    }
    x.push_str("</Lyrics>\n");
}

fn write_track(
    x: &mut String,
    score: &Score,
    index: usize,
    part: std::ops::Range<usize>,
    t: &Track,
) {
    let (itype, iname) = instrument_type(t);
    let _ = writeln!(x, "<Track id=\"{index}\">");
    let _ = writeln!(x, "<Name>{}</Name>", cdata(&t.name));
    let short: String = t.name.chars().take(8).collect();
    let _ = writeln!(x, "<ShortName>{}</ShortName>", cdata(&short));
    let _ = writeln!(
        x,
        "<Color>{} {} {}</Color>",
        t.color.r, t.color.g, t.color.b
    );
    x.push_str("<SystemsDefautLayout>3</SystemsDefautLayout>\n<SystemsLayout>3</SystemsLayout>\n");
    x.push_str("<PalmMute>0</PalmMute>\n<AutoAccentuation>0</AutoAccentuation>\n<PlayingStyle>Default</PlayingStyle>\n");
    let _ = write!(
        x,
        "<InstrumentSet>\n<Name>{iname}</Name>\n<Type>{itype}</Type>\n<LineCount>5</LineCount>\n<Elements>\n"
    );
    if t.kind == TrackKind::Drums {
        for key in drum_keys(t) {
            let _ = writeln!(
                x,
                "<Element><Name>Drum {key}</Name><Type>snare</Type><SoundbankName></SoundbankName><Articulations><Articulation><Name></Name><StaffLine>{}</StaffLine><Noteheads>noteheadBlack noteheadBlack noteheadBlack</Noteheads><TechniquePlacement>outside</TechniquePlacement><TechniqueSymbol></TechniqueSymbol><InputMidiNumbers>{key}</InputMidiNumbers><OutputMidiNumber>{key}</OutputMidiNumber></Articulation></Articulations></Element>",
                drum_staff_line(key)
            );
        }
    } else {
        x.push_str("<Element><Name>Pitched</Name><Type>pitched</Type><SoundbankName></SoundbankName><Articulations><Articulation><Name></Name><StaffLine>0</StaffLine><Noteheads>noteheadBlack noteheadHalf noteheadWhole</Noteheads><TechniquePlacement>outside</TechniquePlacement><TechniqueSymbol></TechniqueSymbol><InputMidiNumbers></InputMidiNumbers><OutputMidiNumber>0</OutputMidiNumber></Articulation></Articulations></Element>\n");
    }
    x.push_str("</Elements>\n</InstrumentSet>\n");
    let octave = if t.kind == TrackKind::Fretted { -1 } else { 0 };
    let _ = writeln!(
        x,
        "<Transpose><Chromatic>0</Chromatic><Octave>{octave}</Octave></Transpose>"
    );
    let _ = writeln!(
        x,
        "<Sounds><Sound><Name>{n}</Name><Label>{n}</Label><Path></Path><Role>User</Role><MIDI><LSB>0</LSB><MSB>{}</MSB><Program>{}</Program></MIDI></Sound></Sounds>",
        t.midi.bank,
        t.midi.program,
        n = cdata(iname)
    );
    let (primary, secondary) = if t.is_percussion() {
        (9, 9)
    } else {
        (t.midi.channel.min(15), t.midi.effect_channel.min(15))
    };
    let _ = writeln!(
        x,
        "<MidiConnection><Port>0</Port><PrimaryChannel>{primary}</PrimaryChannel><SecondaryChannel>{secondary}</SecondaryChannel><ForeOneChannelPerString>false</ForeOneChannelPerString></MidiConnection>"
    );
    let state = if t.solo {
        "Solo"
    } else if t.mute {
        "Mute"
    } else {
        "Default"
    };
    let _ = writeln!(
        x,
        "<PlaybackState>{state}</PlaybackState>\n<AudioEngineState>MIDI</AudioEngineState>"
    );

    write_track_lyrics(x, score, part.start);

    x.push_str("<Staves>\n");
    for _ in part {
        x.push_str("<Staff>\n<Properties>\n");
        let _ = writeln!(
            x,
            "<Property name=\"CapoFret\"><Fret>{}</Fret></Property>",
            t.capo
        );
        let _ = writeln!(
            x,
            "<Property name=\"FretCount\"><Number>{}</Number></Property>",
            t.fret_count.max(1)
        );
        if t.kind == TrackKind::Fretted {
            let pitches: Vec<String> = t.strings.iter().rev().map(|p| p.to_string()).collect();
            let _ = writeln!(
                x,
                "<Property name=\"Tuning\"><Pitches>{}</Pitches><Flat></Flat><Instrument>{iname}</Instrument><Label>{}</Label><LabelVisible>true</LabelVisible></Property>",
                pitches.join(" "),
                cdata("")
            );
            write_diagrams(x, t);
        }
        x.push_str("<Name><![CDATA[Standard]]></Name>\n</Properties>\n</Staff>\n");
    }
    x.push_str("</Staves>\n</Track>\n");
}

/// Chord diagrams used in a track, by position of first use.
fn chords(t: &Track) -> Vec<&Chord> {
    let mut out: Vec<&Chord> = Vec::new();
    for c in t
        .bars
        .iter()
        .flat_map(|b| &b.voices)
        .flat_map(|v| &v.beats)
        .filter_map(|b| b.chord.as_ref())
    {
        if !out.contains(&c) {
            out.push(c);
        }
    }
    out
}

fn write_diagrams(x: &mut String, t: &Track) {
    let list = chords(t);
    if list.is_empty() {
        return;
    }
    let n = t.strings.len();
    x.push_str("<Property name=\"DiagramCollection\"><Items>\n");
    for (i, c) in list.iter().enumerate() {
        let name = c
            .name
            .replace('&', "&amp;")
            .replace('"', "&quot;")
            .replace('<', "&lt;");
        let _ = write!(
            x,
            "<Item id=\"{i}\" name=\"{name}\"><Diagram stringCount=\"{n}\" fretCount=\"5\" baseFret=\"{}\" barsStates=\"\">",
            c.first_fret
        );
        for (s, f) in c.frets.iter().enumerate().take(n) {
            if let Some(f) = f {
                let _ = write!(x, "<Fret string=\"{}\" fret=\"{f}\" />", n - 1 - s);
            }
        }
        x.push_str("</Diagram></Item>\n");
    }
    x.push_str("</Items></Property>\n");
}

fn drum_keys(t: &Track) -> Vec<u8> {
    let mut keys: Vec<u8> = t
        .bars
        .iter()
        .flat_map(|b| &b.voices)
        .flat_map(|v| &v.beats)
        .flat_map(|b| &b.notes)
        .flat_map(|n| std::iter::once(n.fret).chain(n.effects.grace.map(|g| g.fret)))
        .collect();
    keys.sort_unstable();
    keys.dedup();
    if keys.is_empty() {
        keys.push(38);
    }
    keys
}

fn drum_staff_line(key: u8) -> i32 {
    match key {
        35 | 36 => 7,
        37..=40 => 3,
        41 | 43 => 6,
        45 | 47 => 4,
        48 | 50 => 2,
        42 | 44 | 46 => -1,
        _ => -2,
    }
}

fn write_master_bar(x: &mut String, mb: &MasterBar, bar_ids: &[usize]) {
    x.push_str("<MasterBar>\n");
    let ks = mb.key_signature;
    let _ = writeln!(
        x,
        "<Key><AccidentalCount>{}</AccidentalCount><Mode>{}</Mode><TransposeAs>{}</TransposeAs></Key>",
        ks.accidentals,
        if ks.minor { "Minor" } else { "Major" },
        if ks.accidentals < 0 {
            "Flats"
        } else {
            "Sharps"
        }
    );
    let _ = writeln!(
        x,
        "<Time>{}/{}</Time>",
        mb.time_signature.numerator, mb.time_signature.denominator
    );
    if mb.repeat_start || mb.repeat_end.is_some() {
        let _ = writeln!(
            x,
            "<Repeat start=\"{}\" end=\"{}\" count=\"{}\" />",
            mb.repeat_start,
            mb.repeat_end.is_some(),
            mb.repeat_end.unwrap_or(0)
        );
    }
    if mb.alternate_endings != 0 {
        let endings: Vec<String> = (0..8)
            .filter(|i| mb.alternate_endings & (1 << i) != 0)
            .map(|i| (i + 1).to_string())
            .collect();
        let _ = writeln!(
            x,
            "<AlternateEndings>{}</AlternateEndings>",
            endings.join(" ")
        );
    }
    if let Some(m) = &mb.marker {
        let _ = writeln!(
            x,
            "<Section><Letter>{}</Letter><Text>{}</Text></Section>",
            cdata(""),
            cdata(&m.title)
        );
    }
    if !mb.directions.is_empty() {
        x.push_str("<Directions>");
        for d in &mb.directions {
            let name = DIRECTION_NAMES
                .iter()
                .find(|(dir, _)| dir == d)
                .map_or("", |(_, n)| n);
            let tag = if matches!(
                d,
                Direction::Coda
                    | Direction::DoubleCoda
                    | Direction::Segno
                    | Direction::SegnoSegno
                    | Direction::Fine
            ) {
                "Target"
            } else {
                "Jump"
            };
            let _ = write!(x, "<{tag}>{name}</{tag}>");
        }
        x.push_str("</Directions>\n");
    }
    if mb.double_bar {
        x.push_str("<DoubleBar />\n");
    }
    if mb.free_time {
        x.push_str("<FreeTime />\n");
    }
    let feel = match mb.triplet_feel {
        TripletFeel::None => None,
        TripletFeel::Eighth => Some("Triplet8th"),
        TripletFeel::Sixteenth => Some("Triplet16th"),
        TripletFeel::DottedEighth => Some("Dotted8th"),
        TripletFeel::DottedSixteenth => Some("Dotted16th"),
        TripletFeel::ScottishEighth => Some("Scottish8th"),
        TripletFeel::ScottishSixteenth => Some("Scottish16th"),
    };
    if let Some(f) = feel {
        let _ = writeln!(x, "<TripletFeel>{f}</TripletFeel>");
    }
    let list: Vec<String> = bar_ids.iter().map(|i| i.to_string()).collect();
    let _ = write!(x, "<Bars>{}</Bars>\n</MasterBar>\n", list.join(" "));
}

fn clef(t: &Track) -> &'static str {
    match t.kind {
        TrackKind::Drums => "Neutral",
        TrackKind::Fretted if instrument_type(t).0 == "electricBass" => "F4",
        TrackKind::Pitched | TrackKind::Chant if t.staff_clef() == StaffClef::Bass => "F4",
        _ => "G2",
    }
}

fn write_bar(ids: &mut Ids, t: &Track, bar: Option<&Bar>) {
    let id = ids.next_bar;
    ids.next_bar += 1;
    let mut voice_ids = [-1i64; 4];
    if let Some(bar) = bar {
        for (v, voice) in bar.voices.iter().take(4).enumerate() {
            if voice.beats.iter().all(|b| b.empty) {
                continue;
            }
            voice_ids[v] = ids.next_voice as i64;
            write_voice(ids, t, voice);
        }
    }
    let list: Vec<String> = voice_ids.iter().map(|v| v.to_string()).collect();
    let _ = writeln!(
        ids.bars,
        "<Bar id=\"{id}\"><Clef>{}</Clef><Voices>{}</Voices></Bar>",
        clef(t),
        list.join(" ")
    );
}

fn write_voice(ids: &mut Ids, t: &Track, voice: &Voice) {
    let id = ids.next_voice;
    ids.next_voice += 1;
    let mut beat_ids = Vec::new();
    // Ties and hammer-ons are marked on both notes in GPIF; look ahead per string.
    let beats: Vec<&Beat> = voice.beats.iter().filter(|b| !b.empty).collect();
    let mut hopo_from: HashMap<u8, bool> = HashMap::new();
    for (i, beat) in beats.iter().enumerate() {
        let next = beats.get(i + 1);
        // Grace notes become their own beats before the main beat, one per duration.
        let graces: Vec<(&Note, &Grace)> = beat
            .notes
            .iter()
            .filter_map(|n| n.effects.grace.as_ref().map(|g| (n, g)))
            .collect();
        let mut kinds: Vec<(DurationValue, bool)> = Vec::new();
        for (_, g) in &graces {
            if !kinds.contains(&(g.duration, g.on_beat)) {
                kinds.push((g.duration, g.on_beat));
            }
        }
        for kind in kinds {
            let group: Vec<(&Note, &Grace)> = graces
                .iter()
                .copied()
                .filter(|(_, g)| (g.duration, g.on_beat) == kind)
                .collect();
            beat_ids.push(write_grace_beat(ids, t, &group));
        }
        let tie_origin =
            |n: &Note| next.is_some_and(|b| b.notes.iter().any(|m| m.string == n.string && m.tie));
        beat_ids.push(write_beat(ids, t, beat, &tie_origin, &mut hopo_from));
    }
    let list: Vec<String> = beat_ids.iter().map(|i| i.to_string()).collect();
    let _ = writeln!(
        ids.voices,
        "<Voice id=\"{id}\"><Beats>{}</Beats></Voice>",
        list.join(" ")
    );
}

fn write_grace_beat(ids: &mut Ids, t: &Track, graces: &[(&Note, &Grace)]) -> usize {
    let rhythm = ids.rhythm(Duration::new(graces[0].1.duration));
    let mut note_ids = Vec::new();
    for (n, g) in graces {
        let note = Note {
            fret: g.fret,
            dead: g.dead,
            effects: NoteEffects {
                hammer_pull: g.transition == GraceTransition::Hammer,
                slides: if g.transition == GraceTransition::Slide {
                    vec![SlideKind::Legato]
                } else {
                    vec![]
                },
                ..NoteEffects::default()
            },
            ..Note::new(n.string, g.fret)
        };
        note_ids.push(write_note(ids, t, &note, false, false, false));
    }
    let kind = if graces[0].1.on_beat {
        "OnBeat"
    } else {
        "BeforeBeat"
    };
    let list: Vec<String> = note_ids.iter().map(|i| i.to_string()).collect();
    ids.add_beat(format!(
        "<GraceNotes>{kind}</GraceNotes><Dynamic>{}</Dynamic><Rhythm ref=\"{rhythm}\" /><Notes>{}</Notes>",
        dynamic(graces[0].1.velocity),
        list.join(" ")
    ))
}

fn write_beat(
    ids: &mut Ids,
    t: &Track,
    beat: &Beat,
    tie_origin: &dyn Fn(&Note) -> bool,
    hopo_from: &mut HashMap<u8, bool>,
) -> usize {
    let rhythm = ids.rhythm(beat.duration);
    let mut b = String::new();
    let velocity = beat.notes.first().map_or(DEFAULT_VELOCITY, |n| n.velocity);
    let _ = write!(
        b,
        "<Dynamic>{}</Dynamic><Rhythm ref=\"{rhythm}\" />",
        dynamic(velocity)
    );
    if !beat.notes.is_empty() {
        let mut note_ids = Vec::new();
        for n in &beat.notes {
            let hopo_dest = hopo_from.get(&n.string).copied().unwrap_or(false);
            let tapped = beat.effects.slap == Slap::Tap;
            note_ids.push(write_note(ids, t, n, tie_origin(n), hopo_dest, tapped));
        }
        hopo_from.clear();
        for n in &beat.notes {
            hopo_from.insert(n.string, n.effects.hammer_pull);
        }
        let list: Vec<String> = note_ids.iter().map(|i| i.to_string()).collect();
        let _ = write!(b, "<Notes>{}</Notes>", list.join(" "));
    } else {
        hopo_from.clear();
    }
    if let Some(text) = &beat.text {
        let _ = write!(b, "<FreeText>{}</FreeText>", cdata(text));
    }
    if let Some(c) = &beat.chord {
        if let Some(i) = ids.chords.iter().position(|x| x == c) {
            let _ = write!(b, "<Chord>{i}</Chord>");
        }
    }
    if !beat.lyrics.is_empty() {
        b.push_str("<Lyrics>");
        for l in &beat.lyrics {
            let _ = write!(b, "<Line>{}</Line>", cdata(l));
        }
        b.push_str("</Lyrics>");
    }
    let fx = &beat.effects;
    if fx.fade_in {
        b.push_str("<Fadding>FadeIn</Fadding>");
    }
    let ottava = match fx.ottava {
        Ottava::None => None,
        Ottava::Ottava => Some("8va"),
        Ottava::OttavaBassa => Some("8vb"),
        Ottava::Quindicesima => Some("15ma"),
        Ottava::QuindicesimaBassa => Some("15mb"),
    };
    if let Some(o) = ottava {
        let _ = write!(b, "<Ottavia>{o}</Ottavia>");
    }
    if let Some(d) = beat.notes.iter().find_map(|n| n.effects.tremolo_picking) {
        let v = match d {
            DurationValue::Eighth => "1/8",
            DurationValue::Sixteenth => "1/16",
            _ => "1/32",
        };
        let _ = write!(b, "<Tremolo>{v}</Tremolo>");
    }
    let mut props = String::new();
    let dir = |d: StrokeDirection| {
        if d == StrokeDirection::Up {
            "Up"
        } else {
            "Down"
        }
    };
    if let Some(s) = fx.stroke {
        let _ = write!(
            props,
            "<Property name=\"Brush\"><Direction>{}</Direction></Property>",
            dir(s.direction)
        );
    }
    if fx.pick_stroke != StrokeDirection::None {
        let _ = write!(
            props,
            "<Property name=\"PickStroke\"><Direction>{}</Direction></Property>",
            dir(fx.pick_stroke)
        );
    }
    if fx.rasgueado {
        props.push_str("<Property name=\"Rasgueado\"><Rasgueado>ii_1</Rasgueado></Property>");
    }
    match fx.slap {
        Slap::Slap => props.push_str("<Property name=\"Slapped\"><Enable /></Property>"),
        Slap::Pop => props.push_str("<Property name=\"Popped\"><Enable /></Property>"),
        _ => {}
    }
    if fx.vibrato {
        props.push_str("<Property name=\"VibratoWTremBar\"><Strength>Wide</Strength></Property>");
    }
    if !props.is_empty() {
        let _ = write!(b, "<Properties>{props}</Properties>");
    }
    ids.add_beat(b)
}

fn write_note(
    ids: &mut Ids,
    t: &Track,
    n: &Note,
    tie_origin: bool,
    hopo_dest: bool,
    tapped: bool,
) -> usize {
    let key = t.pitch_of(n);
    let mut x = String::new();
    if tie_origin || n.tie {
        let _ = write!(
            x,
            "<Tie origin=\"{tie_origin}\" destination=\"{}\" />",
            n.tie
        );
    }
    let fx = &n.effects;
    if fx.vibrato {
        x.push_str("<Vibrato>Slight</Vibrato>");
    }
    if fx.let_ring {
        x.push_str("<LetRing />");
    }
    if n.ghost {
        x.push_str("<AntiAccent>Normal</AntiAccent>");
    }
    let accent = u8::from(fx.staccato)
        | match n.accent {
            Accent::Heavy => 0x04,
            Accent::Normal => 0x08,
            Accent::None => 0,
        };
    if accent != 0 {
        let _ = write!(x, "<Accent>{accent}</Accent>");
    }
    if let Some(tr) = fx.trill {
        let open = if t.kind == TrackKind::Fretted {
            key as i32 - n.fret as i32
        } else {
            0
        };
        let _ = write!(x, "<Trill>{}</Trill>", open + tr.fret as i32);
    }
    if t.kind == TrackKind::Drums && !ids.gp6 {
        let art = ids.drum_keys.iter().position(|k| *k == key).unwrap_or(0);
        let _ = write!(x, "<InstrumentArticulation>{art}</InstrumentArticulation>");
    }
    x.push_str("<Properties>");
    if t.kind == TrackKind::Fretted {
        let s = t
            .strings
            .len()
            .saturating_sub(1)
            .saturating_sub(n.string as usize);
        let _ = write!(
            x,
            "<Property name=\"String\"><String>{s}</String></Property><Property name=\"Fret\"><Fret>{}</Fret></Property>",
            n.fret
        );
    }
    if !ids.gp6 {
        let _ = write!(
            x,
            "<Property name=\"Midi\"><Number>{key}</Number></Property><Property name=\"ConcertPitch\">{p}</Property><Property name=\"TransposedPitch\">{p}</Property>",
            p = pitch_xml(key)
        );
    } else if t.kind == TrackKind::Drums {
        if let Some((element, variation)) = crate::gpif::gp6_drum_element(key) {
            let _ = write!(
                x,
                "<Property name=\"Element\"><Element>{element}</Element></Property><Property name=\"Variation\"><Variation>{variation}</Variation></Property>"
            );
        }
        let _ = write!(
            x,
            "<Property name=\"Midi\"><Number>{key}</Number></Property>"
        );
    } else if t.kind != TrackKind::Fretted {
        let _ = write!(
            x,
            "<Property name=\"Octave\"><Number>{}</Number></Property><Property name=\"Tone\"><Step>{}</Step></Property>",
            key / 12,
            key % 12
        );
    }
    if n.dead {
        x.push_str("<Property name=\"Muted\"><Enable /></Property>");
    }
    if fx.palm_mute {
        x.push_str("<Property name=\"PalmMuted\"><Enable /></Property>");
    }
    if fx.hammer_pull {
        x.push_str("<Property name=\"HopoOrigin\"><Enable /></Property>");
    }
    if hopo_dest {
        x.push_str("<Property name=\"HopoDestination\"><Enable /></Property>");
    }
    if tapped {
        x.push_str("<Property name=\"Tapped\"><Enable /></Property>");
    }
    if !fx.slides.is_empty() {
        let kinds = [
            SlideKind::Shift,
            SlideKind::Legato,
            SlideKind::OutDown,
            SlideKind::OutUp,
            SlideKind::IntoFromBelow,
            SlideKind::IntoFromAbove,
        ];
        let flags = kinds
            .iter()
            .enumerate()
            .filter(|(_, k)| fx.slides.contains(k))
            .fold(0u32, |m, (i, _)| m | (1 << i));
        let _ = write!(
            x,
            "<Property name=\"Slide\"><Flags>{flags}</Flags></Property>"
        );
    }
    if let Some(h) = fx.harmonic {
        let (kind, fret) = match h {
            Harmonic::Natural => ("Natural", 12),
            Harmonic::Artificial { semitones } => ("Artificial", harmonic_fret(semitones)),
            Harmonic::Tapped { fret } => ("Tap", fret.saturating_sub(n.fret)),
            Harmonic::Pinch => ("Pinch", 12),
            Harmonic::Semi => ("Semi", 12),
        };
        let _ = write!(
            x,
            "<Property name=\"HarmonicType\"><HType>{kind}</HType></Property><Property name=\"HarmonicFret\"><HFret>{fret}</HFret></Property>"
        );
    }
    if let Some(b) = &fx.bend {
        write_bend(&mut x, b);
    }
    x.push_str("</Properties>");
    ids.add_note(x)
}

fn write_bend(x: &mut String, b: &Bend) {
    let pct = |p: u8| p as f32 * 100.0 / Bend::MAX_POSITION as f32;
    let val = |v: i16| v as f32 * 25.0;
    let (Some(first), Some(last)) = (b.points.first(), b.points.last()) else {
        return;
    };
    x.push_str("<Property name=\"Bended\"><Enable /></Property>");
    let _ = write!(
        x,
        "<Property name=\"BendOriginValue\"><Float>{}</Float></Property>",
        val(first.value)
    );
    if b.points.len() > 2 {
        let m = &b.points[1..b.points.len() - 1];
        let _ = write!(
            x,
            "<Property name=\"BendMiddleValue\"><Float>{}</Float></Property>",
            val(m[0].value)
        );
        let _ = write!(
            x,
            "<Property name=\"BendMiddleOffset1\"><Float>{}</Float></Property>",
            pct(m[0].position)
        );
        let _ = write!(
            x,
            "<Property name=\"BendMiddleOffset2\"><Float>{}</Float></Property>",
            pct(m[m.len() - 1].position)
        );
    }
    let _ = write!(
        x,
        "<Property name=\"BendDestinationValue\"><Float>{}</Float></Property>",
        val(last.value)
    );
    let _ = write!(
        x,
        "<Property name=\"BendDestinationOffset\"><Float>{}</Float></Property>",
        pct(last.position)
    );
}
