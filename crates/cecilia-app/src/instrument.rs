//! The fretboard and piano panels: the notes of the focused pane's track at the edit cursor
//! (or under the playhead while playing), on a guitar neck or a keyboard. `:fretboard`
//! (`<leader>F`) and `:piano` (`<leader>K`) show or hide them.

use crate::app::{App, cursor_tick};
use crate::theme::Rgba;
use cecilia_core::{Score, TrackKind};

/// A note the panels show.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PanelNote {
    /// String (0 = highest) and fret on fretted tracks.
    pub string: usize,
    pub fret: u8,
    /// Sounding MIDI pitch (capo included).
    pub pitch: u8,
}

/// What the fretboard and piano panels show.
#[derive(Debug, Clone, PartialEq)]
pub struct InstrumentView {
    pub track: usize,
    /// Open-string pitches, index 0 = highest string (empty for non-fretted tracks).
    pub strings: Vec<u8>,
    pub frets: u8,
    pub capo: u8,
    pub fretted: bool,
    pub notes: Vec<PanelNote>,
    pub color: Rgba,
    /// The cursor's string, when the notes are the cursor's (not the playhead's).
    pub cursor_string: Option<usize>,
}

/// Names of pitch classes.
pub const NOTE_NAMES: [&str; 12] = [
    "C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B",
];

/// The notes of a track sounding at (bar, tick), in every voice.
pub fn notes_at(score: &Score, track: usize, bar: usize, tick: u32) -> Vec<PanelNote> {
    let Some(t) = score.tracks.get(track) else {
        return Vec::new();
    };
    let Some(b) = t.bars.get(bar) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for voice in &b.voices {
        let mut start = 0;
        for beat in &voice.beats {
            let end = start + beat.ticks().max(1);
            if (start..end).contains(&tick) {
                for n in &beat.notes {
                    out.push(PanelNote {
                        string: n.string as usize,
                        fret: n.fret,
                        pitch: t.pitch_of(n),
                    });
                }
            }
            start = end;
        }
    }
    out
}

impl App {
    /// What the fretboard and piano show for the focused pane. `play` is the playhead
    /// (buffer, bar, tick), used while that buffer plays.
    pub fn instrument_view(&self, play: Option<(usize, usize, u32)>) -> Option<InstrumentView> {
        let pane = self.pane();
        let score = &self.buffers[pane.buffer].score;
        let track = score.tracks.get(pane.cursor.track)?;
        let playing = play.filter(|&(b, ..)| b == pane.buffer && (self.playing || self.paused));
        let (notes, cursor_string) = match playing {
            Some((_, bar, tick)) => (notes_at(score, pane.cursor.track, bar, tick), None),
            None => {
                let tick = cursor_tick(score, &pane.cursor);
                (
                    notes_at(score, pane.cursor.track, pane.cursor.bar, tick),
                    Some(pane.cursor.string),
                )
            }
        };
        let fretted = track.kind == TrackKind::Fretted && !track.strings.is_empty();
        Some(InstrumentView {
            track: pane.cursor.track,
            strings: if fretted {
                track.strings.clone()
            } else {
                Vec::new()
            },
            frets: if track.fret_count == 0 {
                24
            } else {
                track.fret_count.min(36)
            },
            capo: track.capo,
            fretted,
            notes,
            color: self.track_color(pane.buffer, pane.cursor.track),
            cursor_string: cursor_string.filter(|_| fretted),
        })
    }
}

impl App {
    /// A click on the fretboard at `position` (frets from the nut) on `string`: that note in the
    /// cursor beat, or off again. Below a capo it is the capo's open string.
    pub fn click_fret(&mut self, string: usize, position: u8) {
        let capo = self
            .buffer()
            .score
            .tracks
            .get(self.pane().cursor.track)
            .map_or(0, |t| t.capo);
        let fret = position.saturating_sub(capo);
        let duration = self.insert_duration();
        self.edit(|cx| crate::edit::toggle_fret(cx, string, fret, duration));
    }

    /// A click on a piano key: that pitch in the cursor beat (as a chord note), or off again.
    pub fn click_key(&mut self, pitch: u8) {
        let duration = self.insert_duration();
        self.edit(|cx| crate::edit::toggle_pitch(cx, pitch, duration));
    }
}

/// What the tuner shows for a reading.
#[derive(Debug, Clone, PartialEq)]
pub struct TunerView {
    pub freq: f32,
    /// The nearest note ("A") and its octave.
    pub note: &'static str,
    pub octave: i32,
    /// How far from that note, in cents (−50 to +50).
    pub cents: f32,
    /// The nearest open string of the track's tuning: its index (0 = highest), its note
    /// name and octave, and how far from it in cents.
    pub string: Option<(usize, &'static str, i32, f32)>,
}

/// Reads a frequency as a note against a reference A4, and the nearest open string.
pub fn tuner_view(freq: f32, a4: f64, strings: &[u8]) -> TunerView {
    let midi = 69.0 + 12.0 * (freq as f64 / a4).log2();
    let nearest = midi.round();
    let pc = (nearest as i64).rem_euclid(12) as usize;
    let string = strings
        .iter()
        .enumerate()
        .min_by(|a, b| {
            (midi - *a.1 as f64)
                .abs()
                .total_cmp(&(midi - *b.1 as f64).abs())
        })
        .filter(|(_, open)| (midi - **open as f64).abs() < 6.0)
        .map(|(i, &open)| {
            (
                i,
                NOTE_NAMES[open as usize % 12],
                open as i32 / 12 - 1,
                ((midi - open as f64) * 100.0) as f32,
            )
        });
    TunerView {
        freq,
        note: NOTE_NAMES[pc],
        octave: nearest as i32 / 12 - 1,
        cents: ((midi - nearest) * 100.0) as f32,
        string,
    }
}

impl App {
    /// The tuner's reading for display, against the focused track's tuning.
    pub fn tuner_view(&self) -> Option<TunerView> {
        let r = self.tuner_reading?;
        let track = self.buffer().score.tracks.get(self.pane().cursor.track);
        let strings: Vec<u8> = track
            .filter(|t| t.kind == TrackKind::Fretted)
            .map(|t| t.strings.iter().map(|&p| p + t.capo).collect())
            .unwrap_or_default();
        Some(tuner_view(
            r.freq,
            self.config.file.audio.tuner_a4,
            &strings,
        ))
    }
}

#[cfg(test)]
mod tests {
    use crate::keys::parse_keys;
    use crate::{App, Command, Config};

    #[test]
    fn the_tuner_names_the_note_and_the_nearest_string() {
        use super::tuner_view;
        let standard = [64, 59, 55, 50, 45, 40];
        let v = tuner_view(110.0, 440.0, &standard);
        assert_eq!((v.note, v.octave), ("A", 2));
        assert!(v.cents.abs() < 0.01);
        assert_eq!(v.string.map(|s| (s.0, s.1, s.2)), Some((4, "A", 2)));
        // A flat low E: 81 Hz is about −30 cents from E2.
        let v = tuner_view(81.0, 440.0, &standard);
        assert_eq!((v.note, v.octave), ("E", 2));
        assert!((v.cents + 30.0).abs() < 2.0, "{}", v.cents);
        assert_eq!(v.string.unwrap().0, 5);
        // With A4 = 432 Hz, 864 Hz is an A5 in tune, too high for any open string.
        let v = tuner_view(864.0, 432.0, &standard);
        assert_eq!(v.note, "A");
        assert!(v.cents.abs() < 0.01);
        assert!(v.string.is_none());
    }

    #[test]
    fn the_panels_show_the_cursors_notes_or_the_playheads() {
        let mut app = App::new(Config::default(), None);
        app.run(Command::New);
        for k in parse_keys("i3<Down>5<Esc>", &[]).unwrap() {
            app.handle_key(k);
        }
        let v = app.instrument_view(None).unwrap();
        assert!(v.fretted && v.strings.len() == 6);
        let mut frets: Vec<(usize, u8)> = v.notes.iter().map(|n| (n.string, n.fret)).collect();
        frets.sort();
        assert_eq!(frets, [(0, 3), (1, 5)]);
        // High E fret 3 is G4; the cursor's string is marked.
        assert!(v.notes.iter().any(|n| n.pitch == 67));
        assert_eq!(v.cursor_string, Some(1));
        // While playing, the playhead's notes (an empty bar here).
        app.playing = true;
        app.playing_buffer = Some(app.pane().buffer);
        let v = app
            .instrument_view(Some((app.pane().buffer, 1, 0)))
            .unwrap();
        assert!(v.notes.is_empty() && v.cursor_string.is_none());
        // Clicks: a fret puts the note on that string (and again takes it off); a key adds the
        // pitch on a free string.
        app.playing = false;
        app.playing_buffer = None;
        app.click_fret(2, 4);
        let notes = |app: &App| {
            let mut v: Vec<(usize, u8)> = app
                .instrument_view(None)
                .unwrap()
                .notes
                .iter()
                .map(|n| (n.string, n.fret))
                .collect();
            v.sort();
            v
        };
        assert_eq!(notes(&app), [(0, 3), (1, 5), (2, 4)]);
        assert_eq!(app.pane().cursor.string, 2);
        app.click_fret(2, 4);
        assert_eq!(notes(&app), [(0, 3), (1, 5)]);
        app.click_key(52); // E3: D string fret 2 (cursor's string G can't, the D string is free)
        assert!(notes(&app).contains(&(3, 2)), "{:?}", notes(&app));
        app.click_key(52);
        assert_eq!(notes(&app), [(0, 3), (1, 5)]);
        // Toggles.
        app.run(Command::Fretboard(None));
        app.run(Command::Piano(Some(true)));
        assert!(
            app.has_tool(crate::app::PaneKind::Fretboard)
                && app.has_tool(crate::app::PaneKind::Piano)
        );
    }
}
