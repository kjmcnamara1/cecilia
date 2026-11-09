//! Playback engine for cecilia: score → MIDI event sequence → synthesizer → audio device.

pub mod beats;
mod clip;
pub mod pitch;
mod player;
mod sequence;
mod synth;
pub mod tuner;

pub use clip::AudioClip;
pub use player::{
    Mix, Player, RenderOptions, Renderer, Trainer, encode_flac, encode_wav, make_synth,
    render_offline, render_offline_with,
};
pub use sequence::{Location, MidiEvent, SYNC_RATE, Sequence, SequenceOptions, TimedEvent};
pub use synth::{PluckSynth, SoundFontSynth, Synth};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("cannot load SoundFont {0}")]
    SoundFont(String),
    #[error("audio device: {0}")]
    Device(String),
    #[error("cannot decode audio: {0}")]
    Decode(String),
}
