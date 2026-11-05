//! Reads GABC files and writes them back with cecilia's writer into a directory (for round-trip
//! checks against Gregorio), reporting files whose re-read model differs.
//!
//! cargo run -p cecilia-formats --example gabcrewrite -- OUT_DIR file.gabc ...

fn main() {
    let mut args = std::env::args().skip(1);
    let out = std::path::PathBuf::from(args.next().expect("usage: gabcrewrite OUT_DIR FILES"));
    std::fs::create_dir_all(&out).unwrap();
    let (mut same, mut differ) = (0, 0);
    for (i, path) in args.enumerate() {
        let Ok(text) = std::fs::read_to_string(&path) else {
            continue;
        };
        let Ok(chant) = cecilia_formats::gabc::parse(&text) else {
            continue;
        };
        let written = cecilia_formats::gabc::write(&chant, "");
        let again = cecilia_formats::gabc::parse(&written).unwrap();
        if again == chant {
            same += 1;
        } else {
            differ += 1;
            println!("model differs: {path}");
        }
        std::fs::write(out.join(format!("{i:04}.gabc")), written).unwrap();
        println!("{i:04} {path}");
    }
    eprintln!("{same} identical, {differ} differ");
}
