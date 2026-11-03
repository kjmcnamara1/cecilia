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

/// A punctum (square note) centred on `(cx, cy)`: crisp flat sides with an arched top and
/// bottom (a curve bulging up in the middle), as in engraved chant books. `square` makes the
/// arch nearly flat: the lower note of a pes is almost a square.
pub fn punctum(cx: f64, cy: f64, w: f64, h: f64, square: bool) -> BezPath {
    let (x0, x1, y0, y1) = (cx - w / 2.0, cx + w / 2.0, cy - h / 2.0, cy + h / 2.0);
    let a = if square { 0.04 * h } else { 0.13 * h };
    let mut p = BezPath::new();
    p.move_to((x0, y0 + a));
    p.quad_to((cx, y0 - a), (x1, y0 + a));
    p.line_to((x1, y1));
    p.quad_to((cx, y1 - 2.0 * a), (x0, y1));
    p.close_path();
    p
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

/// The vertical stroke joining the notes of a neume at the edge where they touch: `x` its
/// centre, from `y0` to `y1`.
pub fn joint(x: f64, y0: f64, y1: f64, thick: f64) -> BezPath {
    let t = thick / 2.0;
    let (a, b) = (y0.min(y1), y0.max(y1));
    let mut p = BezPath::new();
    p.move_to((x - t, a));
    p.line_to((x + t, a));
    p.line_to((x + t, b));
    p.line_to((x - t, b));
    p.close_path();
    p
}

/// The bracket of the C clef: a tall bar with rounded ends and a half-round notch cut into its
/// right side at the clef's line `(x, y)` (its left edge), `sp` a staff space.
fn clef_bar(x: f64, y: f64, sp: f64) -> BezPath {
    let (w, hh, r, n) = (0.95 * sp, 1.15 * sp, 0.46 * sp, 0.46 * sp);
    let k = 0.5523 * n;
    let (x1, cx) = (x + w, x + w);
    let mut p = BezPath::new();
    p.move_to((x + r, y - hh));
    p.line_to((x1 - r, y - hh));
    p.curve_to(
        (x1 - r * 0.45, y - hh),
        (x1, y - hh + r * 0.45),
        (x1, y - hh + r),
    );
    p.line_to((cx, y - n));
    p.curve_to((cx - k, y - n), (cx - n, y - k), (cx - n, y));
    p.curve_to((cx - n, y + k), (cx - k, y + n), (cx, y + n));
    p.line_to((x1, y + hh - r));
    p.curve_to(
        (x1, y + hh - r * 0.45),
        (x1 - r * 0.45, y + hh),
        (x1 - r, y + hh),
    );
    p.line_to((x + r, y + hh));
    p.curve_to(
        (x + r * 0.45, y + hh),
        (x, y + hh - r * 0.45),
        (x, y + hh - r),
    );
    p.line_to((x, y - hh + r));
    p.curve_to(
        (x, y - hh + r * 0.45),
        (x + r * 0.45, y - hh),
        (x + r, y - hh),
    );
    p.close_path();
    p
}

/// A C (do) clef on the line at `(x, y)`: a tall rounded bar with a round notch.
pub fn do_clef(x: f64, y: f64, sp: f64) -> BezPath {
    clef_bar(x, y, sp)
}

/// An F (fa) clef: the same bar, and two squares either side of the line its F is on.
pub fn fa_clef(x: f64, y: f64, sp: f64) -> Vec<BezPath> {
    let dot = |dy: f64| punctum(x + 1.4 * sp, y + dy * sp, 0.55 * sp, 0.5 * sp, false);
    vec![clef_bar(x, y, sp), dot(-0.62), dot(0.62)]
}

/// A custos at `(x, y)` pointing to the next note: a small head with a curved tail up or down.
pub fn custos(x: f64, y: f64, sp: f64, tail_up: bool) -> BezPath {
    let s = if tail_up { -1.0 } else { 1.0 };
    let mut p = BezPath::new();
    p.move_to((x, y + 0.25 * sp));
    p.line_to((x, y - 0.05 * sp));
    p.line_to((x + 0.7 * sp, y - 0.3 * sp));
    p.line_to((x + 0.7 * sp, y + s * 1.0 * sp));
    p.line_to((x + 0.85 * sp, y + s * 1.25 * sp));
    p.line_to((x + 0.85 * sp, y + 0.3 * sp));
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
