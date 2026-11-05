fn main() {
    let path = std::env::args().nth(1).unwrap();
    let mut score = cecilia_formats::read_file(path.as_ref()).unwrap();
    let a = cecilia_audio::Sequence::build(
        &score,
        &cecilia_audio::SequenceOptions {
            audio_sync: true,
            ..Default::default()
        },
    );
    println!("padding {}", score.audio.as_ref().unwrap().frame_padding);
    for p in score.audio.as_ref().unwrap().points.iter().take(12) {
        println!("{p:?}");
    }
    score.audio.as_mut().unwrap().points.clear();
    let b = cecilia_audio::Sequence::build(
        &score,
        &cecilia_audio::SequenceOptions {
            audio_sync: true,
            ..Default::default()
        },
    );
    for (k, &(t, s, bpm)) in a.anchors.iter().enumerate().take(12) {
        println!(
            "anchor {k}: tick {t} synced {s:.3}s bpm {bpm:.2}  constant {:.3}s",
            b.time_at_tick(t)
        );
    }
}
