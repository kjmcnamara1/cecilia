//! Audio sync of a chant score by hand: tapping along with the singing, setting the cursor
//! note to the playhead, nudging a point. The sync lives on notes (see
//! [`cecilia_core::chant_sync`]); the player's bar points are derived from it after every edit.

use cecilia_audio::SYNC_RATE;
use cecilia_core::chant::{Chant, transcribe};

use crate::App;

/// How far (seconds) a tap is moved to reach a sung onset.
pub const SNAP_WINDOW: f64 = 0.07;

/// What one tap syncs.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum SyncUnit {
    /// The first note of each syllable.
    #[default]
    Syllable,
    /// Every note.
    Note,
}

impl SyncUnit {
    pub fn name(self) -> &'static str {
        match self {
            SyncUnit::Syllable => "syllable",
            SyncUnit::Note => "note",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "syllable" | "syl" => Some(SyncUnit::Syllable),
            "note" => Some(SyncUnit::Note),
            _ => None,
        }
    }
}

/// A tapping session on a chant score.
#[derive(Debug, Clone, Default)]
pub struct ChantTap {
    /// The note (id) the next tap syncs; none after the last.
    pub next: Option<u32>,
    /// Taps made, for undo: the note and the frame its point had before.
    pub trail: Vec<(u32, Option<i64>)>,
    /// Text of the syllable to tap next, for the status line.
    pub label: String,
}

/// The notes (reading-order indices) a tap can sync, in order.
pub fn tap_targets(chant: &Chant, unit: SyncUnit) -> Vec<usize> {
    match unit {
        SyncUnit::Note => (0..chant.note_count()).collect(),
        SyncUnit::Syllable => {
            let mut out = Vec::new();
            let mut k = 0;
            for s in &chant.syllables {
                let n = s.notes().count();
                if n > 0 {
                    out.push(k);
                }
                k += n;
            }
            out
        }
    }
}

/// How the syllable of a note reads in "tap: next …".
fn label_of(chant: &Chant, note: usize) -> String {
    let text = chant
        .syllable_of(note)
        .map(|s| chant.syllables[s].plain_text())
        .unwrap_or_default();
    let text = text.trim();
    if text.is_empty() {
        format!("note {}", note + 1)
    } else {
        text.to_string()
    }
}

impl App {
    pub(crate) fn chant_sync_ready(&mut self) -> bool {
        if !self.buffer().score.is_chant() {
            self.error("sync: not a chant score");
            return false;
        }
        if self.buffer().score.audio.is_none() {
            self.error("no audio attached (use :audio FILE)");
            return false;
        }
        true
    }

    /// Starts tapping at the cursor note (the syllable it is in). False (with a message) when
    /// the score has no audio or no notes.
    pub(crate) fn chant_tap_start(&mut self) -> bool {
        if !self.chant_sync_ready() {
            return false;
        }
        let Some(at) = self.chant_cursor_note() else {
            self.error("sync: the chant has no notes");
            return false;
        };
        let track = self.pane().cursor.track;
        let chant = self.buffer().score.tracks[track].chant.as_deref().unwrap();
        let targets = tap_targets(chant, self.pa.sync_unit);
        let first = targets
            .iter()
            .rev()
            .find(|&&k| k <= at)
            .copied()
            .unwrap_or(0);
        let tap = ChantTap {
            next: chant.note_ids().get(first).copied(),
            trail: Vec::new(),
            label: label_of(chant, first),
        };
        let msg = format!("tap: next {} (:sync-tap to tap, :sync to stop)", tap.label);
        self.pa.chant_tap = Some(tap);
        self.info(msg);
        true
    }

    /// Moves the cursor to a note, as an edit does.
    fn goto_note(cx: &mut crate::edit::EditCx, chant: &Chant, note: usize) {
        if let (Some((bar, beat)), Some(n)) = (transcribe(chant).place_of(note), chant.note(note)) {
            cx.cursor.bar = bar;
            cx.cursor.beat = beat;
            cx.cursor.line = n.pitch as i32;
        }
    }

    /// A tap at `secs` (audio time at the playhead): syncs the next note there, snapped to the
    /// nearest onset of `onsets`, and moves on.
    pub fn chant_tap(&mut self, secs: f64, onsets: &[f32]) {
        let Some(tap) = self.pa.chant_tap.clone() else {
            return;
        };
        let Some(note) = tap.next else {
            return self.info("tap: end of the chant (:sync-undo to go back, :sync to stop)");
        };
        let frame =
            (cecilia_audio::beats::snap(onsets, secs, SNAP_WINDOW) * SYNC_RATE).round() as i64;
        let unit = self.pa.sync_unit;
        let done = self.edit(|cx| {
            let t = cx.cursor.track;
            let score = cx.whole();
            let chant = score.tracks[t]
                .chant
                .as_deref()
                .ok_or("not a chant score")?;
            let idx = chant
                .index_of_id(note)
                .ok_or("tap: the note being synced is gone")?;
            let targets = tap_targets(chant, unit);
            let pos = targets.iter().position(|&k| k >= idx);
            let next = pos
                .and_then(|p| targets.get(if targets[p] == idx { p + 1 } else { p }))
                .copied();
            let ids = chant.note_ids();
            let label = next.map(|k| label_of(chant, k));
            let next_id = next.map(|k| ids[k]);
            let chant = chant.clone();
            let old = score.set_chant_sync_point(note, frame);
            score.refresh_chant_sync();
            Self::goto_note(cx, &chant, next.unwrap_or(idx));
            Ok((old, next_id, label))
        });
        let Some((old, next_id, label)) = done else {
            return;
        };
        if let Some(t) = self.pa.chant_tap.as_mut() {
            t.trail.push((note, old));
            t.next = next_id;
            if let Some(l) = &label {
                t.label = l.clone();
            }
        }
        match label {
            Some(l) => self.info(format!("tap: next {l}")),
            None => self.info("tap: end of the chant (:sync-undo to go back, :sync to stop)"),
        }
    }

    /// Takes back the last tap: its point returns to what it was and the next tap syncs that
    /// note again.
    pub fn chant_sync_undo(&mut self) {
        let Some(tap) = self.pa.chant_tap.clone() else {
            return self.error("sync-undo: not tapping (start with :sync)");
        };
        let Some(&(note, old)) = tap.trail.last() else {
            return self.info("sync-undo: nothing tapped yet");
        };
        let done = self.edit(|cx| {
            let t = cx.cursor.track;
            let score = cx.whole();
            match old {
                Some(f) => {
                    score.set_chant_sync_point(note, f);
                }
                None => {
                    score.remove_chant_sync_point(note);
                }
            }
            score.refresh_chant_sync();
            let chant = score.tracks[t]
                .chant
                .as_deref()
                .ok_or("not a chant score")?;
            let idx = chant.index_of_id(note).ok_or("the note is gone")?;
            let label = label_of(chant, idx);
            let chant = chant.clone();
            Self::goto_note(cx, &chant, idx);
            Ok(label)
        });
        let Some(label) = done else {
            return;
        };
        if let Some(t) = self.pa.chant_tap.as_mut() {
            t.trail.pop();
            t.next = Some(note);
            t.label = label.clone();
        }
        self.info(format!("tap: next {label}"));
    }

    /// Sets the cursor note to `secs` (audio time at the playhead), snapped to an onset.
    pub fn chant_sync_here(&mut self, secs: f64, onsets: &[f32]) {
        if !self.chant_sync_ready() {
            return;
        }
        let Some(at) = self.chant_cursor_note() else {
            return self.error("sync-here: the chant has no notes");
        };
        let frame =
            (cecilia_audio::beats::snap(onsets, secs, SNAP_WINDOW) * SYNC_RATE).round() as i64;
        let done = self.edit(|cx| {
            let t = cx.cursor.track;
            let score = cx.whole();
            let id = score.tracks[t]
                .chant
                .as_deref()
                .and_then(|c| c.note_ids().get(at).copied())
                .ok_or("no note under the cursor")?;
            score.set_chant_sync_point(id, frame);
            score.refresh_chant_sync();
            Ok(())
        });
        if done.is_some() {
            self.info(format!(
                "sync: note {} at {:.2} s",
                at + 1,
                frame as f64 / SYNC_RATE
            ));
        }
    }

    /// Moves the sync point under the cursor (its note's, else its syllable's first note's) by
    /// `ms` milliseconds.
    pub fn chant_sync_nudge(&mut self, ms: i32) {
        if !self.chant_sync_ready() {
            return;
        }
        let Some(at) = self.chant_cursor_note() else {
            return self.error("sync-nudge: the chant has no notes");
        };
        let done = self.edit(|cx| {
            let t = cx.cursor.track;
            let score = cx.whole();
            let chant = score.tracks[t]
                .chant
                .as_deref()
                .ok_or("not a chant score")?;
            let ids = chant.note_ids();
            let first = chant.syllable_of(at).map(|s| {
                chant.syllables[..s]
                    .iter()
                    .map(|x| x.notes().count())
                    .sum::<usize>()
            });
            let points = &score
                .audio
                .as_ref()
                .ok_or("no audio attached")?
                .chant_points;
            let find = |k: usize| points.iter().find(|p| p.note == ids[k]).copied();
            let p = find(at)
                .or_else(|| first.and_then(find))
                .ok_or("sync-nudge: no sync point on this note or its syllable")?;
            let frame = (p.frame + (ms as f64 * SYNC_RATE / 1000.0).round() as i64).max(0);
            score.set_chant_sync_point(p.note, frame);
            score.refresh_chant_sync();
            Ok(frame)
        });
        if let Some(f) = done {
            self.info(format!(
                "sync point at {:.3} s ({ms:+} ms)",
                f as f64 / SYNC_RATE
            ));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Config;
    use crate::command::Command;
    use cecilia_core::{AudioSync, ChantSyncPoint};

    /// Two bars: `Al(fg) le(h)`, then `lu(g) ia.(f.)` (notes 1–5; syllables start at 1, 3, 4, 5).
    const TEXT: &str = "name: t;\n%%\n(c4) Al(fg)le(h) (;) lu(g)ia.(f.) (::)\n";

    fn app() -> App {
        let mut a = App::new(Config::default(), None);
        let chant = cecilia_formats::gabc::parse(TEXT).unwrap();
        a.buffers[0].score = cecilia_formats::gabc::chant_score(chant);
        a.buffers[0].score.audio = Some(AudioSync {
            file: Some("x.mp3".into()),
            ..Default::default()
        });
        a
    }

    fn run(a: &mut App, line: &str) {
        a.run(Command::parse(line).unwrap());
    }

    fn points(a: &App) -> Vec<(u32, i64)> {
        let mut p: Vec<_> = a
            .buffer()
            .score
            .audio
            .as_ref()
            .unwrap()
            .chant_points
            .iter()
            .map(|p| (p.note, p.frame))
            .collect();
        p.sort();
        p
    }

    fn secs(s: f64) -> i64 {
        (s * SYNC_RATE).round() as i64
    }

    fn message(a: &App) -> String {
        a.message
            .as_ref()
            .map(|m| m.text.clone())
            .unwrap_or_default()
    }

    /// An onset every tenth of a second at 1.03, 2.0 and 3.5 s.
    fn onsets() -> Vec<f32> {
        let mut o = vec![0.0; 1000];
        for t in [103, 200, 350] {
            o[t] = 1.0;
        }
        o
    }

    fn cursor_note(a: &App) -> Option<usize> {
        a.chant_cursor_note()
    }

    #[test]
    fn tapping_walks_the_syllables_in_order_and_snaps_to_onsets() {
        let mut a = app();
        run(&mut a, "sync");
        assert!(a.pa.chant_tap.is_some());
        assert!(message(&a).contains("tap: next Al"), "{}", message(&a));
        let on = onsets();
        a.chant_tap(1.0, &on); // 30 ms before the onset at 1.03
        assert_eq!(cursor_note(&a), Some(2), "the cursor follows to `le`");
        assert!(message(&a).contains("tap: next le"));
        a.chant_tap(2.04, &on); // snaps to 2.0
        a.chant_tap(2.7, &on); // no onset near: as tapped
        a.chant_tap(3.55, &on);
        assert_eq!(
            points(&a),
            [
                (1, secs(1.03)),
                (3, secs(2.0)),
                (4, secs(2.7)),
                (5, secs(3.5))
            ]
        );
        assert!(a.pa.chant_tap.as_ref().unwrap().next.is_none());
        assert!(message(&a).contains("end of the chant"));
        assert!(a.pa.flags().contains("SYNC TAP"));
        // The player's bar points are derived: one at each bar's first note.
        let bars = &a.buffer().score.audio.as_ref().unwrap().points;
        assert_eq!(
            bars.iter().map(|p| (p.bar, p.frame)).collect::<Vec<_>>(),
            [(0, secs(1.03)), (1, secs(2.7)),]
        );
        run(&mut a, "sync");
        assert!(a.pa.chant_tap.is_none());
    }

    #[test]
    fn a_note_unit_taps_every_note() {
        let mut a = app();
        run(&mut a, "sync-unit note");
        run(&mut a, "sync");
        for i in 0..5 {
            a.chant_tap(1.0 + i as f64, &[]);
        }
        assert_eq!(points(&a).len(), 5);
        assert!(Command::parse("sync-unit word").is_err());
    }

    #[test]
    fn tapping_starts_at_the_cursor_note_and_redoes_a_section() {
        let mut a = app();
        run(&mut a, "sync");
        a.chant_tap(1.0, &[]);
        a.chant_tap(2.0, &[]);
        run(&mut a, "sync");
        // Back on `le` (the cursor followed the taps to `lu`: go to `le`).
        let id = a.focused();
        let p = a.panes.get_mut(&id).unwrap();
        p.cursor.bar = 0;
        p.cursor.beat = 2;
        run(&mut a, "sync");
        assert!(message(&a).contains("tap: next le"), "{}", message(&a));
        a.chant_tap(2.5, &[]);
        assert_eq!(points(&a), [(1, secs(1.0)), (3, secs(2.5))]);
        // In the middle of a syllable, its first note is next.
        run(&mut a, "sync");
        let p = a.panes.get_mut(&id).unwrap();
        p.cursor.bar = 0;
        p.cursor.beat = 1;
        run(&mut a, "sync");
        assert!(message(&a).contains("tap: next Al"), "{}", message(&a));
    }

    #[test]
    fn undo_takes_back_a_tap() {
        let mut a = app();
        assert!(a.pa.chant_tap.is_none());
        run(&mut a, "sync-undo");
        assert!(a.message.as_ref().unwrap().error);
        run(&mut a, "sync");
        a.chant_tap(1.0, &[]);
        a.chant_tap(2.0, &[]);
        run(&mut a, "sync-undo");
        assert_eq!(points(&a), [(1, secs(1.0))]);
        assert!(message(&a).contains("tap: next le"));
        assert_eq!(cursor_note(&a), Some(2));
        // The next tap sets `le` again; undoing a tap over an old point brings the old one back.
        a.chant_tap(2.5, &[]);
        run(&mut a, "sync-undo");
        run(&mut a, "sync-undo");
        assert!(points(&a).is_empty());
        assert!(a.buffer().score.audio.as_ref().unwrap().points.is_empty());
        a.buffers[0].score.set_chant_sync_point(1, secs(9.0));
        a.chant_tap(3.0, &[]);
        assert_eq!(points(&a), [(1, secs(3.0))]);
        run(&mut a, "sync-undo");
        assert_eq!(points(&a), [(1, secs(9.0))]);
    }

    #[test]
    fn here_and_nudge_set_and_move_a_point() {
        let mut a = app();
        let id = a.focused();
        a.panes.get_mut(&id).unwrap().cursor.beat = 2;
        a.chant_sync_here(2.0, &onsets());
        assert_eq!(points(&a), [(3, secs(2.0))]);
        run(&mut a, "sync-nudge 10");
        run(&mut a, "sync-nudge -30");
        assert_eq!(points(&a), [(3, secs(2.0) - 882)]);
        // A note without a point of its own nudges its syllable's.
        let p = a.panes.get_mut(&id).unwrap();
        p.cursor.beat = 0;
        run(&mut a, "sync-nudge 10");
        assert!(a.message.as_ref().unwrap().error);
        a.buffers[0].score.set_chant_sync_point(1, secs(1.0));
        a.panes.get_mut(&id).unwrap().cursor.beat = 1;
        run(&mut a, "sync-nudge 10");
        assert_eq!(points(&a)[0], (1, secs(1.0) + 441));
        assert_eq!(a.pa.sync_unit, SyncUnit::Syllable);
        assert!(Command::parse("sync-nudge x").is_err());
    }

    #[test]
    fn points_follow_their_notes_through_edits_and_go_with_a_deleted_note() {
        let mut a = app();
        a.buffers[0].score.set_chant_sync_point(1, secs(1.0));
        a.buffers[0].score.set_chant_sync_point(4, secs(4.0));
        a.buffers[0].score.refresh_chant_sync();
        // A note inserted at the start, and a division in the first bar.
        let id = a.focused();
        a.panes.get_mut(&id).unwrap().cursor.beat = 0;
        run(&mut a, "chant-syllable");
        run(&mut a, "chant-bar ,");
        assert_eq!(points(&a), [(1, secs(1.0)), (4, secs(4.0))]);
        let chant = a.buffer().score.tracks[0].chant.as_deref().unwrap();
        let k = chant.index_of_id(4).unwrap();
        assert_eq!(a.buffer().score.chant_note_frames().unwrap()[k], secs(4.0));
        // Deleting note 4 drops its point and says so.
        let (bar, beat) = chant.place_of(k).unwrap();
        a.chant_delete_range((bar, beat), (bar, beat));
        assert_eq!(points(&a), [(1, secs(1.0))]);
        assert!(
            message(&a).contains("1 sync point dropped"),
            "{}",
            message(&a)
        );
        // Undo brings the note and its point back.
        a.run(Command::Undo);
        assert_eq!(points(&a), [(1, secs(1.0)), (4, secs(4.0))]);
    }

    #[test]
    fn conversion_keeps_the_audio_where_it_is_both_ways() {
        let mut a = app();
        a.buffers[0].score.set_chant_sync_point(1, secs(1.0));
        a.buffers[0].score.set_chant_sync_point(4, secs(3.0));
        a.buffers[0].score.refresh_chant_sync();
        let bars: Vec<(usize, i64)> = a.buffers[0]
            .score
            .audio
            .as_ref()
            .unwrap()
            .points
            .iter()
            .map(|p| (p.bar, p.frame))
            .collect();
        assert_eq!(bars.len(), 2);
        assert_eq!(bars[0], (0, secs(1.0)));
        // chant → modern: the bar points are the modern score's.
        a.run(Command::ChantToModern);
        let modern = &a.buffers[1].score;
        assert!(!modern.is_chant());
        let m = modern.audio.as_ref().unwrap();
        assert!(m.chant_points.is_empty());
        let got: Vec<_> = m.points.iter().map(|p| (p.bar, p.frame)).collect();
        assert_eq!(got, bars);
        // modern → chant: each bar point is on the note that starts that bar.
        a.run(Command::ModernToChant);
        let back = &a.buffer().score;
        assert!(back.is_chant());
        let audio = back.audio.as_ref().unwrap();
        assert_eq!(
            audio
                .chant_points
                .iter()
                .map(|p| p.frame)
                .collect::<Vec<_>>(),
            [bars[0].1, bars[1].1]
        );
        let chant = back.tracks[0].chant.as_deref().unwrap();
        let first = |bar: usize| {
            let t = cecilia_core::chant::transcribe(chant);
            t.positions.iter().position(|p| p.0 == bar).unwrap()
        };
        let want: Vec<ChantSyncPoint> = audio.chant_points.clone();
        assert_eq!(chant.index_of_id(want[0].note), Some(0));
        assert!(first(0) == 0);
        // The frame of the first bar start is where it was.
        assert_eq!(audio.points[0].frame, bars[0].1);
        assert_eq!(
            back.chant_note_frames().unwrap()[chant.index_of_id(want[1].note).unwrap()],
            bars[1].1
        );
    }

    #[test]
    fn writing_gabc_warns_that_the_audio_is_not_saved() {
        let mut a = app();
        let dir = std::env::temp_dir().join(format!("cecilia-chantsync-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        run(&mut a, &format!("w {}", dir.join("s.gabc").display()));
        assert!(
            message(&a).contains("not saved in .gabc"),
            "{}",
            message(&a)
        );
        run(&mut a, &format!("w {}", dir.join("s.cc").display()));
        assert!(!message(&a).contains("not saved"), "{}", message(&a));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn the_unit_is_a_property_of_a_chant_score() {
        let mut a = app();
        a.commit_property(crate::properties::Field::SyncUnit, "note", false);
        assert_eq!(a.pa.sync_unit, SyncUnit::Note);
    }

    #[test]
    fn replacing_a_syllable_as_gabc_keeps_the_points_of_the_other_notes() {
        let mut a = app();
        a.buffers[0].score.set_chant_sync_point(1, secs(1.0));
        a.buffers[0].score.set_chant_sync_point(4, secs(4.0));
        a.buffers[0].score.refresh_chant_sync();
        // `le` is note 3: replace it by a syllable of two notes; the others stay.
        let id = a.focused();
        a.panes.get_mut(&id).unwrap().cursor.beat = 2;
        run(&mut a, "gabc le(hi)");
        assert_eq!(points(&a), [(1, secs(1.0)), (4, secs(4.0))]);
        let chant = a.buffer().score.tracks[0].chant.as_deref().unwrap();
        assert_eq!(chant.index_of_id(4), Some(4));
    }
}
