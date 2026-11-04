//! Turns a [`Score`] into a time-ordered list of MIDI events for real-time playback.

use muse_core::Score;
use muse_core::performance::{Performance, PlayedBar};

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

/// Written score position (bar index and tick within the bar).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Location {
    pub bar: usize,
    pub tick: u32,
}

#[derive(Debug, Clone, Default)]
pub struct Sequence {
    pub events: Vec<TimedEvent>,
    pub perf: Performance,
    pub duration: f64,
    /// Sync anchors (performance tick, audio seconds, tempo after the anchor). When set, times
    /// are positions in the audio track; otherwise the notated tempo is used, shifted by
    /// `offset`.
    pub anchors: Vec<(u64, f64, f64)>,
    pub offset: f64,
    /// Metronome beats: (time, downbeat).
    pub beats: Vec<(f64, bool)>,
}

/// Audio sample rate Guitar Pro's sync frames are counted in.
pub const SYNC_RATE: f64 = 44_100.0;

/// Which tracks to render.
#[derive(Debug, Clone, Default)]
pub struct SequenceOptions {
    /// Per-track mute override; tracks beyond the list use the score's mute/solo flags.
    pub muted: Vec<bool>,
    /// Time the sequence to the score's audio track (sync points and padding).
    pub audio_sync: bool,
}

impl Sequence {
    pub fn build(score: &Score, options: &SequenceOptions) -> Self {
        let perf = Performance::build(score, &options.muted);
        let mut seq = Sequence {
            perf,
            ..Sequence::default()
        };
        if options.audio_sync {
            if let Some(audio) = &score.audio {
                let pad = audio.frame_padding as f64 / SYNC_RATE;
                seq.offset = -pad;
                let mut anchors: Vec<(u64, f64, f64)> = audio
                    .points
                    .iter()
                    .filter_map(|p| {
                        let pb = seq
                            .perf
                            .bars
                            .iter()
                            .filter(|b| b.bar == p.bar)
                            .nth(p.occurrence as usize)?;
                        Some((pb.start, p.frame as f64 / SYNC_RATE - pad, p.tempo))
                    })
                    .collect();
                anchors.sort_by_key(|a| a.0);
                anchors.dedup_by_key(|a| a.0);
                // Keep audio times increasing.
                anchors.dedup_by(|b, a| b.1 <= a.1);
                seq.anchors = anchors;
            }
        }
        let perf = &seq.perf;
        let mut events = Vec::with_capacity(perf.notes.len() * 2 + perf.tracks.len() * 3);
        for t in perf.tracks.iter().filter(|t| !t.muted) {
            let channel = t.channel;
            for event in [
                MidiEvent::Program {
                    channel,
                    program: t.program,
                    bank: t.bank,
                },
                MidiEvent::Control {
                    channel,
                    controller: 7,
                    value: t.volume,
                },
                MidiEvent::Control {
                    channel,
                    controller: 10,
                    value: t.pan,
                },
            ] {
                events.push(TimedEvent { time: 0.0, event });
            }
        }
        for n in &perf.notes {
            let (channel, key) = (n.channel, n.key);
            events.push(TimedEvent {
                time: seq.time_at_tick(n.start),
                event: MidiEvent::NoteOn {
                    channel,
                    key,
                    velocity: n.velocity,
                },
            });
            events.push(TimedEvent {
                time: seq.time_at_tick(n.end),
                event: MidiEvent::NoteOff { channel, key },
            });
        }
        events.sort_by(|a, b| {
            a.time
                .total_cmp(&b.time)
                .then(order_key(&a.event).cmp(&order_key(&b.event)))
        });
        let mut beats = Vec::new();
        for pb in &perf.bars {
            let ts = score.master_bars[pb.bar].time_signature;
            let unit = (muse_core::TICKS_PER_QUARTER * 4 / ts.denominator.max(1) as u32).max(1);
            let mut t = 0;
            while t < pb.ticks {
                beats.push((seq.time_at_tick(pb.start + t as u64), t == 0));
                t += unit;
            }
        }
        seq.duration = seq.time_at_tick(perf.total_ticks());
        seq.events = events;
        seq.beats = beats;
        seq
    }

    /// Notated tempo (bpm) at a performance tick.
    fn score_bpm_at(&self, tick: u64) -> f64 {
        let i = self.perf.tempo.partition_point(|t| t.tick <= tick);
        self.perf
            .tempo
            .get(i.saturating_sub(1))
            .map_or(120.0, |t| t.bpm)
    }

    /// Time (in the audio track's timeline when synced) at a performance tick. Between sync
    /// points the score's own tempo changes are kept, scaled so the points line up.
    pub fn time_at_tick(&self, tick: u64) -> f64 {
        let a = &self.anchors;
        let p = |t: u64| self.perf.time_at_tick(t);
        if a.is_empty() {
            return p(tick) + self.offset;
        }
        let i = a.partition_point(|x| x.0 <= tick);
        // Outside the anchored range, the nearest point's tempo relative to the score's.
        let scaled = |(t0, s0, bpm): (u64, f64, f64)| {
            let ratio = self.score_bpm_at(t0) / bpm.max(1.0);
            s0 + (p(tick) - p(t0)) * ratio
        };
        if i == 0 {
            return scaled(a[0]);
        }
        let (t0, s0, _) = a[i - 1];
        match a.get(i) {
            Some(&(t1, s1, _)) => {
                let span = (p(t1) - p(t0)).max(1e-9);
                s0 + (s1 - s0) * (p(tick) - p(t0)) / span
            }
            None => scaled(a[i - 1]),
        }
    }

    /// Alias of [`Self::time_at_tick`] for callers holding a played-bar tick.
    pub fn perf_time(&self, tick: u64) -> f64 {
        self.time_at_tick(tick)
    }

    /// Inverse of [`Self::time_at_tick`] (which is monotonic), by bisection.
    pub fn tick_at_time(&self, time: f64) -> u64 {
        if self.anchors.is_empty() {
            return self.perf.tick_at_time(time - self.offset);
        }
        let (mut lo, mut hi) = (0u64, self.perf.total_ticks().max(1) * 4);
        if self.time_at_tick(lo) >= time {
            return 0;
        }
        while hi - lo > 1 {
            let mid = (lo + hi) / 2;
            if self.time_at_tick(mid) <= time {
                lo = mid;
            } else {
                hi = mid;
            }
        }
        lo
    }

    pub fn bars(&self) -> &[PlayedBar] {
        &self.perf.bars
    }

    /// Written position being played at `time`.
    pub fn location_at(&self, time: f64) -> Option<Location> {
        let tick = self.tick_at_time(time);
        let bars = &self.perf.bars;
        let i = bars.partition_point(|b| b.start <= tick).checked_sub(1)?;
        let pb = bars[i];
        Some(Location {
            bar: pb.bar,
            tick: ((tick - pb.start) as u32).min(pb.ticks.saturating_sub(1)),
        })
    }

    /// Time at which a written position is first played.
    pub fn time_of(&self, loc: Location) -> Option<f64> {
        let pb = self.perf.bars.iter().find(|b| b.bar == loc.bar)?;
        Some(self.time_at_tick(pb.start + loc.tick.min(pb.ticks) as u64))
    }

    /// Index of the first event at or after `time`.
    pub fn event_index_at(&self, time: f64) -> usize {
        self.events.partition_point(|e| e.time < time)
    }
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

#[cfg(test)]
mod tests {
    use super::*;
    use muse_core::*;

    #[test]
    fn audio_sync_points_warp_time() {
        let beat = Beat {
            duration: Duration::new(DurationValue::Whole),
            notes: vec![Note::new(0, 0)],
            ..Beat::default()
        };
        let track = Track {
            strings: vec![64],
            bars: vec![
                Bar {
                    voices: vec![Voice { beats: vec![beat] }],
                    ..Default::default()
                };
                3
            ],
            ..Track::default()
        };
        let mut score = Score {
            tempo: 120,
            master_bars: vec![MasterBar::default(); 3],
            tracks: vec![track],
            ..Score::default()
        };
        // Bar 2 starts 3 s into the recording (slower than the notated 2 s).
        score.audio = Some(AudioSync {
            file: None,
            frame_padding: 0,
            points: vec![
                SyncPoint {
                    bar: 0,
                    occurrence: 0,
                    frame: 0,
                    tempo: 120.0,
                },
                SyncPoint {
                    bar: 1,
                    occurrence: 0,
                    frame: 3 * 44_100,
                    tempo: 60.0,
                },
            ],
            ..AudioSync::default()
        });
        let seq = Sequence::build(
            &score,
            &SequenceOptions {
                audio_sync: true,
                ..Default::default()
            },
        );
        assert!((seq.time_of(Location { bar: 1, tick: 0 }).unwrap() - 3.0).abs() < 1e-9);
        assert!((seq.time_of(Location { bar: 0, tick: 1920 }).unwrap() - 1.5).abs() < 1e-9);
        // After the last point, its tempo (60 bpm): one 4/4 bar = 4 s.
        assert!((seq.time_of(Location { bar: 2, tick: 0 }).unwrap() - 7.0).abs() < 1e-9);
        assert_eq!(seq.location_at(5.0), Some(Location { bar: 1, tick: 1920 }));
        assert_eq!(seq.beats.len(), 12);
    }

    #[test]
    fn timing_and_locations() {
        let beat = Beat {
            duration: Duration::new(DurationValue::Whole),
            notes: vec![Note::new(0, 0)],
            ..Beat::default()
        };
        let track = Track {
            strings: vec![64, 59, 55, 50, 45, 40],
            bars: vec![
                Bar {
                    voices: vec![Voice { beats: vec![beat] }],
                    ..Default::default()
                };
                2
            ],
            ..Track::default()
        };
        let score = Score {
            tempo: 120,
            master_bars: vec![MasterBar::default(); 2],
            tracks: vec![track],
            ..Score::default()
        };
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

    /// A chant staff only draws the track's notes: playback is the same whichever staves the
    /// track shows, and a chord still sounds as a chord with one note engraved.
    #[test]
    fn playback_ignores_the_staff_kind() {
        let quarter = Duration::new(DurationValue::Quarter);
        let mut chord = Beat::rest(quarter);
        chord.notes = vec![Note::new(0, 3), Note::new(2, 4), Note::new(4, 5)];
        chord.chant_division = Some(":".into());
        let mut marked = Beat::rest(quarter);
        marked.notes = vec![Note::new(1, 2)];
        marked.notes[0].chant = Some(Box::new(ChantMarks {
            inclinatum: true,
            neume_start: true,
            mods: vec!["v".into(), ".".into()],
            ..ChantMarks::default()
        }));
        let mut tied = Beat::rest(quarter);
        tied.notes = vec![Note::new(1, 2)];
        tied.notes[0].tie = true;
        let beats = vec![chord, marked, Beat::rest(quarter), tied];
        let build = |staves: Vec<Staff>| {
            let track = Track {
                strings: vec![64, 59, 55, 50, 45, 40],
                staves,
                bars: vec![
                    Bar {
                        voices: vec![Voice {
                            beats: beats.clone(),
                        }],
                        ..Default::default()
                    };
                    2
                ],
                ..Track::default()
            };
            let score = Score {
                tempo: 100,
                master_bars: vec![MasterBar::default(); 2],
                tracks: vec![track],
                ..Score::default()
            };
            let seq = Sequence::build(&score, &SequenceOptions::default());
            (score, seq)
        };
        let (_, standard) = build(vec![Staff::Standard(Clef::Treble)]);
        assert!(standard.events.len() >= 10, "the chord's three notes sound");
        for staves in [
            vec![Staff::Tab],
            vec![Staff::Chant],
            vec![Staff::Standard(Clef::Treble), Staff::Chant, Staff::Tab],
        ] {
            let (score, seq) = build(staves);
            assert_eq!(seq.events, standard.events);
            assert_eq!(seq.duration, standard.duration);
            assert_eq!(seq.beats, standard.beats);
            // Building the chant view for the staff leaves the track alone.
            let before = score.tracks[0].clone();
            let _ = muse_core::melody::ChantView::build(
                &score,
                &score.tracks[0],
                muse_core::melody::ChordPolicy::Top,
            );
            assert_eq!(score.tracks[0], before);
        }
    }
}
