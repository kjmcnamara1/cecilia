//! Debug helper: `cargo run -p muse-formats --example gpdump FILE [TRACK BAR]` prints a score
//! summary, or the beats of one bar.

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let bytes = std::fs::read(args.get(1).expect("usage: gpdump FILE [TRACK BAR]")).unwrap();
    let (score, trailing) = muse_formats::gp::read_with_trailing(&bytes).expect("parse");
    if let (Some(t), Some(b)) = (args.get(2), args.get(3)) {
        let bar = &score.tracks[t.parse::<usize>().unwrap()].bars[b.parse::<usize>().unwrap()];
        for (v, voice) in bar.voices.iter().enumerate() {
            for beat in &voice.beats {
                println!("v{v} {beat:?}");
            }
        }
        return;
    }
    println!(
        "version {:?}, trailing {trailing}",
        score.extras.get("gp.version")
    );
    println!(
        "bars {}, tracks {}",
        score.master_bars.len(),
        score.tracks.len()
    );
    for (i, t) in score.tracks.iter().enumerate() {
        let short: Vec<String> = t
            .bars
            .iter()
            .zip(&score.master_bars)
            .enumerate()
            .filter_map(|(n, (b, mb))| {
                let got: u32 = b.voices[0].beats.iter().map(|x| x.ticks()).sum();
                (got != mb.ticks()).then(|| format!("{n}:{got}/{}", mb.ticks()))
            })
            .take(5)
            .collect();
        println!(
            "track {i} '{}' strings {:?} mismatched bars {short:?}",
            t.name, t.strings
        );
    }
}
