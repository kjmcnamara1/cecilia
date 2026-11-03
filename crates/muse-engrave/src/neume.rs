//! Smooth shapes for square notation: rounded and slightly slanted note heads, the slanted
//! strokes that join the notes of a neume, the C and F clefs and the custos. Used by the chant
//! engraver and by the chant staff of ordinary tracks.

use kurbo::{BezPath, Point};

use crate::display::{Color, Item, Page};

/// A closed polygon with its corners rounded by `r` (clamped to half of the shortest side).
pub fn rounded_polygon(points: &[(f64, f64)], r: f64) -> BezPath {
    let n = points.len();
    let mut path = BezPath::new();
    let pt = |i: usize| Point::new(points[i % n].0, points[i % n].1);
    for i in 0..n {
        let (prev, cur, next) = (pt(i + n - 1), pt(i), pt(i + 1));
        let (a, b) = (prev - cur, next - cur);
        let k = r.min(a.hypot() / 2.0).min(b.hypot() / 2.0);
        let start = cur + a / a.hypot().max(1e-9) * k;
        let end = cur + b / b.hypot().max(1e-9) * k;
        if i == 0 {
            path.move_to(start);
        } else {
            path.line_to(start);
        }
        path.quad_to(cur, end);
    }
    path.close_path();
    path
}

/// A punctum (square note) centred on `(cx, cy)`: softly rounded, its top and bottom edges
/// sloping a little as in engraved chant books.
pub fn punctum(cx: f64, cy: f64, w: f64, h: f64) -> BezPath {
    let (x0, x1, y0, y1) = (cx - w / 2.0, cx + w / 2.0, cy - h / 2.0, cy + h / 2.0);
    let slope = 0.07 * h;
    rounded_polygon(
        &[
            (x0, y0 + slope),
            (x1, y0 - slope),
            (x1, y1 - slope),
            (x0, y1 + slope),
        ],
        0.26 * h,
    )
}

/// A punctum inclinatum: a rounded diamond.
pub fn inclinatum(cx: f64, cy: f64, w: f64, h: f64) -> BezPath {
    rounded_polygon(
        &[
            (cx, cy - h / 2.0),
            (cx + w / 2.0, cy),
            (cx, cy + h / 2.0),
            (cx - w / 2.0, cy),
        ],
        0.12 * w,
    )
}

/// The slanted stroke from the right edge of one note to the left edge of the next in a
/// neume: its upper and lower edges run from `(x0, y0)` to `(x1, y1)`, `thick` apart.
pub fn link(x0: f64, y0: f64, x1: f64, y1: f64, thick: f64) -> BezPath {
    let t = thick / 2.0;
    let mut p = BezPath::new();
    p.move_to((x0, y0 - t));
    p.line_to((x1, y1 - t));
    p.line_to((x1, y1 + t));
    p.line_to((x0, y0 + t));
    p.close_path();
    p
}

/// A C (do) clef centred on the line `(x, y)` (its left edge), `sp` a staff space: a round
/// backed bracket whose two arms end in square tips.
pub fn do_clef(x: f64, y: f64, sp: f64) -> BezPath {
    let (u, v) = (sp, sp);
    let pts = [
        (0.0, -1.05),
        (0.9, -1.05),
        (0.9, -0.3),
        (0.45, -0.3),
        (0.45, 0.3),
        (0.9, 0.3),
        (0.9, 1.05),
        (0.0, 1.05),
    ];
    let pts: Vec<(f64, f64)> = pts.iter().map(|&(a, b)| (x + a * u, y + b * v)).collect();
    rounded_polygon(&pts, 0.2 * sp)
}

/// An F (fa) clef: the same bracket reaching further down, and two dots either side of the
/// line its F is on.
pub fn fa_clef(x: f64, y: f64, sp: f64) -> Vec<BezPath> {
    let pts = [
        (0.0, -1.05),
        (0.85, -1.05),
        (0.85, -0.3),
        (0.45, -0.3),
        (0.45, 0.3),
        (0.85, 0.3),
        (0.85, 1.05),
        (0.0, 1.05),
    ];
    let pts: Vec<(f64, f64)> = pts.iter().map(|&(a, b)| (x + a * sp, y + b * sp)).collect();
    let dot = |dy: f64| punctum(x + 1.35 * sp, y + dy * sp, 0.55 * sp, 0.5 * sp);
    vec![rounded_polygon(&pts, 0.2 * sp), dot(-0.62), dot(0.62)]
}

/// A custos at `(x, y)` pointing to the next note: a small head with a curved tail up or down.
pub fn custos(x: f64, y: f64, sp: f64, tail_up: bool) -> BezPath {
    let s = if tail_up { -1.0 } else { 1.0 };
    let mut p = BezPath::new();
    p.move_to((x, y));
    p.line_to((x + 0.3 * sp, y - 0.12 * sp));
    p.line_to((x + 0.3 * sp, y + s * 0.8 * sp));
    p.line_to((x + 0.4 * sp, y + s * 1.05 * sp));
    p.line_to((x + 0.4 * sp, y + 0.12 * sp));
    p.line_to((x, y + 0.2 * sp));
    p.close_path();
    p
}

/// Fills a path black.
pub fn fill(page: &mut Page, path: BezPath) {
    page.items.push(Item::Path {
        path,
        color: Color::BLACK,
        stroke: None,
    });
}
