//! The tracks panel, as in Guitar Pro or TuxGuitar: one row per track of the focused file with
//! its number, volume, solo and mute, name and instrument, and a block per bar (coloured where
//! the bar has notes) marking the edit cursor and the playhead. `<leader>T` shows it, `<C-w>t`
//! focuses it (Tracks mode: `j`/`k` choose a track, `h`/`l` a bar, `s`, `m`, `+`, `-`, `c`).

use crate::app::App;
use crate::command::Command;
use crate::config::Mode;
use crate::keys::Key;
use crate::theme::Rgba;
use muse_core::{Color, Score, Track, TrackKind};

/// One track as the panel shows it.
#[derive(Debug, Clone, PartialEq)]
pub struct TrackRow {
    /// Track index (0-based).
    pub index: usize,
    pub name: String,
    pub instrument: String,
    /// MIDI volume, 0–127.
    pub volume: u8,
    pub mute: bool,
    pub solo: bool,
    /// Heard (not muted, and not silenced by another track's solo).
    pub audible: bool,
    pub color: Rgba,
    /// The colour was chosen for this track (`:track-color`).
    pub custom_color: bool,
    /// Per bar: whether it has notes (not only rests).
    pub bars: Vec<bool>,
}

/// Volume step of `track-volume-up` / `-down`, in percent.
const VOLUME_STEP: i32 = 5;

/// Whether bar `bar` of a track has notes.
pub fn bar_has_notes(track: &Track, bar: usize) -> bool {
    track.bars.get(bar).is_some_and(|b| {
        b.voices
            .iter()
            .flat_map(|v| &v.beats)
            .any(|beat| !beat.notes.is_empty())
    })
}

/// Length of a bar in ticks (from its time signature).
pub fn bar_ticks(score: &Score, bar: usize) -> u32 {
    score
        .master_bars
        .get(bar)
        .map_or(3840, |m| m.ticks())
        .max(1)
}

/// Name of the sound a track plays.
pub fn instrument_name(track: &Track) -> String {
    match track.kind {
        TrackKind::Drums => "Drum Kit".into(),
        TrackKind::Chant => "Voice (chant)".into(),
        _ => GM_PROGRAMS
            .get(track.midi.program as usize)
            .copied()
            .unwrap_or("?")
            .into(),
    }
}

fn to_rgba(c: Color) -> Rgba {
    Rgba::rgb(c.r, c.g, c.b)
}

impl App {
    /// The colour of a track of a buffer: its own, or the theme's for its position.
    pub fn track_color(&self, buffer: usize, track: usize) -> Rgba {
        self.buffers
            .get(buffer)
            .and_then(|b| b.score.tracks.get(track))
            .and_then(|t| t.display_color)
            .map(to_rgba)
            .unwrap_or_else(|| self.config.theme.track_color(track))
    }

    /// The rows of the tracks panel, for the focused pane's file.
    pub fn track_rows(&self) -> Vec<TrackRow> {
        let buffer = self.pane().buffer;
        let score = &self.buffers[buffer].score;
        let any_solo = score.tracks.iter().any(|t| t.solo);
        score
            .tracks
            .iter()
            .enumerate()
            .map(|(i, t)| TrackRow {
                index: i,
                name: if t.name.trim().is_empty() {
                    format!("Track {}", i + 1)
                } else {
                    t.name.trim().to_string()
                },
                instrument: instrument_name(t),
                volume: t.midi.volume.min(127),
                mute: t.mute,
                solo: t.solo,
                audible: !t.mute && (!any_solo || t.solo),
                color: self.track_color(buffer, i),
                custom_color: t.display_color.is_some(),
                bars: (0..score.master_bars.len())
                    .map(|b| bar_has_notes(t, b))
                    .collect(),
            })
            .collect()
    }

    /// Keys while the tracks panel has the focus: its own keymap (`[keymap.tracks]`).
    pub(crate) fn tracks_key(&mut self, key: Key) {
        if self.pending.is_empty() {
            if let Some(d) = key.as_char().and_then(|c| c.to_digit(10)) {
                if d != 0 || self.count.is_some() {
                    self.count = Some(
                        self.count
                            .unwrap_or(0)
                            .saturating_mul(10)
                            .saturating_add(d as usize),
                    );
                    return;
                }
            }
        }
        self.pending.push(key);
        let cmds = match self.config.keymap(Mode::Tracks).lookup(&self.pending) {
            crate::config::Lookup::Exact(cmds) => cmds.to_vec(),
            crate::config::Lookup::Prefix => return,
            crate::config::Lookup::None => {
                self.pending.clear();
                self.count = None;
                return;
            }
        };
        self.pending.clear();
        let count = self.count.take();
        for cmd in cmds {
            self.run_counted(cmd, count);
        }
    }

    /// Groups the following edits into one undo step (a volume drag), until called with false.
    pub fn group_edits(&mut self, on: bool) {
        self.ed.group = on.then_some(false);
    }

    /// Sets a track's volume (0–127) in the focused file.
    pub fn set_track_volume(&mut self, track: usize, volume: u8) {
        self.edit(|cx| {
            let t = cx.whole().tracks.get_mut(track).ok_or("no such track")?;
            t.midi.volume = volume.min(127);
            Ok(())
        });
    }

    /// Selects a track and bar from the panel: the focused pane's cursor goes there.
    pub fn select_track_bar(&mut self, track: usize, bar: Option<usize>) {
        let score = &self.buffers[self.pane().buffer].score;
        if track >= score.tracks.len() {
            return;
        }
        let mut c = self.pane().cursor;
        if c.track != track {
            c.track = track;
            c.string = 0;
            c.voice = 0;
        }
        if let Some(b) = bar {
            if b != c.bar {
                c.bar = b;
                c.beat = 0;
            }
        }
        crate::app::clamp_cursor(score, &mut c);
        self.pane_mut().cursor = c;
    }

    pub(crate) fn run_tracks(&mut self, cmd: &Command) -> bool {
        use Command::*;
        match cmd {
            Tracks(on) => {
                self.tracks_panel = on.unwrap_or(!self.tracks_panel);
                if !self.tracks_panel && self.mode == Mode::Tracks {
                    self.mode = Mode::Normal;
                }
            }
            TracksFocus => {
                if self.mode == Mode::Tracks {
                    self.mode = Mode::Normal;
                } else {
                    self.tracks_panel = true;
                    self.mode = Mode::Tracks;
                    self.pending.clear();
                }
            }
            TrackVolume(change) => {
                let t = self.pane().cursor.track;
                let Some(track) = self.buffer().score.tracks.get(t) else {
                    self.error("no track");
                    return true;
                };
                let pct = (track.midi.volume as i32 * 100 + 63) / 127;
                let pct = match change {
                    VolumeChange::To(p) => *p as i32,
                    VolumeChange::By(d) => pct + d,
                    VolumeChange::Up => pct + VOLUME_STEP,
                    VolumeChange::Down => pct - VOLUME_STEP,
                }
                .clamp(0, 100);
                self.set_track_volume(t, ((pct * 127 + 50) / 100) as u8);
                self.info(format!("track {} volume {pct}%", t + 1));
            }
            TrackColor(arg) => {
                let t = self.pane().cursor.track;
                let b = self.pane().buffer;
                let color = match arg.as_deref() {
                    Some("off" | "default" | "none") => None,
                    Some(hex) => match Rgba::parse(hex) {
                        Some(c) => Some(c),
                        None => {
                            self.error(format!("track-color: '{hex}' is not a colour (#rrggbb)"));
                            return true;
                        }
                    },
                    // No colour: the next one of the theme's palette.
                    None => {
                        let palette = &self.config.theme.track_colors;
                        let now = self.track_color(b, t);
                        let i = palette.iter().position(|&c| c == now).map_or(0, |i| i + 1);
                        palette.get(i % palette.len().max(1)).copied()
                    }
                };
                self.edit(|cx| {
                    let track = cx.whole().tracks.get_mut(t).ok_or("no such track")?;
                    track.display_color = color.map(|c| Color {
                        r: c.r,
                        g: c.g,
                        b: c.b,
                    });
                    Ok(())
                });
            }
            _ => return false,
        }
        true
    }
}

/// How `track-volume` changes the volume (in percent).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VolumeChange {
    To(u32),
    By(i32),
    Up,
    Down,
}

impl VolumeChange {
    /// `80`, `+10` or `-10`.
    pub fn parse(s: &str) -> Option<Self> {
        let s = s.trim().trim_end_matches('%');
        if let Some(d) = s.strip_prefix('+') {
            return d.parse().ok().map(VolumeChange::By);
        }
        if s.starts_with('-') {
            return s.parse().ok().map(VolumeChange::By);
        }
        s.parse().ok().map(VolumeChange::To)
    }
}

/// General MIDI program names.
pub const GM_PROGRAMS: [&str; 128] = [
    "Acoustic Grand Piano",
    "Bright Acoustic Piano",
    "Electric Grand Piano",
    "Honky-tonk Piano",
    "Electric Piano 1",
    "Electric Piano 2",
    "Harpsichord",
    "Clavinet",
    "Celesta",
    "Glockenspiel",
    "Music Box",
    "Vibraphone",
    "Marimba",
    "Xylophone",
    "Tubular Bells",
    "Dulcimer",
    "Drawbar Organ",
    "Percussive Organ",
    "Rock Organ",
    "Church Organ",
    "Reed Organ",
    "Accordion",
    "Harmonica",
    "Tango Accordion",
    "Nylon Guitar",
    "Steel Guitar",
    "Jazz Guitar",
    "Clean Guitar",
    "Muted Guitar",
    "Overdriven Guitar",
    "Distortion Guitar",
    "Guitar Harmonics",
    "Acoustic Bass",
    "Fingered Bass",
    "Picked Bass",
    "Fretless Bass",
    "Slap Bass 1",
    "Slap Bass 2",
    "Synth Bass 1",
    "Synth Bass 2",
    "Violin",
    "Viola",
    "Cello",
    "Contrabass",
    "Tremolo Strings",
    "Pizzicato Strings",
    "Orchestral Harp",
    "Timpani",
    "String Ensemble 1",
    "String Ensemble 2",
    "Synth Strings 1",
    "Synth Strings 2",
    "Choir Aahs",
    "Voice Oohs",
    "Synth Voice",
    "Orchestra Hit",
    "Trumpet",
    "Trombone",
    "Tuba",
    "Muted Trumpet",
    "French Horn",
    "Brass Section",
    "Synth Brass 1",
    "Synth Brass 2",
    "Soprano Sax",
    "Alto Sax",
    "Tenor Sax",
    "Baritone Sax",
    "Oboe",
    "English Horn",
    "Bassoon",
    "Clarinet",
    "Piccolo",
    "Flute",
    "Recorder",
    "Pan Flute",
    "Blown Bottle",
    "Shakuhachi",
    "Whistle",
    "Ocarina",
    "Square Lead",
    "Sawtooth Lead",
    "Calliope Lead",
    "Chiff Lead",
    "Charang Lead",
    "Voice Lead",
    "Fifths Lead",
    "Bass + Lead",
    "New Age Pad",
    "Warm Pad",
    "Polysynth Pad",
    "Choir Pad",
    "Bowed Pad",
    "Metallic Pad",
    "Halo Pad",
    "Sweep Pad",
    "Rain",
    "Soundtrack",
    "Crystal",
    "Atmosphere",
    "Brightness",
    "Goblins",
    "Echoes",
    "Sci-fi",
    "Sitar",
    "Banjo",
    "Shamisen",
    "Koto",
    "Kalimba",
    "Bagpipe",
    "Fiddle",
    "Shanai",
    "Tinkle Bell",
    "Agogo",
    "Steel Drums",
    "Woodblock",
    "Taiko Drum",
    "Melodic Tom",
    "Synth Drum",
    "Reverse Cymbal",
    "Guitar Fret Noise",
    "Breath Noise",
    "Seashore",
    "Bird Tweet",
    "Telephone Ring",
    "Helicopter",
    "Applause",
    "Gunshot",
];

#[cfg(test)]
mod tests {
    use crate::keys::parse_keys;
    use crate::{App, Command, Config, Mode};

    fn keys(app: &mut App, s: &str) {
        for k in parse_keys(s, &[crate::Key::char(' ')]).unwrap() {
            app.handle_key(k);
        }
    }

    #[test]
    fn the_tracks_panel_lists_tracks_and_takes_the_focus() {
        let mut app = App::new(Config::default(), None);
        app.run(Command::New);
        keys(&mut app, "i5<Esc>");
        app.run(Command::TrackAdd("guitar".into()));
        let rows = app.track_rows();
        assert_eq!(rows.len(), 2);
        assert!(rows[0].bars[0] && !rows[0].bars[1]);
        assert_ne!(rows[0].color, rows[1].color);
        // <C-w>t focuses the panel (showing it); j/l move the edit cursor.
        keys(&mut app, "<C-w>t");
        assert!(app.tracks_panel && app.mode == Mode::Tracks);
        keys(&mut app, "ggkll");
        assert_eq!((app.pane().cursor.track, app.pane().cursor.bar), (0, 2));
        keys(&mut app, "j");
        assert_eq!(app.pane().cursor.track, 1);
        // Solo, mute and volume.
        keys(&mut app, "s");
        let t = app.pane().cursor.track;
        assert!(app.track_rows()[t].solo);
        assert!(!app.track_rows()[1 - t].audible);
        keys(&mut app, "m");
        assert!(app.track_rows()[t].mute);
        let v = app.track_rows()[t].volume;
        keys(&mut app, "--");
        assert!(app.track_rows()[t].volume < v);
        app.run(Command::TrackVolume(super::VolumeChange::To(100)));
        assert_eq!(app.track_rows()[t].volume, 127);
        // Colours: cycle the palette, set one, back to the theme's.
        keys(&mut app, "c");
        assert!(app.track_rows()[t].custom_color);
        app.run(Command::TrackColor(Some("#123456".into())));
        assert_eq!(app.track_rows()[t].color.hex(), "#123456");
        app.run(Command::TrackColor(Some("off".into())));
        assert!(!app.track_rows()[t].custom_color);
        // Leaving: Esc; <leader>T hides it.
        keys(&mut app, "<Esc>");
        assert_eq!(app.mode, Mode::Normal);
        keys(&mut app, "<Space>T");
        assert!(!app.tracks_panel && !app.tracks_shown());
        // Hidden, it shows for a moment when a key changes the track…
        keys(&mut app, "l");
        assert!(!app.tracks_shown());
        keys(&mut app, "K");
        assert!(app.tracks_peeking() && app.tracks_shown() && !app.tracks_panel);
        // …unless ui.tracks_peek is 0.
        let (config, _) = Config::parse("[ui]\ntracks_peek = 0\n").unwrap();
        let mut app = App::new(config, None);
        app.run(Command::New);
        app.run(Command::TrackAdd("guitar".into()));
        keys(&mut app, "K");
        assert!(!app.tracks_shown());
    }
}
