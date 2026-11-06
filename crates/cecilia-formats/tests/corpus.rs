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

/// Lyrics read from every corpus file become gabc+ text that lays the syllables out where the
/// file put them.
#[test]
fn lyrics_become_gabc_plus_text_in_place() {
    let files = corpus_files(&["gp3", "gp4", "gp5", "gpx", "gp"]);
    if files.is_empty() {
        eprintln!("no corpus files; skipping");
        return;
    }
    let lyrics = |s: &cecilia_core::Score| -> Vec<(usize, usize, usize, Vec<String>)> {
        let mut out = Vec::new();
        for (t, track) in s.tracks.iter().enumerate() {
            for (b, bar) in track.bars.iter().enumerate() {
                for (i, beat) in bar
                    .voices
                    .first()
                    .map_or(&[][..], |v| &v.beats)
                    .iter()
                    .enumerate()
                {
                    let mut l = beat.lyrics.clone();
                    while l.last().is_some_and(|s| s.is_empty()) {
                        l.pop();
                    }
                    if !l.is_empty() {
                        out.push((t, b, i, l));
                    }
                }
            }
        }
        out
    };
    let (mut with, mut moved, mut rows) = (0, Vec::new(), 0);
    for f in &files {
        let bytes = std::fs::read(f).unwrap();
        let ext = f.extension().and_then(|e| e.to_str()).unwrap_or("");
        let raw = match ext {
            "gpx" => cecilia_formats::gpif::read_gpx(&bytes),
            "gp" => cecilia_formats::gpif::read_gp7(&bytes),
            _ => cecilia_formats::gp::read(&bytes),
        };
        let Ok(raw) = raw else { continue };
        let read = cecilia_formats::read_bytes(&bytes, Some(f)).unwrap();
        let (a, b) = (lyrics(&raw), lyrics(&read));
        if a.is_empty() {
            continue;
        }
        with += 1;
        // The same syllables on the same beats (a verse row may differ where the text can't
        // say it).
        let flat =
            |v: &[(usize, usize, usize, Vec<String>)]| -> Vec<(usize, usize, usize, Vec<String>)> {
                let mut v: Vec<_> = v
                    .iter()
                    .map(|(t, b, i, l)| {
                        // A beat of dashes is a dash word now; a run of dashes is one.
                        let mut l: Vec<String> = l
                            .iter()
                            .filter(|s| !s.trim().is_empty() && !s.chars().all(|c| c == '-'))
                            .map(|s| {
                                let t = s.trim_end_matches('-');
                                if t.len() < s.len() {
                                    format!("{t}-")
                                } else {
                                    s.clone()
                                }
                            })
                            .collect();
                        l.sort();
                        (*t, *b, *i, l)
                    })
                    .filter(|x| !x.3.is_empty())
                    .collect();
                v.sort();
                v
            };
        if flat(&a) != flat(&b) {
            let first = flat(&a).into_iter().zip(flat(&b)).find(|(x, y)| x != y);
            moved.push(format!("{}: {first:?}", f.display()));
            if std::env::var("LYRICS_DEBUG").is_ok_and(|v| f.display().to_string().contains(&v)) {
                for t in &read.tracks {
                    eprintln!("{}", t.lyrics_text);
                }
            }
        } else {
            // Rows, with dash-only beats as none.
            let rows_of = |v: &[(usize, usize, usize, Vec<String>)]| -> Vec<_> {
                v.iter()
                    .map(|(t, b, i, l)| {
                        let mut l: Vec<String> = l
                            .iter()
                            .map(|s| {
                                if s.chars().all(|c| c == '-') {
                                    String::new()
                                } else {
                                    s.trim_end_matches('-').to_string()
                                }
                            })
                            .collect();
                        while l.last().is_some_and(String::is_empty) {
                            l.pop();
                        }
                        (*t, *b, *i, l)
                    })
                    .filter(|x| !x.3.is_empty())
                    .collect()
            };
            if rows_of(&a) != rows_of(&b) {
                rows += 1;
                eprintln!("rows: {}", f.display());
            }
        }
    }
    eprintln!(
        "{with} files with lyrics, {} with syllables moved, {rows} with verse rows changed",
        moved.len()
    );
    assert!(moved.is_empty(), "\n{}", moved.join("\n"));
}
