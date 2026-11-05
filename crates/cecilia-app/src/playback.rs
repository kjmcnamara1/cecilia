//! Playback control shared by the frontends: turns [`Effect`]s into player commands, loads
//! audio tracks in the background, records sync taps and reports the playing position.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::mpsc::{Receiver, TryRecvError, channel};

use cecilia_audio::{AudioClip, Location, Player, SYNC_RATE, Sequence, SequenceOptions};
use cecilia_core::SyncPoint;

use crate::{App, Effect};

/// A buffer's decoded audio track.
pub struct LoadedAudio {
    /// At the file's own rate.
    pub clip: Arc<AudioClip>,
    /// Resampled for the output device (made on first play).
    device: Option<Arc<AudioClip>>,
    /// Min/max envelope, 100 per second.
    pub peaks: Arc<Vec<(f32, f32)>>,
    onsets: Option<Vec<f32>>,
}

enum AudioState {
    Loading(Receiver<Result<AudioClip, String>>),
    Ready(LoadedAudio),
    Failed,
}

/// Owns the audio player, per-buffer sequences and audio tracks, and carries out playback
/// [`Effect`]s.
#[derive(Default)]
pub struct Playback {
    player: Option<Player>,
    sequences: HashMap<usize, Arc<Sequence>>,
    loaded: Option<usize>,
    audio: HashMap<usize, AudioState>,
    /// Buffer whose audio is set on the player.
    audio_on_player: Option<usize>,
    /// The tuner's input capture, while the tuner is shown.
    tuner: Option<cecilia_audio::tuner::Tuner>,
}

impl Playback {
    pub fn sequence(&mut self, app: &App, buffer: usize) -> Arc<Sequence> {
        self.sequences
            .entry(buffer)
            .or_insert_with(|| {
                let score = &app.buffers[buffer].score;
                let options = SequenceOptions {
                    audio_sync: score.audio.is_some(),
                    ..Default::default()
                };
                Arc::new(Sequence::build(score, &options))
            })
            .clone()
    }

    fn ensure_player(&mut self, app: &mut App) -> bool {
        if self.player.is_none() {
            let soundfont = app
                .config
                .file
                .audio
                .soundfont
                .clone()
                .or_else(crate::config::find_system_soundfont);
            match Player::new(soundfont.as_deref()) {
                Ok(p) => {
                    if let Some(w) = &p.warning {
                        app.error(format!("{w}; using built-in synth"));
                    }
                    self.player = Some(p);
                    self.loaded = None;
                    self.audio_on_player = None;
                }
                Err(e) => {
                    app.error(e.to_string());
                    return false;
                }
            }
        }
        true
    }

    /// Starts decoding a buffer's audio track if it has one and it is not loaded.
    pub fn ensure_audio(&mut self, app: &App, buffer: usize) {
        if self.audio.contains_key(&buffer) {
            return;
        }
        let b = &app.buffers[buffer];
        let Some(audio) = &b.score.audio else { return };
        let embedded = audio.embedded.clone();
        let file = audio.file.clone();
        let base = b.path.as_ref().and_then(|p| p.parent().map(PathBuf::from));
        let (tx, rx) = channel();
        std::thread::spawn(move || {
            let ext = file
                .as_deref()
                .and_then(|f| f.rsplit('.').next())
                .map(str::to_string);
            let result = match (embedded, file) {
                (Some(bytes), _) => {
                    AudioClip::decode((*bytes).clone(), ext.as_deref()).map_err(|e| e.to_string())
                }
                (None, Some(f)) => {
                    let p = PathBuf::from(&f);
                    let p = if p.is_absolute() {
                        p
                    } else {
                        base.unwrap_or_default().join(p)
                    };
                    AudioClip::load(&p).map_err(|e| e.to_string())
                }
                (None, None) => Err("no audio file".to_string()),
            };
            let _ = tx.send(result);
        });
        self.audio.insert(buffer, AudioState::Loading(rx));
    }

    /// Picks up finished audio loads.
    fn poll_audio(&mut self, app: &mut App) {
        for (buffer, state) in self.audio.iter_mut() {
            if let AudioState::Loading(rx) = state {
                match rx.try_recv() {
                    Ok(Ok(clip)) => {
                        let peaks = Arc::new(clip.peaks(100));
                        app.info(format!("audio loaded: {:.0} s", clip.duration()));
                        *state = AudioState::Ready(LoadedAudio {
                            clip: Arc::new(clip),
                            device: None,
                            peaks,
                            onsets: None,
                        });
                        if app.playing_buffer == Some(*buffer) {
                            self.audio_on_player = None; // set on the player at the next sync
                        }
                    }
                    Ok(Err(e)) => {
                        app.error(format!("audio: {e}"));
                        *state = AudioState::Failed;
                    }
                    Err(TryRecvError::Empty) => {}
                    Err(TryRecvError::Disconnected) => *state = AudioState::Failed,
                }
            }
        }
        // Late-arriving audio joins the current playback.
        if let (Some(b), None) = (app.playing_buffer, self.audio_on_player) {
            if self.player.is_some() && matches!(self.audio.get(&b), Some(AudioState::Ready(_))) {
                self.push_audio(b);
            }
        }
    }

    /// The loaded audio of a buffer, if ready.
    pub fn loaded_audio(&self, buffer: usize) -> Option<&LoadedAudio> {
        match self.audio.get(&buffer) {
            Some(AudioState::Ready(a)) => Some(a),
            _ => None,
        }
    }

    /// Loudness (0–1) of a buffer's audio under each bar, once the audio has loaded.
    pub fn bar_levels(&mut self, app: &App, buffer: usize) -> Option<Vec<f32>> {
        let peaks = self.loaded_audio(buffer)?.peaks.clone();
        let score = &app.buffers.get(buffer)?.score;
        let n = score.master_bars.len();
        let last = n.checked_sub(1)?;
        let end = crate::tracks::bar_ticks(score, last);
        let times: Vec<Option<f64>> = (0..=n)
            .map(|b| {
                if b < n {
                    self.time_of(app, buffer, b, 0)
                } else {
                    self.time_of(app, buffer, last, end)
                }
            })
            .collect();
        Some(
            (0..n)
                .map(|b| match (times[b], times[b + 1]) {
                    (Some(a), Some(z)) => {
                        let (a, z) = ((a * 100.0).max(0.0) as usize, (z * 100.0).max(0.0) as usize);
                        peaks
                            .get(a.min(peaks.len())..z.max(a + 1).min(peaks.len()))
                            .unwrap_or_default()
                            .iter()
                            .map(|&(lo, hi)| lo.abs().max(hi.abs()))
                            .fold(0.0, f32::max)
                    }
                    _ => 0.0,
                })
                .collect(),
        )
    }

    pub fn audio_loading(&self, buffer: usize) -> bool {
        matches!(self.audio.get(&buffer), Some(AudioState::Loading(_)))
    }

    fn push_audio(&mut self, buffer: usize) {
        let Some(player) = &self.player else { return };
        let rate = player.sample_rate;
        let clip = match self.audio.get_mut(&buffer) {
            Some(AudioState::Ready(a)) => Some(
                a.device
                    .get_or_insert_with(|| Arc::new(a.clip.resampled(rate)))
                    .clone(),
            ),
            _ => None,
        };
        player.set_audio(clip);
        self.audio_on_player = Some(buffer);
    }

    fn push_settings(&mut self, app: &App) {
        let Some(buffer) = app.playing_buffer.or(Some(app.pane().buffer)) else {
            return;
        };
        let Some(score) = app.buffers.get(buffer).map(|b| &b.score) else {
            return;
        };
        let seq = self.sequences.get(&buffer).cloned();
        let Some(player) = &self.player else { return };
        player.set_speed(app.config.file.audio.speed);
        player.set_mix(app.pa.levels(score));
        player.set_metronome(app.pa.metronome);
        player.set_transpose(app.pa.transpose);
        player.set_trainer(
            app.pa
                .trainer
                .map(|(step, target)| cecilia_audio::Trainer { step, target }),
        );
        let range = app.pa.loop_bars.and_then(|(a, b)| {
            let seq = seq?;
            let start = seq.time_of(Location { bar: a, tick: 0 })?;
            let pb = seq.bars().iter().find(|x| x.bar == b)?;
            Some((start, seq.perf_time(pb.start + pb.ticks as u64)))
        });
        player.set_loop(range);
    }

    pub fn apply(&mut self, app: &mut App, effect: Effect) {
        match effect {
            Effect::Play { buffer, bar, tick } | Effect::Stop { buffer, bar, tick } => {
                let play = matches!(effect, Effect::Play { .. });
                if !self.ensure_player(app) {
                    return;
                }
                self.ensure_audio(app, buffer);
                self.poll_audio(app);
                let seq = self.sequence(app, buffer);
                if self.loaded != Some(buffer) {
                    self.player.as_ref().unwrap().load(seq.clone());
                    self.loaded = Some(buffer);
                }
                if self.audio_on_player != Some(buffer) {
                    self.push_audio(buffer);
                    if self.audio_loading(buffer) {
                        app.info("loading audio…");
                        self.audio_on_player = None;
                    }
                }
                self.push_settings(app);
                let player = self.player.as_ref().unwrap();
                player.seek(seq.time_of(Location { bar, tick }).unwrap_or(0.0));
                if play {
                    player.play(app.pa.count_in);
                    app.playing = true;
                } else {
                    player.pause();
                    app.playing = false;
                }
            }
            Effect::Seek { buffer, bar, tick } => {
                if self.player.is_some() && self.loaded == Some(buffer) {
                    let seq = self.sequence(app, buffer);
                    let time = seq.time_of(Location { bar, tick }).unwrap_or(0.0);
                    self.player.as_ref().unwrap().seek(time);
                } else if app.playing {
                    self.apply(app, Effect::Play { buffer, bar, tick });
                }
            }
            Effect::Resume => {
                match (&self.player, app.playing_buffer) {
                    (Some(p), Some(b)) if self.loaded == Some(b) => p.play(false),
                    _ => {
                        // Nothing loaded to resume: start from the cursor.
                        let pane = app.pane().clone();
                        let tick =
                            crate::cursor_tick(&app.buffers[pane.buffer].score, &pane.cursor);
                        self.apply(
                            app,
                            Effect::Play {
                                buffer: pane.buffer,
                                bar: pane.cursor.bar,
                                tick,
                            },
                        );
                    }
                }
            }
            Effect::Pause => {
                if let Some(p) = &self.player {
                    p.pause();
                }
                app.playing = false;
            }
            Effect::ConfigReloaded => {
                // Recreate the player on next use so a changed SoundFont takes effect.
                if !app.playing {
                    self.player = None;
                }
            }
            Effect::ScoreChanged { buffer } => {
                self.sequences.remove(&buffer);
                if self.loaded == Some(buffer) {
                    if app.playing || app.paused {
                        // Edits made while playing are heard at once: the player swaps in the
                        // new sequence and keeps its position.
                        let seq = self.sequence(app, buffer);
                        if let Some(p) = &self.player {
                            p.load(seq);
                        }
                    } else {
                        self.loaded = None;
                    }
                }
            }
            Effect::AudioChanged { buffer } => {
                self.sequences.remove(&buffer);
                self.audio.remove(&buffer);
                if self.loaded == Some(buffer) {
                    self.loaded = None;
                }
                if self.audio_on_player == Some(buffer) {
                    if let Some(p) = &self.player {
                        p.set_audio(None);
                    }
                    self.audio_on_player = None;
                }
                self.ensure_audio(app, buffer);
            }
            Effect::PlaybackSettings => self.push_settings(app),
            // Handled by the frontend (see `dialog::apply`).
            Effect::OpenDialog { .. } | Effect::SaveDialog { .. } => {}
            Effect::SyncTap => self.sync_tap(app),
            Effect::SyncDetect => self.sync_detect(app),
            Effect::Tuner(on) => {
                self.tuner = None;
                if on {
                    match cecilia_audio::tuner::Tuner::start() {
                        Ok(t) => {
                            app.info(format!("tuner listening to {}", t.device));
                            self.tuner = Some(t);
                        }
                        Err(e) => {
                            app.tuner_error = Some(e.to_string());
                            app.error(format!("tuner: {e}"));
                        }
                    }
                }
            }
        }
    }

    fn sync_tap(&mut self, app: &mut App) {
        let (Some(buffer), Some(player)) = (app.playing_buffer, self.player.as_ref()) else {
            return app.error("start playback with the audio first (:sync)");
        };
        let Some(index) = app.pa.sync_next else {
            return;
        };
        let t = player.position();
        let seq = self.sequence(app, buffer);
        let Some(pb) = seq.bars().get(index).copied() else {
            app.pa.sync_next = None;
            return app.info("sync: end of score");
        };
        let occurrence = seq.bars()[..index]
            .iter()
            .filter(|b| b.bar == pb.bar)
            .count() as u32;
        let pad = app.buffers[buffer]
            .score
            .audio
            .as_ref()
            .map_or(0, |a| a.frame_padding);
        let frame = (t * SYNC_RATE).round() as i64 + pad;
        app.add_sync_point(SyncPoint {
            bar: pb.bar,
            occurrence,
            frame,
            tempo: 0.0,
        });
        app.pa.sync_next = Some(index + 1);
        app.info(format!("sync: bar {} at {:.2} s", pb.bar + 1, t));
    }

    fn sync_detect(&mut self, app: &mut App) {
        let buffer = app.pane().buffer;
        self.ensure_audio(app, buffer);
        let seq = self.sequence(app, buffer);
        let Some(AudioState::Ready(audio)) = self.audio.get_mut(&buffer) else {
            return app.error("audio is not loaded yet (try again in a moment)");
        };
        let onsets = audio.onsets.get_or_insert_with(|| audio.clip.onsets());
        let score = &app.buffers[buffer].score;
        let pad = score.audio.as_ref().map_or(0, |a| a.frame_padding);
        let pad_secs = pad as f64 / SYNC_RATE;
        let beat_len = 60.0 / score.tempo.max(1) as f64;
        let Some((period, phase)) = cecilia_audio::beats::estimate_beats(onsets, beat_len) else {
            return app.error("sync-detect: the audio is too short");
        };
        // Start from the last existing point, else the first beat.
        let perf = &seq.perf;
        let existing = score
            .audio
            .as_ref()
            .map(|a| a.points.clone())
            .unwrap_or_default();
        let anchor = existing
            .iter()
            .filter_map(|p| {
                let i = perf
                    .bars
                    .iter()
                    .enumerate()
                    .filter(|(_, b)| b.bar == p.bar)
                    .nth(p.occurrence as usize)
                    .map(|(i, _)| i)?;
                Some((i, p.frame as f64 / SYNC_RATE - pad_secs))
            })
            .max_by_key(|a| a.0)
            .unwrap_or((0, phase));
        // Follow the beat grid from the anchor, snapping each bar to a nearby onset; the grid
        // itself only drifts by part of each correction, so one bad snap doesn't carry over.
        let mut points = Vec::new();
        let mut t = anchor.1;
        for (i, pb) in perf.bars.iter().enumerate().skip(anchor.0) {
            if i > anchor.0 {
                let snapped = cecilia_audio::beats::snap(onsets, t, 0.07);
                t += (snapped - t) * 0.3;
                let occurrence = perf.bars[..i].iter().filter(|b| b.bar == pb.bar).count() as u32;
                points.push(SyncPoint {
                    bar: pb.bar,
                    occurrence,
                    frame: ((snapped + pad_secs) * SYNC_RATE).round() as i64,
                    tempo: 0.0,
                });
            }
            let ts = score.master_bars[pb.bar].time_signature;
            let beats = pb.ticks as f64
                / (cecilia_core::TICKS_PER_QUARTER as f64 * 4.0 / ts.denominator.max(1) as f64);
            let beat = period * (4.0 / ts.denominator.max(1) as f64);
            t += beats * beat;
            if t > audio.clip.duration() {
                break;
            }
        }
        app.set_detected_sync(points);
        self.sequences.remove(&buffer);
        self.loaded = None;
    }

    /// Updates `app.playing` from the player and picks up loaded audio.
    pub fn sync(&mut self, app: &mut App) {
        if let Some(p) = &self.player {
            app.set_playing(p.is_playing());
        }
        if let Some(t) = &self.tuner {
            app.tuner_reading = t.reading();
        }
        self.poll_audio(app);
    }

    /// Current playback speed (the trainer changes it while looping).
    pub fn speed(&self) -> Option<f64> {
        self.player.as_ref().map(Player::speed)
    }

    /// Current playback location for the playing buffer: (buffer, location, time, duration).
    pub fn location(&self, app: &App) -> Option<(usize, Location, f64, f64)> {
        let buffer = app.playing_buffer?;
        let player = self.player.as_ref()?;
        let seq = self.sequences.get(&buffer)?;
        let t = player.position();
        Some((buffer, seq.location_at(t)?, t, seq.duration))
    }

    /// Audio-timeline position of a written position (for waveform display).
    pub fn time_of(&mut self, app: &App, buffer: usize, bar: usize, tick: u32) -> Option<f64> {
        self.sequence(app, buffer).time_of(Location { bar, tick })
    }
}
