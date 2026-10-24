//! Debug helper: `notecompare A B` reports how many sounding notes (onset on a 32nd/triplet
//! grid, pitch) of A are also in B. Files may be in any supported format.

use std::collections::HashSet;

fn notes(path: &str) -> HashSet<(u64, u8)> {
    let mut score =
        muse_formats::read_file(path.as_ref()).unwrap_or_else(|e| panic!("{path}: {e}"));
    // Strums offset chord notes by a few ticks; compare written onsets.
    for beat in score
        .tracks
        .iter_mut()
        .flat_map(|t| &mut t.bars)
        .flat_map(|b| &mut b.voices)
        .flat_map(|v| &mut v.beats)
    {
        beat.effects.stroke = None;
    }
    let perf = muse_core::Performance::build(&score, &vec![false; score.tracks.len()]);
    let grid = |t: u64| {
        let snap = |g: u64| (t + g / 2) / g * g;
        let (a, b) = (snap(120), snap(160));
        if a.abs_diff(t) <= b.abs_diff(t) { a } else { b }
    };
    perf.notes
        .iter()
        .filter(|n| n.channel != 9)
        .map(|n| (grid(n.start), n.key))
        .collect()
}

/// Track and written bar of the first note at (tick, key), for diagnostics.
fn locate(path: &str, tick: u64, key: u8) -> Option<(usize, usize, u64)> {
    let score = muse_formats::read_file(path.as_ref()).ok()?;
    let perf = muse_core::Performance::build(&score, &vec![false; score.tracks.len()]);
    let n = perf
        .notes
        .iter()
        .find(|n| n.key == key && n.start.abs_diff(tick) <= 80)?;
    let pb = perf.bars.iter().rev().find(|b| b.start <= n.start)?;
    Some((n.track, pb.bar, n.start - pb.start))
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let (a, b) = (notes(&args[1]), notes(&args[2]));
    let common = a.intersection(&b).count();
    println!(
        "{common}\t{}\t{}\t{:.1}%",
        a.len(),
        b.len(),
        common as f64 * 100.0 / a.len().max(1) as f64
    );
    if std::env::var_os("SHOW").is_some() {
        let mut x: Vec<_> = a.difference(&b).collect();
        let mut y: Vec<_> = b.difference(&a).collect();
        x.sort();
        y.sort();
        if let Ok(range) = std::env::var("RANGE") {
            let (lo, hi) = range.split_once('-').unwrap();
            let (lo, hi): (u64, u64) = (lo.parse().unwrap(), hi.parse().unwrap());
            let mut aa: Vec<_> = a.iter().filter(|n| n.0 >= lo && n.0 <= hi).collect();
            let mut bb: Vec<_> = b.iter().filter(|n| n.0 >= lo && n.0 <= hi).collect();
            aa.sort();
            bb.sort();
            println!("A {aa:?}\nB {bb:?}");
        }
        if let Some(first) = x.first() {
            println!(
                "first A-only note at {first:?}: (track, bar, offset) = {:?}",
                locate(&args[1], first.0, first.1)
            );
        }
        println!("only in A: {:?}", &x[..x.len().min(8)]);
        println!("only in B: {:?}", &y[..y.len().min(8)]);
    }
}
