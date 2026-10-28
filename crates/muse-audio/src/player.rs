//! Real-time playback: a [`Renderer`] driven by the audio device callback, controlled from the
//! UI thread through a [`Player`].
//!
//! The clock is a position in the score's audio timeline (seconds). MIDI events, the audio
//! track, metronome clicks and loops are all scheduled on it; `speed` scales how fast it runs,
//! with the audio time-stretched (signalsmith) so its pitch is kept.

use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{Receiver, Sender, channel};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

use crate::Error;
use crate::clip::AudioClip;
use crate::sequence::{MidiEvent, Sequence};
use crate::synth::{PluckSynth, SoundFontSynth, Synth};

const BLOCK: usize = 64;

/// Levels of the mixer's sources (0.0..=1.5).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Mix {
    pub midi: f32,
    pub audio: f32,
    pub metronome: f32,
    /// Scales the whole output.
    pub master: f32,
}

impl Default for Mix {
    fn default() -> Self {
        Self {
            midi: 1.0,
            audio: 1.0,
            metronome: 0.7,
            master: 1.0,
        }
    }
}

/// Speed trainer: each completed loop raises the speed by `step` until `target`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Trainer {
    pub step: f64,
    pub target: f64,
}

enum Command {
    Load(Arc<Sequence>),
    Audio(Option<Arc<AudioClip>>),
    Play { count_in: bool },
    Pause,
    Seek(f64),
    Speed(f64),
    Transpose(f32),
    Loop(Option<(f64, f64)>),
    Mix(Mix),
    Metronome(bool),
    Trainer(Option<Trainer>),
}

#[derive(Default)]
struct Shared {
    playing: AtomicBool,
    /// Playback position (seconds) as f64 bits.
    position: AtomicU64,
    speed: AtomicU64,
}

struct Click {
    phase: f32,
    freq: f32,
    gain: f32,
    remaining: usize,
}

/// Renders a sequence (and audio track) through a synth. Owned by the audio thread.
pub struct Renderer {
    seq: Arc<Sequence>,
    synth: Box<dyn Synth>,
    sample_rate: f64,
    time: f64,
    next_event: usize,
    next_beat: usize,
    playing: bool,
    speed: f64,
    transpose: f32,
    audio: Option<Arc<AudioClip>>,
    /// Read position in the audio clip (frames) while stretching.
    audio_pos: f64,
    stretch: Option<signalsmith_stretch::Stretch>,
    stretch_input: Vec<f32>,
    loop_range: Option<(f64, f64)>,
    trainer: Option<Trainer>,
    mix: Mix,
    metronome: bool,
    /// Count-in: remaining click times (seconds from now) and remaining duration.
    count_in: Option<(Vec<f64>, f64)>,
    clicks: Vec<Click>,
    left: [f32; BLOCK],
    right: [f32; BLOCK],
    audio_buf: [f32; BLOCK * 2],
}

impl Renderer {
    pub fn new(synth: Box<dyn Synth>, sample_rate: u32) -> Self {
        Self {
            seq: Arc::new(Sequence::default()),
            synth,
            sample_rate: sample_rate as f64,
            time: 0.0,
            next_event: 0,
            next_beat: 0,
            playing: false,
            speed: 1.0,
            transpose: 0.0,
            audio: None,
            audio_pos: 0.0,
            stretch: None,
            stretch_input: Vec::new(),
            loop_range: None,
            trainer: None,
            mix: Mix::default(),
            metronome: false,
            count_in: None,
            clicks: Vec::new(),
            left: [0.0; BLOCK],
            right: [0.0; BLOCK],
            audio_buf: [0.0; BLOCK * 2],
        }
    }

    pub fn load(&mut self, seq: Arc<Sequence>) {
        self.seq = seq;
        self.seek(self.time);
    }

    pub fn set_audio(&mut self, clip: Option<Arc<AudioClip>>) {
        self.audio = clip;
        self.seek(self.time);
    }

    fn end_time(&self) -> f64 {
        let audio_end = self.audio.as_ref().map_or(0.0, |a| a.duration());
        self.seq.duration.max(audio_end)
    }

    pub fn seek(&mut self, time: f64) {
        self.synth.all_notes_off();
        let start = self
            .seq
            .events
            .first()
            .map_or(0.0, |e| e.time.min(0.0))
            .min(time);
        self.time = time.clamp(start, self.end_time().max(start));
        let idx = self.seq.event_index_at(self.time);
        // Re-send program/controller setup that precedes the seek point.
        for e in &self.seq.events[..idx] {
            if !matches!(
                e.event,
                MidiEvent::NoteOn { .. } | MidiEvent::NoteOff { .. }
            ) {
                self.synth.handle(e.event);
            }
        }
        self.next_event = idx;
        self.next_beat = self.seq.beats.partition_point(|b| b.0 < self.time);
        self.audio_pos = self.time * self.sample_rate;
        if let Some(s) = self.stretch.as_mut() {
            s.reset();
        }
    }

    pub fn set_playing(&mut self, playing: bool, count_in: bool) {
        if self.playing && !playing {
            self.synth.all_notes_off();
        }
        self.count_in = None;
        if playing && self.time >= self.end_time() {
            self.seek(0.0);
        }
        if playing && count_in {
            // One bar of clicks at the tempo around the start.
            let beats: Vec<(f64, bool)> = self
                .seq
                .beats
                .iter()
                .skip(self.next_beat)
                .take(9)
                .copied()
                .collect();
            let beat_len = beats
                .windows(2)
                .map(|w| w[1].0 - w[0].0)
                .find(|d| *d > 0.05)
                .unwrap_or(0.5);
            let per_bar = beats
                .iter()
                .skip(1)
                .position(|b| b.1)
                .map_or(4, |p| p + 1)
                .clamp(2, 12);
            let times: Vec<f64> = (0..per_bar).map(|i| i as f64 * beat_len).collect();
            self.count_in = Some((times, per_bar as f64 * beat_len));
            self.playing = false;
            return;
        }
        self.playing = playing;
    }

    pub fn set_speed(&mut self, speed: f64) {
        self.speed = speed.clamp(0.25, 2.0);
        self.update_stretch();
    }

    pub fn set_transpose(&mut self, semitones: f32) {
        self.transpose = semitones.clamp(-24.0, 24.0);
        self.update_stretch();
    }

    fn update_stretch(&mut self) {
        let needed = (self.speed - 1.0).abs() > 1e-6 || self.transpose.abs() > 1e-6;
        if !needed {
            self.stretch = None;
            return;
        }
        let rate = self.sample_rate as u32;
        let s = self
            .stretch
            .get_or_insert_with(|| signalsmith_stretch::Stretch::preset_default(2, rate));
        s.set_transpose_factor_semitones(self.transpose, None);
    }

    pub fn time(&self) -> f64 {
        self.time
    }

    pub fn is_playing(&self) -> bool {
        self.playing || self.count_in.is_some()
    }

    fn click(&mut self, downbeat: bool) {
        let gain = self.mix.metronome * if downbeat { 0.5 } else { 0.35 };
        self.clicks.push(Click {
            phase: 0.0,
            freq: if downbeat { 1760.0 } else { 1320.0 },
            gain,
            remaining: (self.sample_rate * 0.04) as usize,
        });
    }

    /// Audio track samples for the next block (interleaved into `audio_buf`).
    fn render_audio(&mut self, frames: usize) {
        let buf = &mut self.audio_buf[..frames * 2];
        buf.fill(0.0);
        let Some(clip) = self.audio.as_ref() else {
            return;
        };
        let total = clip.frames() as f64;
        match self.stretch.as_mut() {
            None => {
                let start = (self.time * self.sample_rate).round();
                for i in 0..frames {
                    let f = start + i as f64;
                    if f >= 0.0 && f < total {
                        let k = f as usize * 2;
                        buf[i * 2] = clip.samples[k];
                        buf[i * 2 + 1] = clip.samples[k + 1];
                    }
                }
            }
            Some(stretch) => {
                let next = self.audio_pos + frames as f64 * self.speed;
                let (a, b) = (self.audio_pos.floor() as i64, next.floor() as i64);
                self.stretch_input.clear();
                for f in a..b {
                    if f >= 0 && (f as f64) < total {
                        let k = f as usize * 2;
                        self.stretch_input.push(clip.samples[k]);
                        self.stretch_input.push(clip.samples[k + 1]);
                    } else {
                        self.stretch_input.extend_from_slice(&[0.0, 0.0]);
                    }
                }
                stretch.process(&self.stretch_input, &mut *buf);
                self.audio_pos = next;
            }
        }
    }

    fn count_in_step(&mut self, dt: f64) {
        let Some((times, remaining)) = self.count_in.as_mut() else {
            return;
        };
        let step = dt * self.speed.min(1.0);
        let mut due = 0;
        let first_pending = times.len();
        times.retain(|t| {
            let fire = *t < step;
            due += usize::from(fire);
            !fire
        });
        for t in times.iter_mut() {
            *t -= step;
        }
        *remaining -= step;
        let done = *remaining <= 0.0;
        let downbeat_fired = due > 0
            && first_pending == self.count_in.as_ref().map_or(0, |c| c.0.len()) + due
            && first_pending > 0;
        for k in 0..due {
            self.click(downbeat_fired && k == 0 && first_pending >= 2);
        }
        if done {
            self.count_in = None;
            self.playing = true;
        }
    }

    /// Fills an interleaved buffer with `channels` channels.
    pub fn process(&mut self, out: &mut [f32], channels: usize) {
        let channels = channels.max(1);
        for chunk in out.chunks_mut(BLOCK * channels) {
            let frames = chunk.len() / channels;
            let dt = frames as f64 / self.sample_rate;
            // Count-in clicks run in real time before the clock starts.
            self.count_in_step(dt);
            if self.playing {
                let end = self.time + dt * self.speed;
                while let Some(e) = self.seq.events.get(self.next_event) {
                    if e.time > end {
                        break;
                    }
                    self.synth.handle(e.event);
                    self.next_event += 1;
                }
                while let Some(&(t, down)) = self.seq.beats.get(self.next_beat) {
                    if t > end {
                        break;
                    }
                    if self.metronome {
                        self.click(down);
                    }
                    self.next_beat += 1;
                }
                self.render_audio(frames);
                self.time = end;
                if let Some((a, b)) = self.loop_range {
                    if self.time >= b {
                        if let Some(tr) = self.trainer {
                            self.set_speed((self.speed + tr.step).min(tr.target));
                        }
                        self.seek(a);
                    }
                }
                if self.time >= self.end_time() {
                    self.time = self.end_time();
                    self.playing = false;
                    self.synth.all_notes_off();
                }
            } else {
                self.audio_buf[..frames * 2].fill(0.0);
            }
            let (l, r) = (&mut self.left[..frames], &mut self.right[..frames]);
            self.synth.render(l, r);
            let (midi, audio) = (self.mix.midi, self.mix.audio);
            let rate = self.sample_rate as f32;
            for (i, frame) in chunk.chunks_mut(channels).enumerate() {
                let mut sl = l[i] * midi + self.audio_buf[i * 2] * audio;
                let mut sr = r[i] * midi + self.audio_buf[i * 2 + 1] * audio;
                for c in &mut self.clicks {
                    if c.remaining > 0 {
                        let s = (c.phase * std::f32::consts::TAU).sin() * c.gain;
                        c.gain *= 0.9993;
                        c.phase = (c.phase + c.freq / rate).fract();
                        c.remaining -= 1;
                        sl += s;
                        sr += s;
                    }
                }
                let master = self.mix.master;
                frame[0] = sl * master;
                if channels > 1 {
                    frame[1] = sr * master;
                    for s in &mut frame[2..] {
                        *s = 0.0;
                    }
                }
            }
            self.clicks.retain(|c| c.remaining > 0);
        }
    }
}

/// Offline rendering settings.
#[derive(Clone, Default)]
pub struct RenderOptions {
    pub audio: Option<Arc<AudioClip>>,
    pub mix: Mix,
    pub metronome: bool,
}

/// Renders a sequence offline (stereo interleaved) from its start for `seconds`, e.g. for
/// tests or WAV/FLAC export.
pub fn render_offline(
    seq: Arc<Sequence>,
    synth: Box<dyn Synth>,
    sample_rate: u32,
    seconds: f64,
) -> Vec<f32> {
    render_offline_with(seq, synth, sample_rate, seconds, &RenderOptions::default())
}

pub fn render_offline_with(
    seq: Arc<Sequence>,
    synth: Box<dyn Synth>,
    sample_rate: u32,
    seconds: f64,
    options: &RenderOptions,
) -> Vec<f32> {
    let mut r = Renderer::new(synth, sample_rate);
    r.mix = options.mix;
    r.metronome = options.metronome;
    r.audio = options.audio.clone();
    let start = seq.events.first().map_or(0.0, |e| e.time.min(0.0)).min(0.0);
    r.load(seq);
    r.seek(start);
    r.set_playing(true, false);
    let mut out = vec![0.0; (seconds * sample_rate as f64) as usize * 2];
    r.process(&mut out, 2);
    out
}

/// Encodes interleaved stereo samples as 16-bit WAV.
pub fn encode_wav(samples: &[f32], rate: u32) -> Result<Vec<u8>, Error> {
    let spec = hound::WavSpec {
        channels: 2,
        sample_rate: rate,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut bytes = Vec::new();
    let e = |e: hound::Error| Error::Decode(e.to_string());
    let mut w = hound::WavWriter::new(std::io::Cursor::new(&mut bytes), spec).map_err(e)?;
    for s in samples {
        w.write_sample((s.clamp(-1.0, 1.0) * 32767.0) as i16)
            .map_err(e)?;
    }
    w.finalize().map_err(e)?;
    Ok(bytes)
}

/// Encodes interleaved stereo samples as 16-bit FLAC.
pub fn encode_flac(samples: &[f32], rate: u32) -> Result<Vec<u8>, Error> {
    use flacenc::component::BitRepr;
    use flacenc::error::Verify;
    let ints: Vec<i32> = samples
        .iter()
        .map(|s| (s.clamp(-1.0, 1.0) * 32767.0) as i32)
        .collect();
    let config = flacenc::config::Encoder::default()
        .into_verified()
        .map_err(|(_, e)| Error::Decode(format!("{e:?}")))?;
    let source = flacenc::source::MemSource::from_samples(&ints, 2, 16, rate as usize);
    let stream = flacenc::encode_with_fixed_block_size(&config, source, config.block_size)
        .map_err(|e| Error::Decode(format!("{e:?}")))?;
    let mut sink = flacenc::bitsink::ByteSink::new();
    stream
        .write(&mut sink)
        .map_err(|e| Error::Decode(format!("{e:?}")))?;
    Ok(sink.as_slice().to_vec())
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
    /// Output sample rate (audio clips are resampled to it).
    pub sample_rate: u32,
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
        shared.speed.store(1.0f64.to_bits(), Ordering::Relaxed);
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
            sample_rate: config.sample_rate.0,
            synth_name,
            warning,
        })
    }

    fn send(&self, c: Command) {
        let _ = self.tx.send(c);
    }

    pub fn load(&self, seq: Arc<Sequence>) {
        self.send(Command::Load(seq));
    }

    /// Sets the audio track (already at [`Player::sample_rate`]).
    pub fn set_audio(&self, clip: Option<Arc<AudioClip>>) {
        self.send(Command::Audio(clip));
    }

    pub fn play(&self, count_in: bool) {
        self.shared.playing.store(true, Ordering::Relaxed);
        self.send(Command::Play { count_in });
    }

    pub fn pause(&self) {
        self.shared.playing.store(false, Ordering::Relaxed);
        self.send(Command::Pause);
    }

    pub fn seek(&self, time: f64) {
        self.shared
            .position
            .store(time.to_bits(), Ordering::Relaxed);
        self.send(Command::Seek(time));
    }

    pub fn set_speed(&self, speed: f64) {
        self.shared.speed.store(speed.to_bits(), Ordering::Relaxed);
        self.send(Command::Speed(speed));
    }

    pub fn set_transpose(&self, semitones: f32) {
        self.send(Command::Transpose(semitones));
    }

    pub fn set_loop(&self, range: Option<(f64, f64)>) {
        self.send(Command::Loop(range));
    }

    pub fn set_mix(&self, mix: Mix) {
        self.send(Command::Mix(mix));
    }

    pub fn set_metronome(&self, on: bool) {
        self.send(Command::Metronome(on));
    }

    pub fn set_trainer(&self, trainer: Option<Trainer>) {
        self.send(Command::Trainer(trainer));
    }

    pub fn is_playing(&self) -> bool {
        self.shared.playing.load(Ordering::Relaxed)
    }

    /// Current position in seconds (audio timeline when synced).
    pub fn position(&self) -> f64 {
        f64::from_bits(self.shared.position.load(Ordering::Relaxed))
    }

    /// Current speed (the trainer may change it).
    pub fn speed(&self) -> f64 {
        f64::from_bits(self.shared.speed.load(Ordering::Relaxed))
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
                        Command::Audio(clip) => renderer.set_audio(clip),
                        Command::Play { count_in } => renderer.set_playing(true, count_in),
                        Command::Pause => renderer.set_playing(false, false),
                        Command::Seek(t) => renderer.seek(t),
                        Command::Speed(s) => renderer.set_speed(s),
                        Command::Transpose(t) => renderer.set_transpose(t),
                        Command::Loop(l) => renderer.loop_range = l,
                        Command::Mix(m) => renderer.mix = m,
                        Command::Metronome(m) => renderer.metronome = m,
                        Command::Trainer(t) => renderer.trainer = t,
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
                shared
                    .speed
                    .store(renderer.speed.to_bits(), Ordering::Relaxed);
            },
            |e| eprintln!("audio stream error: {e}"),
            None,
        )
        .map_err(|e| Error::Device(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sequence::TimedEvent;

    fn note_seq() -> Arc<Sequence> {
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
        Arc::new(Sequence {
            events,
            duration: 1.0,
            beats: vec![(0.0, true), (0.5, false)],
            ..Sequence::default()
        })
    }

    #[test]
    fn offline_render_plays_events() {
        let out = render_offline(note_seq(), Box::new(PluckSynth::new(48_000)), 48_000, 1.0);
        let first: f32 = out[..48_000].iter().map(|x| x * x).sum();
        assert!(first > 0.1);
    }

    #[test]
    fn audio_track_mixes_and_stretches() {
        let rate = 48_000usize;
        // A 1 kHz tone, 2 s.
        let samples: Vec<f32> = (0..rate * 2)
            .flat_map(|i| {
                let s = (i as f32 * 1000.0 * std::f32::consts::TAU / rate as f32).sin() * 0.5;
                [s, s]
            })
            .collect();
        let clip = Arc::new(AudioClip {
            rate: rate as u32,
            samples,
        });
        let mut r = Renderer::new(Box::new(PluckSynth::new(rate as u32)), rate as u32);
        r.mix.midi = 0.0;
        r.load(note_seq());
        r.set_audio(Some(clip));
        r.set_speed(0.5);
        r.set_playing(true, false);
        let mut out = vec![0.0; rate * 2];
        r.process(&mut out, 2);
        // Half speed: one second of output consumed half a second of audio.
        assert!((r.time() - 0.5).abs() < 0.01, "{}", r.time());
        let energy: f32 = out[rate..].iter().map(|x| x * x).sum::<f32>() / rate as f32;
        assert!(energy > 0.05, "{energy}");
    }

    #[test]
    fn loops_and_count_in() {
        let mut r = Renderer::new(Box::new(PluckSynth::new(48_000)), 48_000);
        r.load(note_seq());
        r.loop_range = Some((0.0, 0.25));
        r.set_playing(true, true);
        assert!(r.is_playing() && !r.playing);
        let mut out = vec![0.0; 48_000 * 2];
        r.process(&mut out, 2);
        // The count-in (2 beats of 0.5 s) took the second; then the loop keeps time < 0.25.
        let mut more = vec![0.0; 48_000 * 2];
        r.process(&mut more, 2);
        assert!(r.time() < 0.25 && r.is_playing(), "{}", r.time());
    }
}
