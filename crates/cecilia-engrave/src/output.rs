//! Rendering pages to SVG, PNG and PDF. Every item is reduced to filled or stroked outlines.

use kurbo::{BezPath, PathEl, Shape};

use crate::display::{Color, Item, Page};
use crate::fonts::fonts;

/// How an outline is painted.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Paint {
    Fill(Color),
    Stroke(Color, f64),
}

/// Converts a display item to outlines.
pub fn outlines(item: &Item) -> Vec<(BezPath, Paint)> {
    match item {
        Item::Line {
            from,
            to,
            width,
            color,
        } => {
            let mut p = BezPath::new();
            p.move_to(*from);
            p.line_to(*to);
            vec![(p, Paint::Stroke(*color, *width))]
        }
        Item::Rect { rect, color } => vec![(rect.to_path(0.1), Paint::Fill(*color))],
        Item::Path {
            path,
            color,
            stroke,
        } => vec![(
            path.clone(),
            match stroke {
                Some(w) => Paint::Stroke(*color, *w),
                None => Paint::Fill(*color),
            },
        )],
        Item::Glyph {
            font,
            ch,
            at,
            size,
            color,
        } => fonts()
            .glyph_path(*font, *ch, at.x, at.y, *size)
            .map(|p| vec![(p, Paint::Fill(*color))])
            .unwrap_or_default(),
        Item::Text {
            font,
            text,
            at,
            size,
            align,
            color,
        } => {
            vec![(
                fonts().text_path(*font, text, at.x, at.y, *size, *align),
                Paint::Fill(*color),
            )]
        }
    }
}

fn hex(c: Color) -> String {
    format!("#{:02x}{:02x}{:02x}", c.r, c.g, c.b)
}

fn svg_path_data(p: &BezPath) -> String {
    let mut d = String::new();
    for el in p.elements() {
        use std::fmt::Write;
        let _ = match el {
            PathEl::MoveTo(a) => write!(d, "M{:.2} {:.2}", a.x, a.y),
            PathEl::LineTo(a) => write!(d, "L{:.2} {:.2}", a.x, a.y),
            PathEl::QuadTo(a, b) => write!(d, "Q{:.2} {:.2} {:.2} {:.2}", a.x, a.y, b.x, b.y),
            PathEl::CurveTo(a, b, c) => write!(
                d,
                "C{:.2} {:.2} {:.2} {:.2} {:.2} {:.2}",
                a.x, a.y, b.x, b.y, c.x, c.y
            ),
            PathEl::ClosePath => write!(d, "Z"),
        };
    }
    d
}

pub fn to_svg(page: &Page) -> String {
    let mut out = format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{w}pt\" height=\"{h}pt\" viewBox=\"0 0 {w} {h}\">\n<rect width=\"100%\" height=\"100%\" fill=\"#fff\"/>\n",
        w = page.width,
        h = page.height
    );
    for item in &page.items {
        for (path, paint) in outlines(item) {
            let d = svg_path_data(&path);
            match paint {
                Paint::Fill(c) => out.push_str(&format!("<path d=\"{d}\" fill=\"{}\"/>\n", hex(c))),
                Paint::Stroke(c, w) => out.push_str(&format!(
                    "<path d=\"{d}\" fill=\"none\" stroke=\"{}\" stroke-width=\"{w:.2}\"/>\n",
                    hex(c)
                )),
            }
        }
    }
    out.push_str("</svg>\n");
    out
}

fn skia_path(p: &BezPath) -> Option<tiny_skia::Path> {
    let mut b = tiny_skia::PathBuilder::new();
    for el in p.elements() {
        match *el {
            PathEl::MoveTo(a) => b.move_to(a.x as f32, a.y as f32),
            PathEl::LineTo(a) => b.line_to(a.x as f32, a.y as f32),
            PathEl::QuadTo(a, c) => b.quad_to(a.x as f32, a.y as f32, c.x as f32, c.y as f32),
            PathEl::CurveTo(a, c, d) => b.cubic_to(
                a.x as f32, a.y as f32, c.x as f32, c.y as f32, d.x as f32, d.y as f32,
            ),
            PathEl::ClosePath => b.close(),
        }
    }
    b.finish()
}

/// Rasterises a page at `scale` pixels per point to PNG bytes.
pub fn to_png(page: &Page, scale: f64) -> Result<Vec<u8>, String> {
    let w = (page.width * scale).ceil().max(1.0) as u32;
    let h = (page.height * scale).ceil().max(1.0) as u32;
    let mut pixmap = tiny_skia::Pixmap::new(w, h).ok_or("page too large")?;
    pixmap.fill(tiny_skia::Color::WHITE);
    let transform = tiny_skia::Transform::from_scale(scale as f32, scale as f32);
    for item in &page.items {
        for (path, paint) in outlines(item) {
            let Some(p) = skia_path(&path) else { continue };
            let mut sp = tiny_skia::Paint {
                anti_alias: true,
                ..Default::default()
            };
            match paint {
                Paint::Fill(c) => {
                    sp.set_color_rgba8(c.r, c.g, c.b, c.a);
                    pixmap.fill_path(&p, &sp, tiny_skia::FillRule::Winding, transform, None);
                }
                Paint::Stroke(c, width) => {
                    sp.set_color_rgba8(c.r, c.g, c.b, c.a);
                    // At least one device pixel, so thin lines survive low resolutions.
                    let width = width.max(1.0 / scale);
                    let stroke = tiny_skia::Stroke {
                        width: width as f32,
                        ..Default::default()
                    };
                    pixmap.stroke_path(&p, &sp, &stroke, transform, None);
                }
            }
        }
    }
    pixmap.encode_png().map_err(|e| e.to_string())
}

/// Writes pages as a PDF document (vector outlines, no embedded fonts).
pub fn to_pdf(pages: &[Page]) -> Vec<u8> {
    use pdf_writer::{Content, Finish, Pdf, Rect as PdfRect, Ref};
    let mut pdf = Pdf::new();
    let catalog = Ref::new(1);
    let tree = Ref::new(2);
    let mut next = 3;
    let mut page_refs = Vec::new();
    for page in pages {
        let page_ref = Ref::new(next);
        let content_ref = Ref::new(next + 1);
        next += 2;
        page_refs.push(page_ref);
        let mut c = Content::new();
        // PDF's y axis points up: flip the page.
        c.transform([1.0, 0.0, 0.0, -1.0, 0.0, page.height as f32]);
        for item in &page.items {
            for (path, paint) in outlines(item) {
                let mut empty = true;
                let mut cur = kurbo::Point::ZERO;
                for el in path.elements() {
                    empty = false;
                    match *el {
                        PathEl::MoveTo(a) => {
                            c.move_to(a.x as f32, a.y as f32);
                            cur = a;
                        }
                        PathEl::LineTo(a) => {
                            c.line_to(a.x as f32, a.y as f32);
                            cur = a;
                        }
                        PathEl::QuadTo(a, b) => {
                            // Elevate the quadratic to a cubic.
                            let p1 = cur + (a - cur) * (2.0 / 3.0);
                            let p2 = b + (a - b) * (2.0 / 3.0);
                            c.cubic_to(
                                p1.x as f32,
                                p1.y as f32,
                                p2.x as f32,
                                p2.y as f32,
                                b.x as f32,
                                b.y as f32,
                            );
                            cur = b;
                        }
                        PathEl::CurveTo(a, b, d) => {
                            c.cubic_to(
                                a.x as f32, a.y as f32, b.x as f32, b.y as f32, d.x as f32,
                                d.y as f32,
                            );
                            cur = d;
                        }
                        PathEl::ClosePath => {
                            c.close_path();
                        }
                    }
                }
                if empty {
                    continue;
                }
                match paint {
                    Paint::Fill(col) => {
                        c.set_fill_rgb(
                            col.r as f32 / 255.0,
                            col.g as f32 / 255.0,
                            col.b as f32 / 255.0,
                        );
                        c.fill_nonzero();
                    }
                    Paint::Stroke(col, w) => {
                        c.set_stroke_rgb(
                            col.r as f32 / 255.0,
                            col.g as f32 / 255.0,
                            col.b as f32 / 255.0,
                        );
                        c.set_line_width(w as f32);
                        c.stroke();
                    }
                }
            }
        }
        pdf.stream(content_ref, &c.finish());
        let mut p = pdf.page(page_ref);
        p.media_box(PdfRect::new(
            0.0,
            0.0,
            page.width as f32,
            page.height as f32,
        ));
        p.parent(tree);
        p.contents(content_ref);
        p.finish();
    }
    pdf.pages(tree)
        .kids(page_refs.iter().copied())
        .count(page_refs.len() as i32);
    pdf.catalog(catalog).pages(tree);
    pdf.finish()
}
