//! Prints the GPIF XML inside a `.gpx` file (for inspecting Guitar Pro 6 files).
//!
//! cargo run -p muse-formats --example gpxgpif -- file.gpx

fn main() {
    let path = std::env::args().nth(1).expect("usage: gpxgpif FILE.gpx");
    let bytes = std::fs::read(path).unwrap();
    for (name, data) in muse_formats::gpx::read_container(&bytes).unwrap() {
        if name == "score.gpif" {
            print!("{}", String::from_utf8_lossy(&data));
        }
    }
}
