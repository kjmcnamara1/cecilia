//! The tracks panel, as in Guitar Pro or TuxGuitar: one row per track of the focused file with
//! its number, volume, solo and mute, name and instrument, and a block per bar (coloured where
//! the bar has notes) marking the edit cursor and the playhead. `<leader>T` shows it, `<C-w>t`
//! focuses it (Tracks mode: `j`/`k` choose a track, `h`/`l` a bar, `s`, `m`, `+`, `-`, `c`).

use crate::app::{App, PaneKind};
use crate::command::{Command, Strip};
use crate::config::Mode;
use crate::keys::Key;
use crate::theme::Rgba;
use muse_core::{Color, Score, Track, TrackKind};

/// One row of the panel: a track, the audio track or the synth master.
#[derive(Debug, Clone, PartialEq)]
pub struct TrackRow {
    /// What the row's volume, solo and mute act on.
    pub strip: Strip,
    /// Track index (0-based); for the audio and master rows, the number of tracks.
    pub index: usize,
    pub name: String,
    pub instrument: String,
    /// MIDI volume, 0–127 (the slider's level on that scale for the audio and master rows).
    pub volume: u8,
    pub mute: bool,
    pub solo: bool,
    /// Heard (not muted, and not silenced by another track's solo).
    pub audible: bool,
    pub color: Rgba,
    /// The colour was chosen for this track (`:track-color`).
    pub custom_color: bool,
    /// Per bar: whether it has notes (not only rests); empty for the audio and master rows.
    pub bars: Vec<bool>,
    /// The master row's section markers: bar, letter and title.
    pub markers: Vec<(usize, String, String)>,
}

/// Highest level of the audio and master sliders (`:volume audio 150`).
pub const MIX_MAX: f32 = 1.5;

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

    /// The rows of the tracks panel, for the focused pane's file: its tracks, then its audio
    /// track (when it has one), then the synth master.
    pub fn track_rows(&self) -> Vec<TrackRow> {
        let buffer = self.pane().buffer;
        let score = &self.buffers[buffer].score;
        let has_audio = score.audio.is_some();
        let synth_solo = score.tracks.iter().any(|t| t.solo);
        let any_solo = synth_solo || (has_audio && self.pa.audio_solo);
        let n = score.tracks.len();
        let level = |v: f32| ((v / MIX_MAX).clamp(0.0, 1.0) * 127.0).round() as u8;
        let mut rows: Vec<TrackRow> = score
            .tracks
            .iter()
            .enumerate()
            .map(|(i, t)| TrackRow {
                strip: Strip::Track(i),
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
                markers: Vec::new(),
            })
            .collect();
        if let Some(audio) = &score.audio {
            rows.push(TrackRow {
                strip: Strip::Audio,
                index: n,
                name: "Audio Track".into(),
                instrument: audio
                    .file
                    .as_deref()
                    .and_then(|f| std::path::Path::new(f).file_name())
                    .map_or_else(String::new, |f| f.to_string_lossy().into_owned()),
                volume: level(self.pa.mix.audio),
                mute: self.pa.audio_mute,
                solo: self.pa.audio_solo,
                audible: !self.pa.audio_mute && (!any_solo || self.pa.audio_solo),
                color: self.config.theme.accent,
                custom_color: false,
                bars: Vec::new(),
                markers: Vec::new(),
            });
        }
        // The master: muted when every synth track is, soloed when any is.
        rows.push(TrackRow {
            strip: Strip::Master,
            index: n,
            name: "Master".into(),
            instrument: String::new(),
            volume: level(self.pa.mix.master),
            mute: n > 0 && score.tracks.iter().all(|t| t.mute),
            solo: synth_solo,
            audible: true,
            color: self.config.theme.chrome_text,
            custom_color: false,
            bars: Vec::new(),
            markers: score
                .master_bars
                .iter()
                .enumerate()
                .filter_map(|(b, m)| {
                    let m = m.marker.as_ref()?;
                    Some((b, m.letter.trim().to_string(), m.title.trim().to_string()))
                })
                .collect(),
        });
        rows
    }

    /// Sets a row's volume from the panel's slider (0.0–1.0 of its range).
    pub fn set_strip_volume(&mut self, strip: Strip, frac: f64) {
        let frac = frac.clamp(0.0, 1.0);
        match strip {
            Strip::Track(t) => self.set_track_volume(t, (frac * 127.0).round() as u8),
            Strip::Audio | Strip::Master => {
                let v = frac as f32 * MIX_MAX;
                if strip == Strip::Audio {
                    self.pa.mix.audio = v;
                } else {
                    self.pa.mix.master = v;
                }
                self.effects.push(crate::Effect::PlaybackSettings);
            }
        }
    }

    /// The track a track command works on: the tracks pane's selected track while it has the
    /// focus, else the score pane's.
    fn target_track(&self) -> usize {
        match self.selected_strip() {
            Some(Strip::Track(t)) => t,
            _ => self.pane().cursor.track,
        }
    }

    /// Keys while a track is renamed in place: they edit the name; Enter keeps it, Escape
    /// drops it.
    pub(crate) fn rename_key(&mut self, key: Key) {
        use crate::keys::KeyCode;
        let Some((t, name)) = self.renaming.as_mut() else {
            return;
        };
        match key.code {
            KeyCode::Esc => {
                self.renaming = None;
                self.mode = Mode::Normal;
            }
            KeyCode::Enter => {
                let (t, name) = (*t, name.clone());
                self.renaming = None;
                self.mode = Mode::Normal;
                self.run(Command::TrackRename(format!("{name} {}", t + 1)));
            }
            KeyCode::Backspace => {
                name.pop();
            }
            _ => {
                if let Some(c) = key.as_char() {
                    name.push(c);
                }
            }
        }
    }

    /// The tracks panel's selected row, while the panel has the focus.
    pub fn selected_strip(&self) -> Option<Strip> {
        if self.focused_kind() != PaneKind::Tracks {
            return None;
        }
        let rows = self.track_rows();
        rows.get(self.tracks_selected.min(rows.len().saturating_sub(1)))
            .map(|r| r.strip)
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
            // Not a tracks key: the keys go on as in any pane (`<C-w>` moves, `:` …).
            crate::config::Lookup::None => {
                for k in std::mem::take(&mut self.pending) {
                    self.normal_key(k);
                }
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
            Tracks(on) => self.set_tool(PaneKind::Tracks, *on),
            // Focuses the tracks pane (showing it), or from it the score pane again.
            TracksFocus => {
                if self.focused_kind() == PaneKind::Tracks {
                    self.set_focus(self.focused());
                } else {
                    self.set_tool(PaneKind::Tracks, Some(true));
                    let id = self.tool_pane(PaneKind::Tracks).unwrap();
                    self.set_focus(id);
                    self.pending.clear();
                    // The selection starts on the track the score pane shows.
                    self.tracks_selected = self.pane().cursor.track;
                }
            }
            TracksDown | TracksUp => {
                let n = self.track_rows().len();
                let sel = self.tracks_selected.min(n.saturating_sub(1));
                self.tracks_selected = if matches!(cmd, TracksDown) {
                    (sel + 1).min(n.saturating_sub(1))
                } else {
                    sel.saturating_sub(1)
                };
            }
            TrackRename(arg) => {
                // A trailing number names the track; the rest is the name.
                let arg = arg.trim();
                let (name, t) = match arg.rsplit_once(' ') {
                    Some((name, n)) if n.parse::<usize>().is_ok_and(|n| n > 0) => {
                        (name.trim(), n.parse::<usize>().unwrap() - 1)
                    }
                    _ => (arg, self.pane().cursor.track),
                };
                if name.is_empty() {
                    self.error("track-rename: the name is empty");
                    return true;
                }
                let name = name.to_string();
                self.edit(|cx| {
                    let track = cx.whole().tracks.get_mut(t).ok_or("no such track")?;
                    track.name = name;
                    Ok(())
                });
            }
            TrackRenameStart => {
                let t = self.target_track();
                if let Some(track) = self.buffer().score.tracks.get(t) {
                    self.renaming = Some((t, track.name.trim().to_string()));
                    self.mode = Mode::Insert;
                }
            }
            TrackInstrument(None) => {
                let t = self.target_track();
                self.open_picker(crate::picker::PickerKind::Instrument(t));
            }
            TrackInstrument(Some(arg)) => {
                let mut words = arg.split_whitespace();
                let what = words.next().unwrap_or_default().to_string();
                let t = match words.next().map(str::parse::<usize>) {
                    Some(Ok(n)) if n > 0 => n - 1,
                    Some(_) => {
                        self.error("track-instrument: expected PROGRAM [track number]");
                        return true;
                    }
                    None => self.target_track(),
                };
                let program = match what.as_str() {
                    "drums" => None,
                    p => match p.parse::<u8>() {
                        Ok(p) if (1..=128).contains(&p) => Some(p - 1),
                        _ => {
                            self.error(format!(
                                "track-instrument: '{p}' is not a program (1–128) or drums"
                            ));
                            return true;
                        }
                    },
                };
                self.edit(|cx| {
                    let track = cx.whole().tracks.get_mut(t).ok_or("no such track")?;
                    let empty = track
                        .bars
                        .iter()
                        .flat_map(|b| &b.voices)
                        .flat_map(|v| &v.beats)
                        .all(|b| b.notes.is_empty());
                    match program {
                        // Drum notes and pitches mean different things: only an empty track
                        // changes between them.
                        None if track.kind != TrackKind::Drums => {
                            if !empty {
                                return Err("track-instrument: only an empty track becomes drums".into());
                            }
                            track.kind = TrackKind::Drums;
                            track.strings = vec![0; 6];
                            track.midi.channel = 9;
                            track.midi.effect_channel = 9;
                            track.staves.clear();
                        }
                        Some(p) if track.kind == TrackKind::Drums => {
                            if !empty {
                                return Err("track-instrument: drums with notes can't change to an instrument".into());
                            }
                            track.kind = TrackKind::Pitched;
                            track.strings.clear();
                            track.midi.channel = 0;
                            track.midi.effect_channel = 0;
                            track.midi.program = p;
                            track.staves.clear();
                        }
                        Some(p) => track.midi.program = p,
                        None => {}
                    }
                    Ok(())
                });
            }
            TracksOpen => match self.selected_strip() {
                Some(Strip::Track(t)) => self.select_track_bar(t, None),
                Some(_) => self.info("the audio and master rows have no notation to show"),
                None => {}
            },
            TrackVolume(change) => {
                let strip = self
                    .selected_strip()
                    .unwrap_or(Strip::Track(self.pane().cursor.track));
                let pct = match strip {
                    Strip::Track(t) => match self.buffer().score.tracks.get(t) {
                        Some(track) => (track.midi.volume as i32 * 100 + 63) / 127,
                        None => {
                            self.error("no track");
                            return true;
                        }
                    },
                    Strip::Audio => (self.pa.mix.audio / MIX_MAX * 100.0).round() as i32,
                    Strip::Master => (self.pa.mix.master / MIX_MAX * 100.0).round() as i32,
                };
                let pct = match change {
                    VolumeChange::To(p) => *p as i32,
                    VolumeChange::By(d) => pct + d,
                    VolumeChange::Up => pct + VOLUME_STEP,
                    VolumeChange::Down => pct - VOLUME_STEP,
                }
                .clamp(0, 100);
                match strip {
                    Strip::Track(t) => {
                        self.set_track_volume(t, ((pct * 127 + 50) / 100) as u8);
                        self.info(format!("track {} volume {pct}%", t + 1));
                    }
                    _ => {
                        self.set_strip_volume(strip, pct as f64 / 100.0);
                        let name = if strip == Strip::Audio {
                            "audio"
                        } else {
                            "master"
                        };
                        self.info(format!("{name} volume {pct}%"));
                    }
                }
            }
            TrackColor(arg) => {
                let t = match self.selected_strip() {
                    Some(Strip::Track(t)) => t,
                    Some(_) => {
                        self.error("track-color: the audio and master rows have no colour to set");
                        return true;
                    }
                    None => self.pane().cursor.track,
                };
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
    use crate::app::PaneKind;
    use crate::keys::parse_keys;
    use crate::{App, Command, Config, Mode, Strip};

    fn keys(app: &mut App, s: &str) {
        for k in parse_keys(s, &[crate::Key::char(' ')]).unwrap() {
            app.handle_key(k);
        }
    }

    #[test]
    fn tool_panes_renaming_and_instruments() {
        let mut app = App::new(Config::default(), None);
        app.run(Command::New);
        app.viewport = crate::layout::Rect {
            x: 0,
            y: 0,
            w: 100,
            h: 40,
        };
        app.run(Command::Tracks(Some(true)));
        let score = app.focused();
        // A pane like any other: <C-w>j focuses it, the score pane stays the one worked on.
        keys(&mut app, "<C-w>j");
        assert_eq!(app.focused_kind(), PaneKind::Tracks);
        assert_eq!(app.focused(), score);
        assert_eq!(app.mode, Mode::Normal);
        // <C-w>K moves it to the top.
        keys(&mut app, "<C-w>K");
        let tracks = app.tool_pane(PaneKind::Tracks).unwrap();
        let top = app.layout().into_iter().min_by_key(|(_, r)| r.y).unwrap().0;
        assert_eq!(top, tracks);
        keys(&mut app, "<C-w>j");
        assert_eq!(app.focused_kind(), PaneKind::Score);

        // Renaming: a command, then in place (Enter keeps, Escape drops), with undo.
        app.run(Command::TrackRename("Lead Guitar 1".into()));
        assert_eq!(app.buffer().score.tracks[0].name, "Lead Guitar");
        keys(&mut app, "<C-w>kt");
        assert_eq!(app.mode, Mode::Insert);
        assert_eq!(app.renaming.as_ref().unwrap().1, "Lead Guitar");
        keys(&mut app, "<BS><BS><BS><BS><BS><BS>Bass<CR>");
        assert_eq!(app.buffer().score.tracks[0].name, "Lead Bass");
        assert_eq!(app.mode, Mode::Normal);
        keys(&mut app, "tXY<Esc>");
        assert_eq!(app.buffer().score.tracks[0].name, "Lead Bass");
        keys(&mut app, "u");
        assert_eq!(app.buffer().score.tracks[0].name, "Lead Guitar");

        // Instruments: `i` opens the picker; choosing runs track-instrument.
        keys(&mut app, "i");
        assert_eq!(app.mode, Mode::Picker);
        assert!(app.palette().unwrap().items.len() > 128);
        keys(&mut app, "<Esc>");
        app.run(Command::TrackInstrument(Some("34 1".into())));
        assert_eq!(app.buffer().score.tracks[0].midi.program, 33);
        // A guitar with notes doesn't become drums.
        keys(&mut app, "<C-w>j");
        keys(&mut app, "i5<Esc>");
        app.run(Command::TrackInstrument(Some("drums 1".into())));
        assert_ne!(
            app.buffer().score.tracks[0].kind,
            muse_core::TrackKind::Drums
        );
    }

    #[test]
    fn tool_panes_keep_their_size_and_dock_globally_or_locally() {
        use crate::layout::{Axis, Rect};
        let (config, _) = Config::parse("[ui]\nglobal_tools = [\"tracks\"]\n").unwrap();
        let mut app = App::new(config, None);
        app.run(Command::New);
        app.set_viewport(Rect {
            x: 0,
            y: 0,
            w: 100,
            h: 50,
        });
        let rect = |app: &App, id| app.layout().into_iter().find(|(p, _)| *p == id).unwrap().1;
        // Two score panes side by side share the width equally.
        app.run(Command::VSplit);
        let (left, right) = {
            let mut ids: Vec<_> = app.layout().into_iter().collect();
            ids.sort_by_key(|(_, r)| r.x);
            (ids[0].0, ids[1].0)
        };
        assert_eq!(rect(&app, left).w, rect(&app, right).w);
        // A global tool spans the whole width at its natural height.
        app.run(Command::Tracks(Some(true)));
        let tracks = app.tool_pane(PaneKind::Tracks).unwrap();
        let h = app.fixed_size(tracks, Axis::Stacked).unwrap();
        assert_eq!((rect(&app, tracks).w, rect(&app, tracks).h), (100, h));
        // A local one sits under the score pane it works on, as wide as it.
        app.run(Command::Fretboard(Some(true)));
        let fret = app.tool_pane(PaneKind::Fretboard).unwrap();
        let score = rect(&app, app.focused());
        assert_eq!((rect(&app, fret).x, rect(&app, fret).w), (score.x, score.w));
        // Sizes persist as panes come and go; score panes take up the rest.
        let fret_h = rect(&app, fret).h;
        app.run(Command::Split);
        assert_eq!((rect(&app, tracks).h, rect(&app, fret).h), (h, fret_h));
        app.run(Command::Quit { force: false });
        assert_eq!((rect(&app, tracks).h, rect(&app, fret).h), (h, fret_h));
        // Dragging its border resizes the tool pane itself.
        let d = app
            .dividers()
            .into_iter()
            .find(|d| d.axis == Axis::Stacked && d.pos == rect(&app, tracks).y)
            .unwrap();
        app.drag_divider(&d, d.pos - 5);
        assert_eq!(rect(&app, tracks).h, h + 5);
    }

    #[test]
    fn local_tools_follow_their_score_pane_and_tools_share_a_border() {
        use crate::layout::{Axis, Rect};
        let (config, _) = Config::parse("[ui]\nglobal_tools = []\n").unwrap();
        let mut app = App::new(config, None);
        app.run(Command::New);
        app.set_viewport(Rect {
            x: 0,
            y: 0,
            w: 100,
            h: 60,
        });
        let first = app.focused();
        app.run(Command::Fretboard(Some(true)));
        let fret = app.tool_pane(PaneKind::Fretboard).unwrap();
        // Another score pane becomes the active one; the fretboard still shows the first.
        app.run(Command::VSplit);
        let second = app.focused();
        assert_ne!(first, second);
        assert_eq!(app.tool_score(fret), first);
        // The second gets its own; focusing the first one's makes the first active again.
        app.run(Command::Fretboard(Some(true)));
        let fret2 = app.tool_pane(PaneKind::Fretboard).unwrap();
        assert_ne!(fret, fret2);
        assert_eq!(app.tool_score(fret2), second);
        app.set_focus(fret);
        assert_eq!(app.focused(), first);
        // Closing a score pane closes its local tools.
        app.set_focus(first);
        app.run(Command::Quit { force: false });
        assert!(!app.panes.contains_key(&fret) && app.panes.contains_key(&fret2));

        // Two global tools along the bottom: their shared border moves between them only.
        let mut app = App::new(Config::default(), None);
        app.run(Command::New);
        app.set_viewport(Rect {
            x: 0,
            y: 0,
            w: 100,
            h: 60,
        });
        app.run(Command::Tracks(Some(true)));
        app.run(Command::Piano(Some(true)));
        let rect = |app: &App, id| app.layout().into_iter().find(|(p, _)| *p == id).unwrap().1;
        let (tracks, piano) = (
            app.tool_pane(PaneKind::Tracks).unwrap(),
            app.tool_pane(PaneKind::Piano).unwrap(),
        );
        let score_h = rect(&app, app.focused()).h;
        let (th, ph) = (rect(&app, tracks).h, rect(&app, piano).h);
        let d = app
            .dividers()
            .into_iter()
            .find(|d| d.axis == Axis::Stacked && d.pos == rect(&app, piano).y)
            .unwrap();
        app.drag_divider(&d, d.pos - 2);
        assert_eq!(rect(&app, tracks).h, th - 2);
        assert_eq!(rect(&app, piano).h, ph + 2);
        assert_eq!(rect(&app, app.focused()).h, score_h);
    }

    #[test]
    fn tracks_peek_while_no_tracks_pane() {
        let mut app = App::new(Config::default(), None);
        app.run(Command::New);
        app.run(Command::TrackAdd("guitar".into()));
        assert!(!app.tracks_peeking());
        keys(&mut app, "K");
        assert!(app.tracks_peeking());
        // With a tracks pane, nothing peeks.
        app.run(Command::Tracks(Some(true)));
        assert!(!app.tracks_peeking());
    }

    #[test]
    fn audio_and_master_rows() {
        let mut app = App::new(Config::default(), None);
        app.run(Command::New);
        app.run(Command::TrackAdd("guitar".into()));
        let b = app.pane().buffer;
        app.buffers[b].score.audio = Some(muse_core::AudioSync {
            file: Some("song.mp3".into()),
            ..Default::default()
        });
        let rows = app.track_rows();
        let strips: Vec<Strip> = rows.iter().map(|r| r.strip).collect();
        assert_eq!(
            strips,
            vec![
                Strip::Track(0),
                Strip::Track(1),
                Strip::Audio,
                Strip::Master
            ]
        );
        assert_eq!(rows[2].instrument, "song.mp3");
        let score = |a: &App| a.buffer().score.clone();

        // Soloing the audio silences the synth; muting it turns its solo off.
        app.run(Command::Solo(Some(Strip::Audio)));
        let rows = app.track_rows();
        assert!(rows[2].audible && !rows[0].audible);
        assert_eq!(app.pa.levels(&score(&app)).midi, 0.0);
        app.run(Command::Mute(Some(Strip::Audio)));
        assert!(app.pa.audio_mute && !app.pa.audio_solo);
        assert_eq!(app.pa.levels(&score(&app)).audio, 0.0);
        app.run(Command::Mute(Some(Strip::Audio)));

        // A synth solo silences the audio.
        app.run(Command::Solo(Some(Strip::Track(1))));
        assert_eq!(app.pa.levels(&score(&app)).audio, 0.0);
        assert!(!app.track_rows()[0].audible);
        // The master's solo is on while any synth track is soloed: toggling clears them all.
        assert!(app.track_rows()[3].solo);
        app.run(Command::Solo(Some(Strip::Master)));
        assert!(score(&app).tracks.iter().all(|t| !t.solo));
        // Off, it solos every synth track.
        app.run(Command::Solo(Some(Strip::Master)));
        assert!(score(&app).tracks.iter().all(|t| t.solo));
        // The master's mute mutes all (clearing their solos), then unmutes all.
        app.run(Command::Mute(Some(Strip::Master)));
        assert!(score(&app).tracks.iter().all(|t| t.mute && !t.solo));
        assert!(app.track_rows()[3].mute);
        app.run(Command::Mute(Some(Strip::Master)));
        assert!(score(&app).tracks.iter().all(|t| !t.mute));

        // The master's volume scales only the output.
        let volumes: Vec<u8> = score(&app).tracks.iter().map(|t| t.midi.volume).collect();
        app.set_strip_volume(Strip::Master, 0.5);
        assert!((app.pa.mix.master - 0.75).abs() < 1e-6);
        let after: Vec<u8> = score(&app).tracks.iter().map(|t| t.midi.volume).collect();
        assert_eq!(volumes, after);
        assert_eq!(app.pa.levels(&score(&app)).master, app.pa.mix.master);

        // In the panel, the audio row is selected with j and its keys act on it.
        keys(&mut app, "<C-w>tkkkkjj");
        assert_eq!(app.selected_strip(), Some(Strip::Audio));
        let before = app.pa.mix.audio;
        keys(&mut app, "-m");
        assert!(app.pa.mix.audio < before && app.pa.audio_mute);
    }

    #[test]
    fn the_tracks_panel_lists_tracks_and_takes_the_focus() {
        let mut app = App::new(Config::default(), None);
        app.run(Command::New);
        keys(&mut app, "i5<Esc>");
        app.run(Command::TrackAdd("guitar".into()));
        let rows = app.track_rows();
        // Two tracks, then the synth master.
        assert_eq!(rows.len(), 3);
        assert_eq!(rows[2].strip, Strip::Master);
        assert!(rows[0].bars[0] && !rows[0].bars[1]);
        assert_ne!(rows[0].color, rows[1].color);
        // <C-w>t focuses the panel (showing it) with the pane's track selected; j/k select
        // rows without changing the pane, <CR> shows the selected track; h/l move the bar.
        // It is a pane like any other: the mode stays Normal.
        keys(&mut app, "<C-w>t");
        assert!(app.has_tool(PaneKind::Tracks) && app.focused_kind() == PaneKind::Tracks);
        assert_eq!(app.mode, Mode::Normal);
        assert_eq!(app.tracks_selected, 1);
        keys(&mut app, "ggkll");
        assert_eq!((app.pane().cursor.track, app.pane().cursor.bar), (1, 2));
        assert_eq!(app.selected_strip(), Some(Strip::Track(0)));
        keys(&mut app, "<CR>");
        assert_eq!(app.pane().cursor.track, 0);
        // Down past the tracks: the master row; the pane stays on its track.
        keys(&mut app, "jjj");
        assert_eq!(app.selected_strip(), Some(Strip::Master));
        assert_eq!(app.pane().cursor.track, 0);
        keys(&mut app, "<CR>");
        assert_eq!(app.pane().cursor.track, 0);
        keys(&mut app, "k<CR>");
        assert_eq!(app.pane().cursor.track, 1);
        // Solo, mute and volume act on the selected row.
        keys(&mut app, "s");
        let t = app.pane().cursor.track;
        assert!(app.track_rows()[t].solo);
        assert!(!app.track_rows()[1 - t].audible);
        // Mute turns solo off on the same track.
        keys(&mut app, "m");
        assert!(app.track_rows()[t].mute && !app.track_rows()[t].solo);
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
        // Leaving: Esc focuses the score again; <leader>T hides it.
        keys(&mut app, "<Esc>");
        assert_eq!(app.focused_kind(), PaneKind::Score);
        keys(&mut app, "<Space>T");
        assert!(!app.has_tool(PaneKind::Tracks));
    }
}
