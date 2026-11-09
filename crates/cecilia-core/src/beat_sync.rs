//! Audio sync of a modern score by beat. A sync point sits on a beat ([`BeatSyncPoint`], by the
//! stable [`Beat::id`]), so it follows the beat through edits. Between two synced beats the time
//! is shared by ticks (the beats keep their relative lengths); before the first and after the last
//! point the nearest stretch's rate is used. The player works on bars and notes, so after every
//! change [`Score::refresh_beat_sync`] derives the bar-level [`SyncPoint`]s (what Guitar Pro
//! stores) and the per-tick anchors from the beat points. Repeated bars are synced on their
//! first play; later plays carry on at the rate there.

use std::collections::{HashMap, HashSet};

use crate::chant_sync::{NoteWarp, SYNC_FRAME_RATE};
use crate::{BeatSyncPoint, Performance, Score, SyncPoint, TICKS_PER_QUARTER};

/// A beat of a voice on the waveform's ruler.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BeatMark {
    /// The beat's stable id (0 until [`Score::assign_beat_ids`] has run).
    pub beat: u32,
    pub bar: usize,
    /// The beat's index in its voice.
    pub index: usize,
    /// Audio position in frames (with the audio's padding, like a sync point's).
    pub frame: i64,
}

/// A beat of the time signature (not of the notes) on the waveform's ruler.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MeterMark {
    pub bar: usize,
    /// From 1: the beat of the bar, as many as the numerator.
    pub beat: usize,
    /// Audio position in frames (with the audio's padding).
    pub frame: i64,
}

impl Score {
    /// Where the beats of the time signature are in the audio: each bar's numerator beats, the
    /// first at the bar's start (4/4: 1 2 3 4; 6/8: 1 2 3 4 5 6). `None` without audio.
    pub fn meter_marks(&self) -> Option<Vec<MeterMark>> {
        self.audio.as_ref()?;
        let starts = self.first_bar_starts();
        let warp = self.beat_warp(&starts)?;
        let mut out = Vec::new();
        for (bar, start) in starts.iter().enumerate() {
            let Some(start) = start else { continue };
            let n = self.master_bars[bar].time_signature.numerator.max(1) as u64;
            let len = self.played_ticks(bar) as u64;
            for k in 0..n {
                out.push(MeterMark {
                    bar,
                    beat: k as usize + 1,
                    frame: warp.frame_at((start + k * len / n) as f64).round() as i64,
                });
            }
        }
        out.sort_by_key(|m| m.frame);
        Some(out)
    }

    /// Gives every beat an id that no other has, keeping those that are unique (a pasted beat
    /// is a copy of the original, the later one gets a new id). Returns whether any changed.
    pub fn assign_beat_ids(&mut self) -> bool {
        let mut seen = HashSet::new();
        let mut next = self
            .tracks
            .iter()
            .flat_map(|t| &t.bars)
            .flat_map(|b| &b.voices)
            .flat_map(|v| &v.beats)
            .map(|b| b.id)
            .max()
            .unwrap_or(0)
            + 1;
        let mut changed = false;
        for beat in self
            .tracks
            .iter_mut()
            .flat_map(|t| &mut t.bars)
            .flat_map(|b| &mut b.voices)
            .flat_map(|v| &mut v.beats)
            .filter(|b| !b.empty)
        {
            if beat.id == 0 || !seen.insert(beat.id) {
                beat.id = next;
                seen.insert(next);
                next += 1;
                changed = true;
            }
        }
        changed
    }

    /// The performance tick at which each bar is first played.
    fn first_bar_starts(&self) -> Vec<Option<u64>> {
        let mut starts = vec![None; self.master_bars.len()];
        for pb in Performance::build(self, &[]).bars {
            if let Some(s) = starts.get_mut(pb.bar).filter(|s| s.is_none()) {
                *s = Some(pb.start);
            }
        }
        starts
    }

    /// The start tick, in its bar, of every beat in the score, by id (the first with the id).
    fn beat_ticks_in_bar(&self) -> HashMap<u32, (usize, u64)> {
        let mut out = HashMap::new();
        for t in &self.tracks {
            for (b, bar) in t.bars.iter().enumerate() {
                for v in &bar.voices {
                    let mut tick = 0u64;
                    for beat in v.beats.iter().filter(|x| !x.empty) {
                        if beat.id != 0 {
                            out.entry(beat.id).or_insert((b, tick));
                        }
                        tick += beat.ticks() as u64;
                    }
                }
            }
        }
        out
    }

    /// The warp through the sync points: the beat points, else the bar points (as Guitar Pro
    /// has them) at the start of their bars.
    fn beat_warp(&self, starts: &[Option<u64>]) -> Option<NoteWarp> {
        let audio = self.audio.as_ref()?;
        let anchors: Vec<(u64, i64)> = if audio.beat_points.is_empty() {
            audio
                .points
                .iter()
                .filter(|p| p.occurrence == 0)
                .filter_map(|p| Some((starts.get(p.bar).copied().flatten()?, p.frame)))
                .collect()
        } else {
            let at = self.beat_ticks_in_bar();
            audio
                .beat_points
                .iter()
                .filter_map(|p| {
                    let &(bar, tick) = at.get(&p.beat)?;
                    Some((starts.get(bar).copied().flatten()? + tick, p.frame))
                })
                .collect()
        };
        Some(NoteWarp::new(anchors, self.tempo as f64))
    }

    /// Where every beat of a voice is in the audio, for the waveform's ruler and the cursor:
    /// `None` without audio. Empty placeholder beats are left out.
    pub fn beat_marks(&self, track: usize, voice: usize) -> Option<Vec<BeatMark>> {
        self.audio.as_ref()?;
        let starts = self.first_bar_starts();
        let warp = self.beat_warp(&starts)?;
        let t = self.tracks.get(track)?;
        let mut out = Vec::new();
        for (bar, b) in t.bars.iter().enumerate() {
            let (Some(v), Some(start)) = (b.voices.get(voice), starts.get(bar).copied().flatten())
            else {
                continue;
            };
            let mut tick = 0u64;
            for (index, beat) in v.beats.iter().enumerate().filter(|(_, x)| !x.empty) {
                out.push(BeatMark {
                    beat: beat.id,
                    bar,
                    index,
                    frame: warp.frame_at((start + tick) as f64).round() as i64,
                });
                tick += beat.ticks() as u64;
            }
        }
        Some(out)
    }

    /// Turns the bar points of the audio sync into beat points on the first beat of each bar
    /// in a voice, so that going on by beat keeps what was synced by bar. A no-op once there
    /// are beat points.
    pub fn bar_points_to_beats(&mut self, track: usize, voice: usize) {
        let Some(audio) = self.audio.as_ref() else {
            return;
        };
        if !audio.beat_points.is_empty() {
            return;
        }
        self.assign_beat_ids();
        let points = self
            .audio
            .as_ref()
            .map(|a| a.points.clone())
            .unwrap_or_default();
        let mut out = Vec::new();
        for p in points.iter().filter(|p| p.occurrence == 0) {
            let first = self
                .tracks
                .get(track)
                .and_then(|t| t.bars.get(p.bar))
                .and_then(|b| b.voices.get(voice))
                .and_then(|v| v.beats.iter().find(|b| !b.empty));
            if let Some(b) = first {
                out.push(BeatSyncPoint {
                    beat: b.id,
                    frame: p.frame,
                });
            }
        }
        self.audio.as_mut().unwrap().beat_points = out;
    }

    /// Sets the sync point of a beat to an audio frame, returning the frame it had. The bar
    /// points are not refreshed ([`Score::refresh_beat_sync`]).
    pub fn set_beat_sync_point(&mut self, beat: u32, frame: i64) -> Option<i64> {
        let audio = self.audio.as_mut()?;
        let old = audio.beat_points.iter().position(|p| p.beat == beat);
        let old = old.map(|i| audio.beat_points.remove(i).frame);
        audio.beat_points.push(BeatSyncPoint { beat, frame });
        old
    }

    /// Takes the sync point off a beat, returning the frame it had.
    pub fn remove_beat_sync_point(&mut self, beat: u32) -> Option<i64> {
        let audio = self.audio.as_mut()?;
        let i = audio.beat_points.iter().position(|p| p.beat == beat)?;
        Some(audio.beat_points.remove(i).frame)
    }

    /// Keeps a modern score's audio sync in step with its beats: drops points whose beat is gone
    /// (returns how many), and derives the bar points and the per-tick anchors from the rest.
    /// A no-op for a chant, without audio, or without beat points (bar points as imported stay).
    pub fn refresh_beat_sync(&mut self) -> usize {
        if self.is_chant() || self.tracks.iter().any(|t| t.chant.is_some()) {
            return 0;
        }
        let Some(audio) = self.audio.as_ref() else {
            return 0;
        };
        if audio.beat_points.is_empty() {
            self.audio.as_mut().unwrap().note_anchors.clear();
            return 0;
        }
        // (Copies made by an edit get ids of their own.)
        self.assign_beat_ids();
        let at = self.beat_ticks_in_bar();
        let audio = self.audio.as_mut().unwrap();
        let before = audio.beat_points.len();
        audio.beat_points.retain(|p| at.contains_key(&p.beat));
        let dropped = before - audio.beat_points.len();
        let starts = self.first_bar_starts();
        let warp = self.beat_warp(&starts).unwrap();
        let audio = self.audio.as_mut().unwrap();
        if warp.is_empty() {
            audio.points.clear();
            audio.note_anchors.clear();
            return dropped;
        }
        let per_tick =
            |rate: f64| SYNC_FRAME_RATE * 60.0 / (rate.max(1e-9) * TICKS_PER_QUARTER as f64);
        // The ticks of the points: where the player times the notes from.
        let mut ticks: Vec<u64> = audio
            .beat_points
            .iter()
            .filter_map(|p| {
                let &(bar, tick) = at.get(&p.beat)?;
                Some(starts.get(bar).copied().flatten()? + tick)
            })
            .collect();
        ticks.sort_unstable();
        ticks.dedup();
        let mut anchors: Vec<(u64, i64, f64)> = Vec::new();
        for &tick in &ticks {
            let frame = warp.frame_at(tick as f64).round() as i64;
            if anchors.last().is_none_or(|a| a.1 < frame) {
                anchors.push((tick, frame, 0.0));
            }
        }
        for k in 0..anchors.len() {
            let rate = match anchors.get(k + 1) {
                Some(n) => (n.1 - anchors[k].1) as f64 / (n.0 - anchors[k].0) as f64,
                None => warp.rate_at(anchors[k].0 as f64),
            };
            anchors[k].2 = per_tick(rate);
        }
        audio.note_anchors = anchors;
        // A bar point at the start of every bar, where the warp puts it.
        let mut bars: Vec<(usize, u64)> = starts
            .iter()
            .enumerate()
            .filter_map(|(b, s)| Some((b, (*s)?)))
            .collect();
        bars.sort_by_key(|b| b.1);
        let frames: Vec<f64> = bars.iter().map(|b| warp.frame_at(b.1 as f64)).collect();
        let mut out: Vec<SyncPoint> = Vec::new();
        for (k, &(bar, tick)) in bars.iter().enumerate() {
            let frame = frames[k];
            if frame < 0.0 {
                continue;
            }
            let rate = match bars.get(k + 1) {
                Some(n) if frames[k + 1] > frame => (frames[k + 1] - frame) / (n.1 - tick) as f64,
                _ => warp.rate_at(tick as f64),
            };
            out.push(SyncPoint {
                bar,
                occurrence: 0,
                frame: frame.round() as i64,
                tempo: per_tick(rate),
            });
        }
        audio.points = out;
        dropped
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::*;

    /// Two bars of 4/4, four quarter notes each, with audio at 120 quarters a minute.
    fn score() -> Score {
        let bar = || Bar {
            voices: vec![Voice {
                beats: (0..4)
                    .map(|_| Beat {
                        duration: Duration::new(DurationValue::Quarter),
                        notes: vec![Note::new(0, 0)],
                        ..Beat::default()
                    })
                    .collect(),
            }],
            ..Default::default()
        };
        let mut s = Score {
            tempo: 120,
            master_bars: vec![MasterBar::default(); 2],
            tracks: vec![Track {
                strings: vec![64, 59, 55, 50, 45, 40],
                bars: vec![bar(), bar()],
                ..Track::default()
            }],
            ..Score::default()
        };
        s.audio = Some(AudioSync::default());
        s
    }

    fn ids(s: &Score) -> Vec<u32> {
        s.tracks[0]
            .bars
            .iter()
            .flat_map(|b| &b.voices[0].beats)
            .map(|b| b.id)
            .collect()
    }

    #[test]
    fn ids_are_unique_and_stay_with_their_beats() {
        let mut s = score();
        assert!(s.assign_beat_ids());
        let ids0 = ids(&s);
        assert!(ids0.iter().all(|&i| i != 0));
        assert_eq!(ids0.iter().collect::<HashSet<_>>().len(), 8);
        assert!(!s.assign_beat_ids(), "nothing to do the second time");
        // A beat moves with its id; of two copies the later one gets a new id.
        s.tracks[0].bars[0].voices[0].beats.remove(0);
        let dup = s.tracks[0].bars[0].voices[0].beats[0].clone();
        s.tracks[0].bars[1].voices[0].beats.push(dup);
        s.assign_beat_ids();
        let now = ids(&s);
        assert_eq!(now[..7], ids0[1..8]);
        assert!(now[7] != 0 && !ids0.contains(&now[7]));
    }

    #[test]
    fn beat_points_time_the_beats_between_them() {
        let mut s = score();
        s.assign_beat_ids();
        let ids = ids(&s);
        // Beat 1 at 1 s, the first beat of bar 2 (tick 4 quarters) at 5 s: a second a beat is
        // slower than the 0.5 s of the score's tempo.
        s.set_beat_sync_point(ids[0], 44_100);
        s.set_beat_sync_point(ids[4], 5 * 44_100);
        s.refresh_beat_sync();
        let marks = s.beat_marks(0, 0).unwrap();
        let frames: Vec<i64> = marks.iter().map(|m| m.frame).collect();
        assert_eq!(
            frames[..5],
            [44_100, 2 * 44_100, 3 * 44_100, 4 * 44_100, 5 * 44_100]
        );
        assert_eq!((marks[5].bar, marks[5].index), (1, 1));
        // The bar points are derived, with the tempo of the recording (60 a minute).
        let audio = s.audio.as_ref().unwrap();
        assert_eq!(audio.points.len(), 2);
        assert_eq!(audio.points[1].frame, 5 * 44_100);
        assert!((audio.points[0].tempo - 60.0).abs() < 0.01);
        // …and the notes are anchored where the ruler has them.
        assert_eq!(audio.note_anchors.len(), 2);
    }

    #[test]
    fn points_follow_their_beat_and_go_with_a_deleted_one() {
        let mut s = score();
        s.assign_beat_ids();
        let ids = ids(&s);
        s.set_beat_sync_point(ids[2], 44_100);
        s.set_beat_sync_point(ids[6], 3 * 44_100);
        // A beat before them goes: the points stay on their beats.
        s.tracks[0].bars[0].voices[0].beats.remove(0);
        assert_eq!(s.refresh_beat_sync(), 0);
        let m = s.beat_marks(0, 0).unwrap();
        assert_eq!(m.iter().find(|x| x.beat == ids[2]).unwrap().frame, 44_100);
        // The beat of a point is deleted: the point goes with it.
        s.tracks[0].bars[1].voices[0].beats.remove(2);
        assert_eq!(s.refresh_beat_sync(), 1);
        assert_eq!(s.audio.as_ref().unwrap().beat_points.len(), 1);
    }

    #[test]
    fn bar_points_become_beat_points_without_moving() {
        let mut s = score();
        let audio = s.audio.as_mut().unwrap();
        audio.points = vec![
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
                tempo: 80.0,
            },
        ];
        let before: Vec<i64> = s
            .beat_marks(0, 0)
            .unwrap()
            .iter()
            .map(|m| m.frame)
            .collect();
        assert_eq!(before[4], 3 * 44_100);
        s.bar_points_to_beats(0, 0);
        assert_eq!(s.audio.as_ref().unwrap().beat_points.len(), 2);
        s.refresh_beat_sync();
        let after: Vec<i64> = s
            .beat_marks(0, 0)
            .unwrap()
            .iter()
            .map(|m| m.frame)
            .collect();
        assert_eq!(before[..5], after[..5]);
    }

    #[test]
    fn the_meter_marks_follow_the_numerator() {
        let mut s = score();
        let per_bar = |s: &Score, bar: usize| -> Vec<usize> {
            s.meter_marks()
                .unwrap()
                .iter()
                .filter(|m| m.bar == bar)
                .map(|m| m.beat)
                .collect()
        };
        assert_eq!(per_bar(&s, 0), [1, 2, 3, 4]);
        s.master_bars[1].time_signature = TimeSignature {
            numerator: 6,
            denominator: 8,
        };
        assert_eq!(per_bar(&s, 1), [1, 2, 3, 4, 5, 6]);
        // At the score's tempo (120 a minute): a quarter is half a second, an eighth a quarter.
        let m = s.meter_marks().unwrap();
        let f = |bar, beat| {
            m.iter()
                .find(|x| x.bar == bar && x.beat == beat)
                .unwrap()
                .frame
        };
        assert_eq!(f(0, 2) - f(0, 1), 22_050);
        assert_eq!(f(1, 2) - f(1, 1), 11_025);
    }
}
