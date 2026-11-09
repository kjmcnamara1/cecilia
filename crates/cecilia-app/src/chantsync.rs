//! Audio sync of a chant score by hand: tapping along with the singing, setting the cursor
//! note to the playhead, nudging a point. The sync lives on notes (see
//! [`cecilia_core::chant_sync`]); the player's bar points are derived from it after every edit.

use cecilia_audio::SYNC_RATE;
use cecilia_audio::pitch::Segment;
use cecilia_core::chant::{Cell, CellKind, Chant, transcribe};

use crate::App;

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

/// The notes a tap can sync (those of syllables that have notes): `(index in the transcription's cells,
/// id)`, in order. A syllable unit taps the first note of each syllable.
pub fn tap_targets(chant: &Chant, unit: SyncUnit) -> Vec<(usize, u32)> {
    let cells = transcribe(chant).cells;
    let mut out = Vec::new();
    let mut last = None;
    for (i, c) in cells.iter().enumerate() {
        // Rests and divisions (syllables without notes) are not tapped.
        let Some(id) = c.note.and_then(|_| chant.cell_id(c)) else {
            continue;
        };
        if unit == SyncUnit::Note || last != Some(c.syllable) {
            out.push((i, id));
        }
        last = Some(c.syllable);
    }
    out
}

/// How the syllable of a cell reads in "tap: next …".
fn label_of(chant: &Chant, cell: &Cell) -> String {
    let text = chant
        .syllables
        .get(cell.syllable)
        .map(|s| s.plain_text())
        .unwrap_or_default();
    let text = text.trim();
    if !text.is_empty() {
        return text.to_string();
    }
    match cell.kind {
        CellKind::Note => format!("note {}", cell.note.map_or(0, |n| n + 1)),
        CellKind::Slot => "rest".to_string(),
        CellKind::Division => "division".to_string(),
    }
}

/// The cell with an id, and its index.
fn cell_by_id(chant: &Chant, id: u32) -> Option<(usize, Cell)> {
    let t = transcribe(chant);
    t.cells
        .iter()
        .position(|c| chant.cell_id(c) == Some(id))
        .map(|i| (i, t.cells[i]))
}

impl App {
    /// A time (seconds) moved to the strongest onset of the audio within `ui.sync_snap`
    /// milliseconds of it; as it is when that is 0 (off).
    fn snapped(&self, secs: f64, onsets: &[f32]) -> f64 {
        let window = self.config.file.ui.sync_snap / 1000.0;
        if window > 0.0 {
            cecilia_audio::beats::snap(onsets, secs, window)
        } else {
            secs
        }
    }

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

    /// Where the cursor's cell is in the audio (seconds, without the padding), as the sync has
    /// it: the time of its tick on the waveform, a division's own (not that of the note after
    /// it). None off a chant or without audio.
    pub fn chant_cursor_audio_time(&self) -> Option<f64> {
        let c = self.pane().cursor;
        let score = &self.buffer().score;
        let chant = score.tracks.get(c.track)?.chant.as_deref()?;
        let i = transcribe(chant).cell_near(c.bar, c.beat)?;
        let pad = score.audio.as_ref()?.frame_padding as f64 / SYNC_RATE;
        let frame = *score.chant_cell_frames()?.get(i)?;
        Some(frame as f64 / SYNC_RATE - pad)
    }

    /// The cell under the cursor (else the nearest before it) with its index and id.
    pub(crate) fn chant_cursor_cell_id(&self) -> Option<(usize, Cell, u32)> {
        let c = self.pane().cursor;
        let chant = self.buffer().score.tracks.get(c.track)?.chant.as_deref()?;
        let t = transcribe(chant);
        let i = t.cell_near(c.bar, c.beat)?;
        Some((i, t.cells[i], chant.cell_id(&t.cells[i])?))
    }

    /// Starts tapping at the cursor cell (the syllable it is in). False (with a message) when
    /// the score has no audio or nothing to sync.
    pub(crate) fn chant_tap_start(&mut self) -> bool {
        if !self.chant_sync_ready() {
            return false;
        }
        let Some((at, ..)) = self.chant_cursor_cell_id() else {
            self.error("sync: the chant has nothing to sync");
            return false;
        };
        let track = self.pane().cursor.track;
        let chant = self.buffer().score.tracks[track].chant.as_deref().unwrap();
        let targets = tap_targets(chant, self.pa.sync_unit);
        let Some(&(k, id)) = targets
            .iter()
            .rev()
            .find(|&&(k, _)| k <= at)
            .or(targets.first())
        else {
            self.error("sync: the chant has nothing to sync");
            return false;
        };
        let cells = transcribe(chant).cells;
        let tap = ChantTap {
            next: Some(id),
            trail: Vec::new(),
            label: label_of(chant, &cells[k]),
        };
        let msg = format!("tap: next {} (:sync-tap to tap, :sync to stop)", tap.label);
        self.pa.chant_tap = Some(tap);
        self.info(msg);
        true
    }

    /// Moves the cursor to a cell, as an edit does.
    fn goto_cell(cx: &mut crate::edit::EditCx, chant: &Chant, cell: &Cell) {
        cx.cursor.bar = cell.bar;
        cx.cursor.beat = cell.beat;
        if let Some(n) = cell.note.and_then(|n| chant.note(n)) {
            cx.cursor.line = n.pitch as i32;
        }
    }

    /// A tap at `secs` (audio time at the playhead): syncs the next cell there, snapped to the
    /// strongest of the `onsets` close by, and moves on.
    pub fn chant_tap(&mut self, secs: f64, onsets: &[f32]) {
        let Some(tap) = self.pa.chant_tap.clone() else {
            return;
        };
        // In Sync mode the cell under the cursor is synced (the cursor moves on by itself);
        // otherwise the next one of the tapping.
        let at_cursor = self
            .syncing
            .then(|| self.chant_cursor_cell_id())
            .flatten()
            .and_then(|(i, cell, id)| {
                // On a rest or a division: the next note.
                if cell.note.is_some() {
                    return Some(id);
                }
                let track = self.pane().cursor.track;
                let chant = self.buffer().score.tracks[track].chant.as_deref()?;
                tap_targets(chant, self.pa.sync_unit)
                    .into_iter()
                    .find(|&(k, _)| k > i)
                    .map(|(_, id)| id)
            });
        let Some(id) = at_cursor.or(tap.next) else {
            return self.info("tap: end of the chant (:sync-undo to go back, :sync to stop)");
        };
        let frame = (self.snapped(secs, onsets) * SYNC_RATE).round() as i64;
        let unit = self.pa.sync_unit;
        let done = self.edit(|cx| {
            let t = cx.cursor.track;
            let score = cx.whole();
            let chant = score.tracks[t]
                .chant
                .as_deref()
                .ok_or("not a chant score")?;
            let (idx, _) = cell_by_id(chant, id).ok_or("tap: the cell being synced is gone")?;
            let targets = tap_targets(chant, unit);
            let next = targets.iter().find(|&&(k, _)| k > idx).copied();
            let cells = transcribe(chant).cells;
            let label = next.map(|(k, _)| label_of(chant, &cells[k]));
            let land = cells[next.map_or(idx, |(k, _)| k)];
            let chant = chant.clone();
            let old = score.set_chant_sync_point(id, frame);
            score.refresh_chant_sync();
            Self::goto_cell(cx, &chant, &land);
            Ok((old, next.map(|(_, id)| id), label))
        });
        let Some((old, next_id, label)) = done else {
            return;
        };
        if let Some(t) = self.pa.chant_tap.as_mut() {
            t.trail.push((id, old));
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
    /// cell again.
    pub fn chant_sync_undo(&mut self) {
        let Some(tap) = self.pa.chant_tap.clone() else {
            return self.error("sync-undo: not tapping (start with :sync)");
        };
        let Some(&(id, old)) = tap.trail.last() else {
            return self.info("sync-undo: nothing tapped yet");
        };
        let done = self.edit(|cx| {
            let t = cx.cursor.track;
            let score = cx.whole();
            match old {
                Some(f) => {
                    score.set_chant_sync_point(id, f);
                }
                None => {
                    score.remove_chant_sync_point(id);
                }
            }
            score.refresh_chant_sync();
            let chant = score.tracks[t]
                .chant
                .as_deref()
                .ok_or("not a chant score")?;
            let (_, cell) = cell_by_id(chant, id).ok_or("the cell is gone")?;
            let label = label_of(chant, &cell);
            let chant = chant.clone();
            Self::goto_cell(cx, &chant, &cell);
            Ok(label)
        });
        let Some(label) = done else {
            return;
        };
        if let Some(t) = self.pa.chant_tap.as_mut() {
            t.trail.pop();
            t.next = Some(id);
            t.label = label.clone();
        }
        self.info(format!("tap: next {label}"));
    }

    /// Sets the cursor cell (a note, a rest, a division) to `secs` (audio time at the
    /// playhead), snapped to the strongest of the `onsets` close by.
    pub fn chant_sync_here(&mut self, secs: f64, onsets: &[f32]) {
        if !self.chant_sync_ready() {
            return;
        }
        let Some((_, _, id)) = self.chant_cursor_cell_id() else {
            return self.error("sync-here: the chant has nothing to sync");
        };
        let frame = (self.snapped(secs, onsets) * SYNC_RATE).round() as i64;
        let done = self.edit(|cx| {
            let score = cx.whole();
            score.set_chant_sync_point(id, frame);
            score.refresh_chant_sync();
            Ok(())
        });
        if done.is_some() {
            self.info(format!("sync: at {:.2} s", frame as f64 / SYNC_RATE));
        }
    }

    /// The sync points in a range of the score (`from` to `to`, as `(bar, beat)` of the cursor):
    /// a chant's by cell id (only cells that have one), a modern score's by bar.
    fn sync_points_in(&self, from: (usize, usize), to: (usize, usize)) -> Vec<u32> {
        let score = &self.buffer().score;
        let Some(audio) = &score.audio else {
            return Vec::new();
        };
        let Some(chant) = score
            .tracks
            .get(self.pane().cursor.track)
            .and_then(|t| t.chant.as_deref())
        else {
            return (from.0..=to.0.min(score.master_bars.len().saturating_sub(1)))
                .map(|b| b as u32)
                .collect();
        };
        transcribe(chant)
            .cells
            .iter()
            .filter(|c| (c.bar, c.beat) >= from && (c.bar, c.beat) <= to)
            .filter_map(|c| chant.cell_id(c))
            .filter(|id| audio.chant_points.iter().any(|p| p.note == *id))
            .collect()
    }

    /// `d` and `c` while syncing: they take the sync points of the range away, nothing else.
    pub(crate) fn sync_operate(
        &mut self,
        op: crate::command::Operator,
        from: (usize, usize),
        to: (usize, usize),
    ) {
        if op == crate::command::Operator::Yank {
            return self.info("yank: sync points are not yanked");
        }
        let chant = self.buffer().score.is_chant();
        let ids = self.sync_points_in(from, to);
        let n = self.edit(|cx| {
            let Some(audio) = cx.whole().audio.as_mut() else {
                return Err("no audio attached".to_string());
            };
            let before = audio.chant_points.len() + audio.points.len();
            if chant {
                audio.chant_points.retain(|p| !ids.contains(&p.note));
            } else {
                audio.points.retain(|p| !ids.contains(&(p.bar as u32)));
            }
            let n = before - audio.chant_points.len() - audio.points.len();
            cx.whole().refresh_chant_sync();
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
    fn chant_sync_nudge_range(&mut self, from: (usize, usize), to: (usize, usize), ms: i32) {
        let ids = self.sync_points_in(from, to);
        if ids.is_empty() {
            return self.info("sync-nudge: no sync points in the selection");
        }
        let d = (ms as f64 * SYNC_RATE / 1000.0).round() as i64;
        let n = ids.len();
        self.edit(|cx| {
            let score = cx.whole();
            if let Some(audio) = score.audio.as_mut() {
                for p in audio
                    .chant_points
                    .iter_mut()
                    .filter(|p| ids.contains(&p.note))
                {
                    p.frame = (p.frame + d).max(0);
                }
            }
            score.refresh_chant_sync();
            Ok(())
        });
        self.info(format!(
            "{n} sync point{} moved ({ms:+} ms)",
            if n == 1 { "" } else { "s" }
        ));
    }

    /// `:sync-detect` for chant: lays the chant's notes along the notes `sung` in the recording
    /// (by pitch, in order) and puts a sync point at each of the syllables (or, with the note
    /// unit, the notes) that found theirs. `pad` is the audio's padding in seconds.
    pub fn chant_sync_detected(&mut self, sung: &[Segment], pad: f64) {
        if !self.chant_sync_ready() {
            return;
        }
        let track = self.pane().cursor.track;
        let Some(chant) = self.buffer().score.tracks[track].chant.as_deref() else {
            return;
        };
        let notes = chant.resolved();
        let ids = chant.note_ids();
        // Notes of one pitch in a syllable are one sung note; the point goes on its first.
        let mut runs: Vec<usize> = Vec::new();
        for (i, n) in notes.iter().enumerate() {
            let same = i > 0 && notes[i - 1].syllable == n.syllable && notes[i - 1].midi == n.midi;
            if !same {
                runs.push(i);
            }
        }
        let melody: Vec<f32> = runs.iter().map(|&i| notes[i].midi as f32).collect();
        let found = cecilia_audio::pitch::align(&melody, sung);
        let first_of_syllable = |i: usize| i == 0 || notes[i - 1].syllable != notes[i].syllable;
        let by_syllable = self.pa.sync_unit == SyncUnit::Syllable;
        let points: Vec<(u32, i64)> = runs
            .iter()
            .zip(&found)
            .filter_map(|(&i, f)| Some((i, f.as_ref()?)))
            .filter(|&(i, _)| !by_syllable || first_of_syllable(i))
            .map(|(i, &j)| {
                let secs = sung[j].start + pad;
                (ids[i], (secs * SYNC_RATE).round() as i64)
            })
            .collect();
        let wanted = if by_syllable {
            notes
                .iter()
                .enumerate()
                .filter(|&(i, _)| first_of_syllable(i))
                .count()
        } else {
            runs.len()
        };
        if points.is_empty() {
            return self.error("sync-detect: found none of the chant's notes in the singing");
        }
        let n = points.len();
        self.edit(|cx| {
            let score = cx.whole();
            for (id, frame) in points {
                score.set_chant_sync_point(id, frame);
            }
            score.refresh_chant_sync();
            Ok(())
        });
        self.info(format!(
            "sync-detect: {n} of {wanted} {}s placed by pitch (fix with :sync mode)",
            if by_syllable { "syllable" } else { "note" }
        ));
    }

    /// Sets a sync point at `secs` (audio time with the padding) on the note nearest to that
    /// time as the sync has it now, among those the sync unit taps (a syllable's first note, or
    /// every note), snapped to the strongest of the `onsets` nearby; the cursor goes there.
    /// Returns the id of the note.
    pub(crate) fn chant_sync_closest(&mut self, secs: f64, onsets: &[f32]) -> Option<u32> {
        if !self.chant_sync_ready() {
            return None;
        }
        let track = self.pane().cursor.track;
        let score = &self.buffer().score;
        let chant = score.tracks.get(track)?.chant.as_deref()?;
        let frames = score.chant_cell_frames()?;
        let cells = transcribe(chant).cells;
        // The notes the sync unit taps, and the divisions (which Enter passes over, but a
        // click can sync).
        let mut candidates = tap_targets(chant, self.pa.sync_unit);
        candidates.extend(
            cells
                .iter()
                .enumerate()
                .filter(|(_, c)| c.kind == CellKind::Division)
                .filter_map(|(k, c)| Some((k, chant.cell_id(c)?))),
        );
        let target = candidates
            .into_iter()
            .map(|(k, id)| {
                let at = frames.get(k).copied().unwrap_or(0) as f64 / SYNC_RATE;
                ((at - secs).abs(), k, id)
            })
            .min_by(|a, b| a.0.total_cmp(&b.0))?;
        let (_, k, id) = target;
        let cell = cells[k];
        let frame = (self.snapped(secs, onsets) * SYNC_RATE).round() as i64;
        let chant = chant.clone();
        self.edit(|cx| {
            let score = cx.whole();
            score.set_chant_sync_point(id, frame);
            score.refresh_chant_sync();
            Self::goto_cell(cx, &chant, &cell);
            Ok(())
        })?;
        Some(id)
    }

    /// Moves the sync point of the cursor cell by `ms` milliseconds. A cell with no point
    /// gets one first, where the sync puts it now (a rest or division: where the next note
    /// is).
    pub fn chant_sync_nudge(&mut self, ms: i32) {
        if !self.chant_sync_ready() {
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
            return self.chant_sync_nudge_range(from, to, ms);
        }
        let Some((_, cell, id)) = self.chant_cursor_cell_id() else {
            return self.error("sync-nudge: the chant has nothing to sync");
        };
        let done = self.edit(|cx| {
            let t = cx.cursor.track;
            let score = cx.whole();
            let existing = score
                .audio
                .as_ref()
                .ok_or("no audio attached")?
                .chant_points
                .iter()
                .find(|p| p.note == id)
                .map(|p| p.frame);
            let (base, added) = match existing {
                Some(f) => (f, false),
                None => {
                    let chant = score.tracks[t]
                        .chant
                        .as_deref()
                        .ok_or("not a chant score")?;
                    let k = transcribe(chant)
                        .cells
                        .iter()
                        .position(|c| (c.syllable, c.item) == (cell.syllable, cell.item))
                        .unwrap_or(0);
                    let frames = score.chant_cell_frames().unwrap_or_default();
                    (frames.get(k).or(frames.last()).copied().unwrap_or(0), true)
                }
            };
            let frame = (base + (ms as f64 * SYNC_RATE / 1000.0).round() as i64).max(0);
            score.set_chant_sync_point(id, frame);
            score.refresh_chant_sync();
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
        // The division `(;)` is passed over.
        assert!(message(&a).contains("tap: next lu"), "{}", message(&a));
        a.chant_tap(2.7, &on); // no onset near: as tapped
        a.chant_tap(3.55, &on);
        assert_eq!(
            points(&a),
            [
                (1, secs(1.03)),
                (3, secs(2.0)),
                (4, secs(2.7)),
                (5, secs(3.5)),
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
    fn sync_is_a_mode_with_its_own_keys_that_opens_the_waveform() {
        use crate::config::Mode;
        let mut a = app();
        assert!(!a.has_tool(crate::app::PaneKind::Audio));
        run(&mut a, "sync");
        assert_eq!(a.mode, Mode::Sync);
        assert!(a.has_tool(crate::app::PaneKind::Audio));
        assert!(a.pa.chant_tap.is_some());
        // Motions work, edits do not.
        a.handle_key(crate::keys::Key::char('l'));
        assert_eq!(a.mode, Mode::Sync);
        let before = a.buffer().score.clone();
        a.handle_key(crate::keys::Key::char('x'));
        assert_eq!(a.buffer().score, before);
        // Enter syncs the cell under the cursor and the cursor moves to the next unit.
        let at = a.chant_cursor_note();
        a.chant_tap(1.0, &[]);
        assert_eq!(points(&a).len(), 1);
        assert_ne!(a.chant_cursor_note(), at);
        // A command line returns to Sync mode.
        a.handle_key(crate::keys::Key::char(':'));
        assert_eq!(a.mode, Mode::Command);
        a.handle_key(crate::keys::Key::new(crate::keys::KeyCode::Esc));
        assert_eq!(a.mode, Mode::Sync);
        a.handle_key(crate::keys::Key::new(crate::keys::KeyCode::Esc));
        assert_eq!(a.mode, Mode::Normal);
        assert!(a.pa.chant_tap.is_none());
        assert!(a.has_tool(crate::app::PaneKind::Audio), "the pane stays");
        // `:sync` again while in the mode leaves it.
        run(&mut a, "sync");
        run(&mut a, "sync");
        assert_eq!(a.mode, Mode::Normal);
    }

    #[test]
    fn detecting_by_pitch_places_the_syllables_along_the_singing() {
        let mut a = app();
        let notes = a.buffer().score.tracks[0]
            .chant
            .as_deref()
            .unwrap()
            .resolved();
        // Sung a tone up, a second apart, after a first sound that is not in the chant.
        let mut sung = vec![Segment {
            start: 0.2,
            end: 0.6,
            midi: 40.5,
        }];
        for (k, n) in notes.iter().enumerate() {
            sung.push(Segment {
                start: 1.0 + k as f64,
                end: 1.8 + k as f64,
                midi: n.midi as f32 + 2.0,
            });
        }
        a.chant_sync_detected(&sung, 0.0);
        // The first note of each syllable (notes 1, 3, 4, 5).
        assert_eq!(
            points(&a),
            [
                (1, secs(1.0)),
                (3, secs(3.0)),
                (4, secs(4.0)),
                (5, secs(5.0))
            ]
        );
        assert!(message(&a).contains("4 of 4"), "{}", message(&a));
        run(&mut a, "sync-unit note");
        a.chant_sync_detected(&sung, 0.5);
        assert_eq!(points(&a).len(), 5);
        assert_eq!(points(&a)[1], (2, secs(2.5)));
    }

    fn press(a: &mut App, keys: &str) {
        for c in keys.chars() {
            a.handle_key(crate::keys::Key::char(c));
        }
    }

    #[test]
    fn d_and_c_in_sync_mode_and_visual_mode_take_sync_points_only() {
        use crate::config::Mode;
        let mut a = app();
        for (id, t) in [(1, 1.0), (2, 1.5), (3, 2.0), (4, 3.0), (5, 4.0)] {
            a.buffers[0].score.set_chant_sync_point(id, secs(t));
        }
        a.buffers[0].score.refresh_chant_sync();
        let notes = a.buffer().score.tracks[0].chant.clone();
        run(&mut a, "sync");
        // `dl`: the point of the cursor's cell.
        press(&mut a, "dl");
        assert_eq!(
            points(&a).iter().map(|p| p.0).collect::<Vec<_>>(),
            [2, 3, 4, 5]
        );
        assert!(
            message(&a).contains("1 sync point removed"),
            "{}",
            message(&a)
        );
        assert_eq!(a.buffer().score.tracks[0].chant, notes, "the notes stay");
        assert_eq!(a.mode, Mode::Sync);
        // Visual: nudge and delete the points of the selection (notes 1-3 at beats 0-2).
        press(&mut a, "vll");
        assert_eq!(a.mode, Mode::Visual);
        press(&mut a, "]");
        assert!(
            message(&a).contains("2 sync points moved"),
            "{}",
            message(&a)
        );
        assert!(
            points(&a).contains(&(2, secs(1.5) + 441)),
            "{:?}",
            points(&a)
        );
        assert!(points(&a).contains(&(3, secs(2.0) + 441)));
        assert!(
            points(&a).contains(&(4, secs(3.0))),
            "outside the selection"
        );
        press(&mut a, "d");
        assert_eq!(a.mode, Mode::Sync, "back to Sync mode");
        assert_eq!(points(&a).iter().map(|p| p.0).collect::<Vec<_>>(), [4, 5]);
        // `c` clears too, and does not start Insert mode; `dd` takes the bar's.
        press(&mut a, "cl");
        assert_eq!(a.mode, Mode::Sync);
        press(&mut a, "dd");
        assert_eq!(a.buffer().score.tracks[0].chant, notes);
        run(&mut a, "sync");
        assert_eq!(a.mode, Mode::Normal);
    }

    #[test]
    fn play_starts_at_the_cursor_in_sync_mode_even_when_paused_there() {
        use crate::app::Effect;
        let mut a = app();
        run(&mut a, "sync");
        a.take_effects();
        a.paused = true;
        a.paused_at = Some((a.focused(), a.pane().buffer, a.pane().cursor));
        run(&mut a, "play-toggle");
        let effects = a.take_effects();
        assert!(
            effects.iter().any(|e| matches!(e, Effect::Play { .. })),
            "{effects:?}"
        );
        assert!(!effects.contains(&Effect::Resume));
    }

    #[test]
    fn x_in_sync_mode_removes_the_sync_point_at_the_cursor_and_the_status_line_says_where() {
        let mut a = app();
        for (id, t) in [(1, 1.0), (3, 2.0)] {
            a.buffers[0].score.set_chant_sync_point(id, secs(t));
        }
        a.buffers[0].score.refresh_chant_sync();
        run(&mut a, "sync");
        press(&mut a, "x");
        assert_eq!(points(&a), [(3, secs(2.0))]);
        // The status line and `:goto` read the same places: Al, le, the division, lu.
        let place = |a: &App| crate::app::position_text(&a.buffer().score, &a.pane().cursor);
        assert_eq!(place(&a), "1:1.1");
        press(&mut a, "ll");
        assert_eq!(place(&a), "1:2.1");
        press(&mut a, "l");
        assert_eq!(place(&a), "2:0");
        run(&mut a, "2:2");
        assert_eq!(place(&a), "2:2.1");
        run(&mut a, "1:1.2");
        assert_eq!(place(&a), "1:1.2");
        run(&mut a, "2:0");
        assert_eq!(place(&a), "2:0");
    }

    #[test]
    fn the_midi_notes_sound_where_the_ruler_puts_them() {
        use cecilia_audio::{MidiEvent, Sequence, SequenceOptions};
        let mut a = app();
        // Points on the first and fourth notes only, the second bar after a division.
        a.buffers[0].score.set_chant_sync_point(1, secs(1.0));
        a.buffers[0].score.set_chant_sync_point(4, secs(5.0));
        a.buffers[0].score.refresh_chant_sync();
        let score = &a.buffer().score;
        let want: Vec<f64> = score
            .chant_note_frames()
            .unwrap()
            .iter()
            .map(|&f| f as f64 / SYNC_RATE)
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
        assert_eq!(got.len(), want.len());
        for (g, w) in got.iter().zip(&want) {
            assert!((g - w).abs() < 1e-3, "{got:?} {want:?}");
        }
    }

    #[test]
    fn the_cursor_on_a_division_is_at_the_divisions_own_time() {
        let mut a = app();
        let id = a.focused();
        let at = |a: &App| a.chant_cursor_audio_time().unwrap();
        // `h` (the last note of bar 1), the division `(;)`, then `lu`.
        a.panes.get_mut(&id).unwrap().cursor.beat = 2;
        let before = at(&a);
        a.panes.get_mut(&id).unwrap().cursor.beat = 3;
        let division = at(&a);
        a.panes.get_mut(&id).unwrap().cursor.bar = 1;
        a.panes.get_mut(&id).unwrap().cursor.beat = 0;
        let after = at(&a);
        assert!(
            before < division && division < after,
            "{before} {division} {after}"
        );
    }

    #[test]
    fn in_sync_mode_the_cursor_follows_the_playhead_when_following_but_never_seeks() {
        use crate::app::Effect;
        let mut a = app();
        run(&mut a, "sync");
        a.playing = true;
        a.playing_buffer = Some(0);
        a.take_effects();
        // Following: the playhead past the division takes the cursor to the next bar.
        a.follow_playback(0, 1, 0);
        assert_eq!(a.pane().cursor.bar, 1);
        // Moving the cursor does not seek the playback.
        press(&mut a, "h");
        assert!(
            !a.take_effects()
                .iter()
                .any(|e| matches!(e, Effect::Seek { .. }))
        );
        // With following off the cursor stays where it is.
        run(&mut a, "follow-toggle");
        let at = a.pane().cursor;
        a.follow_playback(0, 0, 0);
        assert_eq!(a.pane().cursor, at);
    }

    #[test]
    fn enter_on_a_division_syncs_the_next_note() {
        let mut a = app();
        run(&mut a, "sync");
        let id = a.focused();
        // The division `(;)` is the fourth cell.
        a.panes.get_mut(&id).unwrap().cursor.beat = 3;
        a.chant_tap(2.0, &[]);
        assert_eq!(points(&a), [(4, secs(2.0))]);
    }

    #[test]
    fn the_mouse_adds_grabs_and_moves_sync_points_on_the_waveform() {
        let mut a = app();
        assert!(!a.wave_sync_editing());
        run(&mut a, "sync");
        assert!(a.wave_sync_editing());
        // A press where there is no point adds one on the syllable (or division) nearest the
        // mouse, not the one the cursor is on, and drags it. Nothing is synced: the cells lie
        // at the tempo from 0 s. The division `(;)` (id 6) is the fourth cell.
        let frames = a.buffer().score.chant_cell_frames().unwrap();
        let t = frames[3] as f64 / SYNC_RATE;
        a.wave_mouse_down(0, t, 0.001);
        assert_eq!(a.wave_pressed(&[]), Some((0, t)));
        assert_eq!(points(&a), [(6, frames[3])]);
        let id = 6;
        a.wave_mouse_move(2.5);
        assert_eq!(points(&a), [(id, secs(2.5))]);
        a.wave_mouse_up();
        assert_eq!(points(&a).len(), 1);
        // Pressing near it grabs it instead of adding one.
        a.wave_mouse_down(0, 2.55, 0.1);
        a.wave_pressed(&[]);
        a.wave_mouse_move(3.0);
        a.wave_mouse_up();
        assert_eq!(points(&a), [(id, secs(3.0))]);
        // The press, the drag and the release are one undo step each press.
        run(&mut a, "undo");
        assert_eq!(points(&a), [(id, secs(2.5))]);
    }

    #[test]
    fn zoom_commands_go_to_the_waveform_pane_when_it_has_the_focus() {
        use crate::app::PaneKind;
        let mut a = app();
        a.set_tool(PaneKind::Audio, Some(true));
        let id = a.tool_pane(PaneKind::Audio).unwrap();
        let score_zoom = a.zoom();
        a.set_focus(id);
        run(&mut a, "zoom-in");
        assert_eq!(a.take_wave_zoom(id), Some(1.25));
        run(&mut a, "zoom-reset");
        assert_eq!(a.take_wave_zoom(id), Some(0.0));
        assert_eq!(a.take_wave_zoom(id), None);
        assert_eq!(a.zoom(), score_zoom, "the score's zoom is untouched");
    }

    #[test]
    fn a_note_unit_taps_every_note_and_skips_rests_and_divisions() {
        let mut a = app();
        run(&mut a, "sync-unit note");
        run(&mut a, "sync");
        // Five notes and two divisions, which Enter passes over.
        for i in 0..5 {
            a.chant_tap(1.0 + i as f64, &[]);
        }
        assert_eq!(
            points(&a).iter().map(|p| p.0).collect::<Vec<_>>(),
            [1, 2, 3, 4, 5]
        );
        assert!(Command::parse("sync-unit word").is_err());
    }

    /// The places of every cell of a chant, as labels.
    fn labels(gabc: &str) -> Vec<String> {
        let chant = cecilia_formats::gabc::parse(gabc).unwrap();
        let places = chant.places();
        transcribe(&chant)
            .cells
            .iter()
            .map(|c| places.of_cell(c).unwrap().label())
            .collect()
    }

    #[test]
    fn places_are_bar_syllable_note_with_a_bar_after_each_division() {
        // The start of Salve Regina: a division is the first of its bar (`2:0`, not `1:7.0`),
        // and a syllable's notes are .1, .2.
        let got = labels(
            "name: t;\n%%\n(c4) Sal(c)ve(e) Re(g) gí(h) na,(g.) <c>*</c>(;) ma(h)ter(j) mi(i)se(h)ri(g)cór(h)di(g)æ,(g.) (;) Vi(j)ta,(g) dul(h)cé(ff)do,(d.) (;)\n",
        );
        let at = |i: usize| got[i].as_str();
        assert_eq!(&got[..5], ["1:1.1", "1:2.1", "1:3.1", "1:4.1", "1:5.1"]);
        assert_eq!(at(5), "2:0");
        assert_eq!(at(6), "2:1.1");
        assert_eq!(at(13), "2:8.1", "{got:?}");
        assert_eq!(at(14), "3:0");
        assert_eq!(
            &got[15..],
            ["3:1.1", "3:2.1", "3:3.1", "3:4.1", "3:4.2", "3:5.1", "4:0"]
        );
        // References read back, a missing note being the first, a division 0.
        use cecilia_core::chant::Place;
        let p = |s| Place::parse(s).unwrap();
        assert_eq!(p("3:4"), p("3:4.1"));
        assert_eq!(p("3:4.2").label(), "3:4.2");
        assert_eq!(p("2:0").label(), "2:0");
        assert!(Place::parse("0:1").is_none() && Place::parse("x").is_none());
    }

    #[test]
    fn the_ruler_marks_bars_syllables_and_extra_notes() {
        let mut a = app();
        a.buffers[0].score.set_chant_sync_point(1, secs(1.0));
        a.buffers[0].score.set_chant_sync_point(5, secs(5.0));
        a.buffers[0].score.refresh_chant_sync();
        let marks = a.buffer().score.chant_marks().unwrap();
        let labels: Vec<String> = marks.iter().map(|m| m.label()).collect();
        // Al(fg) le(h) | lu(g) ia.(f.)
        // The divisions are cells of their own, with a tick like any syllable.
        assert_eq!(
            labels,
            ["1:1.1", "1:1.2", "1:2.1", "2:0", "2:1.1", "2:2.1", "3:0"]
        );
        assert_eq!(
            marks
                .iter()
                .map(|m| (m.is_bar(), m.is_syllable()))
                .collect::<Vec<_>>(),
            [
                (false, true),
                (false, false),
                (false, true),
                (true, false),
                (false, true),
                (false, true),
                (true, false)
            ]
        );
        assert_eq!(marks[0].frame, secs(1.0));
        assert_eq!(marks[5].frame, secs(5.0));
        // The division takes time: it is between the notes either side of it.
        assert!(marks[2].frame < marks[3].frame && marks[3].frame < marks[4].frame);
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
        let p = a.panes.get_mut(&id).unwrap();
        p.cursor.beat = 0;
        let at = a.buffer().score.chant_note_frames().unwrap()[0];
        run(&mut a, "sync-nudge 10");
        // A note with no point gets one where it sounds, nudged.
        assert!(message(&a).contains("added"), "{}", message(&a));
        assert!(points(&a).contains(&(1, at + 441)), "{:?}", points(&a));
        // Every note has a point of its own: the cursor's, not its syllable's.
        a.buffers[0].score.set_chant_sync_point(1, secs(1.0));
        a.panes.get_mut(&id).unwrap().cursor.beat = 1;
        let at = a.buffer().score.chant_note_frames().unwrap()[1];
        run(&mut a, "sync-nudge 10");
        assert_eq!(points(&a)[0], (1, secs(1.0)));
        assert!(points(&a).contains(&(2, at + 441)), "{:?}", points(&a));
        // A division of its own takes a point too.
        a.panes.get_mut(&id).unwrap().cursor.beat = 3;
        run(&mut a, "sync-nudge 10");
        assert!(points(&a).iter().any(|p| p.0 == 6), "{:?}", points(&a));
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
        assert!(message(&a).contains("1 sync point lost"), "{}", message(&a));
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

    #[test]
    fn a_point_on_a_division_stays_with_it_through_a_text_edit() {
        let mut a = app();
        a.buffers[0].score.set_chant_sync_point(6, secs(2.5));
        a.buffers[0].score.refresh_chant_sync();
        let id = a.focused();
        a.panes.get_mut(&id).unwrap().cursor.beat = 2;
        run(&mut a, "gabc le(hi)");
        assert_eq!(points(&a), [(6, secs(2.5))]);
    }
}
