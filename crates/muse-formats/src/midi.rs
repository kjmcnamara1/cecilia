//! Standard MIDI File import and export.
//!
//! Export writes the score's performance (repeats expanded) as format 1 at 960 ticks per
//! quarter. Import quantises note onsets to a 32nd-note/triplet grid, builds bars from the time
//! signature events, and fingers guitar and bass parts onto standard tunings.

use std::collections::BTreeMap;

use muse_core::performance::Performance;
use muse_core::*;

use crate::{Error, Result};

fn err(message: impl Into<String>) -> Error {
    Error::Parse {
        offset: 0,
        location: Some("midi".into()),
        message: message.into(),
    }
}

// ---------------------------------------------------------------- export

fn varlen(out: &mut Vec<u8>, mut v: u32) {
    let mut stack = [0u8; 5];
    let mut n = 0;
    loop {
        stack[n] = (v & 0x7F) as u8;
        n += 1;
        v >>= 7;
        if v == 0 {
            break;
        }
    }
    for i in (0..n).rev() {
        out.push(stack[i] | if i > 0 { 0x80 } else { 0 });
    }
}

fn chunk(out: &mut Vec<u8>, kind: &[u8; 4], data: &[u8]) {
    out.extend(kind);
    out.extend((data.len() as u32).to_be_bytes());
    out.extend(data);
}

/// Events at absolute ticks, encoded as a track chunk.
fn track_chunk(mut events: Vec<(u64, u8, Vec<u8>)>) -> Vec<u8> {
    // Sort by tick, then priority (note-offs before note-ons at the same tick).
    events.sort_by_key(|e| (e.0, e.1));
    let mut data = Vec::new();
    let mut last = 0;
    for (tick, _, bytes) in events {
        varlen(&mut data, (tick - last) as u32);
        data.extend(bytes);
        last = tick;
    }
    varlen(&mut data, 0);
    data.extend([0xFF, 0x2F, 0x00]);
    data
}

fn meta(kind: u8, data: &[u8]) -> Vec<u8> {
    let mut v = vec![0xFF, kind];
    varlen(&mut v, data.len() as u32);
    v.extend(data);
    v
}

/// Encodes a score as a format 1 Standard MIDI File.
pub fn write_midi(score: &Score) -> Result<Vec<u8>> {
    let perf = Performance::build(score, &vec![false; score.tracks.len()]);
    let mut out = Vec::new();
    let mut header = Vec::new();
    header.extend(1u16.to_be_bytes());
    header.extend((score.tracks.len() as u16 + 1).to_be_bytes());
    header.extend((TICKS_PER_QUARTER as u16).to_be_bytes());
    chunk(&mut out, b"MThd", &header);

    // Conductor track: title, tempo map, time and key signatures.
    let mut conductor = vec![(0, 0, meta(0x03, score.meta.title.as_bytes()))];
    for t in &perf.tempo {
        let us = (60_000_000.0 / t.bpm).round() as u32;
        conductor.push((t.tick, 1, meta(0x51, &us.to_be_bytes()[1..])));
    }
    let mut prev: Option<&MasterBar> = None;
    for pb in &perf.bars {
        let mb = &score.master_bars[pb.bar];
        if prev.is_none_or(|p| p.time_signature != mb.time_signature) {
            let ts = mb.time_signature;
            let den = (ts.denominator.max(1) as f32).log2() as u8;
            conductor.push((pb.start, 2, meta(0x58, &[ts.numerator, den, 24, 8])));
        }
        if prev.is_none_or(|p| p.key_signature != mb.key_signature) {
            let ks = mb.key_signature;
            conductor.push((
                pb.start,
                2,
                meta(0x59, &[ks.accidentals as u8, ks.minor as u8]),
            ));
        }
        if let Some(m) = &mb.marker {
            conductor.push((pb.start, 3, meta(0x06, m.title.as_bytes())));
        }
        prev = Some(mb);
    }
    chunk(&mut out, b"MTrk", &track_chunk(conductor));

    for (ti, t) in score.tracks.iter().enumerate() {
        let pt = perf.tracks[ti];
        let ch = pt.channel & 15;
        let mut events = vec![(0, 0, meta(0x03, t.name.as_bytes()))];
        events.push((0, 1, vec![0xB0 | ch, 0, pt.bank]));
        events.push((0, 1, vec![0xC0 | ch, pt.program]));
        events.push((0, 1, vec![0xB0 | ch, 7, pt.volume]));
        events.push((0, 1, vec![0xB0 | ch, 10, pt.pan]));
        for n in perf.notes.iter().filter(|n| n.track == ti) {
            events.push((n.start, 3, vec![0x90 | ch, n.key, n.velocity]));
            events.push((n.end, 2, vec![0x80 | ch, n.key, 0]));
        }
        chunk(&mut out, b"MTrk", &track_chunk(events));
    }
    Ok(out)
}

// ---------------------------------------------------------------- import

struct RawNote {
    channel: u8,
    key: u8,
    velocity: u8,
    start: u64,
    end: u64,
}

#[derive(Default)]
struct RawTrack {
    name: String,
    program: Option<u8>,
    notes: Vec<RawNote>,
}

struct Smf {
    division: u32,
    tracks: Vec<RawTrack>,
    tempos: Vec<(u64, f64)>,
    time_signatures: Vec<(u64, TimeSignature)>,
    keys: Vec<(u64, KeySignature)>,
    title: Option<String>,
}

struct Bytes<'a> {
    data: &'a [u8],
    pos: usize,
}

impl Bytes<'_> {
    fn u8(&mut self) -> Result<u8> {
        let b = *self
            .data
            .get(self.pos)
            .ok_or_else(|| err("unexpected end of track"))?;
        self.pos += 1;
        Ok(b)
    }

    fn take(&mut self, n: usize) -> Result<&[u8]> {
        let end = self
            .pos
            .checked_add(n)
            .filter(|&e| e <= self.data.len())
            .ok_or_else(|| err("unexpected end of track"))?;
        let s = &self.data[self.pos..end];
        self.pos = end;
        Ok(s)
    }

    fn varlen(&mut self) -> Result<u32> {
        let mut v = 0u32;
        for _ in 0..4 {
            let b = self.u8()?;
            v = (v << 7) | (b & 0x7F) as u32;
            if b & 0x80 == 0 {
                return Ok(v);
            }
        }
        Err(err("invalid variable-length number"))
    }
}

fn parse(bytes: &[u8]) -> Result<Smf> {
    let mut r = Bytes {
        data: bytes,
        pos: 0,
    };
    if r.take(4)? != b"MThd" {
        return Err(err("not a MIDI file"));
    }
    let len = u32::from_be_bytes(r.take(4)?.try_into().unwrap()) as usize;
    let header = r.take(len)?;
    if header.len() < 6 {
        return Err(err("short header"));
    }
    let division = u16::from_be_bytes([header[4], header[5]]);
    if division & 0x8000 != 0 || division == 0 {
        return Err(err("SMPTE time division is not supported"));
    }
    let mut smf = Smf {
        division: division as u32,
        tracks: vec![],
        tempos: vec![],
        time_signatures: vec![],
        keys: vec![],
        title: None,
    };
    while r.pos + 8 <= bytes.len() {
        let kind: [u8; 4] = r.take(4)?.try_into().unwrap();
        let len = u32::from_be_bytes(r.take(4)?.try_into().unwrap()) as usize;
        let data = r.take(len.min(bytes.len() - r.pos))?;
        if &kind == b"MTrk" {
            let track = parse_track(data, &mut smf)?;
            smf.tracks.push(track);
        }
    }
    Ok(smf)
}

fn parse_track(data: &[u8], smf: &mut Smf) -> Result<RawTrack> {
    let mut r = Bytes { data, pos: 0 };
    let mut track = RawTrack::default();
    let mut tick = 0u64;
    let mut status = 0u8;
    // Sounding notes per (channel, key): start tick and velocity.
    let mut on: BTreeMap<(u8, u8), Vec<(u64, u8)>> = BTreeMap::new();
    let first_track = smf.tracks.is_empty();
    while r.pos < data.len() {
        tick += r.varlen()? as u64;
        let mut b = r.u8()?;
        if b < 0x80 {
            // Running status: reuse the previous status byte.
            if status == 0 {
                return Err(err("data byte without status"));
            }
            r.pos -= 1;
            b = status;
        }
        match b {
            0xFF => {
                let kind = r.u8()?;
                let len = r.varlen()? as usize;
                let d = r.take(len)?;
                match kind {
                    0x03 => {
                        let name = String::from_utf8_lossy(d).trim().to_string();
                        if first_track && smf.title.is_none() {
                            smf.title = Some(name.clone());
                        }
                        track.name = name;
                    }
                    0x51 if d.len() == 3 => {
                        let us = u32::from_be_bytes([0, d[0], d[1], d[2]]).max(1);
                        smf.tempos.push((tick, 60_000_000.0 / us as f64));
                    }
                    0x58 if d.len() >= 2 => {
                        let ts = TimeSignature {
                            numerator: d[0].max(1),
                            denominator: 1 << d[1].min(6),
                        };
                        smf.time_signatures.push((tick, ts));
                    }
                    0x59 if d.len() >= 2 => {
                        smf.keys.push((
                            tick,
                            KeySignature {
                                accidentals: d[0] as i8,
                                minor: d[1] != 0,
                            },
                        ));
                    }
                    0x2F => break,
                    _ => {}
                }
            }
            0xF0 | 0xF7 => {
                let len = r.varlen()? as usize;
                r.take(len)?;
            }
            _ => {
                status = b;
                let channel = b & 0x0F;
                match b & 0xF0 {
                    0x80 | 0x90 => {
                        let key = r.u8()? & 0x7F;
                        let vel = r.u8()? & 0x7F;
                        if b & 0xF0 == 0x90 && vel > 0 {
                            on.entry((channel, key)).or_default().push((tick, vel));
                        } else if let Some((start, velocity)) = on
                            .get_mut(&(channel, key))
                            .and_then(|v| (!v.is_empty()).then(|| v.remove(0)))
                        {
                            track.notes.push(RawNote {
                                channel,
                                key,
                                velocity,
                                start,
                                end: tick.max(start + 1),
                            });
                        }
                    }
                    0xA0 | 0xB0 | 0xE0 => {
                        r.take(2)?;
                    }
                    0xC0 => {
                        let p = r.u8()?;
                        track.program.get_or_insert(p & 0x7F);
                    }
                    0xD0 => {
                        r.u8()?;
                    }
                    _ => return Err(err(format!("unknown status byte {b:#x}"))),
                }
            }
        }
    }
    // Close notes left sounding at the end of the track.
    for ((channel, key), starts) in on {
        for (start, velocity) in starts {
            track.notes.push(RawNote {
                channel,
                key,
                velocity,
                start,
                end: tick.max(start + 1),
            });
        }
    }
    Ok(track)
}

/// Snaps a tick to the nearer of the 32nd-note and 16th-triplet grids.
fn quantize(tick: u64) -> u64 {
    let snap = |g: u64| (tick + g / 2) / g * g;
    let (a, b) = (snap(120), snap(160));
    if a.abs_diff(tick) <= b.abs_diff(tick) {
        a
    } else {
        b
    }
}

/// Splits a tick length into note values (longest first), using dots and triplets.
pub(crate) fn durations_for(mut ticks: u32) -> Vec<Duration> {
    let mut out = Vec::new();
    let mut candidates: Vec<Duration> = Vec::new();
    for v in DurationValue::ALL {
        candidates.push(Duration {
            value: v,
            dots: 1,
            tuplet: None,
        });
        candidates.push(Duration::new(v));
        candidates.push(Duration {
            value: v,
            dots: 0,
            tuplet: Some(Tuplet::TRIPLET),
        });
    }
    candidates.sort_by_key(|d| std::cmp::Reverse(d.ticks()));
    while ticks > 0 {
        let Some(d) = candidates
            .iter()
            .find(|d| d.ticks() <= ticks && d.ticks() > 0)
        else {
            break;
        };
        out.push(*d);
        ticks -= d.ticks();
    }
    out
}

/// Reads a Standard MIDI File.
pub fn read_midi(bytes: &[u8]) -> Result<Score> {
    let smf = parse(bytes)?;
    let scale = |t: u64| t * TICKS_PER_QUARTER as u64 / smf.division as u64;

    let mut score = Score {
        tempo: 120,
        ..Score::default()
    };
    score.meta.title = smf.title.clone().unwrap_or_default();

    // Parts: one per (track, channel) that has notes.
    let mut parts: Vec<(String, u8, Option<u8>, Vec<&RawNote>)> = Vec::new();
    for t in &smf.tracks {
        let mut by_channel: BTreeMap<u8, Vec<&RawNote>> = BTreeMap::new();
        for n in &t.notes {
            by_channel.entry(n.channel).or_default().push(n);
        }
        for (ch, notes) in by_channel {
            parts.push((t.name.clone(), ch, t.program, notes));
        }
    }
    let end = parts
        .iter()
        .flat_map(|p| &p.3)
        .map(|n| scale(n.end))
        .max()
        .unwrap_or(0);

    // Bars from time signature changes.
    let mut signatures: Vec<(u64, TimeSignature)> = smf
        .time_signatures
        .iter()
        .map(|(t, s)| (scale(*t), *s))
        .collect();
    signatures.sort_by_key(|s| s.0);
    let mut keys: Vec<(u64, KeySignature)> =
        smf.keys.iter().map(|(t, k)| (scale(*t), *k)).collect();
    keys.sort_by_key(|k| k.0);
    let mut bar_starts = Vec::new();
    let mut tick = 0u64;
    let mut ts = TimeSignature::default();
    let mut ks = KeySignature::default();
    loop {
        if let Some(&(_, s)) = signatures.iter().rev().find(|(t, _)| *t <= tick) {
            ts = s;
        }
        if let Some(&(_, k)) = keys.iter().rev().find(|(t, _)| *t <= tick) {
            ks = k;
        }
        bar_starts.push(tick);
        score.master_bars.push(MasterBar {
            time_signature: ts,
            key_signature: ks,
            ..MasterBar::default()
        });
        tick += ts.bar_ticks() as u64;
        if tick >= end.max(1) || score.master_bars.len() >= 10_000 {
            break;
        }
    }

    let mut tempos: Vec<(u64, f64)> = smf.tempos.iter().map(|(t, b)| (scale(*t), *b)).collect();
    tempos.sort_by_key(|t| t.0);
    for (t, bpm) in tempos {
        let bar = bar_starts.partition_point(|s| *s <= t).saturating_sub(1);
        let offset = (t - bar_starts[bar]) as u32;
        if bar == 0 && offset == 0 {
            score.tempo = bpm.round().max(1.0) as u32;
            continue;
        }
        let changes = &mut score.master_bars[bar].tempo_changes;
        changes.retain(|c| c.tick != offset);
        changes.push(TempoChange { tick: offset, bpm });
    }

    for (name, channel, program, notes) in parts {
        let program = program.unwrap_or(0);
        let kind = if channel == 9 {
            TrackKind::Drums
        } else if (24..=39).contains(&program) {
            TrackKind::Fretted
        } else {
            TrackKind::Pitched
        };
        let lowest = notes.iter().map(|n| n.key).min().unwrap_or(40);
        let strings = match kind {
            // Standard guitar or bass tuning when the part fits, else a tuning reaching its range.
            TrackKind::Fretted if program < 32 && lowest >= 40 => vec![64, 59, 55, 50, 45, 40],
            // Guitar tuned down a few semitones.
            TrackKind::Fretted if program < 32 && lowest >= 35 => [64u8, 59, 55, 50, 45, 40]
                .iter()
                .map(|p| p - (40 - lowest))
                .collect(),
            TrackKind::Fretted if lowest >= 28 => vec![43, 38, 33, 28],
            TrackKind::Fretted => muse_convert::tuning_for_range(lowest),
            TrackKind::Drums => vec![0; 6],
            _ => vec![],
        };
        let mut track = Track {
            name: if name.is_empty() {
                format!("Track {}", score.tracks.len() + 1)
            } else {
                name
            },
            kind,
            strings,
            fret_count: 24,
            midi: MidiSettings {
                program,
                channel,
                effect_channel: channel,
                ..MidiSettings::default()
            },
            ..Track::default()
        };
        let quantized: Vec<(u64, u64, u8, u8)> = notes
            .iter()
            .map(|n| {
                let start = quantize(scale(n.start));
                let end = quantize(scale(n.end)).max(start + 120);
                (start, end, n.key, n.velocity)
            })
            .collect();
        track.bars = build_bars(&score, &bar_starts, &quantized, &track);
        score.tracks.push(track);
    }
    Ok(score)
}

/// Builds voice-1 beats per bar: notes starting together form a chord lasting until the next
/// onset (or their end), gaps become rests, and notes crossing a bar line are tied over.
fn build_bars(
    score: &Score,
    bar_starts: &[u64],
    notes: &[(u64, u64, u8, u8)],
    track: &Track,
) -> Vec<Bar> {
    let mut onsets: BTreeMap<u64, Vec<(u64, u8, u8)>> = BTreeMap::new();
    for &(start, end, key, vel) in notes {
        let chord = onsets.entry(start).or_default();
        // The same pitch struck twice at once (e.g. in two voices) is one note.
        match chord.iter_mut().find(|c| c.1 == key) {
            Some(c) => c.0 = c.0.max(end),
            None => chord.push((end, key, vel)),
        }
    }
    let times: Vec<u64> = onsets.keys().copied().collect();
    let mut position = 0;
    let mut bars = Vec::with_capacity(bar_starts.len());
    // Notes carried over a bar line: (end, key, velocity).
    let mut carried: Vec<(u64, u8, u8)> = Vec::new();
    for (bi, &bar_start) in bar_starts.iter().enumerate() {
        let bar_end = bar_start + score.master_bars[bi].ticks() as u64;
        let mut beats = Vec::new();
        let mut t = bar_start;
        let push_span = |beats: &mut Vec<Beat>,
                         len: u64,
                         chord: &[(u64, u8, u8)],
                         tie: bool,
                         position: &mut i32| {
            let mut first = true;
            for d in durations_for(len as u32) {
                let mut beat = Beat {
                    duration: d,
                    ..Beat::default()
                };
                if !chord.is_empty() {
                    beat.notes = make_notes(track, chord, position);
                    for n in &mut beat.notes {
                        n.tie = tie || !first;
                    }
                }
                beats.push(beat);
                first = false;
            }
        };
        if !carried.is_empty() {
            let next_onset = times
                .iter()
                .find(|&&x| x >= bar_start)
                .copied()
                .unwrap_or(u64::MAX);
            let end = carried
                .iter()
                .map(|c| c.0)
                .max()
                .unwrap()
                .min(next_onset)
                .min(bar_end);
            let chord = std::mem::take(&mut carried);
            push_span(&mut beats, end - t, &chord, true, &mut position);
            if chord.iter().any(|c| c.0 > bar_end) && end == bar_end {
                carried = chord;
            }
            t = end;
        }
        for (&onset, chord) in onsets.range(bar_start..bar_end) {
            if onset < t {
                continue;
            }
            if onset > t {
                push_span(&mut beats, onset - t, &[], false, &mut position);
            }
            let next = times
                .iter()
                .find(|&&x| x > onset)
                .copied()
                .unwrap_or(u64::MAX);
            let longest = chord.iter().map(|c| c.0).max().unwrap();
            let end = longest.min(next).min(bar_end);
            push_span(&mut beats, end - onset, chord, false, &mut position);
            if longest > bar_end && next >= bar_end {
                carried = chord.clone();
            }
            t = end;
        }
        if t < bar_end {
            push_span(&mut beats, bar_end - t, &[], false, &mut position);
        }
        if beats.is_empty() {
            beats.push(Beat::rest(Duration::new(DurationValue::Whole)));
        }
        bars.push(Bar {
            voices: vec![Voice { beats }],
            ..Default::default()
        });
    }
    bars
}

fn make_notes(track: &Track, chord: &[(u64, u8, u8)], position: &mut i32) -> Vec<Note> {
    match track.kind {
        TrackKind::Fretted => {
            let pitches: Vec<u8> = chord.iter().map(|c| c.1).collect();
            muse_convert::fret_chord(&pitches, &track.strings, 0, position)
                .into_iter()
                .zip(chord)
                .filter_map(|(p, c)| {
                    p.map(|(s, f)| Note {
                        velocity: c.2,
                        ..Note::new(s, f)
                    })
                })
                .collect()
        }
        TrackKind::Drums => chord
            .iter()
            .map(|c| Note {
                velocity: c.2,
                ..Note::new(crate::gpif::drum_row(c.1), c.1)
            })
            .collect(),
        _ => chord
            .iter()
            .map(|c| Note {
                velocity: c.2,
                ..Note::new(0, c.1)
            })
            .collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn varlen_encoding() {
        let mut v = vec![];
        varlen(&mut v, 0x0FFF_FFFF);
        assert_eq!(v, [0xFF, 0xFF, 0xFF, 0x7F]);
        let mut r = Bytes { data: &v, pos: 0 };
        assert_eq!(r.varlen().unwrap(), 0x0FFF_FFFF);
    }

    #[test]
    fn duration_decomposition() {
        assert_eq!(
            durations_for(960),
            vec![Duration::new(DurationValue::Quarter)]
        );
        assert_eq!(durations_for(1440)[0].dots, 1);
        assert_eq!(durations_for(320)[0].tuplet, Some(Tuplet::TRIPLET));
        let total: u32 = durations_for(3840 + 240).iter().map(Duration::ticks).sum();
        assert_eq!(total, 4080);
    }

    #[test]
    fn simple_score_round_trips() {
        let beat = |fret| Beat {
            notes: vec![Note::new(1, fret)],
            ..Beat::default()
        };
        let track = Track {
            name: "Guitar".into(),
            strings: vec![64, 59, 55, 50, 45, 40],
            midi: MidiSettings {
                program: 25,
                ..MidiSettings::default()
            },
            bars: vec![
                Bar {
                    voices: vec![Voice {
                        beats: vec![beat(0), beat(1), beat(3), beat(5)]
                    }],
                    ..Default::default()
                };
                2
            ],
            ..Track::default()
        };
        let score = Score {
            tempo: 90,
            master_bars: vec![MasterBar::default(); 2],
            tracks: vec![track],
            ..Score::default()
        };
        let bytes = write_midi(&score).unwrap();
        let back = read_midi(&bytes).unwrap();
        assert_eq!(back.tempo, 90);
        assert_eq!(back.master_bars.len(), 2);
        let t = &back.tracks[0];
        assert_eq!(t.kind, TrackKind::Fretted);
        let pitches: Vec<u8> = t.bars[0].voices[0]
            .beats
            .iter()
            .flat_map(|b| &b.notes)
            .map(|n| t.pitch_of(n))
            .collect();
        assert_eq!(pitches, vec![59, 60, 62, 64]);
    }
}
