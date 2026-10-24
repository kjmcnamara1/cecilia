//! Debug helper: lists (and with a second argument, extracts) the files inside a GPX container.

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let bytes = std::fs::read(&args[1]).unwrap();
    for (name, data) in muse_formats::gpx::read_container(&bytes).unwrap() {
        println!("{name} {}", data.len());
        if let Some(dir) = args.get(2) {
            std::fs::write(std::path::Path::new(dir).join(&name), data).unwrap();
        }
    }
}
