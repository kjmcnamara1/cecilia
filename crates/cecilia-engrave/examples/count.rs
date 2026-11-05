//! Renders a clip of page 1 at high resolution: `count FILE OUT.png`.
fn main() {
    let a: Vec<String> = std::env::args().collect();
    let score = cecilia_formats::read_file(a[1].as_ref()).unwrap();
    let l = cecilia_engrave::engrave(&score, 0, &Default::default());
    let stems = l.pages[0].items.iter().filter(|i| matches!(i, cecilia_engrave::display::Item::Line { from, to, width, .. } if from.x == to.x && *width < 0.7)).count();
    eprintln!("thin vertical lines: {stems}");
    let mut p = l.pages[0].clone();
    p.width = 300.0;
    p.height = 230.0;
    std::fs::write(&a[2], cecilia_engrave::output::to_png(&p, 4.0).unwrap()).unwrap();
}
