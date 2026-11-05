//! How a score is performed: bars in playback order (repeats expanded), the tempo map, and
//! every sounding note with its MIDI channel and tick span. Shared by audio playback and MIDI
//! export.

use std::collections::HashMap;

use crate::{Score, TICKS_PER_QUARTER};

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
pub struct TempoPoint {
    pub tick: u64,
    /// Seconds from the start at which this tempo begins.
    pub time: f64,
    pub bpm: f64,
}

/// Playback settings of one track.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PerfTrack {
    pub channel: u8,
    pub program: u8,
    pub bank: u8,
    pub volume: u8,
    pub pan: u8,
    pub muted: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PerfNote {
    pub track: usize,
    pub channel: u8,
    pub key: u8,
    pub velocity: u8,
    pub start: u64,
    pub end: u64,
}

#[derive(Debug, Clone, Default)]
pub struct Performance {
    pub bars: Vec<PlayedBar>,
    pub tempo: Vec<TempoPoint>,
    pub tracks: Vec<PerfTrack>,
    /// Notes of unmuted tracks, sorted by start tick.
    pub notes: Vec<PerfNote>,
}

const MAX_PLAYED_BARS: usize = 100_000;

/// Expands repeats and alternate endings into the order bars are played.
///
/// Each closing repeat bar keeps its own play counter (as Guitar Pro does), so several closing
/// bars that share one opening bar always terminate. Alternate endings are chosen by the pass
/// number through the current repeated section.
pub fn play_order(score: &Score) -> Vec<usize> {
    let bars = &score.master_bars;
    let mut order = Vec::new();
    let mut counts = vec![0u8; bars.len()];
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
            counts[i] += 1;
            if counts[i] < total {
                pass = pass.saturating_add(1);
                i = repeat_start;
                jumped_to = Some(i);
                continue;
            }
            counts[i] = 0;
            pass = 1;
        }
        i += 1;
    }
    order
}

fn ticks_to_secs(ticks: u64, bpm: f64) -> f64 {
    ticks as f64 / TICKS_PER_QUARTER as f64 * 60.0 / bpm
}

impl Performance {
    /// Builds the performance. `muted` overrides the score's mute/solo flags per track.
    pub fn build(score: &Score, muted: &[bool]) -> Self {
        let mut bars = Vec::new();
        let mut tick = 0u64;
        for b in play_order(score) {
            let ticks = score.played_ticks(b);
            bars.push(PlayedBar {
                bar: b,
                start: tick,
                ticks,
            });
            tick += ticks as u64;
        }

        let mut tempo = vec![TempoPoint {
            tick: 0,
            time: 0.0,
            bpm: score.tempo.max(1) as f64,
        }];
        for pb in &bars {
            for change in &score.master_bars[pb.bar].tempo_changes {
                let tick = pb.start + change.tick.min(pb.ticks) as u64;
                let bpm = change.bpm.clamp(1.0, 2000.0);
                let last = *tempo.last().unwrap();
                if tick == last.tick {
                    tempo.last_mut().unwrap().bpm = bpm;
                } else if tick > last.tick {
                    let time = last.time + ticks_to_secs(tick - last.tick, last.bpm);
                    tempo.push(TempoPoint { tick, time, bpm });
                }
            }
        }

        let any_solo = score.tracks.iter().any(|t| t.solo);
        let tracks = score
            .tracks
            .iter()
            .enumerate()
            .map(|(i, t)| PerfTrack {
                channel: if t.is_percussion() {
                    9
                } else {
                    t.midi.channel.min(15)
                },
                program: t.midi.program.min(127),
                bank: t.midi.bank.min(127),
                volume: t.midi.volume.min(127),
                pan: t.midi.pan.min(127),
                muted: muted
                    .get(i)
                    .copied()
                    .unwrap_or(t.mute || (any_solo && !t.solo)),
            })
            .collect();

        let mut perf = Performance {
            bars,
            tempo,
            tracks,
            notes: Vec::new(),
        };
        for ti in 0..score.tracks.len() {
            if !perf.tracks[ti].muted {
                perf.add_track_notes(score, ti);
            }
        }
        perf.notes.sort_by_key(|n| (n.start, n.track, n.key));
        perf
    }

    fn add_track_notes(&mut self, score: &Score, ti: usize) {
        let track = &score.tracks[ti];
        let channel = self.tracks[ti].channel;
        let mut pending = Vec::new();
        for pb in &self.bars {
            let Some(bar) = track.bars.get(pb.bar) else {
                continue;
            };
            for (vi, voice) in bar.voices.iter().enumerate() {
                let mut t = pb.start;
                for beat in &voice.beats {
                    let len = beat.ticks() as u64;
                    let count = beat.notes.len().max(1) as u64;
                    for (ni, note) in beat.notes.iter().enumerate() {
                        let stroke_offset = beat
                            .effects
                            .stroke
                            .map_or(0, |s| s.duration.ticks() as u64 * ni as u64 / count);
                        let mut note_len =
                            (len as f64 * note.duration_percent.clamp(0.05, 1.0) as f64) as u64;
                        if note.dead {
                            note_len = note_len.min(TICKS_PER_QUARTER as u64 / 8);
                        } else if note.effects.palm_mute || note.effects.staccato {
                            note_len /= 2;
                        }
                        // The beat's dynamic drives all its notes.
                        let base = beat.dynamic.unwrap_or(note.velocity);
                        let velocity = if note.ghost { base / 2 } else { base };
                        pending.push(Pending {
                            voice: vi,
                            string: note.string,
                            key: track.pitch_of(note),
                            start: t + stroke_offset,
                            end: t + note_len.max(1),
                            velocity: velocity.clamp(1, 127),
                            tie: note.tie,
                            let_ring: note.effects.let_ring,
                        });
                    }
                    t += len;
                }
            }
        }

        // Merge ties into the note they continue, and extend let-ring notes to the next note
        // on the same string.
        pending.sort_by_key(|n| (n.start, n.voice, n.string));
        let mut merged: Vec<Pending> = Vec::with_capacity(pending.len());
        let mut last: HashMap<(usize, u8), usize> = HashMap::new();
        for n in pending {
            let slot = (n.voice, n.string);
            if let Some(&i) = last.get(&slot) {
                if n.tie && merged[i].key == n.key {
                    merged[i].end = merged[i].end.max(n.end);
                    continue;
                }
                if merged[i].let_ring {
                    merged[i].end = merged[i].end.max(n.start);
                }
            }
            last.insert(slot, merged.len());
            merged.push(n);
        }
        self.notes.extend(merged.into_iter().map(|n| PerfNote {
            track: ti,
            channel,
            key: n.key,
            velocity: n.velocity,
            start: n.start,
            end: n.end,
        }));
    }

    pub fn total_ticks(&self) -> u64 {
        self.bars.last().map_or(0, |b| b.start + b.ticks as u64)
    }

    pub fn duration(&self) -> f64 {
        self.time_at_tick(self.total_ticks())
    }

    pub fn time_at_tick(&self, tick: u64) -> f64 {
        let i = self.tempo.partition_point(|s| s.tick <= tick);
        let s = self.tempo[i.saturating_sub(1)];
        s.time + ticks_to_secs(tick - s.tick, s.bpm)
    }

    pub fn tick_at_time(&self, time: f64) -> u64 {
        let i = self.tempo.partition_point(|s| s.time <= time);
        let s = self.tempo[i.saturating_sub(1)];
        s.tick + ((time - s.time).max(0.0) * s.bpm / 60.0 * TICKS_PER_QUARTER as f64) as u64
    }
}

#[derive(Debug, Clone, Copy)]
struct Pending {
    voice: usize,
    string: u8,
    key: u8,
    start: u64,
    end: u64,
    velocity: u8,
    tie: bool,
    let_ring: bool,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::*;

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
                    ..Default::default()
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
    fn two_closing_repeats_share_one_opening() {
        let bars = vec![
            MasterBar {
                repeat_start: true,
                ..MasterBar::default()
            },
            MasterBar {
                repeat_end: Some(2),
                ..MasterBar::default()
            },
            MasterBar {
                repeat_end: Some(2),
                ..MasterBar::default()
            },
            MasterBar::default(),
        ];
        assert_eq!(
            play_order(&score_with_bars(bars)),
            vec![0, 1, 0, 1, 2, 0, 1, 0, 1, 2, 3]
        );
    }

    #[test]
    fn tempo_changes_and_ties() {
        let mut score = score_with_bars(vec![MasterBar::default(); 2]);
        score.master_bars[1]
            .tempo_changes
            .push(TempoChange { tick: 0, bpm: 60.0 });
        let perf = Performance::build(&score, &[]);
        assert!((perf.duration() - 6.0).abs() < 1e-9);
        assert_eq!(perf.notes.len(), 2);
        score.tracks[0].bars[1].voices[0].beats[0].notes[0].tie = true;
        let perf = Performance::build(&score, &[]);
        assert_eq!(perf.notes.len(), 1);
        assert_eq!(perf.notes[0].end, 7680);
    }
}
