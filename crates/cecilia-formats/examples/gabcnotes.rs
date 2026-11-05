//! Prints the notation of GABC files as a normalised token list (clefs, alterations, divisions
//! and each note's pitch and shape class), for comparison with Gregorio's `-F dump` output.
//!
//! cargo run -p cecilia-formats --example gabcnotes -- file.gabc ...

use cecilia_core::chant::{Division, NeumeItem, Shape, pitch_letter};

fn main() {
    for path in std::env::args().skip(1) {
        let text = std::fs::read_to_string(&path).unwrap_or_default();
        println!("== {path}");
        let chant = match cecilia_formats::gabc::parse(&text) {
            Ok(c) => c,
            Err(e) => {
                println!("ERROR {e}");
                continue;
            }
        };
        for item in chant.syllables.iter().flat_map(|s| &s.items) {
            match item {
                NeumeItem::Clef(c) => println!("C {}", c.gabc()),
                NeumeItem::Alteration { pitch, sign } => {
                    let kind = match sign.chars().next() {
                        Some('x' | 'X') => "flat",
                        Some('#') => "sharp",
                        _ => "natural",
                    };
                    println!("A {} {kind}", pitch_letter(*pitch, false));
                }
                NeumeItem::Bar(b) => {
                    let d = match Division::of(b) {
                        Division::Virgula => "V",
                        Division::Minima => "MI",
                        Division::Minor => "MN",
                        Division::Maior => "MA",
                        Division::Finalis => "F",
                    };
                    println!("B {d}");
                }
                NeumeItem::Note(n) => {
                    let class = if n.mods.iter().any(|m| m == "W") {
                        "Q".to_string()
                    } else {
                        match n.shape() {
                            Shape::Punctum | Shape::Quadratum => "P".into(),
                            Shape::Inclinatum => "I".into(),
                            Shape::Virga => "V".into(),
                            Shape::VirgaReversa => "VR".into(),
                            Shape::Bivirga(k) => format!("V{k}"),
                            Shape::Stropha(1) => "S".into(),
                            Shape::Stropha(k) => format!("S{k}"),
                            Shape::Quilisma => "Q".into(),
                            Shape::Oriscus(_) => "O".into(),
                        }
                    };
                    println!("N {} {class}", pitch_letter(n.pitch, false));
                }
                _ => {}
            }
        }
    }
}
