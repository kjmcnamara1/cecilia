//! Play-along: attaching audio, the mixer, loops, metronome, count-in, speed trainer and sync
//! points. Audio itself is handled by the frontend's [`crate::playback::Playback`]; this module
//! keeps the settings and edits the score's sync data.

use crate::config::Mode;
use std::path::{Path, PathBuf};

use cecilia_core::*;

use crate::command::{Command, Strip};
use crate::{App, Effect};

#[derive(Debug, Clone)]
pub struct PlayAlong {
    pub mix: cecilia_audio::Mix,
    /// Saved levels while a source is switched off by `:mix`.
    pub mode: MixMode,
    pub metronome: bool,
    pub count_in: bool,
    /// Audio pitch shift in semitones.
    pub transpose: f32,
    /// Looped bars (0-based, inclusive).
    pub loop_bars: Option<(usize, usize)>,
    /// Speed trainer: step and target speed (fractions).
    pub trainer: Option<(f64, f64)>,
    /// Played-bar index the next sync tap assigns, while tapping.
    /// Taps made on a modern score, for undo: the beat (id) and the frame its point had before.
    pub beat_trail: Vec<(u32, Option<i64>)>,
    /// A chant score's tapping session (see [`crate::chantsync`]).
    pub chant_tap: Option<crate::chantsync::ChantTap>,
    /// What one tap syncs in a chant score.
    pub sync_unit: crate::chantsync::SyncUnit,
    /// The audio track is muted / soloed (a track at most one of the two).
    pub audio_mute: bool,
    pub audio_solo: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MixMode {
    Both,
    Audio,
    Midi,
}

impl Default for PlayAlong {
    fn default() -> Self {
        Self {
            mix: cecilia_audio::Mix::default(),
            mode: MixMode::Both,
            metronome: false,
            count_in: false,
            transpose: 0.0,
            loop_bars: None,
            trainer: None,
            beat_trail: Vec::new(),
            chant_tap: None,
            sync_unit: Default::default(),
            audio_mute: false,
            audio_solo: false,
        }
    }
}

impl PlayAlong {
    /// Status-line summary: speed, mix, metronome, count-in, loop, pitch and sync tapping.
    pub fn status(&self, speed: f64, has_audio: bool) -> String {
        let mix = if has_audio {
            format!("  mix:{}", format!("{:?}", self.mode).to_lowercase())
        } else {
            String::new()
        };
        format!("{:.0}%{mix}{}", speed * 100.0, self.flags())
    }

    /// The play-along settings that are on (click, count-in, loop, pitch, sync tap), each
    /// after two spaces.
    pub fn flags(&self) -> String {
        let mut s = String::new();
        if self.metronome {
            s.push_str("  click");
        }
        if self.count_in {
            s.push_str("  count-in");
        }
        if let Some((a, b)) = self.loop_bars {
            s.push_str(&format!("  loop {}–{}", a + 1, b + 1));
            if let Some((step, target)) = self.trainer {
                s.push_str(&format!(" (+{:.0}%→{:.0}%)", step * 100.0, target * 100.0));
            }
        }
        if self.transpose != 0.0 {
            s.push_str(&format!("  pitch {:+}", self.transpose));
        }
        if let Some(t) = &self.chant_tap {
            match t.next {
                Some(_) => s.push_str(&format!("  SYNC TAP → {}", t.label)),
                None => s.push_str("  SYNC TAP end"),
            }
        }
        s
    }

    /// Levels sent to the player, after the mix mode and the audio track's mute and solo: a
    /// solo anywhere silences what is not soloed (the synth tracks' own solos are applied by
    /// the sequence).
    pub fn levels(&self, score: &Score) -> cecilia_audio::Mix {
        let has_audio = score.audio.is_some();
        let mut m = self.mix;
        if has_audio {
            let synth_solo = score.tracks.iter().any(|t| t.solo);
            if self.audio_mute || (synth_solo && !self.audio_solo) {
                m.audio = 0.0;
            }
            if self.audio_solo && !synth_solo {
                m.midi = 0.0;
            }
        }
        match self.mode {
            MixMode::Audio if has_audio => m.midi = 0.0,
            MixMode::Midi => m.audio = 0.0,
            // With audio playing, MIDI defaults to a quieter guide.
            MixMode::Both if has_audio => m.midi *= 0.6,
            _ => {}
        }
        m
    }
}

pub(crate) const AUDIO_EXTENSIONS: &[&str] = &["mp3", "m4a", "aac", "flac", "ogg", "wav", "opus"];

fn words(s: &str) -> Vec<String> {
    s.to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| {
            w.len() > 1
                && !matches!(
                    *w,
                    "the"
                        | "pro"
                        | "guitar"
                        | "ver"
                        | "official"
                        | "video"
                        | "music"
                        | "gp"
                        | "tab"
                        | "and"
                )
        })
        .map(String::from)
        .collect()
}

/// The audio file next to a score (or in an `mp3`/`audio` subfolder) whose name best matches.
pub fn find_audio_for(score_path: &Path) -> Option<PathBuf> {
    let dir = score_path.parent()?;
    let stem = score_path.file_stem()?.to_string_lossy().to_string();
    let target = words(&stem);
    if target.is_empty() {
        return None;
    }
    let mut best: Option<(f64, PathBuf)> = None;
    for d in [
        dir.to_path_buf(),
        dir.join("mp3"),
        dir.join("audio"),
        dir.join("Audio"),
    ] {
        let Ok(entries) = std::fs::read_dir(&d) else {
            continue;
        };
        for e in entries.flatten() {
            let p = e.path();
            let ext = p
                .extension()
                .and_then(|x| x.to_str())
                .unwrap_or_default()
                .to_lowercase();
            if !AUDIO_EXTENSIONS.contains(&ext.as_str()) {
                continue;
            }
            let name = words(&p.file_stem().unwrap_or_default().to_string_lossy());
            let common = target.iter().filter(|w| name.contains(w)).count() as f64;
            let score = common / (target.len().max(name.len()) as f64);
            if score >= 0.5 && best.as_ref().is_none_or(|b| score > b.0) {
                best = Some((score, p));
            }
        }
    }
    best.map(|b| b.1)
}

/// The audio file a score names, if it is inside the score's folder: an absolute path, one
/// climbing out with `..` or with a drive `:` (which a downloaded score could use to read any
/// file) is refused. Windows-style separators in the stored name are accepted. Without a score
/// file the name is relative to the working folder.
pub(crate) fn score_audio_path(score_path: Option<&Path>, name: &str) -> Option<PathBuf> {
    use std::path::Component;
    let rel: PathBuf = name.split(['/', '\\']).filter(|p| !p.is_empty()).collect();
    if name.starts_with(['/', '\\'])
        || name.contains(':')
        || rel
            .components()
            .any(|c| !matches!(c, Component::Normal(_) | Component::CurDir))
    {
        return None;
    }
    let base = score_path.and_then(Path::parent).unwrap_or(Path::new("."));
    Some(base.join(rel))
}

fn relative(path: &Path, base: Option<&Path>) -> String {
    base.and_then(|b| b.parent())
        .and_then(|d| path.strip_prefix(d).ok())
        .unwrap_or(path)
        .to_string_lossy()
        .to_string()
}

impl App {
    pub(crate) fn run_playalong(&mut self, cmd: &Command) -> bool {
        use Command::*;
        match cmd {
            Audio(arg) => self.attach_audio(arg.as_deref()),
            MixMode(m) => {
                self.pa.mode = match m.as_deref() {
                    Some("both") => self::MixMode::Both,
                    Some("audio") => self::MixMode::Audio,
                    Some("midi") => self::MixMode::Midi,
                    Some(other) => {
                        self.error(format!("mix: unknown mode '{other}' (both, audio, midi)"));
                        return true;
                    }
                    None => match self.pa.mode {
                        self::MixMode::Both => self::MixMode::Audio,
                        self::MixMode::Audio => self::MixMode::Midi,
                        self::MixMode::Midi => self::MixMode::Both,
                    },
                };
                self.info(format!(
                    "mix: {}",
                    format!("{:?}", self.pa.mode).to_lowercase()
                ));
                self.effects.push(Effect::PlaybackSettings);
            }
            Metronome => {
                self.pa.metronome = !self.pa.metronome;
                self.info(if self.pa.metronome {
                    "metronome on"
                } else {
                    "metronome off"
                });
                self.effects.push(Effect::PlaybackSettings);
            }
            CountIn => {
                self.pa.count_in = !self.pa.count_in;
                self.info(if self.pa.count_in {
                    "count-in on"
                } else {
                    "count-in off"
                });
            }
            SpeedStep(d) => {
                let pct =
                    ((self.config.file.audio.speed * 100.0).round() as i32 + d).clamp(25, 200);
                self.config.file.audio.speed = pct as f64 / 100.0;
                self.info(format!("speed {pct}%"));
                self.effects.push(Effect::PlaybackSettings);
            }
            Speed(_) => {
                return false;
            }
            LoopToggle => {
                if self.pa.loop_bars.is_some() {
                    self.pa.loop_bars = None;
                    self.info("loop off");
                } else {
                    let c = self.pane().cursor;
                    let (a, b) = match self.selection() {
                        Some((anchor, cursor, _)) => {
                            (anchor.bar.min(cursor.bar), anchor.bar.max(cursor.bar))
                        }
                        None => (c.bar, c.bar),
                    };
                    if self.mode == crate::Mode::Visual {
                        self.run(Normal);
                    }
                    self.pa.loop_bars = Some((a, b));
                    self.info(format!("loop bars {}–{}", a + 1, b + 1));
                }
                self.effects.push(Effect::PlaybackSettings);
            }
            Loop(range) => {
                let n = self.buffer().score.master_bars.len();
                self.pa.loop_bars = range.map(|(a, b)| {
                    (
                        a.saturating_sub(1).min(n.saturating_sub(1)),
                        b.saturating_sub(1).min(n.saturating_sub(1)),
                    )
                });
                match self.pa.loop_bars {
                    Some((a, b)) => self.info(format!("loop bars {}–{}", a + 1, b + 1)),
                    None => self.info("loop off"),
                }
                self.effects.push(Effect::PlaybackSettings);
            }
            Trainer(t) => {
                match t {
                    Some((start, target, step)) => {
                        self.config.file.audio.speed = *start as f64 / 100.0;
                        self.pa.trainer = Some((*step as f64 / 100.0, *target as f64 / 100.0));
                        if self.pa.loop_bars.is_none() {
                            self.run(LoopToggle);
                        }
                        self.info(format!(
                            "trainer: {start}% → {target}% in steps of {step}% per loop"
                        ));
                    }
                    None => {
                        self.pa.trainer = None;
                        self.info("trainer off");
                    }
                }
                self.effects.push(Effect::PlaybackSettings);
            }
            AudioPitch(semitones) => {
                self.pa.transpose = semitones.clamp(-12.0, 12.0);
                self.info(format!("audio pitch {:+} semitones", self.pa.transpose));
                self.effects.push(Effect::PlaybackSettings);
            }
            Sync => {
                // Sync mode: the waveform shows the sync points, Space plays and pauses,
                // Enter syncs the cursor's cell (or the bar being played) and moves on, Esc
                // leaves.
                if self.syncing {
                    self.leave_sync_mode();
                    return true;
                }
                if self.buffer().score.audio.is_none() {
                    self.error("no audio attached (use :audio FILE)");
                    return true;
                }
                if self.buffer().score.is_chant() {
                    if !self.chant_tap_start() {
                        return true;
                    }
                } else {
                    self.pa.beat_trail.clear();
                    self.info(
                        "sync: Enter syncs the cursor beat and moves on, Space plays, Esc leaves",
                    );
                }
                if !self.has_tool(crate::app::PaneKind::Audio) {
                    self.set_tool(crate::app::PaneKind::Audio, Some(true));
                }
                self.syncing = true;
                self.mode = Mode::Sync;
            }
            SyncTap => {
                if self.buffer().score.is_chant() {
                    if self.pa.chant_tap.is_none() && !self.chant_tap_start() {
                        return true;
                    }
                    self.effects.push(Effect::SyncTap);
                    return true;
                }
                if self.beat_sync_ready() {
                    self.effects.push(Effect::SyncTap);
                }
            }
            SyncClear => {
                self.edit(|cx| {
                    if let Some(a) = cx.whole().audio.as_mut() {
                        a.points.clear();
                        a.chant_points.clear();
                        a.beat_points.clear();
                        a.note_anchors.clear();
                    }
                    Ok(())
                });
                if let Some(t) = self.pa.chant_tap.as_mut() {
                    t.trail.clear();
                }
                self.info("sync points cleared");
            }
            SyncDetect => self.effects.push(Effect::SyncDetect),
            SyncUnit(unit) => {
                self.pa.sync_unit = *unit;
                self.info(format!("sync unit: {}", unit.name()));
            }
            SyncUndo if self.buffer().score.is_chant() => self.chant_sync_undo(),
            SyncUndo => self.beat_sync_undo(),
            SyncRemove => {
                let c = self.pane().cursor;
                let (from, to) = match self.selection() {
                    Some((a, b, bars)) => {
                        let (a, b) = ((a.bar, a.beat), (b.bar, b.beat));
                        let (f, t) = if a <= b { (a, b) } else { (b, a) };
                        if bars {
                            ((f.0, 0), (t.0, usize::MAX))
                        } else {
                            (f, t)
                        }
                    }
                    None => ((c.bar, c.beat), (c.bar, c.beat)),
                };
                self.sync_operate(crate::command::Operator::Delete, from, to);
            }
            SyncHere => {
                let ready = if self.buffer().score.is_chant() {
                    self.chant_sync_ready()
                } else {
                    self.beat_sync_ready()
                };
                if ready {
                    self.effects.push(Effect::SyncHere);
                }
            }
            SyncNudge(ms) if self.buffer().score.is_chant() => self.chant_sync_nudge(*ms),
            SyncNudge(ms) => self.beat_sync_nudge(*ms),
            Waveform => {
                let pane = self.pane_mut();
                pane.waveform = !pane.waveform;
                let on = pane.waveform;
                self.info(if on {
                    "waveform on in this pane"
                } else {
                    "waveform off in this pane"
                });
            }
            AudioPane(on) => {
                self.set_tool(crate::app::PaneKind::Audio, *on);
            }
            Volume(src, pct) => {
                let v = (*pct as f32 / 100.0).clamp(0.0, 1.5);
                match src.as_str() {
                    "midi" => self.pa.mix.midi = v,
                    "audio" => self.pa.mix.audio = v,
                    "metronome" | "click" => self.pa.mix.metronome = v,
                    "master" => self.pa.mix.master = v,
                    other => {
                        self.error(format!(
                            "volume: unknown source '{other}' (midi, audio, metronome, master)"
                        ));
                        return true;
                    }
                }
                self.effects.push(Effect::PlaybackSettings);
            }
            Mute(target) | Solo(target) => {
                // A row is soloed, muted or neither: turning one on turns the other off.
                let solo = matches!(cmd, Solo(_));
                // In the tracks panel, the selected row; else the cursor's track.
                let target = target
                    .or_else(|| self.selected_strip())
                    .unwrap_or(Strip::Track(self.pane().cursor.track));
                match target {
                    Strip::Audio => {
                        if self.buffer().score.audio.is_none() {
                            self.error("no audio track (attach one with :audio)");
                            return true;
                        }
                        let pa = &mut self.pa;
                        if solo {
                            pa.audio_solo = !pa.audio_solo;
                            pa.audio_mute &= !pa.audio_solo;
                        } else {
                            pa.audio_mute = !pa.audio_mute;
                            pa.audio_solo &= !pa.audio_mute;
                        }
                    }
                    Strip::Track(t) => {
                        self.edit(|cx| {
                            let track = cx.whole().tracks.get_mut(t).ok_or("no such track")?;
                            if solo {
                                track.solo = !track.solo;
                                track.mute &= !track.solo;
                            } else {
                                track.mute = !track.mute;
                                track.solo &= !track.mute;
                            }
                            Ok(())
                        });
                    }
                    // The master: mute all synth tracks unless all are muted (then unmute
                    // them); solo all unless any is soloed (then clear the solos).
                    Strip::Master => {
                        self.edit(|cx| {
                            let tracks = &mut cx.whole().tracks;
                            if solo {
                                let on = !tracks.iter().any(|t| t.solo);
                                for t in tracks.iter_mut() {
                                    t.solo = on;
                                    t.mute &= !on;
                                }
                            } else {
                                let on = !tracks.iter().all(|t| t.mute);
                                for t in tracks.iter_mut() {
                                    t.mute = on;
                                    t.solo &= !on;
                                }
                            }
                            Ok(())
                        });
                    }
                }
                self.effects.push(Effect::PlaybackSettings);
            }
            _ => return false,
        }
        true
    }

    fn attach_audio(&mut self, arg: Option<&str>) {
        let index = self.pane().buffer;
        let score_path = self.buffers[index].path.clone();
        if arg == Some("off") {
            self.edit(|cx| {
                cx.whole().audio = None;
                Ok(())
            });
            self.effects.push(Effect::AudioChanged { buffer: index });
            return self.info("audio detached");
        }
        let path = match arg {
            Some(a) => {
                let p = PathBuf::from(a.strip_prefix("~/").map_or(a.to_string(), |rest| {
                    format!("{}/{rest}", std::env::var("HOME").unwrap_or_default())
                }));
                if p.is_absolute() {
                    p
                } else {
                    score_path
                        .as_deref()
                        .and_then(Path::parent)
                        .map_or(p.clone(), |d| d.join(&p))
                }
            }
            None => match score_path.as_deref().and_then(find_audio_for) {
                Some(p) => p,
                None => {
                    return self
                        .error("no matching audio file found next to the score (use :audio FILE)");
                }
            },
        };
        if !path.is_file() {
            return self.error(format!("{}: no such file", path.display()));
        }
        let file = relative(&path, score_path.as_deref());
        if score_audio_path(score_path.as_deref(), &file).is_none() {
            return self.error(format!(
                "audio: {} must be in the score's folder (or below it); save the score first if it has no file",
                path.display()
            ));
        }
        self.edit(|cx| {
            let s = cx.whole();
            let keep = s
                .audio
                .as_ref()
                .filter(|a| a.file.as_deref() == Some(file.as_str()) && a.embedded.is_none())
                .map(|a| {
                    (
                        a.points.clone(),
                        a.chant_points.clone(),
                        a.beat_points.clone(),
                        a.frame_padding,
                    )
                });
            let (points, chant_points, beat_points, frame_padding) = keep.unwrap_or_default();
            s.audio = Some(AudioSync {
                file: Some(file.clone()),
                frame_padding,
                points,
                chant_points,
                beat_points,
                note_anchors: Vec::new(),
                embedded: None,
            });
            s.refresh_chant_sync();
            s.refresh_beat_sync();
            Ok(())
        });
        self.effects.push(Effect::AudioChanged { buffer: index });
        self.info(format!(
            "audio: {} (tap sync with :sync, or :sync-detect)",
            path.display()
        ));
    }

    /// Records a sync point and recomputes the tempo of every point from its neighbours.
    pub fn add_sync_point(&mut self, point: SyncPoint) {
        self.edit(|cx| {
            let score = cx.whole();
            let perf = Performance::build(score, &[]);
            let tempo = score.tempo as f64;
            let Some(audio) = score.audio.as_mut() else {
                return Err("no audio attached".into());
            };
            audio
                .points
                .retain(|p| !(p.bar == point.bar && p.occurrence == point.occurrence));
            audio.points.push(point);
            retime(audio, &perf, tempo);
            Ok(())
        });
    }

    /// Replaces sync points from `first` (played-bar index) on with detected ones.
    pub fn set_detected_sync(&mut self, points: Vec<SyncPoint>) {
        let n = points.len();
        self.edit(|cx| {
            let score = cx.whole();
            let perf = Performance::build(score, &[]);
            let tempo = score.tempo as f64;
            let Some(audio) = score.audio.as_mut() else {
                return Err("no audio attached".into());
            };
            for p in points {
                audio
                    .points
                    .retain(|q| !(q.bar == p.bar && q.occurrence == p.occurrence));
                audio.points.push(p);
            }
            retime(audio, &perf, tempo);
            Ok(())
        });
        self.info(format!("{n} sync points from beat detection (u to undo)"));
    }
}

/// Played-bar index of a sync point.
fn played_index(perf: &Performance, p: &SyncPoint) -> Option<usize> {
    perf.bars
        .iter()
        .enumerate()
        .filter(|(_, b)| b.bar == p.bar)
        .nth(p.occurrence as usize)
        .map(|(i, _)| i)
}

/// Sorts points in playing order and sets each one's tempo to reach the next point.
fn retime(audio: &mut AudioSync, perf: &Performance, default_tempo: f64) {
    audio
        .points
        .sort_by_key(|p| played_index(perf, p).unwrap_or(usize::MAX));
    let ticks: Vec<Option<u64>> = audio
        .points
        .iter()
        .map(|p| played_index(perf, p).map(|i| perf.bars[i].start))
        .collect();
    let mut last = default_tempo;
    for i in 0..audio.points.len() {
        let next = audio.points.get(i + 1).copied();
        if let (Some(t0), Some(Some(t1)), Some(n)) = (ticks[i], ticks.get(i + 1), next) {
            let secs = (n.frame - audio.points[i].frame) as f64 / cecilia_audio::SYNC_RATE;
            if secs > 0.0 && *t1 > t0 {
                last = (*t1 - t0) as f64 / TICKS_PER_QUARTER as f64 * 60.0 / secs;
            }
        }
        audio.points[i].tempo = last;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_audio_names() {
        let dir = std::env::temp_dir().join(format!("cecilia-audio-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("mp3")).unwrap();
        for f in [
            "Cake - Love You Madly.mp3",
            "Cake - Short Skirt.mp3",
            "notes.txt",
        ] {
            std::fs::write(dir.join("mp3").join(f), b"").unwrap();
        }
        let found = find_audio_for(&dir.join("Cake - Love You Madly (Pro).gp5")).unwrap();
        assert!(found.ends_with("Cake - Love You Madly.mp3"), "{found:?}");
        assert!(find_audio_for(&dir.join("Metallica - One.gp5")).is_none());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn retime_sets_tempos_between_points() {
        let score = Score {
            tempo: 100,
            master_bars: vec![MasterBar::default(); 4],
            tracks: vec![],
            ..Score::default()
        };
        let perf = Performance::build(&score, &[]);
        let mut audio = AudioSync {
            points: vec![
                SyncPoint {
                    bar: 2,
                    occurrence: 0,
                    frame: 4 * 44_100,
                    tempo: 0.0,
                },
                SyncPoint {
                    bar: 0,
                    occurrence: 0,
                    frame: 0,
                    tempo: 0.0,
                },
            ],
            ..AudioSync::default()
        };
        retime(&mut audio, &perf, 100.0);
        // Two 4/4 bars in 4 s = 120 bpm.
        assert_eq!(audio.points[0].bar, 0);
        assert!((audio.points[0].tempo - 120.0).abs() < 1e-9);
        assert!((audio.points[1].tempo - 120.0).abs() < 1e-9);
    }

    #[test]
    fn a_score_names_audio_only_inside_its_folder() {
        let score = Path::new("/music/song.gp5");
        let ok = |n: &str| score_audio_path(Some(score), n);
        assert_eq!(ok("a.mp3"), Some(PathBuf::from("/music/a.mp3")));
        assert_eq!(ok("mp3/a.mp3"), Some(PathBuf::from("/music/mp3/a.mp3")));
        assert_eq!(ok("mp3\\a.mp3"), Some(PathBuf::from("/music/mp3/a.mp3")));
        assert_eq!(ok("./a.mp3"), Some(PathBuf::from("/music/a.mp3")));
        for bad in [
            "/etc/passwd",
            "\\\\host\\share",
            "../a.mp3",
            "x/../../a.mp3",
            "C:a.mp3",
            "C:\\a.mp3",
        ] {
            assert_eq!(ok(bad), None, "{bad}");
        }
    }

    #[test]
    fn attaching_audio_from_outside_the_score_folder_is_refused() {
        let base = std::env::temp_dir().join(format!("cecilia-audio-out-{}", std::process::id()));
        let (score_dir, other) = (base.join("score"), base.join("other"));
        std::fs::create_dir_all(&score_dir).unwrap();
        std::fs::create_dir_all(&other).unwrap();
        std::fs::write(other.join("a.mp3"), "x").unwrap();
        std::fs::write(score_dir.join("b.mp3"), "x").unwrap();
        let mut app = App::new(crate::Config::default(), None);
        app.run(Command::New);
        app.run(Command::Write(Some(score_dir.join("s.ccm"))));
        app.run(Command::Audio(Some(
            other.join("a.mp3").display().to_string(),
        )));
        assert!(app.last_notification().unwrap().error);
        assert!(app.buffer().score.audio.is_none());
        app.run(Command::Audio(Some("b.mp3".into())));
        assert_eq!(
            app.buffer().score.audio.as_ref().unwrap().file.as_deref(),
            Some("b.mp3")
        );
        std::fs::remove_dir_all(base).ok();
    }
}
