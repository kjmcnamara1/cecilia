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

/// Renders one frame with a fresh [`Renderer`] and returns its RGBA8 pixels.
pub fn render(
    base: &Canvas,
    popups: &Canvas,
    size: (u32, u32),
    base_color: p4::Color,
    blur_sigma: f64,
    backdrop: Option<&p4::Image>,
) -> Result<Vec<u8>> {
    Ok(Renderer::default()
        .render(base, popups, size, base_color, blur_sigma, backdrop)?
        .to_vec())
}

/// More dirty rectangles than this are drawn as their union.
const MAX_DIRTY_RECTS: usize = 8;
/// A frame whose dirty rectangles cover more than this share of it is drawn whole.
const MAX_DIRTY_SHARE: f64 = 0.6;

/// Renders frames of recorded canvases, keeping what it can from the previous one.
///
/// The `base` canvas is the page; it is compared command by command with the last frame's, and
/// only the rectangles where commands changed (the playhead, the cursor, a progress bar) are
/// drawn again into the kept pixels. `popups` (tooltips, menus, notification cards) go over a
/// copy of the finished base each frame, in their own bounding box. The popups' blurred backdrop
/// is kept until the base changes.
#[derive(Default)]
pub struct Renderer {
    /// Size and background the kept pixels were drawn for.
    key: Option<((u32, u32), p4::Color)>,
    /// The last base frame, as flat commands in device space.
    last: Vec<Cmd>,
    /// The finished base frame (opaque, so premultiplied and straight RGBA8 are the same).
    base: Option<Arc<Pixmap>>,
    /// The base blurred with the sigma whose bits are the key.
    blurred: Option<(u64, Vec<u8>)>,
    /// Base plus popups, when there are popups.
    out: Vec<u8>,
    images: Images,
    resources: Option<Resources>,
}

impl Renderer {
    /// Renders `base` and then `popups` (recorded in device pixels) at `size` over `base_color`
    /// and returns RGBA8 pixels (`size.0 * size.1 * 4` bytes, row-major, opaque).
    ///
    /// With popups and `blur_sigma > 0` (in pixels of the target) the popups' backdrop is the
    /// base frame blurred: any image fill in `popups` made with `backdrop` (the placeholder from
    /// [`blur::placeholder`], matched by its blob id) is drawn with the blurred copy instead.
    pub fn render(
        &mut self,
        base: &Canvas,
        popups: &Canvas,
        size: (u32, u32),
        base_color: p4::Color,
        blur_sigma: f64,
        backdrop: Option<&p4::Image>,
    ) -> Result<&[u8]> {
        if size.0 == 0 || size.1 == 0 {
            return Err(anyhow!("cannot render a {}x{} frame", size.0, size.1));
        }
        let too_large = || anyhow!("size {}x{} too large", size.0, size.1);
        let (w, h) = (
            u16::try_from(size.0).map_err(|_| too_large())?,
            u16::try_from(size.1).map_err(|_| too_large())?,
        );
        let leaves = flatten(base.commands());
        let key = Some((size, base_color));
        let dirty = match &self.base {
            Some(_) if self.key == key => dirty_rects(&self.last, &leaves, size),
            _ => None,
        };
        let resources = self.resources.get_or_insert_with(Resources::new);
        if self.images.by_id.len() > 128 {
            self.images.by_id.clear();
        }
        match dirty {
            None => {
                let mut ctx = context(w, h, base_color);
                replay(
                    &leaves,
                    &mut ctx,
                    k11::Affine::IDENTITY,
                    None,
                    &mut self.images,
                );
                self.base = Some(Arc::new(rasterize(ctx, w, h, resources)));
                self.blurred = None;
            }
            Some(rects) => {
                if !rects.is_empty() {
                    self.blurred = None;
                }
                let base_pixels = Arc::make_mut(self.base.as_mut().expect("kept frame"));
                for r in rects {
                    let (rw, rh) = (r.width() as u16, r.height() as u16);
                    let mut ctx = context(rw, rh, base_color);
                    let shift = k11::Affine::translate((-r.x0, -r.y0));
                    replay(&leaves, &mut ctx, shift, Some(r), &mut self.images);
                    let patch = rasterize(ctx, rw, rh, resources);
                    blit(base_pixels.data_as_u8_slice_mut(), size, &patch, r);
                }
            }
        }
        self.key = key;
        self.last = leaves;
        let base_pixels = self.base.as_ref().expect("base frame");
        let popup_leaves = flatten(popups.commands());
        let Some(region) = bounds(&popup_leaves, size) else {
            return Ok(base_pixels.data_as_u8_slice());
        };
        if let Some(backdrop) = backdrop.filter(|_| blur_sigma > 0.0) {
            let bits = blur_sigma.to_bits();
            if self.blurred.as_ref().is_none_or(|(b, _)| *b != bits) {
                let low = blurred_copy(base_pixels.data_as_u8_slice(), size, blur_sigma);
                self.blurred = Some((bits, low));
            }
            let (_, low) = self.blurred.as_ref().expect("blurred backdrop");
            self.images
                .substitute(backdrop, low.clone(), blur::low_size(size));
        }
        // The popups replay into a context that starts with the base behind them.
        let (rw, rh) = (region.width() as u16, region.height() as u16);
        let shift = k11::Affine::translate((-region.x0, -region.y0));
        let mut ctx = context(rw, rh, base_color);
        ctx.set_transform(affine(shift));
        ctx.set_paint(CpuImage {
            image: ImageSource::Pixmap(base_pixels.clone()),
            sampler: p6::ImageSampler::new().with_quality(p6::ImageQuality::Low),
        });
        ctx.fill_rect(&k13::Rect::new(0.0, 0.0, f64::from(w), f64::from(h)));
        replay(&popup_leaves, &mut ctx, shift, None, &mut self.images);
        let patch = rasterize(ctx, rw, rh, resources);
        self.out.clear();
        self.out.extend_from_slice(base_pixels.data_as_u8_slice());
        blit(&mut self.out, size, &patch, region);
        Ok(&self.out)
    }
}

fn rasterize(mut ctx: RenderContext, w: u16, h: u16, resources: &mut Resources) -> Pixmap {
    let mut target = Pixmap::new(w, h);
    ctx.flush();
    ctx.render(&mut target, resources);
    target
}

/// Copies `patch` (the pixels of `rect`) into `frame` of `size`.
fn blit(frame: &mut [u8], (w, _): (u32, u32), patch: &Pixmap, rect: k11::Rect) {
    let (x0, y0) = (rect.x0 as usize, rect.y0 as usize);
    let row = rect.width() as usize * 4;
    for (y, src) in patch.data_as_u8_slice().chunks_exact(row).enumerate() {
        let at = ((y0 + y) * w as usize + x0) * 4;
        frame[at..at + row].copy_from_slice(src);
    }
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

/// `cmds` with nested canvases inlined: every command carries its device-space transform.
fn flatten(cmds: &[Cmd]) -> Vec<Cmd> {
    fn go(cmds: &[Cmd], base: k11::Affine, out: &mut Vec<Cmd>) {
        for cmd in cmds {
            let mut cmd = cmd.clone();
            match &mut cmd {
                Cmd::Append(child, tr) => {
                    go(child, base * tr.unwrap_or(k11::Affine::IDENTITY), out);
                    continue;
                }
                Cmd::Fill { transform, .. }
                | Cmd::Stroke { transform, .. }
                | Cmd::PushLayer { transform, .. }
                | Cmd::Image { transform, .. }
                | Cmd::Blurred { transform, .. } => *transform = base * *transform,
                Cmd::PopLayer => {}
            }
            out.push(cmd);
        }
    }
    let mut out = Vec::with_capacity(cmds.len());
    go(cmds, k11::Affine::IDENTITY, &mut out);
    out
}

/// The device-space area a flat command can paint (None for one that paints nothing itself).
fn bbox(cmd: &Cmd) -> Option<k11::Rect> {
    use k11::Shape;
    // Antialiasing reaches a pixel beyond the geometry.
    let aa = 1.0;
    Some(match cmd {
        Cmd::Fill {
            transform, shape, ..
        } => transform.transform_rect_bbox(shape.bounding_box().inflate(aa, aa)),
        Cmd::Stroke {
            style,
            transform,
            shape,
            ..
        } => {
            // Miters and square caps reach past half the width.
            let pad = style.width * 0.5 * style.miter_limit.max(1.5) + aa;
            transform.transform_rect_bbox(shape.bounding_box().inflate(pad, pad))
        }
        Cmd::PushLayer {
            transform, clip, ..
        } => transform.transform_rect_bbox(clip.bounding_box().inflate(aa, aa)),
        Cmd::Image { image, transform } => transform.transform_rect_bbox(
            k11::Rect::new(0.0, 0.0, f64::from(image.width), f64::from(image.height))
                .inflate(aa, aa),
        ),
        Cmd::Blurred {
            transform,
            rect,
            std_dev,
            ..
        } => {
            let pad = 3.0 * std_dev + aa;
            transform.transform_rect_bbox(rect.inflate(pad, pad))
        }
        Cmd::PopLayer | Cmd::Append(..) => return None,
    })
}

/// `r` widened to whole pixels and cut to the frame (None when nothing is left).
fn pixel_rect(r: k11::Rect, (w, h): (u32, u32)) -> Option<k11::Rect> {
    let r = k11::Rect::new(
        r.x0.floor().max(0.0),
        r.y0.floor().max(0.0),
        r.x1.ceil().min(f64::from(w)),
        r.y1.ceil().min(f64::from(h)),
    );
    (r.width() >= 1.0 && r.height() >= 1.0).then_some(r)
}

/// The pixel rectangle that holds everything `cmds` paint.
fn bounds(cmds: &[Cmd], size: (u32, u32)) -> Option<k11::Rect> {
    cmds.iter()
        .filter_map(bbox)
        .reduce(|a, b| a.union(b))
        .and_then(|r| pixel_rect(r, size))
}

/// The rectangles that differ between two frames' flat commands; None when they cannot be told
/// apart cheaply enough (the commands shifted, or too much changed) and the frame must be drawn
/// whole.
fn dirty_rects(old: &[Cmd], new: &[Cmd], size: (u32, u32)) -> Option<Vec<k11::Rect>> {
    if old.len() != new.len() {
        return None;
    }
    let mut rects: Vec<k11::Rect> = Vec::new();
    for (a, b) in old.iter().zip(new) {
        if a == b {
            continue;
        }
        for r in [bbox(a), bbox(b)].into_iter().flatten() {
            let Some(r) = pixel_rect(r, size) else {
                continue;
            };
            // Merge into any rectangle it touches, so a moving playhead stays one rectangle.
            match rects.iter_mut().find(|q| q.intersect(r).area() > 0.0) {
                Some(q) => *q = q.union(r),
                None => rects.push(r),
            }
        }
    }
    // Merging can make earlier rectangles touch.
    let mut i = 0;
    while i < rects.len() {
        match (0..i).find(|&j| rects[j].intersect(rects[i]).area() > 0.0) {
            Some(j) => {
                let r = rects.swap_remove(i);
                rects[j] = rects[j].union(r);
                i = 0;
            }
            None => i += 1,
        }
    }
    if rects.len() > MAX_DIRTY_RECTS {
        let all = rects.iter().copied().reduce(|a, b| a.union(b));
        rects = all.into_iter().collect();
    }
    let area: f64 = rects.iter().map(|r| r.area()).sum();
    (area <= MAX_DIRTY_SHARE * f64::from(size.0) * f64::from(size.1)).then_some(rects)
}

/// `rgba` (premultiplied or opaque RGBA8, `size` pixels) shrunk to the blur size and blurred with
/// `sigma` pixels of the full-size frame. Returns premultiplied RGBA8 of [`blur::low_size`].
pub fn blurred_copy(rgba: &[u8], size: (u32, u32), sigma: f64) -> Vec<u8> {
    let low = blur::low_size(size);
    let shrunk = shrink(rgba, size, low);
    let sigma = ((sigma / f64::from(DOWNSCALE)) as f32).max(0.5);
    gaussian(&shrunk, low, sigma)
}

/// Box-filter downscale of `src` to `low` (cells at the right and bottom edge average the pixels
/// that exist).
fn shrink(src: &[u8], (w, h): (u32, u32), (lw, lh): (u32, u32)) -> Vec<u8> {
    let (w, h, lw, lh, d) = (
        w as usize,
        h as usize,
        lw as usize,
        lh as usize,
        DOWNSCALE as usize,
    );
    let mut sums = vec![0u32; lw * lh * 4];
    for (y, row) in src.chunks_exact(w * 4).take(h).enumerate() {
        let cells = &mut sums[y / d * lw * 4..][..lw * 4];
        for (cell, pixels) in cells
            .as_chunks_mut::<4>()
            .0
            .iter_mut()
            .zip(row.chunks(d * 4))
        {
            let mut sum = [0u32; 4];
            for px in pixels.as_chunks::<4>().0 {
                sum = [
                    sum[0] + u32::from(px[0]),
                    sum[1] + u32::from(px[1]),
                    sum[2] + u32::from(px[2]),
                    sum[3] + u32::from(px[3]),
                ];
            }
            for (s, v) in cell.iter_mut().zip(sum) {
                *s += v;
            }
        }
    }
    let mut out = vec![0u8; lw * lh * 4];
    for (i, (o, s)) in out
        .as_chunks_mut::<4>()
        .0
        .iter_mut()
        .zip(sums.as_chunks::<4>().0)
        .enumerate()
    {
        let (lx, ly) = (i % lw, i / lw);
        let n = ((w - lx * d).min(d) * (h - ly * d).min(d)) as u32;
        for (o, &s) in o.iter_mut().zip(s.iter()) {
            *o = ((s + n / 2) / n) as u8;
        }
    }
    out
}

/// A gaussian blur of `sigma` pixels, as three box blurs per axis (within a few percent of the
/// true gaussian, at a cost that does not grow with the radius), edges clamped.
fn gaussian(src: &[u8], size: (u32, u32), sigma: f32) -> Vec<u8> {
    // Box width for three passes whose variance adds up to sigma^2.
    let radius = (((4.0 * sigma * sigma + 1.0).sqrt() - 1.0) / 2.0).round() as usize;
    let mut a = src.to_vec();
    let mut b = vec![0u8; src.len()];
    for horizontal in [true, false] {
        for _ in 0..3 {
            box_pass(&a, &mut b, size, radius, horizontal);
            std::mem::swap(&mut a, &mut b);
        }
    }
    a
}

/// One box blur pass of `radius` (along x when `horizontal`), edges clamped, with a running sum
/// per channel. The vertical pass keeps one sum per column and walks whole rows, so memory is
/// read in order.
fn box_pass(src: &[u8], dst: &mut [u8], (w, h): (u32, u32), radius: usize, horizontal: bool) {
    let (w, h, r) = (w as usize, h as usize, radius as isize);
    let n = (2 * radius + 1) as u32;
    // Dividing by `n` as a multiplication and a shift keeps the integer divide out of the loop.
    let recip = (65536 + n / 2) / n;
    // Along a line of `len` positions, `step` bytes apart, one sum per byte of a position
    // (`inner` of them): a pixel's channels per row, or a whole row's per column.
    let (len, step, inner, lines) = if horizontal {
        (w, 4, 4, h)
    } else {
        (h, w * 4, w * 4, 1)
    };
    for line in 0..lines {
        let origin = if horizontal { line * w * 4 } else { 0 };
        let at = |i: isize| origin + i.clamp(0, len as isize - 1) as usize * step;
        let mut sum = vec![0u32; inner];
        for i in -r..=r {
            let p = at(i);
            for (s, &v) in sum.iter_mut().zip(&src[p..p + inner]) {
                *s += u32::from(v);
            }
        }
        for i in 0..len as isize {
            let o = origin + i as usize * step;
            let (add, sub) = (at(i + r + 1), at(i - r));
            let out = &mut dst[o..o + inner];
            let (add, sub) = (&src[add..add + inner], &src[sub..sub + inner]);
            for (((s, d), &a), &b) in sum.iter_mut().zip(out).zip(add).zip(sub) {
                *d = ((*s * recip + 32768) >> 16) as u8;
                *s = *s + u32::from(a) - u32::from(b);
            }
        }
    }
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

/// Draws the flat `cmds` into `ctx`, every transform composed behind `base`.
fn replay(
    cmds: &[Cmd],
    ctx: &mut RenderContext,
    base: k11::Affine,
    cull: Option<k11::Rect>,
    images: &mut Images,
) {
    for cmd in cmds {
        // Layers are kept so the nesting stays balanced; drawing outside `cull` is skipped.
        if let Some(cull) = cull
            && bbox(cmd).is_some_and(|b| b.intersect(cull).area() <= 0.0)
            && !matches!(cmd, Cmd::PushLayer { .. })
        {
            continue;
        }
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
                blend,
                alpha,
                transform,
                clip,
            } => {
                // draw.rs only pushes `Mix::Clip` layers.
                debug_assert!(
                    *blend == p4::BlendMode::from(p4::Mix::Clip),
                    "unsupported layer blend mode {blend:?}"
                );
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
            Cmd::Append(..) => unreachable!("commands are flattened before replay"),
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

    #[test]
    fn dashed_stroke_has_gaps() {
        let mut c = Canvas::new();
        c.stroke(
            &k11::Stroke::new(4.0).with_dashes(0.0, [8.0, 8.0]),
            k11::Affine::IDENTITY,
            red(),
            None,
            &k11::Line::new((0.0, 8.0), (32.0, 8.0)),
        );
        let data = frame(&c, (32, 16));
        assert_eq!(px(&data, 32, 4, 8), [200, 0, 0, 255]);
        assert_eq!(px(&data, 32, 12, 8), BG);
        assert_eq!(px(&data, 32, 20, 8), [200, 0, 0, 255]);
    }

    #[test]
    fn translucent_layer_blends_with_the_background() {
        let mut c = Canvas::new();
        c.push_layer(
            Mix::Clip,
            0.5,
            k11::Affine::IDENTITY,
            &k11::Rect::new(0.0, 0.0, 16.0, 16.0),
        );
        fill(&mut c, k11::Rect::new(0.0, 0.0, 8.0, 16.0));
        c.pop_layer();
        let data = frame(&c, (16, 16));
        let p = px(&data, 16, 4, 8);
        // Half red over the background: between the two on the red channel.
        assert!((p[0] as i32 - 105).abs() <= 3, "{p:?}");
        assert!((p[1] as i32 - 10).abs() <= 3, "{p:?}");
        assert_eq!(px(&data, 16, 12, 8), BG);
    }

    #[test]
    fn image_command_draws_the_pixels() {
        let data: Vec<u8> = [[0, 255, 0, 255], [0, 0, 255, 255]].concat();
        let image = p4::Image::new(data.into(), p4::ImageFormat::Rgba8, 2, 1);
        let mut c = Canvas::new();
        c.draw_image(
            &image,
            k11::Affine::translate((4.0, 4.0)) * k11::Affine::scale(4.0),
        );
        let out = frame(&c, (16, 16));
        assert_eq!(px(&out, 16, 5, 5), [0, 255, 0, 255]);
        assert_eq!(px(&out, 16, 10, 5), [0, 0, 255, 255]);
        assert_eq!(px(&out, 16, 2, 2), BG);
    }

    fn scene(playhead_x: f64, extra: bool) -> Canvas {
        let mut c = Canvas::new();
        fill(&mut c, k11::Rect::new(0.0, 0.0, 40.0, 8.0));
        c.push_layer(
            Mix::Clip,
            0.5,
            k11::Affine::IDENTITY,
            &k11::Rect::new(0.0, 10.0, 64.0, 30.0),
        );
        fill(&mut c, k11::Rect::new(10.0, 12.0, 54.0, 28.0));
        c.pop_layer();
        let mut inner = Canvas::new();
        c.stroke(
            &k11::Stroke::new(3.0),
            k11::Affine::IDENTITY,
            red(),
            None,
            &k11::Line::new((playhead_x, 0.0), (playhead_x, 32.0)),
        );
        fill(&mut inner, k11::Rect::new(0.0, 0.0, 6.0, 6.0));
        c.append(&inner, Some(k11::Affine::translate((50.0, 2.0))));
        if extra {
            fill(&mut c, k11::Rect::new(1.0, 1.0, 3.0, 3.0));
        }
        c
    }

    fn popup(size: (u32, u32)) -> (Canvas, p4::Image) {
        let placeholder = blur::placeholder(size);
        let mut popups = Canvas::new();
        popups.fill(
            Fill::NonZero,
            k11::Affine::IDENTITY,
            &placeholder,
            Some(k11::Affine::scale(f64::from(DOWNSCALE))),
            &k11::Rect::new(20.0, 6.0, 48.0, 22.0),
        );
        (popups, placeholder)
    }

    #[test]
    fn only_the_changed_rectangle_is_drawn_again() {
        let size = (64, 32);
        let (empty, _) = (Canvas::new(), ());
        let mut r = Renderer::default();
        for x in [5.0, 6.0, 30.0, 31.5] {
            let c = scene(x, false);
            let kept = r
                .render(&c, &empty, size, bg(), 0.0, None)
                .unwrap()
                .to_vec();
            assert_eq!(kept, frame(&c, size), "playhead at {x}");
        }
        let (a, b) = (
            flatten(scene(5.0, false).commands()),
            flatten(scene(6.0, false).commands()),
        );
        let dirty = dirty_rects(&a, &b, size).expect("a small change");
        assert_eq!(dirty.len(), 1);
        assert!(dirty[0].area() < 64.0 * 32.0 / 4.0, "{dirty:?}");
        // The same frame again draws nothing.
        assert_eq!(dirty_rects(&a, &a, size), Some(vec![]));
        // A command appearing shifts the rest: drawn whole.
        let c = flatten(scene(6.0, true).commands());
        assert_eq!(dirty_rects(&b, &c, size), None);
        let whole = scene(6.0, true);
        let kept = r
            .render(&whole, &empty, size, bg(), 0.0, None)
            .unwrap()
            .to_vec();
        assert_eq!(kept, frame(&whole, size));
    }

    #[test]
    fn popups_over_a_kept_frame_match_a_fresh_render() {
        let size = (64, 32);
        let (popups, placeholder) = popup(size);
        let mut r = Renderer::default();
        for (x, with_popups) in [
            (5.0, true),
            (5.0, true),
            (9.0, true),
            (9.0, false),
            (12.0, true),
        ] {
            let c = scene(x, false);
            let none = Canvas::new();
            let p = if with_popups { &popups } else { &none };
            let kept = r
                .render(&c, p, size, bg(), 4.0, Some(&placeholder))
                .unwrap()
                .to_vec();
            let fresh = render(&c, p, size, bg(), 4.0, Some(&placeholder)).unwrap();
            assert_eq!(kept, fresh, "playhead at {x}, popups {with_popups}");
        }
    }

    #[test]
    fn empty_size_is_an_error() {
        let c = Canvas::new();
        assert!(render(&c, &c, (0, 10), bg(), 0.0, None).is_err());
        assert!(render(&c, &c, (10, 0), bg(), 0.0, None).is_err());
    }
}
