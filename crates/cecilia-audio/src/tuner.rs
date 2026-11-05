//! A tuner: the default input device (microphone or instrument input) is captured, and a
//! background thread finds the pitch of the latest audio with the YIN method several times a
//! second.

use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::Duration;

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

use crate::Error;

/// A detected pitch.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Reading {
    /// Frequency in Hz.
    pub freq: f32,
    /// How periodic the sound is, 0–1 (1 = a pure tone).
    pub clarity: f32,
}

/// The lowest and highest pitches looked for (below a 5-string bass's low B; a guitar's
/// 24th fret on the high E string is 1319 Hz).
const MIN_FREQ: f32 = 28.0;
const MAX_FREQ: f32 = 1500.0;
/// Samples analysed, after taking the input down to about 22 kHz.
const WINDOW: usize = 2048;
/// Quieter than this (RMS) is silence.
const MIN_LEVEL: f32 = 0.004;

/// Finds the fundamental frequency of `samples` (mono, at `rate` Hz), if they are loud and
/// periodic enough: the YIN difference function, its cumulative-mean normalisation, the first
/// dip under a threshold, and a parabola through it for sub-sample accuracy.
pub fn detect_pitch(samples: &[f32], rate: f32) -> Option<Reading> {
    let n = samples.len();
    let rms = (samples.iter().map(|s| s * s).sum::<f32>() / n.max(1) as f32).sqrt();
    if rms < MIN_LEVEL {
        return None;
    }
    let tau_min = ((rate / MAX_FREQ) as usize).max(2);
    let tau_max = ((rate / MIN_FREQ) as usize).min(n / 2);
    if tau_max <= tau_min + 2 {
        return None;
    }
    let w = n - tau_max;
    let mut d = vec![0f32; tau_max + 1];
    for tau in 1..=tau_max {
        let mut sum = 0.0;
        for i in 0..w {
            let x = samples[i] - samples[i + tau];
            sum += x * x;
        }
        d[tau] = sum;
    }
    // Cumulative mean normalised difference.
    let mut cmnd = vec![1f32; tau_max + 1];
    let mut running = 0.0;
    for tau in 1..=tau_max {
        running += d[tau];
        cmnd[tau] = if running > 0.0 {
            d[tau] * tau as f32 / running
        } else {
            1.0
        };
    }
    const THRESHOLD: f32 = 0.15;
    let mut tau = None;
    let mut t = tau_min;
    while t < tau_max {
        if cmnd[t] < THRESHOLD {
            // Follow the dip to its bottom.
            while t + 1 < tau_max && cmnd[t + 1] < cmnd[t] {
                t += 1;
            }
            tau = Some(t);
            break;
        }
        t += 1;
    }
    // No clear dip: take the deepest one if it is still fairly periodic.
    let tau = tau.or_else(|| {
        let (best, value) = (tau_min..tau_max)
            .map(|t| (t, cmnd[t]))
            .min_by(|a, b| a.1.total_cmp(&b.1))?;
        (value < 0.35).then_some(best)
    })?;
    let refined = if tau > 1 && tau < tau_max {
        let (a, b, c) = (cmnd[tau - 1], cmnd[tau], cmnd[tau + 1]);
        let denom = a - 2.0 * b + c;
        if denom.abs() > f32::EPSILON {
            tau as f32 + 0.5 * (a - c) / denom
        } else {
            tau as f32
        }
    } else {
        tau as f32
    };
    Some(Reading {
        freq: rate / refined,
        clarity: (1.0 - cmnd[tau]).clamp(0.0, 1.0),
    })
}

/// A running tuner. Dropping it stops the capture.
pub struct Tuner {
    _stream: cpal::Stream,
    reading: Arc<Mutex<Option<Reading>>>,
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
    /// The input device's name, for display.
    pub device: String,
}

impl Tuner {
    /// Starts capturing from the default input device.
    pub fn start() -> Result<Self, Error> {
        let host = cpal::default_host();
        let device = host
            .default_input_device()
            .ok_or_else(|| Error::Device("no audio input device".into()))?;
        let name = device.name().unwrap_or_else(|_| "input".into());
        let supported = device
            .default_input_config()
            .map_err(|e| Error::Device(e.to_string()))?;
        let format = supported.sample_format();
        let config: cpal::StreamConfig = supported.into();
        let rate = config.sample_rate.0 as f32;
        let channels = config.channels as usize;
        // Keep a little more than a window of input (at the device's rate).
        let decimate = if rate > 32_000.0 { 2 } else { 1 };
        let keep = WINDOW * decimate * 2;
        let buffer = Arc::new(Mutex::new(VecDeque::<f32>::with_capacity(keep)));
        let push = {
            let buffer = buffer.clone();
            move |mono: &mut dyn Iterator<Item = f32>| {
                if let Ok(mut b) = buffer.lock() {
                    b.extend(mono);
                    let excess = b.len().saturating_sub(keep);
                    b.drain(..excess);
                }
            }
        };
        let err = |e| eprintln!("tuner input: {e}");
        let stream = match format {
            cpal::SampleFormat::F32 => device.build_input_stream(
                &config,
                {
                    let push = push.clone();
                    move |data: &[f32], _| {
                        push(
                            &mut data
                                .chunks(channels)
                                .map(|f| f.iter().sum::<f32>() / channels as f32),
                        )
                    }
                },
                err,
                None,
            ),
            cpal::SampleFormat::I16 => device.build_input_stream(
                &config,
                {
                    let push = push.clone();
                    move |data: &[i16], _| {
                        push(&mut data.chunks(channels).map(|f| {
                            f.iter().map(|&s| s as f32 / 32768.0).sum::<f32>() / channels as f32
                        }))
                    }
                },
                err,
                None,
            ),
            cpal::SampleFormat::U16 => device.build_input_stream(
                &config,
                {
                    let push = push.clone();
                    move |data: &[u16], _| {
                        push(&mut data.chunks(channels).map(|f| {
                            f.iter()
                                .map(|&s| (s as f32 - 32768.0) / 32768.0)
                                .sum::<f32>()
                                / channels as f32
                        }))
                    }
                },
                err,
                None,
            ),
            other => return Err(Error::Device(format!("unsupported sample format {other}"))),
        }
        .map_err(|e| Error::Device(e.to_string()))?;
        stream.play().map_err(|e| Error::Device(e.to_string()))?;

        let reading = Arc::new(Mutex::new(None));
        let stop = Arc::new(AtomicBool::new(false));
        let thread = {
            let (reading, stop) = (reading.clone(), stop.clone());
            std::thread::spawn(move || {
                let mut smooth: Option<f32> = None;
                while !stop.load(Ordering::Relaxed) {
                    std::thread::sleep(Duration::from_millis(50));
                    let samples: Vec<f32> = match buffer.lock() {
                        Ok(b) if b.len() >= WINDOW * decimate => b
                            .iter()
                            .skip(b.len() - WINDOW * decimate)
                            .copied()
                            .collect(),
                        _ => continue,
                    };
                    // Down to about 22 kHz by averaging pairs: plenty for these pitches.
                    let samples: Vec<f32> = samples
                        .chunks(decimate)
                        .map(|c| c.iter().sum::<f32>() / c.len() as f32)
                        .collect();
                    let found = detect_pitch(&samples, rate / decimate as f32);
                    // Smooth small wobbles; follow big jumps (a new note) at once.
                    let out = found.map(|r| {
                        let f = match smooth {
                            Some(s) if (r.freq / s).log2().abs() < 0.03 => s + (r.freq - s) * 0.35,
                            _ => r.freq,
                        };
                        smooth = Some(f);
                        Reading { freq: f, ..r }
                    });
                    if out.is_none() {
                        smooth = None;
                    }
                    if let Ok(mut r) = reading.lock() {
                        *r = out;
                    }
                }
            })
        };
        Ok(Self {
            _stream: stream,
            reading,
            stop,
            thread: Some(thread),
            device: name,
        })
    }

    /// The latest pitch, or `None` while it is quiet or unclear.
    pub fn reading(&self) -> Option<Reading> {
        self.reading.lock().ok().and_then(|r| *r)
    }
}

impl Drop for Tuner {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(t) = self.thread.take() {
            t.join().ok();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tone(freqs: &[(f32, f32)], rate: f32, n: usize) -> Vec<f32> {
        (0..n)
            .map(|i| {
                let t = i as f32 / rate;
                freqs
                    .iter()
                    .map(|(f, a)| a * (2.0 * std::f32::consts::PI * f * t).sin())
                    .sum()
            })
            .collect()
    }

    fn cents(a: f32, b: f32) -> f32 {
        1200.0 * (a / b).log2()
    }

    #[test]
    fn finds_the_fundamental_of_plucked_strings() {
        let rate = 22_050.0;
        // Low E (82.41 Hz) with strong harmonics, A (110), high E (329.6), a bass's low B.
        for f in [82.41f32, 110.0, 329.63, 30.87, 440.0, 1046.5] {
            let s = tone(&[(f, 0.3), (2.0 * f, 0.5), (3.0 * f, 0.25)], rate, WINDOW);
            let r = detect_pitch(&s, rate).unwrap_or_else(|| panic!("{f} Hz not found"));
            assert!(cents(r.freq, f).abs() < 3.0, "{f} Hz read as {}", r.freq);
            assert!(r.clarity > 0.8);
        }
        // Silence and noise give nothing.
        assert!(detect_pitch(&vec![0.0; WINDOW], rate).is_none());
        let mut x = 12345u32;
        let noise: Vec<f32> = (0..WINDOW)
            .map(|_| {
                x ^= x << 13;
                x ^= x >> 17;
                x ^= x << 5;
                (x as f32 / u32::MAX as f32 - 0.5) * 0.5
            })
            .collect();
        assert!(detect_pitch(&noise, rate).is_none());
    }
}
