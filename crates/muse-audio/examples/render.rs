//! Renders the start of a score to a WAV file with the built-in synth and prints levels:
//! `cargo run -p muse-audio --example render -- IN OUT.wav [SECONDS] [SOUNDFONT]`

use std::sync::Arc;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let (input, output) = (&args[1], &args[2]);
    let seconds: f64 = args.get(3).and_then(|s| s.parse().ok()).unwrap_or(20.0);
    let score = muse_formats::read_file(input.as_ref()).expect("read score");
    let seq = Arc::new(muse_audio::Sequence::build(&score, &Default::default()));
    let rate = 44_100;
    let (synth, name, warning) = muse_audio::make_synth(args.get(4).map(|s| s.as_ref()), rate);
    if let Some(w) = warning {
        eprintln!("warning: {w}");
    }
    let samples = muse_audio::render_offline(seq, synth, rate, seconds);
    let peak = samples.iter().fold(0f32, |m, s| m.max(s.abs()));
    let rms = (samples.iter().map(|s| s * s).sum::<f32>() / samples.len() as f32).sqrt();
    let clipped = samples.iter().filter(|s| s.abs() > 1.0).count();
    println!("{name}: peak {peak:.3}, rms {rms:.3}, clipped samples {clipped}");

    let mut wav = Vec::new();
    let data_len = samples.len() as u32 * 2;
    wav.extend(b"RIFF");
    wav.extend((36 + data_len).to_le_bytes());
    wav.extend(b"WAVEfmt ");
    wav.extend(16u32.to_le_bytes());
    wav.extend(1u16.to_le_bytes());
    wav.extend(2u16.to_le_bytes());
    wav.extend(rate.to_le_bytes());
    wav.extend((rate * 4).to_le_bytes());
    wav.extend(4u16.to_le_bytes());
    wav.extend(16u16.to_le_bytes());
    wav.extend(b"data");
    wav.extend(data_len.to_le_bytes());
    for s in samples {
        wav.extend(((s.clamp(-1.0, 1.0) * 32767.0) as i16).to_le_bytes());
    }
    std::fs::write(output, wav).expect("write wav");
}
