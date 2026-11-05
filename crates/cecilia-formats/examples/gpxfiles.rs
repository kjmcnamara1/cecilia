//! Debug helper: lists (and with a second argument, extracts) the files inside a GPX container.

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let bytes = std::fs::read(&args[1]).unwrap();
    if args.get(2).map(String::as_str) == Some("--image") {
        std::fs::write(
            &args[3],
            cecilia_formats::gpx::filesystem_image(&bytes).unwrap(),
        )
        .unwrap();
        return;
    }
    for (name, data) in cecilia_formats::gpx::read_container(&bytes).unwrap() {
        println!("{name} {}", data.len());
        if let Some(dir) = args.get(2) {
            std::fs::write(std::path::Path::new(dir).join(&name), data).unwrap();
        }
    }
}
