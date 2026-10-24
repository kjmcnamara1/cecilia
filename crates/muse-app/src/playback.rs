//! Playback control shared by the frontends: turns [`Effect`]s into player commands and reports
//! the playing position.

use std::collections::HashMap;
use std::sync::Arc;

use muse_audio::{Location, Player, Sequence, SequenceOptions};

use crate::{App, Effect};

/// Owns the audio player and per-buffer sequences, and carries out playback [`Effect`]s.
#[derive(Default)]
pub struct Playback {
    player: Option<Player>,
    sequences: HashMap<usize, Arc<Sequence>>,
    loaded: Option<usize>,
}

impl Playback {
    fn sequence(&mut self, app: &App, buffer: usize) -> Arc<Sequence> {
        self.sequences
            .entry(buffer)
            .or_insert_with(|| {
                Arc::new(Sequence::build(
                    &app.buffers[buffer].score,
                    &SequenceOptions::default(),
                ))
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
                }
                Err(e) => {
                    app.error(e.to_string());
                    return false;
                }
            }
        }
        true
    }

    pub fn apply(&mut self, app: &mut App, effect: Effect) {
        match effect {
            Effect::Play { buffer, bar, tick } | Effect::Stop { buffer, bar, tick } => {
                let play = matches!(effect, Effect::Play { .. });
                if !self.ensure_player(app) {
                    return;
                }
                let seq = self.sequence(app, buffer);
                let player = self.player.as_ref().unwrap();
                if self.loaded != Some(buffer) {
                    player.load(seq.clone());
                    self.loaded = Some(buffer);
                }
                player.set_speed(app.config.file.audio.speed);
                player.seek(seq.time_of(Location { bar, tick }).unwrap_or(0.0));
                if play {
                    player.play();
                    app.playing = true;
                } else {
                    player.pause();
                    app.playing = false;
                }
            }
            Effect::Pause => {
                if let Some(p) = &self.player {
                    p.pause();
                }
                app.playing = false;
            }
            Effect::ScoreChanged { buffer } => {
                // Rebuild the sequence on next play; keep playing the old one meanwhile.
                // The player keeps the old sequence until the next play reloads it.
                self.sequences.remove(&buffer);
                if self.loaded == Some(buffer) {
                    self.loaded = None;
                }
            }
            Effect::ConfigReloaded => {
                // Recreate the player on next use so a changed SoundFont takes effect.
                if !app.playing {
                    self.player = None;
                }
            }
        }
    }

    /// Updates `app.playing` from the player (playback stops by itself at the end).
    pub fn sync(&self, app: &mut App) {
        if let Some(p) = &self.player {
            app.playing = p.is_playing();
        }
    }

    /// Forgets the sequence of a buffer whose score changed.
    pub fn invalidate(&mut self, buffer: usize) {
        self.sequences.remove(&buffer);
        if self.loaded == Some(buffer) {
            self.loaded = None;
        }
    }

    /// Current playback location for the playing buffer: (buffer, location, time, duration).
    pub fn location(&self, app: &App) -> Option<(usize, Location, f64, f64)> {
        let buffer = app.playing_buffer?;
        let player = self.player.as_ref()?;
        let seq = self.sequences.get(&buffer)?;
        let t = player.position();
        Some((buffer, seq.location_at(t)?, t, seq.duration))
    }
}
