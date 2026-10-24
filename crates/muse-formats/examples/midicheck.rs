//! Debug helper: exports a score to MIDI, re-imports it and compares sounding notes per track.

use std::collections::BTreeMap;

fn main() {
    let path = std::env::args().nth(1).expect("usage: midicheck FILE");
    let original = muse_formats::read_file(path.as_ref()).unwrap();
    let back =
        muse_formats::midi::read_midi(&muse_formats::midi::write_midi(&original).unwrap()).unwrap();
    let notes = |s: &muse_core::Score| {
        let perf = muse_core::Performance::build(s, &vec![false; s.tracks.len()]);
        let mut by_channel: BTreeMap<u8, Vec<(u64, u8)>> = BTreeMap::new();
        for n in perf.notes {
            by_channel
                .entry(n.channel)
                .or_default()
                .push((n.start, n.key));
        }
        by_channel
    };
    let (a, b) = (notes(&original), notes(&back));
    for (ch, x) in &a {
        let y = b.get(ch).cloned().unwrap_or_default();
        let missing: Vec<_> = x.iter().filter(|n| !y.contains(n)).take(5).collect();
        println!(
            "channel {ch}: {} vs {} notes; first missing {missing:?}",
            x.len(),
            y.len()
        );
    }
    for (i, t) in back.tracks.iter().enumerate() {
        println!(
            "back track {i} {:?} {:?} program {} ch {}",
            t.name, t.kind, t.midi.program, t.midi.channel
        );
    }
}
