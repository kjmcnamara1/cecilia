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

/// A punctum (square note) centred on `(cx, cy)`: crisp flat sides, an arched top and a gently
/// concave bottom, as in engraved chant books. `square` is the lower note of a pes: a block with
/// a flat top and rounded lower corners.
pub fn punctum(cx: f64, cy: f64, w: f64, h: f64, square: bool) -> BezPath {
    let (x0, x1, y0, y1) = (cx - w / 2.0, cx + w / 2.0, cy - h / 2.0, cy + h / 2.0);
    let mut p = BezPath::new();
    if square {
        let r = 0.16 * w;
        p.move_to((x0, y0));
        p.line_to((x1, y0));
        p.line_to((x1, y1 - r));
        p.quad_to((x1, y1), (x1 - r, y1));
        p.line_to((x0 + r, y1));
        p.quad_to((x0, y1), (x0, y1 - r));
    } else {
        let a = 0.14 * h;
        p.move_to((x0, y0 + a));
        p.quad_to((cx, y0 - a), (x1, y0 + a));
        p.line_to((x1, y1));
        p.quad_to((cx, y1 - 2.0 * a), (x0, y1));
    }
    p.close_path();
    p
}

/// A punctum inclinatum: a diamond with crisp corners.
pub fn inclinatum(cx: f64, cy: f64, w: f64, h: f64) -> BezPath {
    let mut p = BezPath::new();
    p.move_to((cx, cy - h / 2.0));
    p.line_to((cx + w / 2.0, cy));
    p.line_to((cx, cy + h / 2.0));
    p.line_to((cx - w / 2.0, cy));
    p.close_path();
    p
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

/// The bracket of the C clef: a tall bar with rounded ends and a big round notch cut into its
/// right side on the clef's line `(x, y)` (its left edge), `g` the gap between staff lines.
fn clef_bar(x: f64, y: f64, g: f64) -> BezPath {
    let (w, hh, n) = (0.56 * g, 0.86 * g, 0.42 * g);
    let k = 0.5523 * n;
    let x1 = x + w;
    // Rounded ends: quarter ellipses, wide on the left and tight on the right.
    let (rl, rr, ry) = (0.52 * w, 0.2 * w, 0.3 * g);
    let mut p = BezPath::new();
    p.move_to((x + rl, y - hh));
    p.line_to((x1 - rr, y - hh));
    p.curve_to(
        (x1 - rr * 0.4, y - hh),
        (x1, y - hh + ry * 0.7),
        (x1, y - hh + ry),
    );
    p.line_to((x1, y - n));
    p.curve_to((x1 - k, y - n), (x1 - n, y - k), (x1 - n, y));
    p.curve_to((x1 - n, y + k), (x1 - k, y + n), (x1, y + n));
    p.line_to((x1, y + hh - ry));
    p.curve_to(
        (x1, y + hh - ry * 0.7),
        (x1 - rr * 0.4, y + hh),
        (x1 - rr, y + hh),
    );
    p.line_to((x + rl, y + hh));
    p.curve_to(
        (x + rl * 0.4, y + hh),
        (x, y + hh - ry * 0.7),
        (x, y + hh - ry),
    );
    p.line_to((x, y - hh + ry));
    p.curve_to(
        (x, y - hh + ry * 0.7),
        (x + rl * 0.4, y - hh),
        (x + rl, y - hh),
    );
    p.close_path();
    p
}

/// A C (do) clef on the line at `(x, y)`: a tall rounded bar with a round notch.
pub fn do_clef(x: f64, y: f64, g: f64) -> BezPath {
    clef_bar(x, y, g)
}

/// An F (fa) clef: the same bar, and two squares either side of the line its F is on.
pub fn fa_clef(x: f64, y: f64, g: f64) -> Vec<BezPath> {
    let dot = |dy: f64| punctum(x + 1.0 * g, y + dy * g, 0.4 * g, 0.36 * g, false);
    vec![clef_bar(x, y, g), dot(-0.45), dot(0.45)]
}

/// A custos with its right edge at `x` on the line of height `y`: a small block with a curved
/// left top and a thin stem up (or down) its right side.
pub fn custos(x: f64, y: f64, g: f64, tail_up: bool) -> BezPath {
    let s = if tail_up { -1.0 } else { 1.0 };
    let (w, t) = (0.46 * g, 0.05 * g);
    let mut p = BezPath::new();
    p.move_to((x - w, y + 0.26 * g));
    p.line_to((x - w, y - 0.04 * g));
    p.quad_to((x - w, y - 0.3 * g), (x - w * 0.2, y - 0.3 * g));
    p.line_to((x - t, y - 0.3 * g));
    p.line_to((x - t, y + s * 1.0 * g));
    p.line_to((x + t, y + s * 1.0 * g));
    p.line_to((x + t, y + 0.3 * g));
    p.line_to((x - w * 0.2, y + 0.3 * g));
    p.quad_to((x - w, y + 0.3 * g), (x - w, y + 0.26 * g));
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
