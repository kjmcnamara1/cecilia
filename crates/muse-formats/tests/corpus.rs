//! Parses every Guitar Pro file under `test-corpus/` (a local, gitignored directory).
//! Skipped when the directory is absent.

use std::path::{Path, PathBuf};

fn corpus_files(exts: &[&str]) -> Vec<PathBuf> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../test-corpus");
    let mut out = Vec::new();
    let mut stack = vec![root];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for e in entries.flatten() {
            let p = e.path();
            if p.is_dir() {
                stack.push(p);
            } else if p
                .extension()
                .and_then(|e| e.to_str())
                .is_some_and(|e| exts.contains(&e.to_ascii_lowercase().as_str()))
            {
                out.push(p);
            }
        }
    }
    out.sort();
    out
}

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
        match muse_formats::gp::read_with_trailing(&bytes) {
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
