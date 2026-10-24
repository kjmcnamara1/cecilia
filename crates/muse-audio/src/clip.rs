//! Decoded audio tracks (mp3, m4a/aac, flac, ogg, wav) as stereo samples.

use std::io::Cursor;

use symphonia::core::audio::SampleBuffer;
use symphonia::core::codecs::DecoderOptions;
use symphonia::core::errors::Error as SymError;
use symphonia::core::formats::FormatOptions;
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;
use symphonia::core::probe::Hint;

use crate::Error;

/// Interleaved stereo samples.
#[derive(Debug, Clone, Default)]
pub struct AudioClip {
    pub rate: u32,
    pub samples: Vec<f32>,
}

impl AudioClip {
    pub fn frames(&self) -> usize {
        self.samples.len() / 2
    }

    pub fn duration(&self) -> f64 {
        self.frames() as f64 / self.rate.max(1) as f64
    }

    /// Decodes a file's bytes; `extension` (e.g. "mp3") helps format detection.
    pub fn decode(bytes: Vec<u8>, extension: Option<&str>) -> Result<Self, Error> {
        let err = |e: &dyn std::fmt::Display| Error::Decode(e.to_string());
        let mss = MediaSourceStream::new(Box::new(Cursor::new(bytes)), Default::default());
        let mut hint = Hint::new();
        if let Some(ext) = extension {
            hint.with_extension(ext);
        }
        let probed = symphonia::default::get_probe()
            .format(
                &hint,
                mss,
                &FormatOptions::default(),
                &MetadataOptions::default(),
            )
            .map_err(|e| err(&e))?;
        let mut format = probed.format;
        let track = format
            .default_track()
            .ok_or_else(|| Error::Decode("no audio track".into()))?;
        let id = track.id;
        let rate = track.codec_params.sample_rate.unwrap_or(44_100);
        let mut decoder = symphonia::default::get_codecs()
            .make(&track.codec_params, &DecoderOptions::default())
            .map_err(|e| err(&e))?;
        let mut samples = Vec::new();
        loop {
            let packet = match format.next_packet() {
                Ok(p) => p,
                Err(SymError::IoError(e)) if e.kind() == std::io::ErrorKind::UnexpectedEof => break,
                Err(SymError::ResetRequired) => break,
                Err(e) => return Err(err(&e)),
            };
            if packet.track_id() != id {
                continue;
            }
            let decoded = match decoder.decode(&packet) {
                Ok(d) => d,
                Err(SymError::DecodeError(_)) => continue,
                Err(e) => return Err(err(&e)),
            };
            let spec = *decoded.spec();
            let channels = spec.channels.count().max(1);
            let mut buf = SampleBuffer::<f32>::new(decoded.capacity() as u64, spec);
            buf.copy_interleaved_ref(decoded);
            for frame in buf.samples().chunks(channels) {
                let l = frame[0];
                let r = if channels > 1 { frame[1] } else { l };
                samples.push(l);
                samples.push(r);
            }
        }
        if samples.is_empty() {
            return Err(Error::Decode("no audio samples".into()));
        }
        Ok(Self { rate, samples })
    }

    pub fn load(path: &std::path::Path) -> Result<Self, Error> {
        let bytes =
            std::fs::read(path).map_err(|e| Error::Decode(format!("{}: {e}", path.display())))?;
        Self::decode(bytes, path.extension().and_then(|e| e.to_str()))
    }

    /// Resamples to `rate` (linear interpolation).
    pub fn resampled(&self, rate: u32) -> Self {
        if rate == self.rate || self.rate == 0 {
            return self.clone();
        }
        let ratio = self.rate as f64 / rate as f64;
        let frames = (self.frames() as f64 / ratio) as usize;
        let mut samples = Vec::with_capacity(frames * 2);
        let last = self.frames().saturating_sub(1);
        for i in 0..frames {
            let pos = i as f64 * ratio;
            let a = (pos as usize).min(last);
            let b = (a + 1).min(last);
            let t = (pos - a as f64) as f32;
            for c in 0..2 {
                let (x, y) = (self.samples[a * 2 + c], self.samples[b * 2 + c]);
                samples.push(x + (y - x) * t);
            }
        }
        Self { rate, samples }
    }

    /// Min/max envelope with `per_second` buckets, for waveform display.
    pub fn peaks(&self, per_second: u32) -> Vec<(f32, f32)> {
        let size = (self.rate / per_second.max(1)).max(1) as usize;
        self.samples
            .chunks(size * 2)
            .map(|c| {
                let (mut lo, mut hi) = (0f32, 0f32);
                for pair in c.chunks(2) {
                    let m = (pair[0] + pair.get(1).copied().unwrap_or(pair[0])) * 0.5;
                    lo = lo.min(m);
                    hi = hi.max(m);
                }
                (lo, hi)
            })
            .collect()
    }

    /// Onset strength per 10 ms frame (positive energy flux), for beat detection.
    pub fn onsets(&self) -> Vec<f32> {
        let hop = (self.rate / 100).max(1) as usize;
        let energy: Vec<f32> = self
            .samples
            .chunks(hop * 2)
            .map(|c| c.iter().map(|s| s * s).sum::<f32>() / c.len().max(1) as f32)
            .collect();
        let mut out = vec![0.0; energy.len()];
        for i in 1..energy.len() {
            let a = (energy[i - 1] + 1e-9).ln();
            let b = (energy[i] + 1e-9).ln();
            out[i] = (b - a).max(0.0);
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_wav_and_resamples() {
        let spec = hound::WavSpec {
            channels: 1,
            sample_rate: 22_050,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };
        let mut bytes = Vec::new();
        {
            let mut w = hound::WavWriter::new(std::io::Cursor::new(&mut bytes), spec).unwrap();
            for i in 0..22_050 {
                w.write_sample(((i as f32 * 0.05).sin() * 10_000.0) as i16)
                    .unwrap();
            }
            w.finalize().unwrap();
        }
        let clip = AudioClip::decode(bytes, Some("wav")).unwrap();
        assert_eq!(clip.rate, 22_050);
        assert!((clip.duration() - 1.0).abs() < 0.01);
        let r = clip.resampled(44_100);
        assert!((r.duration() - 1.0).abs() < 0.01);
        assert!((100..=101).contains(&clip.peaks(100).len()));
    }
}
