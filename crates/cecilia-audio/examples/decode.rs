//! `decode FILE`: decodes an audio file and prints its length and decode time.
fn main() {
    let path = std::env::args().nth(1).expect("usage: decode FILE");
    let t = std::time::Instant::now();
    let clip = cecilia_audio::AudioClip::load(path.as_ref()).expect("decode");
    let d = t.elapsed();
    let r = clip.resampled(48_000);
    println!(
        "{:.1} s at {} Hz, decoded in {:?}, resampled in {:?}",
        clip.duration(),
        clip.rate,
        d,
        t.elapsed() - d
    );
    let _ = r;
}
