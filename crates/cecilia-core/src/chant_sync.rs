//! Audio sync of a chant score. The sync is kept on notes ([`ChantSyncPoint`], by note id), since
//! the cached bars of a chant are renumbered by every edit. Between two synced notes the time is
//! shared in proportion to the lengths the interpretation gives the notes (mora, Solesmes
//! lengthening); before the first and after the last point the nearest stretch's rate is used.
//! The player works on bars, so after every change [`Score::refresh_chant_sync`] derives
//! bar-level [`SyncPoint`]s from the note points.

use std::collections::HashMap;

use crate::chant::transcribe;
use crate::{ChantSyncPoint, Score, SyncPoint, TICKS_PER_QUARTER};

/// Audio frames per second of sync points.
pub const SYNC_FRAME_RATE: f64 = 44_100.0;

/// Maps note-start ticks to audio frames, piecewise linear through the anchors.
#[derive(Debug, Clone)]
pub struct NoteWarp {
    /// `(tick, frame)`, increasing in both.
    anchors: Vec<(f64, f64)>,
    /// Frames per tick when there is no stretch to take the rate from.
    default_rate: f64,
}

impl NoteWarp {
    /// `anchors` in any order; one per tick (the last wins) and with frames increasing in
    /// tick order (an anchor that goes back in the audio is ignored). `tempo` is quarter notes
    /// per minute, the rate of a warp with fewer than two anchors.
    pub fn new(mut anchors: Vec<(u64, i64)>, tempo: f64) -> Self {
        let default_rate = SYNC_FRAME_RATE * 60.0 / (tempo.max(1.0) * TICKS_PER_QUARTER as f64);
        anchors.sort_by_key(|a| a.0);
        let mut clean: Vec<(f64, f64)> = Vec::new();
        for (tick, frame) in anchors {
            let a = (tick as f64, frame as f64);
            if clean.last().is_some_and(|l| l.0 == a.0) {
                clean.pop();
            }
            if clean.last().is_none_or(|l| a.1 > l.1) {
                clean.push(a);
            }
        }
        Self {
            anchors: clean,
            default_rate,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.anchors.is_empty()
    }

    /// Frames per tick at a tick.
    pub fn rate_at(&self, tick: f64) -> f64 {
        let a = &self.anchors;
        if a.len() < 2 {
            return self.default_rate;
        }
        let k = a.partition_point(|p| p.0 <= tick).clamp(1, a.len() - 1);
        (a[k].1 - a[k - 1].1) / (a[k].0 - a[k - 1].0)
    }

    /// The audio frame of a tick.
    pub fn frame_at(&self, tick: f64) -> f64 {
        let Some(&(t0, f0)) = self
            .anchors
            .iter()
            .rev()
            .find(|p| p.0 <= tick)
            .or(self.anchors.first())
        else {
            return tick * self.default_rate;
        };
        f0 + (tick - t0) * self.rate_at(tick)
    }
}

impl Score {
    /// Start tick of every chant note (reading order) in the performance of the cached bars.
    fn chant_note_ticks(&self, tr: &crate::chant::Transcription) -> Vec<u64> {
        let mut bar_start = Vec::with_capacity(self.master_bars.len());
        let mut tick = 0u64;
        for b in 0..self.master_bars.len() {
            bar_start.push(tick);
            tick += self.played_ticks(b) as u64;
        }
        tr.positions
            .iter()
            .map(|&(bar, beat)| {
                let before: u64 = tr
                    .bars
                    .get(bar)
                    .and_then(|b| b.voices.first())
                    .map_or(0, |v| {
                        v.beats.iter().take(beat).map(|b| b.ticks() as u64).sum()
                    });
                bar_start.get(bar).copied().unwrap_or(tick) + before
            })
            .collect()
    }

    /// Keeps a chant score's audio sync in step with its chant: drops note points whose note is
    /// gone (returns how many) and derives the bar points from the rest. A no-op without audio
    /// or a chant. Call after anything that changed the chant or the note points.
    pub fn refresh_chant_sync(&mut self) -> usize {
        let Some(t) = self.tracks.iter().position(|t| t.chant.is_some()) else {
            return 0;
        };
        let tempo = self.tempo as f64;
        if self.audio.is_none() {
            return 0;
        }
        let chant = self.tracks[t].chant.as_deref().unwrap();
        let ids: HashMap<u32, usize> = chant
            .note_ids()
            .into_iter()
            .enumerate()
            .map(|(i, id)| (id, i))
            .collect();
        let tr = transcribe(chant);
        let starts = self.chant_note_ticks(&tr);
        let audio = self.audio.as_mut().unwrap();
        let before = audio.chant_points.len();
        audio.chant_points.retain(|p| ids.contains_key(&p.note));
        let dropped = before - audio.chant_points.len();
        if audio.chant_points.is_empty() {
            audio.points.clear();
            return dropped;
        }
        let warp = NoteWarp::new(
            audio
                .chant_points
                .iter()
                .map(|p| (starts[ids[&p.note]], p.frame))
                .collect(),
            tempo,
        );
        // A point at the first note of every bar.
        let mut bars: Vec<(usize, f64, f64)> = Vec::new();
        for (i, &(bar, _)) in tr.positions.iter().enumerate() {
            if bars.last().is_some_and(|b| b.0 == bar) {
                continue;
            }
            let tick = starts[i] as f64;
            bars.push((bar, tick, warp.frame_at(tick)));
        }
        let mut out: Vec<SyncPoint> = Vec::new();
        for (k, &(bar, tick, frame)) in bars.iter().enumerate() {
            if frame < 0.0 {
                continue;
            }
            let rate = match bars.get(k + 1) {
                Some(n) if n.2 > frame => (n.2 - frame) / (n.1 - tick),
                _ => warp.rate_at(tick),
            };
            out.push(SyncPoint {
                bar,
                occurrence: 0,
                frame: frame.round() as i64,
                tempo: SYNC_FRAME_RATE * 60.0 / (rate * TICKS_PER_QUARTER as f64),
            });
        }
        audio.points = out;
        dropped
    }

    /// Sets the sync point of a chant note (by id) to an audio frame, returning the frame it had.
    /// The bar points are not refreshed ([`Score::refresh_chant_sync`]).
    pub fn set_chant_sync_point(&mut self, note: u32, frame: i64) -> Option<i64> {
        let audio = self.audio.as_mut()?;
        let old = audio.chant_points.iter().position(|p| p.note == note);
        let old = old.map(|i| audio.chant_points.remove(i).frame);
        audio.chant_points.push(ChantSyncPoint { note, frame });
        old
    }

    /// Takes the sync point off a chant note, returning the frame it had.
    pub fn remove_chant_sync_point(&mut self, note: u32) -> Option<i64> {
        let audio = self.audio.as_mut()?;
        let i = audio.chant_points.iter().position(|p| p.note == note)?;
        Some(audio.chant_points.remove(i).frame)
    }

    /// The audio time of every note's start in frames, for a chant score with note points.
    pub fn chant_note_frames(&self) -> Option<Vec<i64>> {
        let chant = self.tracks.iter().find_map(|t| t.chant.as_deref())?;
        let audio = self.audio.as_ref()?;
        if audio.chant_points.is_empty() {
            return None;
        }
        let tr = transcribe(chant);
        let starts = self.chant_note_ticks(&tr);
        let warp = NoteWarp::new(
            audio
                .chant_points
                .iter()
                .filter_map(|p| Some((starts[chant.index_of_id(p.note)?], p.frame)))
                .collect(),
            self.tempo as f64,
        );
        Some(
            starts
                .iter()
                .map(|&t| warp.frame_at(t as f64).round() as i64)
                .collect(),
        )
    }

    /// Turns bar-level points into points on the first note of each bar (a chant score read
    /// from a file or made from a modern one); the bar points are then derived again.
    pub fn bar_points_to_notes(&mut self) {
        let Some(t) = self.tracks.iter().position(|t| t.chant.is_some()) else {
            return;
        };
        let Some(audio) = self.audio.as_mut() else {
            return;
        };
        let chant = self.tracks[t].chant.as_deref().unwrap();
        let tr = transcribe(chant);
        let ids = chant.note_ids();
        let mut points: Vec<ChantSyncPoint> = Vec::new();
        for p in std::mem::take(&mut audio.points) {
            if p.occurrence != 0 {
                continue;
            }
            if let Some(i) = tr.positions.iter().position(|q| q.0 == p.bar) {
                points.retain(|q| q.note != ids[i]);
                points.push(ChantSyncPoint {
                    note: ids[i],
                    frame: p.frame,
                });
            }
        }
        audio.chant_points = points;
        self.refresh_chant_sync();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn warp_shares_time_by_ticks_and_extends_the_edges() {
        // 960 ticks per 44100 frames at the first stretch, half that rate in the second.
        let w = NoteWarp::new(vec![(1000, 10_000), (1960, 54_100), (2960, 98_200)], 60.0);
        assert!((w.frame_at(1480.0) - 32_050.0).abs() < 1.0);
        let r1 = w.rate_at(1500.0);
        let r2 = w.rate_at(2500.0);
        assert!((r1 - 44_100.0 / 960.0).abs() < 0.01);
        assert!((r2 - 44_100.0 / 1000.0).abs() < 0.01);
        // Before the first point: the first stretch's rate; after the last: the last's.
        assert!((w.frame_at(0.0) - (10_000.0 - 1000.0 * r1)).abs() < 1.0);
        assert!((w.frame_at(3960.0) - (98_200.0 + 1000.0 * r2)).abs() < 1.0);
    }

    #[test]
    fn one_point_uses_the_tempo() {
        let w = NoteWarp::new(vec![(960, 1000)], 120.0);
        assert!(
            (w.frame_at(1920.0) - (1000.0 + 960.0 * 44_100.0 * 60.0 / (120.0 * 960.0))).abs() < 1.0
        );
    }

    #[test]
    fn backwards_points_are_ignored() {
        let w = NoteWarp::new(vec![(0, 100), (960, 50), (1920, 300)], 60.0);
        assert!((w.frame_at(960.0) - 200.0).abs() < 1.0);
    }

    use crate::chant::{Chant, ChantClef, ChantNote, NeumeItem, Syllable, retranscribe};
    use crate::chant_edit::Join;
    use crate::{AudioSync, Track, TrackKind};

    fn syl(text: &str, word_start: bool, items: Vec<NeumeItem>) -> Syllable {
        Syllable {
            text: text.into(),
            word_start,
            items,
        }
    }

    fn note(p: u8) -> NeumeItem {
        NeumeItem::Note(ChantNote::new(p))
    }

    /// `(c4) Al(fg) le(h) ; lu(g) ia(f)`: two bars of 3 and 2 notes (ids 1–5).
    fn score() -> Score {
        let chant = Chant {
            syllables: vec![
                syl("", true, vec![NeumeItem::Clef(ChantClef::default())]),
                syl("Al", true, vec![note(5), note(6)]),
                syl("le", false, vec![note(7)]),
                syl("", true, vec![NeumeItem::Bar(";".into())]),
                syl("lu", true, vec![note(6)]),
                syl("ia", false, vec![note(5)]),
            ],
            ..Chant::default()
        };
        let mut score = Score {
            tempo: 120,
            tracks: vec![Track {
                kind: TrackKind::Chant,
                chant: Some(Box::new(chant)),
                ..Track::default()
            }],
            audio: Some(AudioSync::default()),
            ..Score::default()
        };
        retranscribe(&mut score, 0);
        score
    }

    fn chant_mut(s: &mut Score) -> &mut Chant {
        s.tracks[0].chant.as_deref_mut().unwrap()
    }

    fn bar_frames(s: &Score) -> Vec<(usize, i64)> {
        s.audio
            .as_ref()
            .unwrap()
            .points
            .iter()
            .map(|p| (p.bar, p.frame))
            .collect()
    }

    fn point(s: &mut Score, note: u32, frame: i64) {
        s.set_chant_sync_point(note, frame);
        s.refresh_chant_sync();
    }

    #[test]
    fn notes_get_ids_and_the_bar_points_are_derived() {
        let mut s = score();
        assert_eq!(chant_mut(&mut s).note_ids(), [1, 2, 3, 4, 5]);
        assert!(bar_frames(&s).is_empty());
        // Ten frames a tick: bar 1 starts three eighths in.
        point(&mut s, 1, 10_000);
        point(&mut s, 5, 10_000 + 10 * 4 * 480);
        assert_eq!(bar_frames(&s), [(0, 10_000), (1, 10_000 + 10 * 3 * 480)]);
        let tempo = s.audio.as_ref().unwrap().points[0].tempo;
        assert!((tempo - 44_100.0 * 60.0 / (10.0 * 960.0)).abs() < 1e-6);
    }

    #[test]
    fn time_between_points_follows_the_note_lengths() {
        let mut s = score();
        // A mora doubles the first note: the first bar is 960 + 480 + 480 ticks long.
        chant_mut(&mut s).set_mora(0, 1).unwrap();
        retranscribe(&mut s, 0);
        point(&mut s, 1, 10_000);
        point(&mut s, 5, 10_000 + 10 * (1920 + 480));
        assert_eq!(bar_frames(&s)[1], (1, 10_000 + 10 * 1920));
        // Both notes of the pes share the time by length: the second starts a half in.
        let frames = s.chant_note_frames().unwrap();
        assert_eq!(frames[1] - frames[0], 9600);
        assert_eq!(frames[2] - frames[1], 4800);
    }

    #[test]
    fn before_the_first_and_after_the_last_point_the_nearest_rate_is_used() {
        let mut s = score();
        point(&mut s, 2, 100_000);
        point(&mut s, 4, 100_000 + 10 * 960);
        let f = s.chant_note_frames().unwrap();
        // Ten frames a tick in the stretch; the same before and after it.
        assert_eq!(f[0], 100_000 - 4_800);
        assert_eq!(f[4], f[3] + 4_800);
    }

    #[test]
    fn points_stay_with_their_notes_through_edits() {
        let mut s = score();
        point(&mut s, 1, 10_000);
        point(&mut s, 4, 50_000);
        // A note and a division elsewhere, then one before everything.
        chant_mut(&mut s)
            .insert_note(2, true, Join::Neume, 8)
            .unwrap();
        chant_mut(&mut s).insert_division(0, ",").unwrap();
        chant_mut(&mut s)
            .insert_note(0, false, Join::NewSyllable, 5)
            .unwrap();
        retranscribe(&mut s, 0);
        let a = s.audio.as_ref().unwrap();
        assert_eq!(a.chant_points.len(), 2);
        let chant = s.tracks[0].chant.as_deref().unwrap();
        let (i1, i4) = (chant.index_of_id(1).unwrap(), chant.index_of_id(4).unwrap());
        assert_eq!((i1, i4), (1, 5));
        let f = s.chant_note_frames().unwrap();
        assert_eq!((f[i1], f[i4]), (10_000, 50_000));
    }

    #[test]
    fn a_deleted_note_takes_its_point_with_it() {
        let mut s = score();
        point(&mut s, 2, 10_000);
        point(&mut s, 4, 50_000);
        chant_mut(&mut s).delete_note(3).unwrap();
        retranscribe(&mut s, 0);
        // `retranscribe` already dropped it; one more refresh has nothing left to report.
        let a = s.audio.as_ref().unwrap();
        assert_eq!(a.chant_points.len(), 1);
        assert_eq!(a.chant_points[0].note, 2);
        assert_eq!(s.refresh_chant_sync(), 0);
    }

    #[test]
    fn no_points_no_bar_points() {
        let mut s = score();
        point(&mut s, 1, 10_000);
        s.remove_chant_sync_point(1);
        s.refresh_chant_sync();
        assert!(s.audio.as_ref().unwrap().points.is_empty());
    }

    #[test]
    fn bar_points_become_note_points() {
        let mut s = score();
        let a = s.audio.as_mut().unwrap();
        for (bar, frame) in [(0, 1_000), (1, 30_000)] {
            a.points.push(crate::SyncPoint {
                bar,
                occurrence: 0,
                frame,
                tempo: 100.0,
            });
        }
        s.bar_points_to_notes();
        let a = s.audio.as_ref().unwrap();
        let notes: Vec<(u32, i64)> = a.chant_points.iter().map(|p| (p.note, p.frame)).collect();
        assert_eq!(notes, [(1, 1_000), (4, 30_000)]);
        assert_eq!(bar_frames(&s), [(0, 1_000), (1, 30_000)]);
    }
}
