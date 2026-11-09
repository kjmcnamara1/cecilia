//! Audio sync of a modern score by hand, as for a chant (see [`crate::chantsync`]): tapping along
//! with the music, setting the cursor beat to the playhead, nudging a point, the mouse on the
//! waveform. The sync lives on beats (see [`cecilia_core::beat_sync`]); the player's bar points
//! are derived from it after every edit. Beats are the cells, rests included; the unit of a tap
//! is always one beat of the cursor's voice.

use cecilia_audio::SYNC_RATE;
use cecilia_core::beat_sync::BeatMark;

use crate::App;

/// A tick of the waveform's score ruler.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RulerMark {
    /// Audio position in frames (with the audio's padding).
    pub frame: i64,
    /// The start of a bar: a major tick.
    pub major: bool,
    /// The bar number, on a major tick.
    pub left: Option<String>,
    /// The beat, `bar:beat` as in the status line.
    pub right: String,
}

impl App {
    /// The voice a modern score's sync works on: the cursor's.
    fn sync_voice(&self) -> (usize, usize) {
        let c = self.pane().cursor;
        (c.track, c.voice)
    }

    pub(crate) fn beat_sync_ready(&mut self) -> bool {
        if self.buffer().score.is_chant() {
            self.error("sync: a chant score syncs by note");
            return false;
        }
        if self.buffer().score.audio.is_none() {
            self.error("no audio attached (use :audio FILE)");
            return false;
        }
        true
    }

    /// The beat under the cursor (else the nearest before it) on the voice's ruler: its index
    /// in `marks`.
    fn cursor_mark(&self, marks: &[BeatMark]) -> Option<usize> {
        let c = self.pane().cursor;
        marks
            .iter()
            .rposition(|m| (m.bar, m.index) <= (c.bar, c.beat))
            .or((!marks.is_empty()).then_some(0))
    }

    /// Where the cursor's beat is in the audio (seconds, without the padding), as the sync has
    /// it. None on a chant or without audio.
    pub fn beat_cursor_audio_time(&self) -> Option<f64> {
        let score = &self.buffer().score;
        if score.is_chant() {
            return None;
        }
        let (track, voice) = self.sync_voice();
        let marks = score.beat_marks(track, voice)?;
        let frame = marks.get(self.cursor_mark(&marks)?)?.frame;
        let pad = score.audio.as_ref()?.frame_padding as f64 / SYNC_RATE;
        Some(frame as f64 / SYNC_RATE - pad)
    }

    /// Where the cursor stands in the audio, on a chant or a modern score.
    pub fn sync_cursor_audio_time(&self) -> Option<f64> {
        self.chant_cursor_audio_time()
            .or_else(|| self.beat_cursor_audio_time())
    }

    /// The ticks of the waveform's score ruler: a major one at the first beat of every bar, a
    /// minor one at every other beat. None on a chant or without audio.
    pub fn beat_ruler(&self, buffer: usize) -> Option<Vec<RulerMark>> {
        let score = &self.buffers.get(buffer)?.score;
        if score.is_chant() {
            return None;
        }
        let c = self.pane().cursor;
        let (track, voice) = if self.pane().buffer == buffer {
            (c.track, c.voice)
        } else {
            (0, 0)
        };
        let t = score.tracks.get(track)?;
        let marks = score.beat_marks(track, voice)?;
        let mut first = None;
        Some(
            marks
                .iter()
                .map(|m| {
                    let pos =
                        cecilia_core::position::beat_position(score, t, m.bar, voice, m.index);
                    let label = cecilia_core::position::format_position(m.bar, pos, true);
                    let beat = label
                        .split_once(':')
                        .map_or(label.clone(), |l| l.1.to_string());
                    let major = first != Some(m.bar);
                    first = Some(m.bar);
                    RulerMark {
                        frame: m.frame,
                        major,
                        left: major.then(|| (m.bar + 1).to_string()),
                        right: beat,
                    }
                })
                .collect(),
        )
    }

    /// The id of the beat at `(bar, index)` of a voice, giving the score's beats ids first.
    fn beat_id(
        score: &mut cecilia_core::Score,
        track: usize,
        voice: usize,
        at: (usize, usize),
    ) -> Option<u32> {
        score.assign_beat_ids();
        let id = score
            .tracks
            .get(track)?
            .bars
            .get(at.0)?
            .voices
            .get(voice)?
            .beats
            .get(at.1)
            .filter(|b| !b.empty)?
            .id;
        (id != 0).then_some(id)
    }

    /// Moves the cursor to a beat, as an edit does.
    fn goto_mark(cx: &mut crate::edit::EditCx, m: &BeatMark) {
        cx.cursor.bar = m.bar;
        cx.cursor.beat = m.index;
    }

    /// A tap at `secs` (audio time at the playhead): syncs the cursor's beat there, snapped to
    /// the strongest of the `onsets` close by, and moves the cursor to the next beat.
    pub fn beat_tap(&mut self, secs: f64, onsets: &[f32]) {
        if !self.beat_sync_ready() {
            return;
        }
        let frame = (self.snapped(secs, onsets) * SYNC_RATE).round() as i64;
        let c = self.pane().cursor;
        let done = self.edit(|cx| {
            let (t, v) = (cx.cursor.track, cx.cursor.voice);
            let score = cx.whole();
            score.bar_points_to_beats(t, v);
            let id = Self::beat_id(score, t, v, (c.bar, c.beat)).ok_or("sync: no beat here")?;
            let old = score.set_beat_sync_point(id, frame);
            score.refresh_beat_sync();
            let marks = score.beat_marks(t, v).unwrap_or_default();
            let here = marks.iter().position(|m| m.beat == id);
            let next = here.and_then(|i| marks.get(i + 1)).copied();
            if let Some(n) = &next {
                Self::goto_mark(cx, n);
            }
            Ok((id, old, next.is_some()))
        });
        let Some((id, old, more)) = done else {
            return;
        };
        self.pa.beat_trail.push((id, old));
        if more {
            self.info("tap: next beat (:sync-undo to go back, :sync to stop)");
        } else {
            self.info("tap: end of the score (:sync-undo to go back, :sync to stop)");
        }
    }

    /// Takes back the last tap: its point returns to what it was and the cursor goes back to
    /// its beat.
    pub fn beat_sync_undo(&mut self) {
        let Some(&(id, old)) = self.pa.beat_trail.last() else {
            return self.info("sync-undo: nothing tapped yet");
        };
        let done = self.edit(|cx| {
            let (t, v) = (cx.cursor.track, cx.cursor.voice);
            let score = cx.whole();
            match old {
                Some(f) => {
                    score.set_beat_sync_point(id, f);
                }
                None => {
                    score.remove_beat_sync_point(id);
                }
            }
            score.refresh_beat_sync();
            let marks = score.beat_marks(t, v).unwrap_or_default();
            let m = marks
                .iter()
                .find(|m| m.beat == id)
                .copied()
                .ok_or("the beat is gone")?;
            Self::goto_mark(cx, &m);
            Ok(())
        });
        if done.is_some() {
            self.pa.beat_trail.pop();
            self.info("tap: this beat again");
        }
    }

    /// Sets the cursor beat to `secs` (audio time at the playhead), snapped to the strongest of
    /// the `onsets` close by.
    pub fn beat_sync_here(&mut self, secs: f64, onsets: &[f32]) {
        if !self.beat_sync_ready() {
            return;
        }
        let frame = (self.snapped(secs, onsets) * SYNC_RATE).round() as i64;
        let c = self.pane().cursor;
        let done = self.edit(|cx| {
            let (t, v) = (cx.cursor.track, cx.cursor.voice);
            let score = cx.whole();
            score.bar_points_to_beats(t, v);
            let id = Self::beat_id(score, t, v, (c.bar, c.beat)).ok_or("sync: no beat here")?;
            score.set_beat_sync_point(id, frame);
            score.refresh_beat_sync();
            Ok(())
        });
        if done.is_some() {
            self.info(format!("sync: at {:.2} s", frame as f64 / SYNC_RATE));
        }
    }

    /// The ids of the beats with a sync point in a range of the score (`from` to `to`, as
    /// `(bar, beat)` of the cursor's voice).
    pub(crate) fn beat_points_in(&self, from: (usize, usize), to: (usize, usize)) -> Vec<u32> {
        let score = &self.buffer().score;
        let Some(audio) = &score.audio else {
            return Vec::new();
        };
        let (track, voice) = self.sync_voice();
        let marks = score.beat_marks(track, voice).unwrap_or_default();
        marks
            .iter()
            .filter(|m| (m.bar, m.index) >= from && (m.bar, m.index) <= to)
            .filter(|m| audio.beat_points.iter().any(|p| p.beat == m.beat))
            .map(|m| m.beat)
            .collect()
    }

    /// `d` and `c` while syncing on a modern score: the beat points of the range go.
    pub(crate) fn beat_sync_operate(&mut self, from: (usize, usize), to: (usize, usize)) {
        let ids = self.beat_points_in(from, to);
        let n = self.edit(|cx| {
            let score = cx.whole();
            let audio = score.audio.as_mut().ok_or("no audio attached")?;
            let before = audio.beat_points.len();
            audio.beat_points.retain(|p| !ids.contains(&p.beat));
            let n = before - audio.beat_points.len();
            score.refresh_beat_sync();
            Ok(n)
        });
        if let Some(n) = n {
            self.info(format!(
                "{n} sync point{} removed",
                if n == 1 { "" } else { "s" }
            ));
        }
    }

    /// `:sync-nudge` in Visual mode: moves every sync point of the selection (none is made).
    fn beat_sync_nudge_range(&mut self, from: (usize, usize), to: (usize, usize), ms: i32) {
        let ids = self.beat_points_in(from, to);
        if ids.is_empty() {
            return self.info("sync-nudge: no sync points in the selection");
        }
        let d = (ms as f64 * SYNC_RATE / 1000.0).round() as i64;
        let n = ids.len();
        self.edit(|cx| {
            let score = cx.whole();
            if let Some(audio) = score.audio.as_mut() {
                for p in audio
                    .beat_points
                    .iter_mut()
                    .filter(|p| ids.contains(&p.beat))
                {
                    p.frame = (p.frame + d).max(0);
                }
            }
            score.refresh_beat_sync();
            Ok(())
        });
        self.info(format!(
            "{n} sync point{} moved ({ms:+} ms)",
            if n == 1 { "" } else { "s" }
        ));
    }

    /// Moves the sync point of the cursor beat by `ms` milliseconds. A beat with no point gets
    /// one first, where the sync puts it now.
    pub fn beat_sync_nudge(&mut self, ms: i32) {
        if !self.beat_sync_ready() {
            return;
        }
        if let Some((a, c, bars)) = self.selection() {
            let (a, c) = ((a.bar, a.beat), (c.bar, c.beat));
            let (from, to) = if a <= c { (a, c) } else { (c, a) };
            let (from, to) = if bars {
                ((from.0, 0), (to.0, usize::MAX))
            } else {
                (from, to)
            };
            return self.beat_sync_nudge_range(from, to, ms);
        }
        let c = self.pane().cursor;
        let done = self.edit(|cx| {
            let (t, v) = (cx.cursor.track, cx.cursor.voice);
            let score = cx.whole();
            score.bar_points_to_beats(t, v);
            let id = Self::beat_id(score, t, v, (c.bar, c.beat)).ok_or("sync: no beat here")?;
            let existing = score
                .audio
                .as_ref()
                .ok_or("no audio attached")?
                .beat_points
                .iter()
                .find(|p| p.beat == id)
                .map(|p| p.frame);
            let (base, added) = match existing {
                Some(f) => (f, false),
                None => {
                    let marks = score.beat_marks(t, v).unwrap_or_default();
                    let at = marks.iter().find(|m| m.beat == id).map_or(0, |m| m.frame);
                    (at, true)
                }
            };
            let frame = (base + (ms as f64 * SYNC_RATE / 1000.0).round() as i64).max(0);
            score.set_beat_sync_point(id, frame);
            score.refresh_beat_sync();
            Ok((frame, added))
        });
        if let Some((f, added)) = done {
            self.info(format!(
                "sync point {}at {:.3} s ({ms:+} ms)",
                if added { "added " } else { "" },
                f as f64 / SYNC_RATE
            ));
        }
    }

    /// The mouse on the waveform at `secs` (audio time with the padding): a sync point goes on
    /// the beat nearest to it, snapped to the strongest of the `onsets` close by, and the
    /// cursor goes there. Returns the beat's id.
    pub(crate) fn beat_sync_closest(&mut self, secs: f64, onsets: &[f32]) -> Option<u32> {
        if !self.beat_sync_ready() {
            return None;
        }
        let (track, voice) = self.sync_voice();
        let marks = self.buffer().score.beat_marks(track, voice)?;
        let m = *marks.iter().min_by(|a, b| {
            let d = |m: &BeatMark| (m.frame as f64 / SYNC_RATE - secs).abs();
            d(a).total_cmp(&d(b))
        })?;
        let frame = (self.snapped(secs, onsets) * SYNC_RATE).round() as i64;
        self.edit(|cx| {
            let score = cx.whole();
            score.bar_points_to_beats(track, voice);
            let id = Self::beat_id(score, track, voice, (m.bar, m.index)).ok_or("no beat")?;
            score.set_beat_sync_point(id, frame);
            score.refresh_beat_sync();
            Self::goto_mark(cx, &m);
            Ok(id)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Config;
    use crate::command::Command;
    use cecilia_core::AudioSync;

    fn run(a: &mut App, line: &str) {
        a.run(Command::parse(line).unwrap());
    }

    fn keys(a: &mut App, s: &str) {
        for k in crate::keys::parse_keys(s, &[crate::keys::Key::char(' ')]).unwrap() {
            a.handle_key(k);
        }
    }

    /// A modern score with four quarter beats in its first bar, and audio.
    fn app() -> App {
        let mut a = App::new(Config::default(), None);
        a.run(Command::New);
        keys(&mut a, "i1<CR>l2<CR>l3<CR>l4<CR><Esc>0");
        let b = a.pane().buffer;
        a.buffers[b].score.audio = Some(AudioSync {
            file: Some("x.mp3".into()),
            ..Default::default()
        });
        a
    }

    fn buf(a: &App) -> usize {
        a.pane().buffer
    }

    fn secs(s: f64) -> i64 {
        (s * SYNC_RATE).round() as i64
    }

    /// The frames of the beats with a point, in order of the beats.
    fn points(a: &App) -> Vec<i64> {
        let s = &a.buffer().score;
        let audio = s.audio.as_ref().unwrap();
        s.beat_marks(0, 0)
            .unwrap()
            .iter()
            .filter(|m| audio.beat_points.iter().any(|p| p.beat == m.beat))
            .map(|m| {
                audio
                    .beat_points
                    .iter()
                    .find(|p| p.beat == m.beat)
                    .unwrap()
                    .frame
            })
            .collect()
    }

    fn at(a: &App) -> (usize, usize) {
        (a.pane().cursor.bar, a.pane().cursor.beat)
    }

    #[test]
    fn tapping_syncs_the_cursor_beat_and_moves_to_the_next() {
        let mut a = app();
        run(&mut a, "sync");
        assert!(a.syncing && a.wave_sync_editing());
        a.beat_tap(1.0, &[]);
        assert_eq!(at(&a), (0, 1));
        a.beat_tap(1.7, &[]);
        a.beat_tap(2.1, &[]);
        assert_eq!(at(&a), (0, 3));
        assert_eq!(points(&a), [secs(1.0), secs(1.7), secs(2.1)]);
        // The last beat has no next: the cursor stays.
        a.beat_tap(3.0, &[]);
        assert_eq!(at(&a), (0, 3));
        // The waveform's ruler has them where the sync put them; the bar points follow.
        let marks = a.buffer().score.beat_marks(0, 0).unwrap();
        assert_eq!(marks[1].frame, secs(1.7));
        assert!(!a.buffer().score.audio.as_ref().unwrap().points.is_empty());
        let ruler = a.beat_ruler(buf(&a)).unwrap();
        assert!(ruler[0].major && ruler[0].left.as_deref() == Some("1") && ruler[0].right == "1");
        assert!(!ruler[1].major && ruler[1].right == "2");
        // Undo takes the taps back one by one, the cursor on the beat to tap again.
        run(&mut a, "sync-undo");
        assert_eq!(at(&a), (0, 3));
        assert_eq!(points(&a), [secs(1.0), secs(1.7), secs(2.1)]);
        run(&mut a, "sync-undo");
        assert_eq!(at(&a), (0, 2));
        assert_eq!(points(&a), [secs(1.0), secs(1.7)]);
    }

    #[test]
    fn here_and_nudge_set_and_move_a_point() {
        let mut a = app();
        keys(&mut a, "l");
        a.beat_sync_here(2.0, &[]);
        assert_eq!(points(&a), [secs(2.0)]);
        assert_eq!(at(&a), (0, 1));
        a.beat_sync_nudge(50);
        assert_eq!(points(&a), [secs(2.05)]);
        // A beat with no point gets one first, where the sync has it now.
        keys(&mut a, "l");
        let at_now = a.buffer().score.beat_marks(0, 0).unwrap()[2].frame;
        a.beat_sync_nudge(-10);
        assert_eq!(points(&a)[1], at_now - secs(0.01));
    }

    #[test]
    fn remove_and_the_selection_take_points_away() {
        let mut a = app();
        for (i, t) in [1.0, 1.5, 2.0, 2.5].iter().enumerate() {
            keys(&mut a, if i == 0 { "0" } else { "l" });
            a.beat_sync_here(*t, &[]);
        }
        assert_eq!(points(&a).len(), 4);
        run(&mut a, "sync-remove");
        assert_eq!(points(&a), [secs(1.0), secs(1.5), secs(2.0)]);
        // Nudging a selection moves its points only.
        keys(&mut a, "0vl");
        a.beat_sync_nudge(100);
        assert_eq!(points(&a), [secs(1.1), secs(1.6), secs(2.0)]);
    }

    #[test]
    fn points_stay_on_their_beats_through_edits() {
        let mut a = app();
        keys(&mut a, "l");
        a.beat_sync_here(2.0, &[]);
        let before = points(&a);
        // A rest beat before the cursor: the point is on the same beat, one further along.
        keys(&mut a, "0O");
        keys(&mut a, "<Esc>");
        let s = &a.buffer().score;
        let audio = s.audio.as_ref().unwrap();
        assert_eq!(audio.beat_points.len(), 1);
        assert_eq!(points(&a), before);
        // Deleting that beat takes its point.
        let id = audio.beat_points[0].beat;
        let marks = a.buffer().score.beat_marks(0, 0).unwrap();
        let i = marks.iter().position(|m| m.beat == id).unwrap();
        a.pane_mut().cursor.bar = marks[i].bar;
        a.pane_mut().cursor.beat = marks[i].index;
        run(&mut a, "delete-beat");
        assert!(
            a.buffer()
                .score
                .audio
                .as_ref()
                .unwrap()
                .beat_points
                .is_empty()
        );
    }

    #[test]
    fn the_mouse_adds_grabs_and_moves_beat_points_on_the_waveform() {
        let mut a = app();
        run(&mut a, "sync");
        // Nothing synced: the beats lie at the score's tempo from 0 s. A press adds a point
        // on the beat nearest the mouse, not the one the cursor is on.
        let frames = a.buffer().score.beat_marks(0, 0).unwrap();
        let t = frames[2].frame as f64 / SYNC_RATE;
        a.wave_mouse_down(buf(&a), t, 0.001);
        assert_eq!(a.wave_pressed(&[]), Some((buf(&a), t)));
        assert_eq!(points(&a), [frames[2].frame]);
        assert_eq!(at(&a), (0, 2));
        a.wave_mouse_move(2.5);
        assert_eq!(points(&a), [secs(2.5)]);
        a.wave_mouse_up();
        // Pressing near it grabs it instead.
        a.wave_mouse_down(buf(&a), 2.55, 0.1);
        a.wave_pressed(&[]);
        a.wave_mouse_move(3.0);
        a.wave_mouse_up();
        assert_eq!(points(&a), [secs(3.0)]);
        run(&mut a, "undo");
        assert_eq!(points(&a), [secs(2.5)]);
    }

    #[test]
    fn bar_points_from_a_guitar_pro_score_are_kept_when_a_beat_is_synced() {
        let mut a = app();
        let b = buf(&a);
        a.buffers[b].score.audio.as_mut().unwrap().points = vec![cecilia_core::SyncPoint {
            bar: 0,
            occurrence: 0,
            frame: secs(1.0),
            tempo: 60.0,
        }];
        let first = a.buffer().score.beat_marks(0, 0).unwrap()[0].frame;
        assert_eq!(first, secs(1.0));
        keys(&mut a, "l");
        a.beat_sync_here(2.4, &[]);
        let marks = a.buffer().score.beat_marks(0, 0).unwrap();
        assert_eq!(
            marks[0].frame,
            secs(1.0),
            "the first beat stays where the bar point put it"
        );
        assert_eq!(marks[1].frame, secs(2.4));
    }

    #[test]
    fn the_midi_notes_sound_where_the_ruler_puts_them() {
        use cecilia_audio::{MidiEvent, Sequence, SequenceOptions};
        let mut a = app();
        // Points on the first and third beats only: the notes in between are shared out.
        a.beat_sync_here(1.0, &[]);
        keys(&mut a, "ll");
        a.beat_sync_here(4.0, &[]);
        let score = &a.buffer().score;
        let want: Vec<f64> = score
            .beat_marks(0, 0)
            .unwrap()
            .iter()
            .map(|m| m.frame as f64 / SYNC_RATE)
            .collect();
        let seq = Sequence::build(
            score,
            &SequenceOptions {
                audio_sync: true,
                ..Default::default()
            },
        );
        let got: Vec<f64> = seq
            .events
            .iter()
            .filter(|e| matches!(e.event, MidiEvent::NoteOn { .. }))
            .map(|e| e.time)
            .collect();
        assert!(got.len() >= 4);
        for (g, w) in got.iter().zip(&want) {
            assert!((g - w).abs() < 1e-3, "{got:?} {want:?}");
        }
        assert!(
            (got[1] - 2.5).abs() < 1e-3,
            "halfway between the points: {got:?}"
        );
    }
}
