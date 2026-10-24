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
}

/// Which tracks to render.
#[derive(Debug, Clone, Default)]
pub struct SequenceOptions {
    /// Per-track mute override; tracks beyond the list use the score's mute/solo flags.
    pub muted: Vec<bool>,
}

impl Sequence {
    pub fn build(score: &Score, options: &SequenceOptions) -> Self {
        let perf = Performance::build(score, &options.muted);
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
                time: perf.time_at_tick(n.start),
                event: MidiEvent::NoteOn {
                    channel,
                    key,
                    velocity: n.velocity,
                },
            });
            events.push(TimedEvent {
                time: perf.time_at_tick(n.end),
                event: MidiEvent::NoteOff { channel, key },
            });
        }
        events.sort_by(|a, b| {
            a.time
                .total_cmp(&b.time)
                .then(order_key(&a.event).cmp(&order_key(&b.event)))
        });
        let duration = perf.duration();
        Sequence {
            events,
            perf,
            duration,
        }
    }

    pub fn bars(&self) -> &[PlayedBar] {
        &self.perf.bars
    }

    /// Written position being played at `time`.
    pub fn location_at(&self, time: f64) -> Option<Location> {
        let tick = self.perf.tick_at_time(time);
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
        Some(
            self.perf
                .time_at_tick(pb.start + loc.tick.min(pb.ticks) as u64),
        )
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
                    voices: vec![Voice { beats: vec![beat] }]
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
}
