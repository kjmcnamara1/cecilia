//! `cargo run -p cecilia-engrave --example engrave -- FILE TRACK OUT_PREFIX [page|vertical|horizontal]`
//! writes `OUT_PREFIX-N.png` per page (and `OUT_PREFIX.pdf`).

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let score = cecilia_formats::read_file(a[1].as_ref()).expect("read");
    let track: usize = a[2].parse().unwrap();
    let mode = match a.get(4).map(String::as_str) {
        Some("vertical") => cecilia_engrave::Mode::Vertical { width: 900.0 },
        Some("horizontal") => cecilia_engrave::Mode::Horizontal,
        _ => cecilia_engrave::Mode::Page {
            width: 595.0,
            height: 842.0,
        },
    };
    let t = std::time::Instant::now();
    let layout = cecilia_engrave::engrave(
        &score,
        track,
        &cecilia_engrave::Options {
            mode,
            ..Default::default()
        },
    );
    eprintln!("layout {:?}, {} pages", t.elapsed(), layout.pages.len());
    for (i, p) in layout.pages.iter().enumerate().take(2) {
        std::fs::write(
            format!("{}-{}.png", a[3], i + 1),
            cecilia_engrave::output::to_png(p, 1.5).unwrap(),
        )
        .unwrap();
    }
    std::fs::write(
        format!("{}.pdf", a[3]),
        cecilia_engrave::output::to_pdf(&layout.pages),
    )
    .unwrap();
}
