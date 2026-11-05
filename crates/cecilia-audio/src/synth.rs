//! Synthesizers: SoundFont playback via rustysynth, and a built-in plucked-string fallback used
//! when no SoundFont is configured.

use std::path::Path;
use std::sync::Arc;

use crate::sequence::MidiEvent;

pub trait Synth: Send {
    fn handle(&mut self, event: MidiEvent);
    fn all_notes_off(&mut self);
    /// Renders (adds nothing; overwrites) `left.len()` frames of audio.
    fn render(&mut self, left: &mut [f32], right: &mut [f32]);
}

pub struct SoundFontSynth {
    synth: rustysynth::Synthesizer,
}

impl SoundFontSynth {
    pub fn load(path: &Path, sample_rate: u32) -> Result<Self, crate::Error> {
        let err =
            |e: &dyn std::fmt::Display| crate::Error::SoundFont(format!("{}: {e}", path.display()));
        let mut file = std::fs::File::open(path).map_err(|e| err(&e))?;
        let sf = Arc::new(rustysynth::SoundFont::new(&mut file).map_err(|e| err(&e))?);
        let settings = rustysynth::SynthesizerSettings::new(sample_rate as i32);
        let synth = rustysynth::Synthesizer::new(&sf, &settings).map_err(|e| err(&e))?;
        Ok(Self { synth })
    }
}

impl Synth for SoundFontSynth {
    fn handle(&mut self, event: MidiEvent) {
        match event {
            MidiEvent::NoteOn {
                channel,
                key,
                velocity,
            } => self
                .synth
                .note_on(channel as i32, key as i32, velocity as i32),
            MidiEvent::NoteOff { channel, key } => self.synth.note_off(channel as i32, key as i32),
            MidiEvent::Program {
                channel,
                program,
                bank,
            } => {
                self.synth
                    .process_midi_message(channel as i32, 0xB0, 0, bank as i32);
                self.synth
                    .process_midi_message(channel as i32, 0xC0, program as i32, 0);
            }
            MidiEvent::Control {
                channel,
                controller,
                value,
            } => self.synth.process_midi_message(
                channel as i32,
                0xB0,
                controller as i32,
                value as i32,
            ),
        }
    }

    fn all_notes_off(&mut self) {
        self.synth.note_off_all(false);
    }

    fn render(&mut self, left: &mut [f32], right: &mut [f32]) {
        self.synth.render(left, right);
    }
}

const VOICES: usize = 48;
const PERCUSSION_CHANNEL: u8 = 9;

/// Karplus–Strong plucked strings with noise-burst percussion. No external data needed.
pub struct PluckSynth {
    sample_rate: f32,
    voices: Vec<PluckVoice>,
    volume: [f32; 16],
    pan: [f32; 16],
    rng: u32,
    age: u64,
}

struct PluckVoice {
    buf: Vec<f32>,
    len: usize,
    pos: usize,
    channel: u8,
    key: u8,
    gain: f32,
    decay: f32,
    released: bool,
    drum: bool,
    /// One-pole low-pass coefficient for percussion.
    lowpass: f32,
    state: f32,
    started: u64,
    active: bool,
}

impl PluckSynth {
    pub fn new(sample_rate: u32) -> Self {
        let max_len = (sample_rate as usize / 20).max(64);
        let voices = (0..VOICES)
            .map(|_| PluckVoice {
                buf: vec![0.0; max_len],
                len: 1,
                pos: 0,
                channel: 0,
                key: 0,
                gain: 0.0,
                decay: 0.0,
                released: false,
                drum: false,
                lowpass: 0.0,
                state: 0.0,
                started: 0,
                active: false,
            })
            .collect();
        Self {
            sample_rate: sample_rate as f32,
            voices,
            volume: [100.0 / 127.0; 16],
            pan: [0.5; 16],
            rng: 0x1234_5678,
            age: 0,
        }
    }

    fn noise(&mut self) -> f32 {
        self.rng ^= self.rng << 13;
        self.rng ^= self.rng >> 17;
        self.rng ^= self.rng << 5;
        (self.rng as f32 / u32::MAX as f32) * 2.0 - 1.0
    }

    fn note_on(&mut self, channel: u8, key: u8, velocity: u8) {
        self.age += 1;
        let idx = self
            .voices
            .iter()
            .position(|v| !v.active)
            .unwrap_or_else(|| {
                (0..VOICES)
                    .min_by_key(|&i| self.voices[i].started)
                    .unwrap_or(0)
            });
        let drum = channel == PERCUSSION_CHANNEL;
        let freq = 440.0 * 2f32.powf((key as f32 - 69.0) / 12.0);
        let max_len = self.voices[idx].buf.len();
        let len = if drum {
            max_len
        } else {
            ((self.sample_rate / freq).round() as usize).clamp(2, max_len)
        };
        let brightness = 0.3 + 0.7 * velocity as f32 / 127.0;
        let mut prev = 0.0;
        for i in 0..len {
            let n = self.noise();
            prev += brightness * (n - prev);
            self.voices[idx].buf[i] = prev;
        }
        let sr = self.sample_rate;
        let v = &mut self.voices[idx];
        v.len = len;
        v.pos = 0;
        v.channel = channel;
        v.key = key;
        v.gain = 0.45 * velocity as f32 / 127.0;
        v.released = false;
        v.drum = drum;
        // Drums: low keys (kick/toms) get a darker, longer burst.
        v.lowpass = if key < 48 { 0.05 } else { 0.6 };
        v.state = 0.0;
        v.decay = if drum {
            (-1.0 / (0.12 * sr)).exp()
        } else {
            0.996
        };
        v.started = self.age;
        v.active = true;
    }

    fn note_off(&mut self, channel: u8, key: u8) {
        for v in &mut self.voices {
            if v.active && !v.drum && v.channel == channel && v.key == key {
                v.released = true;
            }
        }
    }
}

impl Synth for PluckSynth {
    fn handle(&mut self, event: MidiEvent) {
        match event {
            MidiEvent::NoteOn {
                channel,
                key,
                velocity,
            } if velocity > 0 => self.note_on(channel, key, velocity),
            MidiEvent::NoteOn { channel, key, .. } | MidiEvent::NoteOff { channel, key } => {
                self.note_off(channel, key)
            }
            MidiEvent::Control {
                channel,
                controller: 7,
                value,
            } => self.volume[channel as usize & 15] = value as f32 / 127.0,
            MidiEvent::Control {
                channel,
                controller: 10,
                value,
            } => self.pan[channel as usize & 15] = value as f32 / 127.0,
            _ => {}
        }
    }

    fn all_notes_off(&mut self) {
        for v in &mut self.voices {
            v.active = false;
        }
    }

    fn render(&mut self, left: &mut [f32], right: &mut [f32]) {
        left.fill(0.0);
        right.fill(0.0);
        let release = (-1.0 / (0.03 * self.sample_rate)).exp();
        for v in self.voices.iter_mut().filter(|v| v.active) {
            let ch = v.channel as usize & 15;
            let (vol, pan) = (self.volume[ch], self.pan[ch]);
            let (gl, gr) = (vol * (1.0 - pan).sqrt(), vol * pan.sqrt());
            for i in 0..left.len() {
                let s;
                if v.drum {
                    let n = v.buf[v.pos];
                    v.pos = (v.pos + 1) % v.len;
                    v.state += v.lowpass * (n - v.state);
                    s = v.state * v.gain * 2.0;
                    v.gain *= v.decay;
                } else {
                    let next = (v.pos + 1) % v.len;
                    let out = v.buf[v.pos];
                    v.buf[v.pos] = v.decay * 0.5 * (out + v.buf[next]);
                    v.pos = next;
                    s = out * v.gain;
                    if v.released {
                        v.gain *= release;
                    }
                }
                left[i] += s * gl;
                right[i] += s * gr;
            }
            if v.gain < 1e-4 {
                v.active = false;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pluck_makes_sound_and_decays() {
        let mut synth = PluckSynth::new(48_000);
        synth.handle(MidiEvent::NoteOn {
            channel: 0,
            key: 64,
            velocity: 100,
        });
        let (mut l, mut r) = (vec![0.0; 4800], vec![0.0; 4800]);
        synth.render(&mut l, &mut r);
        let energy: f32 = l.iter().map(|x| x * x).sum();
        assert!(energy > 0.1, "energy {energy}");
        synth.handle(MidiEvent::NoteOff {
            channel: 0,
            key: 64,
        });
        for _ in 0..20 {
            synth.render(&mut l, &mut r);
        }
        assert!(synth.voices.iter().all(|v| !v.active));
    }
}
