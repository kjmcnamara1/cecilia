//! `aligncheck SCORE`: how well the score's MIDI note times line up with onsets in its audio
//! track, for offsets around zero (a correct sync peaks at 0 ms).
use std::sync::Arc;

fn main() {
    let path = std::env::args()
        .nth(1)
        .expect("usage: aligncheck SCORE [AUDIO]");
    let mut score = cecilia_formats::read_file(path.as_ref()).expect("read");
    let audio = score.audio.as_ref().expect("no audio");
    let clip = match (&audio.embedded, std::env::args().nth(2)) {
        (_, Some(f)) => cecilia_audio::AudioClip::load(f.as_ref()).unwrap(),
        (Some(b), _) => cecilia_audio::AudioClip::decode((**b).clone(), Some("mp3")).unwrap(),
        _ => panic!("no audio data"),
    };
    let onsets = clip.onsets();
    let use_sync = std::env::var_os("NO_SYNC").is_none();
    if !use_sync {
        score.audio.as_mut().unwrap().points.clear();
    }
    let seq = Arc::new(cecilia_audio::Sequence::build(
        &score,
        &cecilia_audio::SequenceOptions {
            audio_sync: true,
            ..Default::default()
        },
    ));
    let (lo, hi) = std::env::var("WIN")
        .ok()
        .and_then(|w| {
            let (a, b) = w.split_once('-')?;
            Some((a.parse::<f64>().ok()?, b.parse::<f64>().ok()?))
        })
        .unwrap_or((f64::MIN, f64::MAX));
    let times: Vec<f64> = seq
        .events
        .iter()
        .filter(|e| matches!(e.event, cecilia_audio::MidiEvent::NoteOn { .. }))
        .map(|e| e.time)
        .filter(|t| *t >= lo && *t <= hi)
        .collect();
    let mut best = (0.0f32, 0i32);
    let mut row = Vec::new();
    for off in (-400..=400).step_by(20) {
        let s: f32 = times
            .iter()
            .filter_map(|t| {
                let i = ((t + off as f64 / 1000.0) * 100.0) as isize;
                (i > 0 && (i as usize) < onsets.len())
                    .then(|| onsets[i as usize - 1].max(onsets[i as usize]))
            })
            .sum::<f32>()
            / times.len().max(1) as f32;
        row.push(format!("{off}:{s:.3}"));
        if s > best.0 {
            best = (s, off);
        }
    }
    println!(
        "best offset {} ms (score {:.3}); {} notes",
        best.1,
        best.0,
        times.len()
    );
    println!("{}", row.join(" "));
}
