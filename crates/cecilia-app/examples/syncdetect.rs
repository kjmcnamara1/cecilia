//! Checks `:sync-detect` against a score whose sync points are known: keeps only the first
//! point, re-detects the rest and prints the error at each original point.
//!
//! cargo run --release -p cecilia-app --example syncdetect -- song.gp

use cecilia_app::playback::Playback;
use cecilia_app::{App, Command};

fn main() {
    let path = std::env::args()
        .nth(1)
        .expect("usage: syncdetect <score.gp>");
    let mut app = App::new(Default::default(), None);
    app.run(Command::Edit(path.into()));
    let truth = app
        .buffer()
        .score
        .audio
        .clone()
        .expect("score has no audio sync");
    let pad = truth.frame_padding as f64 / cecilia_audio::SYNC_RATE;
    let secs = |frame: i64| frame as f64 / cecilia_audio::SYNC_RATE - pad;
    let mut playback = Playback::default();
    let buffer = app.pane().buffer;
    playback.ensure_audio(&app, buffer);
    while playback.loaded_audio(buffer).is_none() {
        assert!(playback.audio_loading(buffer), "audio failed to load");
        std::thread::sleep(std::time::Duration::from_millis(50));
        playback.sync(&mut app);
    }
    app.run(Command::SyncClear);
    app.add_sync_point(truth.points[0]);
    app.run(Command::SyncDetect);
    for effect in app.take_effects() {
        playback.apply(&mut app, effect);
    }
    let found = &app.buffer().score.audio.as_ref().unwrap().points;
    println!("{} points detected", found.len());
    for p in &truth.points {
        let Some(q) = found
            .iter()
            .find(|q| q.bar == p.bar && q.occurrence == p.occurrence)
        else {
            println!("bar {:>3}: not detected", p.bar + 1);
            continue;
        };
        let (a, b) = (secs(p.frame), secs(q.frame));
        println!(
            "bar {:>3}: expected {a:8.3}s  detected {b:8.3}s  error {:+6.0} ms",
            p.bar + 1,
            (b - a) * 1000.0
        );
    }
}
