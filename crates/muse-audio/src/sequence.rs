//! Turns a [`Score`] into a time-ordered list of MIDI events, expanding repeats.

use muse_core::{Score, TICKS_PER_QUARTER, TrackKind};

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum MidiEvent {
    NoteOn {
        channel: u8,
        key: u8,
        velocity: u8,
    },
    NoteOff {
        channel: u8,
        key: u8,
    },
    Program {
        channel: u8,
        program: u8,
        bank: u8,
    },
    Control {
        channel: u8,
        controller: u8,
        value: u8,
    },
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TimedEvent {
    /// Seconds from the start at normal speed.
    pub time: f64,
    pub event: MidiEvent,
}

/// One bar occurrence in playback order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PlayedBar {
    /// Index into `Score::master_bars`.
    pub bar: usize,
    /// Performance tick at which this occurrence starts.
    pub start: u64,
    pub ticks: u32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct TempoSegment {
    tick: u64,
    time: f64,
    bpm: f64,
}

/// Written score position (bar index and tick within the bar).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Location {
    pub bar: usize,
    pub tick: u32,
}

#[derive(Debug, Clone, Default)]
pub struct Sequence {
    pub events: Vec<TimedEvent>,
    pub bars: Vec<PlayedBar>,
    tempo: Vec<TempoSegment>,
    pub duration: f64,
}

/// Which tracks to render.
#[derive(Debug, Clone, Default)]
pub struct SequenceOptions {
    /// Per-track mute override; tracks beyond the list use the score's mute/solo flags.
    pub muted: Vec<bool>,
}

const MAX_PLAYED_BARS: usize = 100_000;

/// Expands repeats and alternate endings into the order bars are played.
pub fn play_order(score: &Score) -> Vec<usize> {
    let bars = &score.master_bars;
    let mut order = Vec::new();
    let mut i = 0;
    let mut repeat_start = 0;
    let mut pass: u8 = 1;
    let mut jumped_to = None;
    while i < bars.len() && order.len() < MAX_PLAYED_BARS {
        let mb = &bars[i];
        if mb.repeat_start && jumped_to != Some(i) {
            repeat_start = i;
            pass = 1;
        }
        jumped_to = None;
        if mb.alternate_endings != 0 && pass <= 8 && mb.alternate_endings & (1 << (pass - 1)) == 0 {
            i += 1;
            continue;
        }
        order.push(i);
        if let Some(total) = mb.repeat_end {
            if pass < total {
                pass += 1;
                i = repeat_start;
                jumped_to = Some(i);
                continue;
            }
            pass = 1;
        }
        i += 1;
    }
    order
}

impl Sequence {
    pub fn build(score: &Score, options: &SequenceOptions) -> Self {
        let order = play_order(score);
        let mut bars = Vec::with_capacity(order.len());
        let mut tick = 0u64;
        for &b in &order {
            let ticks = score.master_bars[b].ticks();
            bars.push(PlayedBar {
                bar: b,
                start: tick,
                ticks,
            });
            tick += ticks as u64;
        }
        let total_ticks = tick;

        // Tempo changes may be written on any track; they apply to the whole score.
        let mut changes: Vec<(u64, u32)> = Vec::new();
        for pb in &bars {
            for track in &score.tracks {
                let Some(bar) = track.bars.get(pb.bar) else {
                    continue;
                };
                for voice in &bar.voices {
                    let mut t = 0;
                    for beat in &voice.beats {
                        if let Some(bpm) = beat.tempo {
                            changes.push((pb.start + t as u64, bpm));
                        }
                        t += beat.ticks();
                    }
                }
            }
        }
        changes.sort_by_key(|c| c.0);
        let mut tempo = vec![TempoSegment {
            tick: 0,
            time: 0.0,
            bpm: score.tempo.max(1) as f64,
        }];
        for (tick, bpm) in changes {
            let last = *tempo.last().unwrap();
            let time = last.time + ticks_to_secs(tick - last.tick, last.bpm);
            if tick == last.tick {
                tempo.last_mut().unwrap().bpm = bpm as f64;
            } else {
                tempo.push(TempoSegment {
                    tick,
                    time,
                    bpm: bpm as f64,
                });
            }
        }

        let mut seq = Sequence {
            events: Vec::new(),
            bars,
            tempo,
            duration: 0.0,
        };
        seq.duration = seq.time_at_tick(total_ticks);
        seq.add_track_events(score, options);
        seq.events.sort_by(|a, b| {
            a.time
                .total_cmp(&b.time)
                .then(order_key(&a.event).cmp(&order_key(&b.event)))
        });
        seq
    }

    fn add_track_events(&mut self, score: &Score, options: &SequenceOptions) {
        let any_solo = score.tracks.iter().any(|t| t.solo);
        for (ti, track) in score.tracks.iter().enumerate() {
            let muted = options
                .muted
                .get(ti)
                .copied()
                .unwrap_or(track.mute || (any_solo && !track.solo));
            if muted || track.kind == TrackKind::Chant {
                continue;
            }
            let channel = if track.is_percussion() {
                9
            } else {
                track.midi.channel.min(15)
            };
            let m = track.midi;
            for event in [
                MidiEvent::Program {
                    channel,
                    program: m.program,
                    bank: m.bank,
                },
                MidiEvent::Control {
                    channel,
                    controller: 7,
                    value: m.volume.min(127),
                },
                MidiEvent::Control {
                    channel,
                    controller: 10,
                    value: m.pan.min(127),
                },
            ] {
                self.events.push(TimedEvent { time: 0.0, event });
            }

            let mut notes = Vec::new();
            for pb in &self.bars {
                let Some(bar) = track.bars.get(pb.bar) else {
                    continue;
                };
                for (vi, voice) in bar.voices.iter().enumerate() {
                    let mut t = pb.start;
                    for beat in &voice.beats {
                        let len = beat.ticks() as u64;
                        let string_count = beat.notes.len().max(1) as u64;
                        for (ni, note) in beat.notes.iter().enumerate() {
                            let key = track.pitch_of(note);
                            let stroke_offset = beat.effects.stroke.map_or(0, |s| {
                                s.duration.ticks() as u64 * ni as u64 / string_count
                            });
                            let mut note_len =
                                (len as f64 * note.duration_percent.clamp(0.05, 1.0) as f64) as u64;
                            if note.dead {
                                note_len = note_len.min(TICKS_PER_QUARTER as u64 / 8);
                            } else if note.effects.palm_mute || note.effects.staccato {
                                note_len /= 2;
                            }
                            notes.push(PendingNote {
                                voice: vi,
                                string: note.string,
                                key,
                                start: t + stroke_offset,
                                end: t + note_len.max(1),
                                velocity: if note.ghost {
                                    note.velocity / 2
                                } else {
                                    note.velocity
                                }
                                .max(1),
                                tie: note.tie,
                                let_ring: note.effects.let_ring,
                            });
                        }
                        t += len;
                    }
                }
            }
            self.add_notes(channel, notes);
        }
    }

    /// Merges ties and let-ring, then emits note on/off events.
    fn add_notes(&mut self, channel: u8, mut notes: Vec<PendingNote>) {
        notes.sort_by_key(|n| (n.start, n.voice, n.string));
        let mut merged: Vec<PendingNote> = Vec::with_capacity(notes.len());
        // Index of the last note per (voice, string) in `merged`.
        let mut last: std::collections::HashMap<(usize, u8), usize> = Default::default();
        for n in notes {
            let slot = (n.voice, n.string);
            if n.tie {
                if let Some(&i) = last.get(&slot) {
                    if merged[i].key == n.key {
                        merged[i].end = merged[i].end.max(n.end);
                        continue;
                    }
                }
            }
            if let Some(&i) = last.get(&slot) {
                if merged[i].let_ring {
                    merged[i].end = merged[i].end.max(n.start);
                }
            }
            last.insert(slot, merged.len());
            merged.push(n);
        }
        for n in merged {
            let (on, off) = (self.time_at_tick(n.start), self.time_at_tick(n.end));
            self.events.push(TimedEvent {
                time: on,
                event: MidiEvent::NoteOn {
                    channel,
                    key: n.key,
                    velocity: n.velocity,
                },
            });
            self.events.push(TimedEvent {
                time: off,
                event: MidiEvent::NoteOff {
                    channel,
                    key: n.key,
                },
            });
        }
    }

    pub fn total_ticks(&self) -> u64 {
        self.bars.last().map_or(0, |b| b.start + b.ticks as u64)
    }

    fn segment_for_tick(&self, tick: u64) -> TempoSegment {
        let i = self.tempo.partition_point(|s| s.tick <= tick);
        self.tempo[i.saturating_sub(1)]
    }

    pub fn time_at_tick(&self, tick: u64) -> f64 {
        let s = self.segment_for_tick(tick);
        s.time + ticks_to_secs(tick - s.tick, s.bpm)
    }

    pub fn tick_at_time(&self, time: f64) -> u64 {
        let i = self.tempo.partition_point(|s| s.time <= time);
        let s = self.tempo[i.saturating_sub(1)];
        s.tick + ((time - s.time).max(0.0) * s.bpm / 60.0 * TICKS_PER_QUARTER as f64) as u64
    }

    /// Written position being played at `time`.
    pub fn location_at(&self, time: f64) -> Option<Location> {
        let tick = self.tick_at_time(time);
        let i = self
            .bars
            .partition_point(|b| b.start <= tick)
            .checked_sub(1)?;
        let pb = self.bars[i];
        Some(Location {
            bar: pb.bar,
            tick: ((tick - pb.start) as u32).min(pb.ticks.saturating_sub(1)),
        })
    }

    /// Time at which a written position is first played.
    pub fn time_of(&self, loc: Location) -> Option<f64> {
        let pb = self.bars.iter().find(|b| b.bar == loc.bar)?;
        Some(self.time_at_tick(pb.start + loc.tick.min(pb.ticks) as u64))
    }

    /// Index of the first event at or after `time`.
    pub fn event_index_at(&self, time: f64) -> usize {
        self.events.partition_point(|e| e.time < time)
    }
}

#[derive(Debug, Clone, Copy)]
struct PendingNote {
    voice: usize,
    string: u8,
    key: u8,
    start: u64,
    end: u64,
    velocity: u8,
    tie: bool,
    let_ring: bool,
}

/// Note-offs sort before note-ons at the same instant so repeated notes retrigger.
fn order_key(e: &MidiEvent) -> u8 {
    match e {
        MidiEvent::Program { .. } => 0,
        MidiEvent::Control { .. } => 1,
        MidiEvent::NoteOff { .. } => 2,
        MidiEvent::NoteOn { .. } => 3,
    }
}

fn ticks_to_secs(ticks: u64, bpm: f64) -> f64 {
    ticks as f64 / TICKS_PER_QUARTER as f64 * 60.0 / bpm
}

#[cfg(test)]
mod tests {
    use super::*;
    use muse_core::*;

    fn score_with_bars(bars: Vec<MasterBar>) -> Score {
        let track = Track {
            strings: vec![64, 59, 55, 50, 45, 40],
            bars: bars
                .iter()
                .map(|_| Bar {
                    voices: vec![Voice {
                        beats: vec![Beat {
                            duration: Duration::new(DurationValue::Whole),
                            notes: vec![Note::new(0, 0)],
                            ..Beat::default()
                        }],
                    }],
                })
                .collect(),
            ..Track::default()
        };
        Score {
            tempo: 120,
            master_bars: bars,
            tracks: vec![track],
            ..Score::default()
        }
    }

    #[test]
    fn repeats_and_alternate_endings() {
        let bars = vec![
            MasterBar::default(),
            MasterBar {
                repeat_start: true,
                ..MasterBar::default()
            },
            MasterBar {
                alternate_endings: 0b01,
                repeat_end: Some(2),
                ..MasterBar::default()
            },
            MasterBar {
                alternate_endings: 0b10,
                ..MasterBar::default()
            },
            MasterBar::default(),
        ];
        assert_eq!(play_order(&score_with_bars(bars)), vec![0, 1, 2, 1, 3, 4]);
    }

    #[test]
    fn timing_and_locations() {
        let score = score_with_bars(vec![MasterBar::default(); 2]);
        let seq = Sequence::build(&score, &SequenceOptions::default());
        // Two 4/4 bars at 120 bpm = 4 seconds.
        assert!((seq.duration - 4.0).abs() < 1e-9);
        assert_eq!(seq.location_at(2.5), Some(Location { bar: 1, tick: 960 }));
        assert_eq!(seq.time_of(Location { bar: 1, tick: 0 }), Some(2.0));
        let ons = seq
            .events
            .iter()
            .filter(|e| matches!(e.event, MidiEvent::NoteOn { .. }))
            .count();
        assert_eq!(ons, 2);
    }

    #[test]
    fn tempo_change_mid_score() {
        let mut score = score_with_bars(vec![MasterBar::default(); 2]);
        score.tracks[0].bars[1].voices[0].beats[0].tempo = Some(60);
        let seq = Sequence::build(&score, &SequenceOptions::default());
        assert!((seq.duration - 6.0).abs() < 1e-9);
    }

    #[test]
    fn ties_merge_into_one_note() {
        let mut score = score_with_bars(vec![MasterBar::default(); 2]);
        score.tracks[0].bars[1].voices[0].beats[0].notes[0].tie = true;
        let seq = Sequence::build(&score, &SequenceOptions::default());
        let ons = seq
            .events
            .iter()
            .filter(|e| matches!(e.event, MidiEvent::NoteOn { .. }))
            .count();
        assert_eq!(ons, 1);
    }
}
