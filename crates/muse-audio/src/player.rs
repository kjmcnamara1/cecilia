//! Real-time playback: a [`Renderer`] driven by the audio device callback, controlled from the
//! UI thread through a [`Player`].

use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{Receiver, Sender, channel};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

use crate::Error;
use crate::sequence::Sequence;
use crate::synth::{PluckSynth, SoundFontSynth, Synth};

const BLOCK: usize = 64;

enum Command {
    Load(Arc<Sequence>),
    Play,
    Pause,
    Seek(f64),
    Speed(f64),
}

#[derive(Default)]
struct Shared {
    playing: AtomicBool,
    /// Playback position in seconds (score time at normal speed), stored as f64 bits.
    position: AtomicU64,
}

/// Renders a sequence through a synth. Owned by the audio thread.
pub struct Renderer {
    seq: Arc<Sequence>,
    synth: Box<dyn Synth>,
    sample_rate: f64,
    time: f64,
    next_event: usize,
    playing: bool,
    speed: f64,
    left: [f32; BLOCK],
    right: [f32; BLOCK],
}

impl Renderer {
    pub fn new(synth: Box<dyn Synth>, sample_rate: u32) -> Self {
        Self {
            seq: Arc::new(Sequence::default()),
            synth,
            sample_rate: sample_rate as f64,
            time: 0.0,
            next_event: 0,
            playing: false,
            speed: 1.0,
            left: [0.0; BLOCK],
            right: [0.0; BLOCK],
        }
    }

    pub fn load(&mut self, seq: Arc<Sequence>) {
        self.seq = seq;
        self.seek(0.0);
    }

    pub fn seek(&mut self, time: f64) {
        self.synth.all_notes_off();
        self.time = time.clamp(0.0, self.seq.duration);
        // Re-send program/controller setup that precedes the seek point.
        let start = self.seq.event_index_at(self.time);
        for e in &self.seq.events[..start] {
            if !matches!(
                e.event,
                crate::MidiEvent::NoteOn { .. } | crate::MidiEvent::NoteOff { .. }
            ) {
                self.synth.handle(e.event);
            }
        }
        self.next_event = start;
    }

    pub fn set_playing(&mut self, playing: bool) {
        if self.playing && !playing {
            self.synth.all_notes_off();
        }
        self.playing = playing;
        if playing && self.time >= self.seq.duration {
            self.seek(0.0);
        }
    }

    pub fn set_speed(&mut self, speed: f64) {
        self.speed = speed.clamp(0.1, 4.0);
    }

    pub fn time(&self) -> f64 {
        self.time
    }

    pub fn is_playing(&self) -> bool {
        self.playing
    }

    /// Fills an interleaved buffer with `channels` channels.
    pub fn process(&mut self, out: &mut [f32], channels: usize) {
        let channels = channels.max(1);
        for chunk in out.chunks_mut(BLOCK * channels) {
            let frames = chunk.len() / channels;
            if self.playing {
                let end = self.time + frames as f64 * self.speed / self.sample_rate;
                while let Some(e) = self.seq.events.get(self.next_event) {
                    if e.time > end {
                        break;
                    }
                    self.synth.handle(e.event);
                    self.next_event += 1;
                }
                self.time = end;
                if self.time >= self.seq.duration {
                    self.time = self.seq.duration;
                    self.playing = false;
                    self.synth.all_notes_off();
                }
            }
            let (l, r) = (&mut self.left[..frames], &mut self.right[..frames]);
            self.synth.render(l, r);
            for (i, frame) in chunk.chunks_mut(channels).enumerate() {
                frame[0] = l[i];
                if channels > 1 {
                    frame[1] = r[i];
                    for s in &mut frame[2..] {
                        *s = 0.0;
                    }
                }
            }
        }
    }
}

/// Renders `seconds` of a sequence offline (stereo interleaved), e.g. for tests or export.
pub fn render_offline(
    seq: Arc<Sequence>,
    synth: Box<dyn Synth>,
    sample_rate: u32,
    seconds: f64,
) -> Vec<f32> {
    let mut r = Renderer::new(synth, sample_rate);
    r.load(seq);
    r.set_playing(true);
    let mut out = vec![0.0; (seconds * sample_rate as f64) as usize * 2];
    r.process(&mut out, 2);
    out
}

/// Creates the synth for a sample rate: the SoundFont if given and loadable, else the built-in.
/// Returns a warning when falling back.
pub fn make_synth(
    soundfont: Option<&Path>,
    sample_rate: u32,
) -> (Box<dyn Synth>, String, Option<String>) {
    match soundfont {
        Some(path) => match SoundFontSynth::load(path, sample_rate) {
            Ok(s) => (Box::new(s), format!("SoundFont {}", path.display()), None),
            Err(e) => (
                Box::new(PluckSynth::new(sample_rate)),
                "built-in synth".into(),
                Some(e.to_string()),
            ),
        },
        None => (
            Box::new(PluckSynth::new(sample_rate)),
            "built-in synth".into(),
            None,
        ),
    }
}

/// Handle to the audio output stream.
pub struct Player {
    _stream: cpal::Stream,
    tx: Sender<Command>,
    shared: Arc<Shared>,
    /// Which synth is in use, for display.
    pub synth_name: String,
    /// Set when the configured SoundFont could not be loaded.
    pub warning: Option<String>,
}

impl Player {
    pub fn new(soundfont: Option<&Path>) -> Result<Self, Error> {
        let host = cpal::default_host();
        let device = host
            .default_output_device()
            .ok_or_else(|| Error::Device("no audio output device".into()))?;
        let supported = device
            .default_output_config()
            .map_err(|e| Error::Device(e.to_string()))?;
        let format = supported.sample_format();
        let config: cpal::StreamConfig = supported.into();
        let (synth, synth_name, warning) = make_synth(soundfont, config.sample_rate.0);
        let renderer = Renderer::new(synth, config.sample_rate.0);
        let (tx, rx) = channel();
        let shared = Arc::new(Shared::default());
        let stream = match format {
            cpal::SampleFormat::F32 => build::<f32>(&device, &config, renderer, rx, shared.clone()),
            cpal::SampleFormat::I16 => build::<i16>(&device, &config, renderer, rx, shared.clone()),
            cpal::SampleFormat::U16 => build::<u16>(&device, &config, renderer, rx, shared.clone()),
            other => return Err(Error::Device(format!("unsupported sample format {other}"))),
        }?;
        stream.play().map_err(|e| Error::Device(e.to_string()))?;
        Ok(Self {
            _stream: stream,
            tx,
            shared,
            synth_name,
            warning,
        })
    }

    pub fn load(&self, seq: Arc<Sequence>) {
        let _ = self.tx.send(Command::Load(seq));
    }

    pub fn play(&self) {
        self.shared.playing.store(true, Ordering::Relaxed);
        let _ = self.tx.send(Command::Play);
    }

    pub fn pause(&self) {
        self.shared.playing.store(false, Ordering::Relaxed);
        let _ = self.tx.send(Command::Pause);
    }

    pub fn seek(&self, time: f64) {
        self.shared
            .position
            .store(time.to_bits(), Ordering::Relaxed);
        let _ = self.tx.send(Command::Seek(time));
    }

    pub fn set_speed(&self, speed: f64) {
        let _ = self.tx.send(Command::Speed(speed));
    }

    pub fn is_playing(&self) -> bool {
        self.shared.playing.load(Ordering::Relaxed)
    }

    /// Current position in seconds of score time.
    pub fn position(&self) -> f64 {
        f64::from_bits(self.shared.position.load(Ordering::Relaxed))
    }
}

fn build<T>(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    mut renderer: Renderer,
    rx: Receiver<Command>,
    shared: Arc<Shared>,
) -> Result<cpal::Stream, Error>
where
    T: cpal::SizedSample + cpal::FromSample<f32>,
{
    let channels = config.channels as usize;
    let mut scratch: Vec<f32> = vec![0.0; 8192];
    device
        .build_output_stream(
            config,
            move |data: &mut [T], _| {
                while let Ok(cmd) = rx.try_recv() {
                    match cmd {
                        Command::Load(seq) => renderer.load(seq),
                        Command::Play => renderer.set_playing(true),
                        Command::Pause => renderer.set_playing(false),
                        Command::Seek(t) => renderer.seek(t),
                        Command::Speed(s) => renderer.set_speed(s),
                    }
                }
                if scratch.len() < data.len() {
                    scratch.resize(data.len(), 0.0);
                }
                let buf = &mut scratch[..data.len()];
                renderer.process(buf, channels);
                for (d, s) in data.iter_mut().zip(buf.iter()) {
                    *d = T::from_sample(s.clamp(-1.0, 1.0));
                }
                shared
                    .playing
                    .store(renderer.is_playing(), Ordering::Relaxed);
                shared
                    .position
                    .store(renderer.time().to_bits(), Ordering::Relaxed);
            },
            |e| eprintln!("audio stream error: {e}"),
            None,
        )
        .map_err(|e| Error::Device(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sequence::{MidiEvent, TimedEvent};

    #[test]
    fn offline_render_plays_events_and_stops_at_end() {
        let events = vec![
            TimedEvent {
                time: 0.0,
                event: MidiEvent::NoteOn {
                    channel: 0,
                    key: 60,
                    velocity: 100,
                },
            },
            TimedEvent {
                time: 0.5,
                event: MidiEvent::NoteOff {
                    channel: 0,
                    key: 60,
                },
            },
        ];
        let seq = Sequence {
            events,
            duration: 0.5,
            ..Sequence::default()
        };
        let out = render_offline(
            Arc::new(seq),
            Box::new(PluckSynth::new(48_000)),
            48_000,
            1.0,
        );
        let first: f32 = out[..48_000].iter().map(|x| x * x).sum();
        assert!(first > 0.1);
    }
}
