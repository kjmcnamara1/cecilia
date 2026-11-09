//! Sync mode on the waveform: pressing the mouse on it adds a sync point of a chant (on the
//! syllable or note nearest to it, snapped to the amplitude) or grabs the one close by, and the audio plays from
//! there while the button is held; dragging moves the point. The frontend gives the audio time
//! under the mouse.

use cecilia_audio::SYNC_RATE;

use crate::app::{App, Effect};

impl App {
    /// Whether the waveform edits sync points with the mouse: Sync mode on a chant with audio.
    pub fn wave_sync_editing(&self) -> bool {
        self.syncing && self.buffer().score.is_chant() && self.buffer().score.audio.is_some()
    }

    /// The button went down at `secs` of the audio; `tol` is how close (seconds) a point is
    /// grabbed from. The edits of the drag that follows are one undo step.
    pub fn wave_mouse_down(&mut self, buffer: usize, secs: f64, tol: f64) {
        self.group_edits(true);
        self.wave_press = Some((buffer, secs, tol));
        self.effects.push(Effect::WavePress);
    }

    /// Carries out the press once the audio is at hand (`onsets`: its onset strength, 100 a
    /// second): the point to grab, or a new one. Returns where the audio should play from.
    pub(crate) fn wave_pressed(&mut self, onsets: &[f32]) -> Option<(usize, f64)> {
        let (buffer, secs, tol) = self.wave_press.take()?;
        let audio = self.buffers.get(buffer)?.score.audio.as_ref()?;
        let pad = audio.frame_padding as f64 / SYNC_RATE;
        let near = audio
            .chant_points
            .iter()
            .map(|p| (p.note, (p.frame as f64 / SYNC_RATE - pad - secs).abs()))
            .filter(|&(_, d)| d <= tol)
            .min_by(|a, b| a.1.total_cmp(&b.1));
        self.wave_grab = match near {
            Some((id, _)) => Some(id),
            // Else a new one, on the syllable (or note) nearest to where the mouse is.
            None => self.chant_sync_closest(secs + pad, onsets),
        };
        Some((buffer, secs))
    }

    /// The mouse moved to `secs` of the audio with a point grabbed: it goes there.
    pub fn wave_mouse_move(&mut self, secs: f64) {
        let Some(id) = self.wave_grab else {
            return;
        };
        let pad = self
            .buffer()
            .score
            .audio
            .as_ref()
            .map_or(0.0, |a| a.frame_padding as f64 / SYNC_RATE);
        let frame = ((secs + pad) * SYNC_RATE).round().max(0.0) as i64;
        self.edit(|cx| {
            let score = cx.whole();
            score.set_chant_sync_point(id, frame);
            score.refresh_chant_sync();
            Ok(())
        });
    }

    /// The button went up: the audio stops, the point is let go.
    pub fn wave_mouse_up(&mut self) {
        self.wave_grab = None;
        self.group_edits(false);
        self.effects.push(Effect::Pause);
    }
}
