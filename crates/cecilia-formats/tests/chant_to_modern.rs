//! A chant score converts to a modern score for modern formats (what `cecilia convert x.gabc
//! y.gp5` does): the cached bars as a pitched track, the syllables as lyrics.

use cecilia_formats::{gabc, read_bytes, write_bytes};

const GABC: &str = "name: t;\n%%\n(c4) Al(fg)le(h) lu(g)ia.(f.) (::)\n";

#[test]
fn a_chant_score_writes_to_modern_formats_as_its_cached_bars() {
    let chant = gabc::chant_score(gabc::parse(GABC).unwrap());
    assert!(chant.is_chant());
    let modern = chant.chant_to_modern().unwrap();
    assert!(!modern.is_chant());
    for ext in ["gp5", "mid", "musicxml"] {
        let bytes = write_bytes(&modern, ext).unwrap();
        let back = read_bytes(&bytes, None).unwrap();
        assert!(!back.is_chant(), "{ext}");
        let notes = |s: &cecilia_core::Score| -> usize {
            s.tracks[0]
                .bars
                .iter()
                .flat_map(|b| &b.voices[0].beats)
                .map(|b| b.notes.len())
                .sum()
        };
        assert_eq!(notes(&back), notes(&chant), "{ext}");
    }
    // The chant itself still writes back as GABC.
    let text = write_bytes(&chant, "gabc").unwrap();
    assert!(String::from_utf8(text).unwrap().contains("(c4)"));
}
