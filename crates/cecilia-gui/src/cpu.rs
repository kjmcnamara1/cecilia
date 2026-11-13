//! A wgpu-free renderer for recorded [`Canvas`]es, on `vello_cpu`.
//!
//! [`render`] replays the same commands the GPU path replays into vello, converting the kurbo 0.11
//! / peniko 0.4 types of the recording into the kurbo 0.13 / peniko 0.6 ones `vello_cpu` uses (the
//! conversion lives only here). Popup backdrop blur follows `blur.rs`: the page is rendered,
//! shrunk by [`blur::DOWNSCALE`], blurred, and substituted for the placeholder image popups fill
//! with.

use std::collections::HashMap;
use std::sync::Arc;

use anyhow::{Result, anyhow};
use vello::kurbo as k11;
use vello::peniko as p4;
use vello_cpu::kurbo as k13;
use vello_cpu::peniko as p6;
use vello_cpu::peniko::color::{AlphaColor, Srgb};
use vello_cpu::{
    Image as CpuImage, ImageSource, Level, PixelMetadata, Pixmap, RenderContext, RenderSettings,
    Resources,
};

use crate::blur::{self, DOWNSCALE};
use crate::canvas::{Canvas, Cmd, Paint};

/// Renders `base` and then `popups` (recorded in device pixels) at `size` over `base_color` and
/// returns RGBA8 pixels (`size.0 * size.1 * 4` bytes, row-major, opaque).
///
/// With popups and `blur_sigma > 0` (in pixels of the target) the popups' backdrop is the base
/// frame blurred: any image fill in `popups` made with `backdrop` (the placeholder from
/// [`blur::placeholder`], matched by its blob id) is drawn with the blurred copy instead.
pub fn render(
    base: &Canvas,
    popups: &Canvas,
    size: (u32, u32),
    base_color: p4::Color,
    blur_sigma: f64,
    backdrop: Option<&p4::Image>,
) -> Result<Vec<u8>> {
    let (w, h) = (
        u16::try_from(size.0).map_err(|_| anyhow!("size {}x{} too large", size.0, size.1))?,
        u16::try_from(size.1).map_err(|_| anyhow!("size {}x{} too large", size.0, size.1))?,
    );
    let mut resources = Resources::new();
    let mut ctx = context(w, h, base_color);
    let mut images = Images::default();
    replay(
        base.commands(),
        &mut ctx,
        k11::Affine::IDENTITY,
        &mut images,
    );
    let blurred = blur_sigma > 0.0 && !popups.commands().is_empty();
    let mut target = Pixmap::new(w, h);
    ctx.render(&mut target, &mut resources);
    if !blurred {
        replay(
            popups.commands(),
            &mut ctx,
            k11::Affine::IDENTITY,
            &mut images,
        );
        ctx.render(&mut target, &mut resources);
        return Ok(finish(target));
    }
    // Popups go over the finished base, so they replay into a context that starts with it.
    if let Some(backdrop) = backdrop {
        let low = blurred_copy(target.data_as_u8_slice(), size, blur_sigma);
        images.substitute(backdrop, low, blur::low_size(size));
    }
    let mut ctx = context(w, h, base_color);
    let base_image = Arc::new(target);
    ctx.set_paint(CpuImage {
        image: ImageSource::Pixmap(base_image),
        sampler: p6::ImageSampler::new().with_quality(p6::ImageQuality::Low),
    });
    ctx.fill_rect(&k13::Rect::new(0.0, 0.0, f64::from(w), f64::from(h)));
    replay(
        popups.commands(),
        &mut ctx,
        k11::Affine::IDENTITY,
        &mut images,
    );
    let mut out = Pixmap::new(w, h);
    ctx.render(&mut out, &mut resources);
    Ok(finish(out))
}

fn context(w: u16, h: u16, color: p4::Color) -> RenderContext {
    let mut ctx = RenderContext::new_with(
        w,
        h,
        RenderSettings {
            level: Level::new(),
            ..Default::default()
        },
    );
    ctx.set_paint(color_cpu(color));
    ctx.fill_rect(&k13::Rect::new(0.0, 0.0, f64::from(w), f64::from(h)));
    ctx
}

/// The pixmap as straight-alpha RGBA8 bytes.
fn finish(pixmap: Pixmap) -> Vec<u8> {
    pixmap.take_rgba8(p6::ImageAlphaType::Alpha)
}

/// `rgba` (premultiplied or opaque RGBA8, `size` pixels) shrunk to the blur size and blurred with
/// `sigma` pixels of the full-size frame. Returns premultiplied RGBA8 of [`blur::low_size`].
pub fn blurred_copy(rgba: &[u8], size: (u32, u32), sigma: f64) -> Vec<u8> {
    let low = blur::low_size(size);
    let shrunk = shrink(rgba, size, low);
    let sigma = ((sigma / f64::from(DOWNSCALE)) as f32).max(0.5);
    let mid = gaussian(&shrunk, low, sigma, true);
    gaussian(&mid, low, sigma, false)
}

/// Box-filter downscale of `src` to `low` (cells at the right and bottom edge average the pixels
/// that exist).
fn shrink(src: &[u8], (w, h): (u32, u32), (lw, lh): (u32, u32)) -> Vec<u8> {
    let mut out = vec![0u8; (lw * lh * 4) as usize];
    for ly in 0..lh {
        for lx in 0..lw {
            let mut sum = [0u32; 4];
            let mut n = 0;
            for y in ly * DOWNSCALE..((ly + 1) * DOWNSCALE).min(h) {
                for x in lx * DOWNSCALE..((lx + 1) * DOWNSCALE).min(w) {
                    let i = ((y * w + x) * 4) as usize;
                    for c in 0..4 {
                        sum[c] += u32::from(src[i + c]);
                    }
                    n += 1;
                }
            }
            let o = ((ly * lw + lx) * 4) as usize;
            for c in 0..4 {
                out[o + c] = ((sum[c] + n / 2) / n.max(1)) as u8;
            }
        }
    }
    out
}

/// One separable gaussian pass (along x when `horizontal`), edges clamped.
fn gaussian(src: &[u8], (w, h): (u32, u32), sigma: f32, horizontal: bool) -> Vec<u8> {
    let radius = (sigma * 3.0).ceil() as i32;
    let weights: Vec<f32> = (-radius..=radius)
        .map(|i| (-0.5 * (i * i) as f32 / (sigma * sigma)).exp())
        .collect();
    let total: f32 = weights.iter().sum();
    let (w, h) = (w as i32, h as i32);
    let mut out = vec![0u8; src.len()];
    for y in 0..h {
        for x in 0..w {
            let mut sum = [0f32; 4];
            for (k, wt) in weights.iter().enumerate() {
                let d = k as i32 - radius;
                let (sx, sy) = if horizontal {
                    ((x + d).clamp(0, w - 1), y)
                } else {
                    (x, (y + d).clamp(0, h - 1))
                };
                let i = ((sy * w + sx) * 4) as usize;
                for c in 0..4 {
                    sum[c] += wt * f32::from(src[i + c]);
                }
            }
            let o = ((y * w + x) * 4) as usize;
            for c in 0..4 {
                out[o + c] = (sum[c] / total).round().clamp(0.0, 255.0) as u8;
            }
        }
    }
    out
}

/// Converted images by blob id, plus the blurred backdrop standing in for the placeholder.
#[derive(Default)]
struct Images {
    by_id: HashMap<u64, Option<Arc<Pixmap>>>,
}

impl Images {
    /// Makes images with `placeholder`'s identity resolve to `premul` pixels of `size`.
    fn substitute(&mut self, placeholder: &p4::Image, premul: Vec<u8>, size: (u32, u32)) {
        let pixmap = Pixmap::from_parts(
            premul,
            size.0 as u16,
            size.1 as u16,
            PixelMetadata {
                may_have_transparency: true,
                alpha_type: p6::ImageAlphaType::AlphaPremultiplied,
            },
        );
        self.by_id
            .insert(placeholder.data.id(), Some(Arc::new(pixmap)));
    }

    fn get(&mut self, image: &p4::Image) -> Option<Arc<Pixmap>> {
        self.by_id
            .entry(image.data.id())
            .or_insert_with(|| {
                let (w, h) = (
                    u16::try_from(image.width).ok()?,
                    u16::try_from(image.height).ok()?,
                );
                let data = image.data.data().to_vec();
                (data.len() == usize::from(w) * usize::from(h) * 4).then(|| {
                    Arc::new(Pixmap::from_parts(
                        data,
                        w,
                        h,
                        PixelMetadata {
                            may_have_transparency: true,
                            alpha_type: p6::ImageAlphaType::Alpha,
                        },
                    ))
                })
            })
            .clone()
    }

    fn paint(&mut self, image: &p4::Image) -> Option<CpuImage> {
        let extend = |e: p4::Extend| match e {
            p4::Extend::Pad => p6::Extend::Pad,
            p4::Extend::Repeat => p6::Extend::Repeat,
            p4::Extend::Reflect => p6::Extend::Reflect,
        };
        let quality = match image.quality {
            p4::ImageQuality::Low => p6::ImageQuality::Low,
            p4::ImageQuality::Medium => p6::ImageQuality::Medium,
            p4::ImageQuality::High => p6::ImageQuality::High,
        };
        let mut sampler = p6::ImageSampler::new()
            .with_quality(quality)
            .with_alpha(image.alpha);
        sampler.x_extend = extend(image.x_extend);
        sampler.y_extend = extend(image.y_extend);
        Some(CpuImage {
            image: ImageSource::Pixmap(self.get(image)?),
            sampler,
        })
    }
}

fn point(p: k11::Point) -> k13::Point {
    k13::Point::new(p.x, p.y)
}

fn path(p: &k11::BezPath) -> k13::BezPath {
    use k11::PathEl::*;
    let mut out = k13::BezPath::new();
    for el in p.elements() {
        match *el {
            MoveTo(a) => out.move_to(point(a)),
            LineTo(a) => out.line_to(point(a)),
            QuadTo(a, b) => out.quad_to(point(a), point(b)),
            CurveTo(a, b, c) => out.curve_to(point(a), point(b), point(c)),
            ClosePath => out.close_path(),
        }
    }
    out
}

fn affine(a: k11::Affine) -> k13::Affine {
    k13::Affine::new(a.as_coeffs())
}

fn rect(r: k11::Rect) -> k13::Rect {
    k13::Rect::new(r.x0, r.y0, r.x1, r.y1)
}

fn color_cpu(c: p4::Color) -> AlphaColor<Srgb> {
    let c = c.to_rgba8();
    AlphaColor::from_rgba8(c.r, c.g, c.b, c.a)
}

fn stroke(s: &k11::Stroke) -> k13::Stroke {
    let cap = |c: k11::Cap| match c {
        k11::Cap::Butt => k13::Cap::Butt,
        k11::Cap::Square => k13::Cap::Square,
        k11::Cap::Round => k13::Cap::Round,
    };
    let join = match s.join {
        k11::Join::Bevel => k13::Join::Bevel,
        k11::Join::Miter => k13::Join::Miter,
        k11::Join::Round => k13::Join::Round,
    };
    let mut out = k13::Stroke::new(s.width)
        .with_join(join)
        .with_miter_limit(s.miter_limit)
        .with_start_cap(cap(s.start_cap))
        .with_end_cap(cap(s.end_cap));
    if !s.dash_pattern.is_empty() {
        out = out.with_dashes(s.dash_offset, s.dash_pattern.iter().copied());
    }
    out
}

/// Sets the current paint of `ctx`; false when it cannot be made (the draw is skipped).
fn set_paint(ctx: &mut RenderContext, paint: &Paint, images: &mut Images) -> bool {
    match paint {
        Paint::Color(c) => ctx.set_paint(color_cpu(*c)),
        Paint::Image(i) => match images.paint(i) {
            Some(i) => ctx.set_paint(i),
            None => return false,
        },
    }
    true
}

/// Draws `cmds` into `ctx`, every transform composed behind `base` (as `Scene::append` does).
fn replay(cmds: &[Cmd], ctx: &mut RenderContext, base: k11::Affine, images: &mut Images) {
    for cmd in cmds {
        match cmd {
            Cmd::Fill {
                style,
                transform,
                paint,
                brush_transform,
                shape,
            } => {
                if !set_paint(ctx, paint, images) {
                    continue;
                }
                ctx.set_transform(affine(base * *transform));
                ctx.set_paint_transform(affine(brush_transform.unwrap_or(k11::Affine::IDENTITY)));
                ctx.set_fill_rule(match style {
                    p4::Fill::NonZero => p6::Fill::NonZero,
                    p4::Fill::EvenOdd => p6::Fill::EvenOdd,
                });
                ctx.fill_path(&path(shape));
            }
            Cmd::Stroke {
                style,
                transform,
                paint,
                brush_transform,
                shape,
            } => {
                if !set_paint(ctx, paint, images) {
                    continue;
                }
                ctx.set_transform(affine(base * *transform));
                ctx.set_paint_transform(affine(brush_transform.unwrap_or(k11::Affine::IDENTITY)));
                ctx.set_stroke(stroke(style));
                ctx.stroke_path(&path(shape));
            }
            Cmd::PushLayer {
                alpha,
                transform,
                clip,
                ..
            } => {
                // draw.rs only pushes `Mix::Clip` layers.
                ctx.set_transform(affine(base * *transform));
                ctx.push_layer(
                    Some(&path(clip)),
                    None,
                    (*alpha != 1.0).then_some(*alpha),
                    None,
                    None,
                );
            }
            Cmd::PopLayer => ctx.pop_layer(),
            Cmd::Image { image, transform } => {
                let Some(paint) = images.paint(image) else {
                    continue;
                };
                ctx.set_paint(paint);
                ctx.set_paint_transform(k13::Affine::IDENTITY);
                ctx.set_transform(affine(base * *transform));
                ctx.fill_rect(&k13::Rect::new(
                    0.0,
                    0.0,
                    f64::from(image.width),
                    f64::from(image.height),
                ));
            }
            Cmd::Blurred {
                transform,
                rect: r,
                color,
                radius,
                std_dev,
            } => {
                ctx.set_paint(color_cpu(*color));
                ctx.set_paint_transform(k13::Affine::IDENTITY);
                ctx.set_transform(affine(base * *transform));
                ctx.fill_blurred_rounded_rect(&rect(*r), *radius as f32, *std_dev as f32);
            }
            Cmd::Append(child, tr) => {
                replay(
                    child,
                    ctx,
                    base * tr.unwrap_or(k11::Affine::IDENTITY),
                    images,
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use vello::peniko::{Fill, Mix};

    const BG: [u8; 4] = [10, 20, 30, 255];

    fn bg() -> p4::Color {
        p4::Color::from_rgba8(BG[0], BG[1], BG[2], BG[3])
    }

    fn red() -> p4::Color {
        p4::Color::from_rgba8(200, 0, 0, 255)
    }

    fn frame(base: &Canvas, size: (u32, u32)) -> Vec<u8> {
        render(base, &Canvas::new(), size, bg(), 0.0, None).unwrap()
    }

    fn px(data: &[u8], width: u32, x: u32, y: u32) -> [u8; 4] {
        let i = ((y * width + x) * 4) as usize;
        data[i..i + 4].try_into().unwrap()
    }

    fn fill(c: &mut Canvas, r: k11::Rect) {
        c.fill(Fill::NonZero, k11::Affine::IDENTITY, red(), None, &r);
    }

    #[test]
    fn cpu_renders_a_filled_rect() {
        let mut c = Canvas::new();
        fill(&mut c, k11::Rect::new(4.0, 4.0, 12.0, 12.0));
        let data = frame(&c, (16, 16));
        assert_eq!(data.len(), 16 * 16 * 4);
        assert_eq!(px(&data, 16, 8, 8), [200, 0, 0, 255]);
        assert_eq!(px(&data, 16, 1, 1), BG);
        assert_eq!(px(&data, 16, 14, 14), BG);
    }

    #[test]
    fn clip_layer_clips() {
        let mut c = Canvas::new();
        c.push_layer(
            Mix::Clip,
            1.0,
            k11::Affine::IDENTITY,
            &k11::Rect::new(0.0, 0.0, 8.0, 16.0),
        );
        fill(&mut c, k11::Rect::new(0.0, 0.0, 16.0, 16.0));
        c.pop_layer();
        let data = frame(&c, (16, 16));
        assert_eq!(px(&data, 16, 4, 8), [200, 0, 0, 255]);
        assert_eq!(px(&data, 16, 12, 8), BG);
    }

    #[test]
    fn stroke_draws_along_the_path() {
        let mut c = Canvas::new();
        c.stroke(
            &k11::Stroke::new(4.0),
            k11::Affine::IDENTITY,
            red(),
            None,
            &k11::Line::new((2.0, 8.0), (14.0, 8.0)),
        );
        let data = frame(&c, (16, 16));
        assert_eq!(px(&data, 16, 8, 8), [200, 0, 0, 255]);
        assert_eq!(px(&data, 16, 8, 2), BG);
    }

    #[test]
    fn appended_canvas_is_translated() {
        let mut inner = Canvas::new();
        fill(&mut inner, k11::Rect::new(0.0, 0.0, 4.0, 4.0));
        let mut c = Canvas::new();
        c.append(&inner, Some(k11::Affine::translate((8.0, 6.0))));
        let data = frame(&c, (16, 16));
        assert_eq!(px(&data, 16, 10, 8), [200, 0, 0, 255]);
        assert_eq!(px(&data, 16, 2, 2), BG);
    }

    #[test]
    fn blur_softens_edges_and_keeps_flat_regions() {
        let (w, h) = (64u32, 16u32);
        let mut src = vec![0u8; (w * h * 4) as usize];
        for y in 0..h {
            for x in 0..w {
                let v = if x < w / 2 { 0 } else { 255 };
                let i = ((y * w + x) * 4) as usize;
                src[i..i + 4].copy_from_slice(&[v, v, v, 255]);
            }
        }
        let low = blur::low_size((w, h));
        let out = blurred_copy(&src, (w, h), 8.0);
        assert_eq!(low, (16, 4));
        assert_eq!(out.len(), (low.0 * low.1 * 4) as usize);
        let at = |x: u32| out[((low.0 + x) * 4) as usize];
        assert_eq!(at(0), 0);
        assert_eq!(at(15), 255);
        assert!(at(7) > 0 && at(7) < 255, "edge cell {}", at(7));
        assert!(at(8) > 0 && at(8) < 255, "edge cell {}", at(8));
        assert!(at(7) < at(8));
    }

    #[test]
    fn popups_show_the_blurred_backdrop() {
        let size = (64, 16);
        let mut base = Canvas::new();
        fill(&mut base, k11::Rect::new(0.0, 0.0, 32.0, 16.0));
        let placeholder = blur::placeholder(size);
        let mut popups = Canvas::new();
        popups.fill(
            Fill::NonZero,
            k11::Affine::IDENTITY,
            &placeholder,
            Some(k11::Affine::scale(f64::from(DOWNSCALE))),
            &k11::Rect::new(0.0, 0.0, 64.0, 16.0),
        );
        let data = render(&base, &popups, size, bg(), 8.0, Some(&placeholder)).unwrap();
        // Far from the edge the backdrop is the flat base; at the edge it is a mix.
        assert_eq!(px(&data, 64, 2, 8), [200, 0, 0, 255]);
        assert_eq!(px(&data, 64, 61, 8), BG);
        let edge = px(&data, 64, 32, 8);
        assert!(edge[0] > BG[0] + 5 && edge[0] < 195, "edge {edge:?}");
    }
}
