//! Playback engine for muse: score → MIDI event sequence → synthesizer → audio device.

mod player;
mod sequence;
mod synth;

pub use player::{Player, Renderer, make_synth, render_offline};
pub use sequence::{Location, MidiEvent, Sequence, SequenceOptions, TimedEvent};
pub use synth::{PluckSynth, SoundFontSynth, Synth};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("cannot load SoundFont {0}")]
    SoundFont(String),
    #[error("audio device: {0}")]
    Device(String),
}
