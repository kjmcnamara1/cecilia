//! MusicXML (partwise) import and export, including compressed `.mxl`.
//!
//! Fretted tracks carry their tuning in `<staff-details>` and each note's string and fret in
//! `<technical>`, so tablature survives a round trip. Drum tracks use unpitched notes with one
//! score instrument per MIDI key.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::fmt::Write as _;

use muse_core::*;
use roxmltree::{Document, Node};

use crate::{Error, Result};

fn err(message: impl Into<String>) -> Error {
    Error::Parse {
        offset: 0,
        location: Some("musicxml".into()),
        message: message.into(),
    }
}

/// MusicXML divisions per quarter note; equal to the model's ticks so durations map 1:1.
const DIVISIONS: u32 = TICKS_PER_QUARTER;

const STEPS: [(&str, i32); 12] = [
    ("C", 0),
    ("C", 1),
    ("D", 0),
    ("D", 1),
    ("E", 0),
    ("F", 0),
    ("F", 1),
    ("G", 0),
    ("G", 1),
    ("A", 0),
    ("A", 1),
    ("B", 0),
];

/// Spelling of a MIDI pitch, using flats in flat keys.
fn spell(midi: u8, key: i8) -> (&'static str, i32, i32) {
    let pc = midi as usize % 12;
    let octave = midi as i32 / 12 - 1;
    let (step, alter) = STEPS[pc];
    if alter == 1 && (key < 0 || (key == 0 && pc == 10)) {
        const FLAT: [&str; 12] = ["", "D", "", "E", "", "", "G", "", "A", "", "B", ""];
        return (FLAT[pc], -1, octave);
    }
    (step, alter, octave)
}

fn esc(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn type_name(v: DurationValue) -> &'static str {
    match v {
        DurationValue::Whole => "whole",
        DurationValue::Half => "half",
        DurationValue::Quarter => "quarter",
        DurationValue::Eighth => "eighth",
        DurationValue::Sixteenth => "16th",
        DurationValue::ThirtySecond => "32nd",
        DurationValue::SixtyFourth => "64th",
        DurationValue::HundredTwentyEighth => "128th",
    }
}

fn type_value(s: &str) -> Option<DurationValue> {
    Some(match s {
        "whole" => DurationValue::Whole,
        "half" => DurationValue::Half,
        "quarter" => DurationValue::Quarter,
        "eighth" => DurationValue::Eighth,
        "16th" => DurationValue::Sixteenth,
        "32nd" => DurationValue::ThirtySecond,
        "64th" => DurationValue::SixtyFourth,
        "128th" => DurationValue::HundredTwentyEighth,
        _ => return None,
    })
}

/// Display position of a drum on the percussion staff (step, octave).
fn drum_display(key: u8) -> (&'static str, i32) {
    match key {
        35 | 36 => ("F", 4),
        37..=40 => ("C", 5),
        41 | 43 => ("A", 4),
        45 | 47 => ("D", 5),
        48 | 50 => ("E", 5),
        42 | 44 | 46 => ("G", 5),
        _ => ("A", 5),
    }
}

// ---------------------------------------------------------------- export

/// Encodes a score as uncompressed MusicXML 4.0.
pub fn write_musicxml(score: &Score) -> Result<Vec<u8>> {
    let mut x = String::with_capacity(1 << 16);
    x.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
    x.push_str("<!DOCTYPE score-partwise PUBLIC \"-//Recordare//DTD MusicXML 4.0 Partwise//EN\" \"http://www.musicxml.org/dtds/partwise.dtd\">\n");
    x.push_str("<score-partwise version=\"4.0\">\n");
    let m = &score.meta;
    let _ = writeln!(x, "<work><work-title>{}</work-title></work>", esc(&m.title));
    if !m.subtitle.is_empty() {
        let _ = writeln!(x, "<movement-title>{}</movement-title>", esc(&m.subtitle));
    }
    x.push_str("<identification>\n");
    for (kind, value) in [
        ("composer", &m.music),
        ("lyricist", &m.words),
        ("arranger", &m.tab_author),
    ] {
        if !value.is_empty() {
            let _ = writeln!(x, "<creator type=\"{kind}\">{}</creator>", esc(value));
        }
    }
    if !m.artist.is_empty() {
        let _ = writeln!(x, "<creator type=\"artist\">{}</creator>", esc(&m.artist));
    }
    if !m.copyright.is_empty() {
        let _ = writeln!(x, "<rights>{}</rights>", esc(&m.copyright));
    }
    x.push_str("<encoding><software>muse</software><supports element=\"accidental\" type=\"no\"/></encoding>\n</identification>\n");

    x.push_str("<part-list>\n");
    // A multi-staff part (several model tracks) is one MusicXML part with several staves.
    let parts = score.parts();
    for (i, t) in parts.iter().map(|p| &score.tracks[p.start]).enumerate() {
        let id = i + 1;
        let _ = writeln!(
            x,
            "<score-part id=\"P{id}\"><part-name>{}</part-name>",
            esc(&t.name)
        );
        let channel = if t.is_percussion() {
            10
        } else {
            t.midi.channel.min(15) + 1
        };
        let volume = t.midi.volume as f32 * 100.0 / 127.0;
        let pan = (t.midi.pan as f32 - 64.0) * 90.0 / 64.0;
        if t.kind == TrackKind::Drums {
            for key in drum_keys(t) {
                let _ = writeln!(
                    x,
                    "<score-instrument id=\"P{id}-I{key}\"><instrument-name>Drum {key}</instrument-name></score-instrument>"
                );
            }
            for key in drum_keys(t) {
                let _ = writeln!(
                    x,
                    "<midi-instrument id=\"P{id}-I{key}\"><midi-channel>10</midi-channel><midi-program>{}</midi-program><midi-unpitched>{}</midi-unpitched><volume>{volume:.1}</volume><pan>{pan:.0}</pan></midi-instrument>",
                    t.midi.program as u32 + 1,
                    key as u32 + 1
                );
            }
        } else {
            let _ = writeln!(
                x,
                "<score-instrument id=\"P{id}-I1\"><instrument-name>{}</instrument-name></score-instrument>",
                esc(&t.name)
            );
            let _ = writeln!(
                x,
                "<midi-instrument id=\"P{id}-I1\"><midi-channel>{channel}</midi-channel><midi-bank>{}</midi-bank><midi-program>{}</midi-program><volume>{volume:.1}</volume><pan>{pan:.0}</pan></midi-instrument>",
                t.midi.bank as u32 + 1,
                t.midi.program as u32 + 1
            );
        }
        x.push_str("</score-part>\n");
    }
    x.push_str("</part-list>\n");

    for (pi, part) in parts.iter().enumerate() {
        write_part(&mut x, score, pi, part.clone());
    }
    x.push_str("</score-partwise>\n");
    Ok(x.into_bytes())
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
    keys
}

/// A note position: (bar, voice, beat, string).
type Pos = (usize, usize, usize, u8);

/// Connections between consecutive notes on a string, which MusicXML marks on both ends.
#[derive(Default)]
struct Links {
    tie_start: HashSet<Pos>,
    slide_start: HashSet<Pos>,
    slide_stop: HashSet<Pos>,
    hammer_start: HashSet<Pos>,
    hammer_stop: HashSet<Pos>,
}

fn links(t: &Track) -> Links {
    let mut out = Links::default();
    let voices = t.bars.iter().map(|b| b.voices.len()).max().unwrap_or(0);
    for v in 0..voices {
        let mut last: HashMap<u8, (Pos, &Note)> = HashMap::new();
        for (bi, bar) in t.bars.iter().enumerate() {
            let Some(voice) = bar.voices.get(v) else {
                continue;
            };
            for (i, beat) in voice.beats.iter().enumerate() {
                for n in &beat.notes {
                    let here = (bi, v, i, n.string);
                    if let Some(&(prev, pn)) = last.get(&n.string) {
                        if n.tie {
                            out.tie_start.insert(prev);
                        }
                        if pn
                            .effects
                            .slides
                            .iter()
                            .any(|s| matches!(s, SlideKind::Shift | SlideKind::Legato))
                        {
                            out.slide_start.insert(prev);
                            out.slide_stop.insert(here);
                        }
                        if pn.effects.hammer_pull {
                            out.hammer_start.insert(prev);
                            out.hammer_stop.insert(here);
                        }
                    }
                }
                for n in &beat.notes {
                    last.insert(n.string, ((bi, v, i, n.string), n));
                }
            }
        }
    }
    out
}

/// Tuplet bracket start/stop per beat: consecutive beats with the same ratio form a group until
/// they fill one tuplet span (e.g. a quarter-note triplet spans two quarters).
fn tuplet_brackets(beats: &[(Option<usize>, Beat)]) -> Vec<(bool, bool)> {
    let mut out = vec![(false, false); beats.len()];
    let mut group: Option<(Tuplet, u32, u32, usize)> = None; // ratio, span, filled, first index
    for (i, (_, b)) in beats.iter().enumerate() {
        let tp = b.duration.tuplet;
        if let Some((ratio, _, _, first)) = group {
            if tp != Some(ratio) {
                out[first].0 = true;
                out[i - 1].1 = true;
                group = None;
            }
        }
        let Some(tp) = tp else { continue };
        let g = group.get_or_insert((tp, tp.times as u32 * b.duration.value.ticks(), 0, i));
        g.2 += b.duration.ticks();
        if g.2 + 2 >= g.1 {
            out[g.3].0 = true;
            out[i].1 = true;
            group = None;
        }
    }
    if let Some((_, _, _, first)) = group {
        out[first].0 = true;
        out[beats.len() - 1].1 = true;
    }
    out
}

/// Exact length of a duration as a fraction of a whole note (odd tuplets do not divide into
/// whole ticks, so bar filling is done exactly).
fn exact(d: Duration) -> (u64, u64) {
    let mut num = 1u64;
    let mut den = d.value.denominator() as u64;
    // Dots: n dots multiply by (2^(n+1) - 1) / 2^n.
    num *= (1u64 << (d.dots + 1)) - 1;
    den <<= d.dots;
    if let Some(t) = d.tuplet {
        num *= t.times as u64;
        den *= t.enters.max(1) as u64;
    }
    reduce(num, den)
}

fn gcd(a: u64, b: u64) -> u64 {
    if b == 0 { a } else { gcd(b, a % b) }
}

fn reduce(n: u64, d: u64) -> (u64, u64) {
    let g = gcd(n, d).max(1);
    (n / g, d / g)
}

fn add(a: (u64, u64), b: (u64, u64)) -> (u64, u64) {
    reduce(a.0 * b.1 + b.0 * a.1, a.1 * b.1)
}

fn less(a: (u64, u64), b: (u64, u64)) -> bool {
    a.0 * b.1 < b.0 * a.1
}

/// Trims a voice to the bar length and pads it with rests (MuseScore rejects over- or
/// under-full measures). Returns beats with their original index (None for padding).
fn fit_voice(voice: &Voice, ts: TimeSignature) -> Vec<(Option<usize>, Beat)> {
    let bar = reduce(ts.numerator as u64, ts.denominator.max(1) as u64);
    let mut out = Vec::new();
    let mut t = (0u64, 1u64);
    for (i, beat) in voice.beats.iter().enumerate() {
        if beat.empty || !less(t, bar) {
            continue;
        }
        let end = add(t, exact(beat.duration));
        if !less(bar, end) {
            out.push((Some(i), beat.clone()));
            t = end;
        } else {
            // Shorten the beat that crosses the bar line to the longest value that fits.
            let left = reduce(bar.0 * t.1 - t.0 * bar.1, bar.1 * t.1);
            let ticks = (left.0 * TICKS_PER_QUARTER as u64 * 4 / left.1) as u32;
            if let Some(d) = crate::midi::durations_for(ticks)
                .into_iter()
                .find(|d| !less(left, exact(*d)))
            {
                out.push((
                    Some(i),
                    Beat {
                        duration: d,
                        ..beat.clone()
                    },
                ));
                t = add(t, exact(d));
            }
        }
    }
    if t.0 > 0 && less(t, bar) {
        // Pad with plain (non-tuplet) values while they fit exactly.
        let mut left = reduce(bar.0 * t.1 - t.0 * bar.1, bar.1 * t.1);
        while left.0 > 0 {
            let ticks = (left.0 * TICKS_PER_QUARTER as u64 * 4).div_ceil(left.1) as u32;
            let Some(d) = crate::midi::durations_for(ticks.max(1))
                .into_iter()
                .find(|d| !less(left, exact(*d)))
            else {
                break;
            };
            let e = exact(d);
            left = reduce(left.0 * e.1 - e.0 * left.1, left.1 * e.1);
            out.push((None, Beat::rest(d)));
        }
    }
    out
}

fn clef_xml(t: &Track) -> String {
    match t.kind {
        TrackKind::Drums => "<clef><sign>percussion</sign></clef>".into(),
        TrackKind::Fretted => {
            let low = t.strings.iter().copied().min().unwrap_or(40);
            if low < 36 {
                "<clef><sign>F</sign><line>4</line><clef-octave-change>-1</clef-octave-change></clef>".into()
            } else {
                "<clef><sign>G</sign><line>2</line><clef-octave-change>-1</clef-octave-change></clef>".into()
            }
        }
        _ => match t.staff_clef() {
            StaffClef::Bass => "<clef><sign>F</sign><line>4</line></clef>".into(),
            StaffClef::Treble => "<clef><sign>G</sign><line>2</line></clef>".into(),
        },
    }
}

fn write_part(x: &mut String, score: &Score, ti: usize, part: std::ops::Range<usize>) {
    let _ = writeln!(x, "<part id=\"P{}\">", ti + 1);
    let staves: Vec<&Track> = score.tracks[part].iter().collect();
    let t = staves[0];
    let multi = staves.len() > 1;
    let links: Vec<Links> = staves.iter().map(|t| links(t)).collect();
    let lyric_lines = staves
        .iter()
        .flat_map(|t| &t.bars)
        .flat_map(|b| &b.voices)
        .flat_map(|v| &v.beats)
        .map(|b| b.lyrics.len())
        .max()
        .unwrap_or(0);
    let mut ending_open = false;
    for (bi, mb) in score.master_bars.iter().enumerate() {
        let prev = bi.checked_sub(1).map(|p| &score.master_bars[p]);
        if bi == 0 && score.anacrusis {
            x.push_str("<measure number=\"0\" implicit=\"yes\">\n");
        } else {
            let _ = writeln!(x, "<measure number=\"{}\">", bi + 1);
        }

        // Left barline: forward repeat and ending start.
        let ending_start = mb.alternate_endings != 0
            && prev.is_none_or(|p| p.alternate_endings != mb.alternate_endings);
        if mb.repeat_start || ending_start {
            x.push_str("<barline location=\"left\">");
            if mb.repeat_start {
                x.push_str("<bar-style>heavy-light</bar-style><repeat direction=\"forward\"/>");
            }
            if ending_start {
                let _ = write!(
                    x,
                    "<ending number=\"{}\" type=\"start\"/>",
                    ending_numbers(mb.alternate_endings)
                );
                ending_open = true;
            }
            x.push_str("</barline>\n");
        }

        let mut attrs = String::new();
        if bi == 0 {
            let _ = write!(attrs, "<divisions>{DIVISIONS}</divisions>");
        }
        if prev.is_none_or(|p| p.key_signature != mb.key_signature) {
            let ks = mb.key_signature;
            let _ = write!(
                attrs,
                "<key><fifths>{}</fifths><mode>{}</mode></key>",
                ks.accidentals,
                if ks.minor { "minor" } else { "major" }
            );
        }
        if prev.is_none_or(|p| p.time_signature != mb.time_signature) || bi == 0 {
            let ts = mb.time_signature;
            let _ = write!(
                attrs,
                "<time{}><beats>{}</beats><beat-type>{}</beat-type></time>",
                if mb.free_time {
                    " print-object=\"no\""
                } else {
                    ""
                },
                ts.numerator,
                ts.denominator
            );
        }
        if bi == 0 && multi {
            let _ = write!(attrs, "<staves>{}</staves>", staves.len());
            for (k, st) in staves.iter().enumerate() {
                attrs.push_str(&clef_xml(st).replacen(
                    "<clef>",
                    &format!("<clef number=\"{}\">", k + 1),
                    1,
                ));
            }
        } else if bi == 0 {
            attrs.push_str(&clef_xml(t));
            if t.kind == TrackKind::Fretted {
                let _ = write!(
                    attrs,
                    "<staff-details><staff-lines>{}</staff-lines>",
                    t.strings.len()
                );
                for (line, &p) in t.strings.iter().rev().enumerate() {
                    let (step, alter, octave) = spell(p, 0);
                    let _ = write!(
                        attrs,
                        "<staff-tuning line=\"{}\"><tuning-step>{step}</tuning-step>",
                        line + 1
                    );
                    if alter != 0 {
                        let _ = write!(attrs, "<tuning-alter>{alter}</tuning-alter>");
                    }
                    let _ = write!(
                        attrs,
                        "<tuning-octave>{octave}</tuning-octave></staff-tuning>"
                    );
                }
                if t.capo > 0 {
                    let _ = write!(attrs, "<capo>{}</capo>", t.capo);
                }
                attrs.push_str("</staff-details>");
            }
        }
        if !attrs.is_empty() {
            let _ = writeln!(x, "<attributes>{attrs}</attributes>");
        }

        if ti == 0 {
            if let Some(m) = &mb.marker {
                let _ = writeln!(
                    x,
                    "<direction placement=\"above\"><direction-type><rehearsal>{}</rehearsal></direction-type></direction>",
                    esc(&m.title)
                );
            }
            let mut tempos: Vec<(u32, f64)> =
                mb.tempo_changes.iter().map(|c| (c.tick, c.bpm)).collect();
            if bi == 0 {
                tempos.insert(0, (0, score.tempo as f64));
            }
            for (tick, bpm) in tempos {
                let offset = if tick > 0 {
                    format!("<offset>{tick}</offset>")
                } else {
                    String::new()
                };
                let _ = writeln!(
                    x,
                    "<direction placement=\"above\"><direction-type><metronome><beat-unit>quarter</beat-unit><per-minute>{}</per-minute></metronome></direction-type>{offset}<sound tempo=\"{bpm}\"/></direction>",
                    bpm.round()
                );
            }
        }

        let mut written = 0;
        let exact = !(mb.free_time || (bi == 0 && score.anacrusis));
        for (k, (st, links)) in staves.iter().zip(&links).enumerate() {
            let t = *st;
            let staff = multi.then_some(k);
            let bar = t.bars.get(bi);
            let voices: Vec<&Voice> = bar.map(|b| b.voices.iter().collect()).unwrap_or_default();
            for (v, voice) in voices.iter().enumerate() {
                if voice.beats.iter().all(|b| b.empty) && v > 0 {
                    continue;
                }
                if written > 0 {
                    let _ = writeln!(x, "<backup><duration>{written}</duration></backup>");
                }
                written = 0;
                let beats: Vec<(Option<usize>, Beat)> = if exact {
                    fit_voice(voice, mb.time_signature)
                } else {
                    voice
                        .beats
                        .iter()
                        .enumerate()
                        .filter(|(_, b)| !b.empty)
                        .map(|(i, b)| (Some(i), b.clone()))
                        .collect()
                };
                let brackets = tuplet_brackets(&beats);
                for ((i, beat), bracket) in beats.iter().zip(brackets) {
                    let pos = i.map(|i| (bi, v, i));
                    written += write_beat(
                        x,
                        t,
                        ti + 1,
                        mb,
                        beat,
                        pos,
                        (v, staff),
                        links,
                        lyric_lines,
                        bracket,
                    );
                }
            }
            // Every staff of a multi-staff part needs something in each measure.
            if multi && written == 0 {
                let _ = writeln!(
                    x,
                    "<note><rest measure=\"yes\"/><duration>{}</duration><voice>{}</voice><staff>{}</staff></note>",
                    mb.ticks(),
                    1 + 4 * k,
                    k + 1
                );
                written = mb.ticks();
            }
        }
        if written == 0 {
            let _ = writeln!(
                x,
                "<note><rest measure=\"yes\"/><duration>{}</duration><voice>1</voice></note>",
                mb.ticks()
            );
        }

        // Right barline: backward repeat and ending stop.
        let next = score.master_bars.get(bi + 1);
        let ending_stop =
            ending_open && next.is_none_or(|n| n.alternate_endings != mb.alternate_endings);
        if mb.repeat_end.is_some() || ending_stop || mb.double_bar {
            x.push_str("<barline location=\"right\">");
            if mb.repeat_end.is_some() {
                x.push_str("<bar-style>light-heavy</bar-style>");
            } else if mb.double_bar {
                x.push_str("<bar-style>light-light</bar-style>");
            }
            if ending_stop {
                let kind = if mb.repeat_end.is_some() {
                    "stop"
                } else {
                    "discontinue"
                };
                let _ = write!(
                    x,
                    "<ending number=\"{}\" type=\"{kind}\"/>",
                    ending_numbers(mb.alternate_endings)
                );
                ending_open = false;
            }
            if let Some(n) = mb.repeat_end {
                let _ = write!(x, "<repeat direction=\"backward\" times=\"{n}\"/>");
            }
            x.push_str("</barline>\n");
        }
        x.push_str("</measure>\n");
    }
    x.push_str("</part>\n");
}

fn ending_numbers(mask: u8) -> String {
    (0..8)
        .filter(|i| mask & (1 << i) != 0)
        .map(|i| (i + 1).to_string())
        .collect::<Vec<_>>()
        .join(", ")
}

/// Writes one beat (chord or rest) and returns the divisions it occupies.
#[allow(clippy::too_many_arguments)]
fn write_beat(
    x: &mut String,
    t: &Track,
    part: usize,
    mb: &MasterBar,
    beat: &Beat,
    pos: Option<(usize, usize, usize)>,
    (v, staff): (usize, Option<usize>),
    links: &Links,
    lyric_lines: usize,
    (tuplet_start, tuplet_stop): (bool, bool),
) -> u32 {
    let at = |set: &HashSet<Pos>, string: u8| {
        pos.is_some_and(|(b, v, i)| set.contains(&(b, v, i, string)))
    };
    let d = beat.duration;
    let ticks = d.ticks();
    // Voices are numbered across the part: 1–4 on the first staff, 5–8 on the second, …
    let voice = v + 1 + 4 * staff.unwrap_or(0);
    let staff_xml = staff.map_or(String::new(), |k| format!("<staff>{}</staff>", k + 1));
    if let Some(text) = &beat.text {
        let _ = writeln!(
            x,
            "<direction placement=\"above\"><direction-type><words>{}</words></direction-type><voice>{voice}</voice></direction>",
            esc(text)
        );
    }
    let rhythm = |x: &mut String| {
        let _ = write!(x, "<type>{}</type>", type_name(d.value));
        for _ in 0..d.dots {
            x.push_str("<dot/>");
        }
        if let Some(tp) = d.tuplet {
            let _ = write!(
                x,
                "<time-modification><actual-notes>{}</actual-notes><normal-notes>{}</normal-notes></time-modification>",
                tp.enters, tp.times
            );
        }
    };
    let tuplet_xml = || {
        let mut s = String::new();
        if tuplet_start {
            s.push_str("<tuplet type=\"start\" bracket=\"yes\"/>");
        }
        if tuplet_stop {
            s.push_str("<tuplet type=\"stop\"/>");
        }
        s
    };
    if beat.notes.is_empty() {
        let _ = write!(
            x,
            "<note><rest/><duration>{ticks}</duration><voice>{voice}</voice>"
        );
        rhythm(x);
        x.push_str(&staff_xml);
        let tx = tuplet_xml();
        if !tx.is_empty() {
            let _ = write!(x, "<notations>{tx}</notations>");
        }
        x.push_str("</note>\n");
        return ticks;
    }
    // Grace notes come first and take no time.
    for n in &beat.notes {
        if let Some(g) = &n.effects.grace {
            let grace_note = Note {
                fret: g.fret,
                dead: g.dead,
                ..Note::new(n.string, g.fret)
            };
            let _ = write!(
                x,
                "<note><grace{}/>",
                if g.on_beat { "" } else { " slash=\"yes\"" }
            );
            write_pitch(x, t, &grace_note, mb.key_signature.accidentals);
            let _ = write!(
                x,
                "<voice>{voice}</voice><type>{}</type>{staff_xml}",
                type_name(g.duration)
            );
            if t.kind == TrackKind::Fretted {
                let _ = write!(
                    x,
                    "<notations><technical><string>{}</string><fret>{}</fret></technical></notations>",
                    n.string + 1,
                    g.fret
                );
            }
            x.push_str("</note>\n");
        }
    }
    for (k, n) in beat.notes.iter().enumerate() {
        x.push_str("<note>");
        if k > 0 {
            x.push_str("<chord/>");
        }
        if t.kind == TrackKind::Drums {
            let (step, octave) = drum_display(n.fret);
            let _ = write!(
                x,
                "<unpitched><display-step>{step}</display-step><display-octave>{octave}</display-octave></unpitched>"
            );
        } else {
            write_pitch(x, t, n, mb.key_signature.accidentals);
        }
        let _ = write!(x, "<duration>{ticks}</duration>");
        let tie_start = at(&links.tie_start, n.string);
        if n.tie {
            x.push_str("<tie type=\"stop\"/>");
        }
        if tie_start {
            x.push_str("<tie type=\"start\"/>");
        }
        if t.kind == TrackKind::Drums {
            let _ = write!(x, "<instrument id=\"P{part}-I{}\"/>", n.fret);
        }
        let _ = write!(x, "<voice>{voice}</voice>");
        rhythm(x);
        if n.dead {
            x.push_str("<notehead>x</notehead>");
        } else if n.ghost {
            x.push_str("<notehead parentheses=\"yes\">normal</notehead>");
        }
        x.push_str(&staff_xml);
        let mut notations = if k == 0 { tuplet_xml() } else { String::new() };
        if n.tie {
            notations.push_str("<tied type=\"stop\"/>");
        }
        if tie_start {
            notations.push_str("<tied type=\"start\"/>");
        }
        let fx = &n.effects;
        let mut articulations = String::new();
        if fx.staccato {
            articulations.push_str("<staccato/>");
        }
        match n.accent {
            Accent::Normal => articulations.push_str("<accent/>"),
            Accent::Heavy => articulations.push_str("<strong-accent/>"),
            Accent::None => {}
        }
        if !articulations.is_empty() {
            let _ = write!(notations, "<articulations>{articulations}</articulations>");
        }
        let mut technical = String::new();
        if t.kind == TrackKind::Fretted {
            let _ = write!(
                technical,
                "<string>{}</string><fret>{}</fret>",
                n.string + 1,
                n.fret
            );
        }
        if at(&links.hammer_stop, n.string) {
            technical.push_str("<hammer-on type=\"stop\"/>");
        }
        if at(&links.hammer_start, n.string) {
            technical.push_str("<hammer-on type=\"start\">H</hammer-on>");
        }
        match fx.harmonic {
            Some(Harmonic::Natural) => technical.push_str("<harmonic><natural/></harmonic>"),
            Some(_) => technical.push_str("<harmonic><artificial/></harmonic>"),
            None => {}
        }
        if let Some(b) = &fx.bend {
            let peak = b
                .points
                .iter()
                .map(|p| p.value)
                .max_by_key(|v| v.abs())
                .unwrap_or(0);
            let _ = write!(
                technical,
                "<bend><bend-alter>{}</bend-alter></bend>",
                peak as f32 / 2.0
            );
        }
        if fx.palm_mute {
            technical.push_str("<other-technical>P.M.</other-technical>");
        }
        if !technical.is_empty() {
            let _ = write!(notations, "<technical>{technical}</technical>");
        }
        if at(&links.slide_stop, n.string) {
            notations.push_str("<slide type=\"stop\" line-type=\"solid\"/>");
        }
        if at(&links.slide_start, n.string) {
            notations.push_str("<slide type=\"start\" line-type=\"solid\"/>");
        }
        if fx.vibrato || beat.effects.vibrato {
            // MusicXML has no vibrato mark; a wavy line would be read as a trill.
            notations.push_str("<ornaments><other-ornament>vibrato</other-ornament></ornaments>");
        }
        if !notations.is_empty() {
            let _ = write!(x, "<notations>{notations}</notations>");
        }
        if k == 0 {
            for (line, syllable) in beat.lyrics.iter().enumerate().take(lyric_lines) {
                if !syllable.is_empty() {
                    // A trailing hyphen marks a syllable continued by the next one.
                    let (text, syllabic) = match syllable.strip_suffix('-') {
                        Some(t) if !t.is_empty() => (t, "begin"),
                        _ => (syllable.as_str(), "single"),
                    };
                    let _ = write!(
                        x,
                        "<lyric number=\"{}\"><syllabic>{syllabic}</syllabic><text>{}</text></lyric>",
                        line + 1,
                        esc(text)
                    );
                }
            }
        }
        x.push_str("</note>\n");
    }
    ticks
}

fn write_pitch(x: &mut String, t: &Track, n: &Note, key: i8) {
    let (step, alter, octave) = spell(t.pitch_of(n), key);
    let _ = write!(x, "<pitch><step>{step}</step>");
    if alter != 0 {
        let _ = write!(x, "<alter>{alter}</alter>");
    }
    let _ = write!(x, "<octave>{octave}</octave></pitch>");
}

/// Encodes a score as compressed MusicXML (`.mxl`).
pub fn write_mxl(score: &Score) -> Result<Vec<u8>> {
    use std::io::Write;
    let xml = write_musicxml(score)?;
    let mut zip = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    let zerr = |e: zip::result::ZipError| err(e.to_string());
    // The mimetype entry comes first and is stored uncompressed.
    let stored =
        zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
    let deflated = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);
    zip.start_file("mimetype", stored).map_err(zerr)?;
    zip.write_all(b"application/vnd.recordare.musicxml")?;
    zip.start_file("META-INF/container.xml", deflated)
        .map_err(zerr)?;
    zip.write_all(b"<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<container><rootfiles><rootfile full-path=\"score.musicxml\" media-type=\"application/vnd.recordare.musicxml+xml\"/></rootfiles></container>\n")?;
    zip.start_file("score.musicxml", deflated).map_err(zerr)?;
    zip.write_all(&xml)?;
    Ok(zip.finish().map_err(zerr)?.into_inner())
}

// ---------------------------------------------------------------- import

fn child<'a>(n: Node<'a, 'a>, name: &str) -> Option<Node<'a, 'a>> {
    n.children().find(|c| c.has_tag_name(name))
}

fn children<'a>(n: Node<'a, 'a>, name: &'a str) -> impl Iterator<Item = Node<'a, 'a>> + 'a {
    n.children().filter(move |c| c.has_tag_name(name))
}

fn text<'a>(n: Node<'a, 'a>, name: &str) -> Option<&'a str> {
    child(n, name).and_then(|c| c.text()).map(str::trim)
}

fn num<T: std::str::FromStr>(s: Option<&str>) -> Option<T> {
    s?.trim().parse().ok()
}

/// Reads compressed MusicXML (`.mxl`).
pub fn read_mxl(bytes: &[u8]) -> Result<Score> {
    let mut zip =
        zip::ZipArchive::new(std::io::Cursor::new(bytes)).map_err(|e| err(e.to_string()))?;
    let mut read = |name: &str| -> Result<String> {
        let mut f = zip.by_name(name).map_err(|e| err(format!("{name}: {e}")))?;
        let mut s = String::new();
        std::io::Read::read_to_string(&mut f, &mut s)?;
        Ok(s)
    };
    let container = read("META-INF/container.xml")?;
    let doc = Document::parse(&container).map_err(|e| err(e.to_string()))?;
    let root = doc
        .descendants()
        .find(|n| n.has_tag_name("rootfile"))
        .and_then(|n| n.attribute("full-path"))
        .ok_or_else(|| err("no rootfile in container"))?
        .to_string();
    read_musicxml(read(&root)?.as_bytes())
}

/// One note as read, before beats are assembled.
#[derive(Clone)]
struct XmlNote {
    start: u32,
    duration: u32,
    rhythm: Option<Duration>,
    pitch: Option<u8>,
    string_fret: Option<(u8, u8)>,
    tie: bool,
    chord: bool,
    grace: Option<(DurationValue, bool)>,
    note: Note,
    lyrics: Vec<(usize, String)>,
    text: Option<String>,
}

#[derive(Default)]
struct PartInfo {
    name: String,
    program: u8,
    channel: u8,
    volume: u8,
    pan: u8,
    drums: bool,
    /// MIDI key per score-instrument id (drum kits).
    unpitched: HashMap<String, u8>,
}

/// Reads uncompressed MusicXML (partwise).
pub fn read_musicxml(bytes: &[u8]) -> Result<Score> {
    let text_all = String::from_utf8_lossy(bytes);
    let opts = roxmltree::ParsingOptions {
        allow_dtd: true,
        ..Default::default()
    };
    let doc = Document::parse_with_options(&text_all, opts).map_err(|e| err(e.to_string()))?;
    let root = doc.root_element();
    if !root.has_tag_name("score-partwise") {
        return Err(err(format!(
            "unsupported root element <{}>",
            root.tag_name().name()
        )));
    }
    let mut score = Score {
        tempo: 120,
        ..Score::default()
    };
    score.meta.title = child(root, "work")
        .and_then(|w| text(w, "work-title"))
        .or(text(root, "movement-title"))
        .unwrap_or_default()
        .to_string();
    if child(root, "work").is_some() {
        score.meta.subtitle = text(root, "movement-title").unwrap_or_default().to_string();
    }
    if let Some(id) = child(root, "identification") {
        for c in children(id, "creator") {
            let v = c.text().unwrap_or_default().trim().to_string();
            match c.attribute("type") {
                Some("composer") => score.meta.music = v,
                Some("lyricist") => score.meta.words = v,
                Some("arranger") => score.meta.tab_author = v,
                Some("artist") => score.meta.artist = v,
                _ => {}
            }
        }
        score.meta.copyright = text(id, "rights").unwrap_or_default().to_string();
    }

    let mut parts: HashMap<String, PartInfo> = HashMap::new();
    if let Some(pl) = child(root, "part-list") {
        for sp in children(pl, "score-part") {
            let Some(id) = sp.attribute("id") else {
                continue;
            };
            let name = child(sp, "part-name")
                .and_then(|n| n.text())
                .unwrap_or_default()
                .to_string();
            let mut info = PartInfo {
                name,
                volume: 100,
                pan: 64,
                ..PartInfo::default()
            };
            for mi in children(sp, "midi-instrument") {
                if let Some(ch) = num::<u8>(text(mi, "midi-channel")) {
                    info.channel = ch.saturating_sub(1).min(15);
                    info.drums |= ch == 10;
                }
                if let Some(p) = num::<u8>(text(mi, "midi-program")) {
                    info.program = p.saturating_sub(1).min(127);
                }
                if let Some(v) = num::<f32>(text(mi, "volume")) {
                    info.volume = (v * 127.0 / 100.0).round().clamp(0.0, 127.0) as u8;
                }
                if let Some(p) = num::<f32>(text(mi, "pan")) {
                    info.pan = (64.0 + p * 64.0 / 90.0).round().clamp(0.0, 127.0) as u8;
                }
                if let (Some(iid), Some(k)) =
                    (mi.attribute("id"), num::<u8>(text(mi, "midi-unpitched")))
                {
                    info.unpitched.insert(iid.to_string(), k.saturating_sub(1));
                }
            }
            parts.insert(id.to_string(), info);
        }
    }

    for (pi, part) in children(root, "part").enumerate() {
        let info = part
            .attribute("id")
            .and_then(|id| parts.remove(id))
            .unwrap_or_default();
        read_part(&mut score, part, pi, info);
    }
    if score.tracks.is_empty() {
        return Err(err("no parts"));
    }
    Ok(score)
}

fn read_part(score: &mut Score, part: Node, index: usize, info: PartInfo) {
    let mut divisions = DIVISIONS;
    let mut tuning: Vec<u8> = Vec::new();
    let mut capo = 0;
    let mut drums = info.drums;
    // First clef seen on each staff.
    let mut clefs: HashMap<u32, StaffClef> = HashMap::new();
    // Notes per (staff, voice) per bar.
    let mut bars: Vec<BTreeMap<(u32, u32), Vec<XmlNote>>> = Vec::new();
    let mut ts = TimeSignature::default();
    // A hidden time signature marks free time (chant transcriptions).
    let mut free = false;
    let mut ks = KeySignature::default();
    let mut open_ending: Option<u8> = None;
    for (bi, measure) in children(part, "measure").enumerate() {
        if bi == 0 && index == 0 && measure.attribute("implicit") == Some("yes") {
            score.anacrusis = true;
        }
        let mut pos: u32 = 0;
        let mut last_start: u32 = 0;
        let mut voices: BTreeMap<(u32, u32), Vec<XmlNote>> = BTreeMap::new();
        let mut mb = MasterBar {
            time_signature: ts,
            free_time: free,
            key_signature: ks,
            ..MasterBar::default()
        };
        let mut pending_text: Option<String> = None;
        let mut ending_closes = false;
        if let Some(mask) = open_ending {
            mb.alternate_endings |= mask;
        }
        let scale = |d: u32, divisions: u32| {
            (d as u64 * TICKS_PER_QUARTER as u64 / divisions.max(1) as u64) as u32
        };
        for el in measure.children().filter(|c| c.is_element()) {
            match el.tag_name().name() {
                "attributes" => {
                    if let Some(d) = num(text(el, "divisions")) {
                        divisions = d;
                    }
                    if let Some(k) = child(el, "key") {
                        ks = KeySignature {
                            accidentals: num(text(k, "fifths")).unwrap_or(0),
                            minor: text(k, "mode") == Some("minor"),
                        };
                        mb.key_signature = ks;
                    }
                    if let Some(t) = child(el, "time") {
                        if let (Some(b), Some(bt)) =
                            (num::<u8>(text(t, "beats")), num::<u8>(text(t, "beat-type")))
                        {
                            ts = TimeSignature {
                                numerator: b.max(1),
                                denominator: bt.max(1),
                            };
                            mb.time_signature = ts;
                        }
                        free = t.attribute("print-object") == Some("no");
                        mb.free_time = free;
                    }
                    for c in children(el, "clef") {
                        let staff = num(c.attribute("number")).unwrap_or(1);
                        let clef = match text(c, "sign") {
                            Some("G") => Some(StaffClef::Treble),
                            Some("F") => Some(StaffClef::Bass),
                            _ => None,
                        };
                        if let Some(clef) = clef {
                            clefs.entry(staff).or_insert(clef);
                        }
                    }
                    if children(el, "clef").any(|c| text(c, "sign") == Some("percussion")) {
                        drums = true;
                    }
                    if let Some(sd) = child(el, "staff-details") {
                        let mut lines: Vec<(u32, u8)> = children(sd, "staff-tuning")
                            .filter_map(|st| {
                                let line: u32 = num(st.attribute("line"))?;
                                let step = text(st, "tuning-step")?;
                                let alter: i32 = num(text(st, "tuning-alter")).unwrap_or(0);
                                let octave: i32 = num(text(st, "tuning-octave"))?;
                                let pc =
                                    STEPS.iter().position(|(s, a)| *s == step && *a == 0)? as i32;
                                Some((line, ((octave + 1) * 12 + pc + alter).clamp(0, 127) as u8))
                            })
                            .collect();
                        lines.sort();
                        if !lines.is_empty() {
                            tuning = lines.iter().rev().map(|l| l.1).collect();
                        }
                        capo = num(text(sd, "capo")).unwrap_or(capo);
                    }
                }
                "direction" => {
                    for dt in children(el, "direction-type") {
                        if let Some(r) = child(dt, "rehearsal") {
                            let title = r.text().unwrap_or_default().to_string();
                            mb.marker = Some(Marker {
                                title,
                                color: Color::default(),
                            });
                        }
                        if let Some(w) = child(dt, "words") {
                            pending_text = Some(w.text().unwrap_or_default().to_string());
                        }
                    }
                    if let Some(tempo) =
                        child(el, "sound").and_then(|s| num::<f64>(s.attribute("tempo")))
                    {
                        let offset: i64 = num(text(el, "offset")).unwrap_or(0);
                        let tick = scale((pos as i64 + offset).max(0) as u32, divisions);
                        if bi == 0 && tick == 0 && index == 0 {
                            score.tempo = tempo.round().max(1.0) as u32;
                        } else if index == 0 {
                            mb.tempo_changes.retain(|c| c.tick != tick);
                            mb.tempo_changes.push(TempoChange { tick, bpm: tempo });
                        }
                    }
                }
                "barline" => {
                    if let Some(r) = child(el, "repeat") {
                        match r.attribute("direction") {
                            Some("forward") => mb.repeat_start = true,
                            Some("backward") => {
                                mb.repeat_end = Some(num(r.attribute("times")).unwrap_or(2))
                            }
                            _ => {}
                        }
                    }
                    if let Some(e) = child(el, "ending") {
                        let mut mask = 0u8;
                        for n in e
                            .attribute("number")
                            .unwrap_or("")
                            .split([',', ' '])
                            .filter_map(|s| s.trim().parse::<u8>().ok())
                        {
                            if (1..=8).contains(&n) {
                                mask |= 1 << (n - 1);
                            }
                        }
                        mb.alternate_endings |= mask;
                        // An ending covers every bar from its start to its stop.
                        match e.attribute("type") {
                            Some("start") => open_ending = Some(mask),
                            Some("stop") | Some("discontinue") => {
                                ending_closes = true;
                            }
                            _ => {}
                        }
                    }
                    if text(el, "bar-style") == Some("light-light") {
                        mb.double_bar = true;
                    }
                }
                "backup" => pos = pos.saturating_sub(num(text(el, "duration")).unwrap_or(0)),
                "forward" => pos += num::<u32>(text(el, "duration")).unwrap_or(0),
                "note" => {
                    let duration: u32 = num(text(el, "duration")).unwrap_or(0);
                    let chord = child(el, "chord").is_some();
                    let start = if chord { last_start } else { pos };
                    let grace = child(el, "grace").map(|g| {
                        (
                            text(el, "type")
                                .and_then(type_value)
                                .unwrap_or(DurationValue::Eighth),
                            g.attribute("slash") != Some("yes"),
                        )
                    });
                    let staff: u32 = num(text(el, "staff")).unwrap_or(1);
                    let voice: u32 = num(text(el, "voice")).unwrap_or(1);
                    let rhythm = text(el, "type").and_then(type_value).map(|value| {
                        let dots = children(el, "dot").count() as u8;
                        let tuplet = child(el, "time-modification").and_then(|tm| {
                            let enters: u8 = num(text(tm, "actual-notes"))?;
                            let times: u8 = num(text(tm, "normal-notes"))?;
                            (enters != times).then_some(Tuplet { enters, times })
                        });
                        Duration {
                            value,
                            dots,
                            tuplet,
                        }
                    });
                    let pitch = child(el, "pitch").and_then(|p| {
                        let step = text(p, "step")?;
                        let alter: f32 = num(text(p, "alter")).unwrap_or(0.0);
                        let octave: i32 = num(text(p, "octave"))?;
                        let pc = STEPS.iter().position(|(s, a)| *s == step && *a == 0)? as i32;
                        u8::try_from((octave + 1) * 12 + pc + alter.round() as i32).ok()
                    });
                    let pitch = pitch
                        .or_else(|| {
                            child(el, "instrument")
                                .and_then(|i| i.attribute("id"))
                                .and_then(|id| info.unpitched.get(id).copied())
                        })
                        .or_else(|| {
                            // Unpitched without an instrument: use the display position.
                            let u = child(el, "unpitched")?;
                            let pc = STEPS
                                .iter()
                                .position(|(s, a)| Some(*s) == text(u, "display-step") && *a == 0)?
                                as i32;
                            u8::try_from((num::<i32>(text(u, "display-octave"))? + 1) * 12 + pc)
                                .ok()
                        });
                    let technical = child(el, "notations").and_then(|n| child(n, "technical"));
                    let string_fret = technical.and_then(|t| {
                        Some((num::<u8>(text(t, "string"))?, num::<u8>(text(t, "fret"))?))
                    });
                    let notations: Vec<Node> = children(el, "notations").collect();
                    let has = |name: &str| {
                        notations
                            .iter()
                            .any(|n| n.descendants().any(|d| d.has_tag_name(name)))
                    };
                    let mut note = Note::new(0, 0);
                    note.dead = text(el, "notehead") == Some("x");
                    note.ghost = child(el, "notehead")
                        .is_some_and(|h| h.attribute("parentheses") == Some("yes"));
                    note.effects.staccato = has("staccato");
                    note.accent = if has("strong-accent") {
                        Accent::Heavy
                    } else if has("accent") {
                        Accent::Normal
                    } else {
                        Accent::None
                    };
                    note.effects.hammer_pull = notations.iter().any(|n| {
                        n.descendants().any(|d| {
                            (d.has_tag_name("hammer-on") || d.has_tag_name("pull-off"))
                                && d.attribute("type") == Some("start")
                        })
                    });
                    note.effects.palm_mute = notations.iter().any(|n| {
                        n.descendants()
                            .any(|d| d.has_tag_name("other-technical") && d.text() == Some("P.M."))
                    });
                    if has("harmonic") {
                        note.effects.harmonic = Some(if has("natural") {
                            Harmonic::Natural
                        } else {
                            Harmonic::Artificial { semitones: 12 }
                        });
                    }
                    if let Some(alter) = notations
                        .iter()
                        .flat_map(|n| n.descendants())
                        .find(|d| d.has_tag_name("bend-alter"))
                        .and_then(|b| num::<f32>(b.text()))
                    {
                        let value = (alter * 2.0).round() as i16;
                        note.effects.bend = Some(Bend {
                            points: vec![
                                BendPoint {
                                    position: 0,
                                    value: 0,
                                    vibrato: false,
                                },
                                BendPoint {
                                    position: Bend::MAX_POSITION,
                                    value,
                                    vibrato: false,
                                },
                            ],
                        });
                    }
                    if notations.iter().any(|n| {
                        n.descendants().any(|d| {
                            d.has_tag_name("slide") && d.attribute("type") == Some("start")
                        })
                    }) {
                        note.effects.slides = vec![SlideKind::Shift];
                    }
                    note.effects.vibrato = notations.iter().any(|n| {
                        n.descendants().any(|d| {
                            d.has_tag_name("other-ornament") && d.text() == Some("vibrato")
                        })
                    });
                    let lyrics = children(el, "lyric")
                        .filter_map(|l| {
                            let mut t = text(l, "text")?.to_string();
                            if matches!(text(l, "syllabic"), Some("begin" | "middle")) {
                                t.push('-');
                            }
                            Some((
                                num::<usize>(l.attribute("number")).unwrap_or(1).max(1) - 1,
                                t,
                            ))
                        })
                        .collect();
                    let xml = XmlNote {
                        start: scale(start, divisions),
                        duration: scale(duration, divisions),
                        rhythm,
                        pitch: if child(el, "rest").is_some() {
                            None
                        } else {
                            pitch
                        },
                        string_fret,
                        tie: children(el, "tie").any(|t| t.attribute("type") == Some("stop")),
                        chord,
                        grace,
                        note,
                        lyrics,
                        text: if chord || grace.is_some() {
                            None
                        } else {
                            pending_text.take()
                        },
                    };
                    voices.entry((staff, voice)).or_default().push(xml);
                    if grace.is_none() && !chord {
                        last_start = pos;
                        pos += duration;
                    }
                }
                _ => {}
            }
        }
        if ending_closes {
            open_ending = None;
        }
        if index == 0 {
            score.master_bars.push(mb);
        } else if let Some(existing) = score.master_bars.get_mut(bi) {
            // Later parts may carry markers or tempo changes the first part lacks.
            if existing.marker.is_none() {
                existing.marker = mb.marker;
            }
        }
        bars.push(voices);
    }

    let kind = if drums {
        TrackKind::Drums
    } else if !tuning.is_empty() {
        TrackKind::Fretted
    } else {
        TrackKind::Pitched
    };
    let strings = match kind {
        TrackKind::Fretted => tuning,
        TrackKind::Drums => vec![0; 6],
        _ => vec![],
    };
    // Each staff becomes a track; further staves are marked as part of the same instrument.
    let staves: Vec<u32> = {
        let mut s: Vec<u32> = bars.iter().flat_map(|b| b.keys().map(|k| k.0)).collect();
        s.sort_unstable();
        s.dedup();
        if s.is_empty() { vec![1] } else { s }
    };
    for (si, &staff) in staves.iter().enumerate() {
        let mut track = Track {
            name: if si == 0 {
                info.name.clone()
            } else {
                format!("{} ({})", info.name, si + 1)
            },
            kind,
            strings: strings.clone(),
            fret_count: 24,
            capo,
            extra_staff: si > 0,
            clef: if kind == TrackKind::Pitched {
                clefs.get(&staff).copied()
            } else {
                None
            },
            midi: MidiSettings {
                program: info.program,
                channel: info.channel,
                effect_channel: info.channel,
                volume: info.volume,
                pan: info.pan,
                ..MidiSettings::default()
            },
            ..Track::default()
        };
        let mut position = 0;
        for voices in &bars {
            let mut bar = Bar::default();
            for notes in voices.range((staff, 0)..(staff, u32::MAX)).map(|(_, n)| n) {
                bar.voices.push(Voice {
                    beats: build_beats(&track, notes, &mut position),
                });
            }
            if bar.voices.is_empty() {
                bar.voices.push(Voice {
                    beats: vec![Beat::rest(Duration::new(DurationValue::Whole))],
                });
            }
            track.bars.push(bar);
        }
        score.tracks.push(track);
    }
}

/// Assembles one voice's notes into beats, filling gaps with rests.
fn build_beats(track: &Track, notes: &[XmlNote], position: &mut i32) -> Vec<Beat> {
    let mut beats: Vec<Beat> = Vec::new();
    let mut t = 0u32;
    let mut graces: Vec<&XmlNote> = Vec::new();
    let mut i = 0;
    while i < notes.len() {
        let first = &notes[i];
        if first.grace.is_some() {
            graces.push(first);
            i += 1;
            continue;
        }
        let mut group = vec![first];
        let mut j = i + 1;
        while j < notes.len() && notes[j].chord && notes[j].grace.is_none() {
            group.push(&notes[j]);
            j += 1;
        }
        i = j;
        if first.start > t {
            for d in crate::midi::durations_for(first.start - t) {
                beats.push(Beat::rest(d));
            }
        }
        let duration = first
            .rhythm
            .filter(|r| r.ticks().abs_diff(first.duration) <= 2)
            .unwrap_or_else(|| {
                crate::midi::durations_for(first.duration.max(1))
                    .first()
                    .copied()
                    .unwrap_or_default()
            });
        let mut beat = Beat {
            duration,
            text: first.text.clone(),
            ..Beat::default()
        };
        let mut lyrics: Vec<String> = Vec::new();
        for (line, s) in &first.lyrics {
            if lyrics.len() <= *line {
                lyrics.resize(line + 1, String::new());
            }
            lyrics[*line] = s.clone();
        }
        beat.lyrics = lyrics;
        let sounding: Vec<&XmlNote> = group
            .iter()
            .copied()
            .filter(|n| n.pitch.is_some())
            .collect();
        beat.notes = place_notes(track, &sounding, position);
        for g in graces.drain(..) {
            let Some(p) = g.pitch else { continue };
            let placed = place_notes(track, &[g], &mut position.clone());
            let Some(gn) = placed.into_iter().next() else {
                continue;
            };
            let target = beat
                .notes
                .iter()
                .position(|n| n.string == gn.string)
                .unwrap_or(0);
            if let Some(n) = beat.notes.get_mut(target) {
                let (value, on_beat) = g.grace.unwrap();
                let fret = if track.kind == TrackKind::Fretted {
                    gn.fret
                } else {
                    p
                };
                n.effects.grace = Some(Grace {
                    fret,
                    velocity: n.velocity,
                    duration: value,
                    dead: g.note.dead,
                    on_beat,
                    transition: GraceTransition::None,
                });
            }
        }
        t = first.start + beat.duration.ticks().max(1);
        beats.push(beat);
    }
    if beats.is_empty() {
        beats.push(Beat::rest(Duration::new(DurationValue::Whole)));
    }
    beats
}

fn place_notes(track: &Track, notes: &[&XmlNote], position: &mut i32) -> Vec<Note> {
    let with = |n: &XmlNote, string: u8, fret: u8| Note {
        string,
        fret,
        tie: n.tie,
        ..n.note.clone()
    };
    match track.kind {
        TrackKind::Fretted => {
            if notes.iter().all(|n| {
                n.string_fret
                    .is_some_and(|(s, _)| s >= 1 && (s as usize) <= track.strings.len())
            }) {
                return notes
                    .iter()
                    .map(|n| {
                        let (s, f) = n.string_fret.unwrap();
                        with(n, s - 1, f)
                    })
                    .collect();
            }
            let pitches: Vec<u8> = notes.iter().map(|n| n.pitch.unwrap()).collect();
            muse_convert::fret_chord(&pitches, &track.strings, track.capo, position)
                .into_iter()
                .zip(notes)
                .filter_map(|(p, n)| p.map(|(s, f)| with(n, s, f)))
                .collect()
        }
        TrackKind::Drums => notes
            .iter()
            .map(|n| {
                let k = n.pitch.unwrap();
                with(n, crate::gpif::drum_row(k), k)
            })
            .collect(),
        _ => notes.iter().map(|n| with(n, 0, n.pitch.unwrap())).collect(),
    }
}
