//! Parses every Guitar Pro file under `test-corpus/` (a local, gitignored directory).
//! Skipped when the directory is absent.

mod common;

use common::corpus_files;

#[test]
fn gp3_to_gp5_corpus_parses_to_eof() {
    let files = corpus_files(&["gp3", "gp4", "gp5"]);
    if files.is_empty() {
        eprintln!("no corpus files; skipping");
        return;
    }
    let mut failures = Vec::new();
    for f in &files {
        let bytes = std::fs::read(f).unwrap();
        match cecilia_formats::gp::read_with_trailing(&bytes) {
            Ok((score, trailing)) => {
                assert!(!score.tracks.is_empty(), "{}: no tracks", f.display());
                for t in &score.tracks {
                    assert_eq!(t.bars.len(), score.master_bars.len(), "{}", f.display());
                }
                // GP3/4 writers commonly pad with zeros; anything else is worth a look.
                let tail = &bytes[bytes.len() - trailing..];
                if tail.iter().any(|&b| b != 0) {
                    eprintln!("warning: {}: {trailing} unread bytes", f.display());
                }
            }
            Err(e) => failures.push(format!("{}: {e}", f.display())),
        }
    }
    eprintln!("{} files, {} failures", files.len(), failures.len());
    assert!(failures.is_empty(), "\n{}", failures.join("\n"));
}

/// Fraction of non-empty voice-1 bars whose beats fill the time signature exactly.
fn full_bar_ratio(score: &cecilia_core::Score) -> f64 {
    let (mut full, mut total) = (0, 0);
    for t in &score.tracks {
        for (i, bar) in t.bars.iter().enumerate() {
            let Some(v) = bar.voices.first() else {
                continue;
            };
            let ticks: u32 = v.beats.iter().map(|b| b.ticks()).sum();
            if ticks == 0 {
                continue;
            }
            total += 1;
            full += usize::from(ticks == score.played_ticks(i));
        }
    }
    if total == 0 {
        1.0
    } else {
        full as f64 / total as f64
    }
}

#[test]
fn gpx_and_gp7_corpus_parses() {
    let files = corpus_files(&["gpx", "gp"]);
    if files.is_empty() {
        eprintln!("no corpus files; skipping");
        return;
    }
    let mut failures = Vec::new();
    for f in &files {
        match cecilia_formats::read_file(f) {
            Ok(score) => {
                assert!(!score.tracks.is_empty(), "{}: no tracks", f.display());
                for t in &score.tracks {
                    assert_eq!(t.bars.len(), score.master_bars.len(), "{}", f.display());
                }
                // Real-world tabs contain some incomplete bars; a low ratio means a misread.
                let ratio = full_bar_ratio(&score);
                if ratio < 0.5 {
                    failures.push(format!(
                        "{}: only {:.0}% of bars are full",
                        f.display(),
                        ratio * 100.0
                    ));
                } else if ratio < 0.9 {
                    eprintln!(
                        "warning: {}: {:.0}% of bars are full",
                        f.display(),
                        ratio * 100.0
                    );
                }
            }
            Err(e) => failures.push(format!("{}: {e}", f.display())),
        }
    }
    eprintln!("{} files, {} failures", files.len(), failures.len());
    assert!(failures.is_empty(), "\n{}", failures.join("\n"));
}
