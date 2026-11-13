//! A recording drawing surface: the seam between `draw.rs` and a renderer.
//!
//! `Canvas` has the method names and signatures of `vello::Scene` (same kurbo 0.11 / peniko 0.4
//! types) but only records commands, with shapes flattened to owned `BezPath`s. The frame reaches
//! the GPU by [`Canvas::replay_vello`] right before rendering; a CPU backend can replay the same
//! commands into its own context.

use std::sync::Arc;

use vello::Scene;
use vello::kurbo::{Affine, BezPath, Rect, Shape, Stroke};
use vello::peniko::{BlendMode, Brush, BrushRef, Color, Fill, Image};

/// What a command paints with: draw.rs only uses solid colors and images.
#[derive(Clone)]
pub enum Paint {
    Color(Color),
    Image(Image),
}

impl From<Color> for Paint {
    fn from(c: Color) -> Self {
        Paint::Color(c)
    }
}

impl From<&Image> for Paint {
    fn from(i: &Image) -> Self {
        Paint::Image(i.clone())
    }
}

impl From<&Brush> for Paint {
    fn from(b: &Brush) -> Self {
        match b {
            Brush::Solid(c) => Paint::Color(*c),
            Brush::Image(i) => Paint::Image(i.clone()),
            Brush::Gradient(_) => unreachable!("gradients are not drawn"),
        }
    }
}

impl Paint {
    fn brush(&self) -> BrushRef<'_> {
        match self {
            Paint::Color(c) => BrushRef::Solid(*c),
            Paint::Image(i) => BrushRef::Image(i),
        }
    }
}

#[derive(Clone)]
pub(crate) enum Cmd {
    Fill {
        style: Fill,
        transform: Affine,
        paint: Paint,
        brush_transform: Option<Affine>,
        shape: Arc<BezPath>,
    },
    Stroke {
        style: Stroke,
        transform: Affine,
        paint: Paint,
        brush_transform: Option<Affine>,
        shape: Arc<BezPath>,
    },
    PushLayer {
        blend: BlendMode,
        alpha: f32,
        transform: Affine,
        clip: Arc<BezPath>,
    },
    PopLayer,
    Image {
        image: Image,
        transform: Affine,
    },
    Blurred {
        transform: Affine,
        rect: Rect,
        color: Color,
        radius: f64,
        std_dev: f64,
    },
    /// A nested canvas, replayed through `Scene::append` so transforms compose as in vello.
    Append(Arc<Vec<Cmd>>, Option<Affine>),
}

/// Tolerance vello encodes fills, clips and undashed strokes at.
const SHAPE_TOLERANCE: f64 = 0.1;
/// Tolerance vello flattens a shape at before dashing it.
const DASH_TOLERANCE: f64 = 0.01;

#[derive(Clone, Default)]
pub struct Canvas {
    cmds: Vec<Cmd>,
}

impl Canvas {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn reset(&mut self) {
        self.cmds.clear();
    }

    pub fn fill(
        &mut self,
        style: Fill,
        transform: Affine,
        brush: impl Into<Paint>,
        brush_transform: Option<Affine>,
        shape: &impl Shape,
    ) {
        self.cmds.push(Cmd::Fill {
            style,
            transform,
            paint: brush.into(),
            brush_transform,
            shape: Arc::new(shape.to_path(SHAPE_TOLERANCE)),
        });
    }

    pub fn stroke(
        &mut self,
        style: &Stroke,
        transform: Affine,
        brush: impl Into<Paint>,
        brush_transform: Option<Affine>,
        shape: &impl Shape,
    ) {
        let tolerance = if style.dash_pattern.is_empty() {
            SHAPE_TOLERANCE
        } else {
            DASH_TOLERANCE
        };
        self.cmds.push(Cmd::Stroke {
            style: style.clone(),
            transform,
            paint: brush.into(),
            brush_transform,
            shape: Arc::new(shape.to_path(tolerance)),
        });
    }

    pub fn push_layer(
        &mut self,
        blend: impl Into<BlendMode>,
        alpha: f32,
        transform: Affine,
        clip: &impl Shape,
    ) {
        self.cmds.push(Cmd::PushLayer {
            blend: blend.into(),
            alpha,
            transform,
            clip: Arc::new(clip.to_path(SHAPE_TOLERANCE)),
        });
    }

    pub fn pop_layer(&mut self) {
        self.cmds.push(Cmd::PopLayer);
    }

    pub fn draw_image(&mut self, image: &Image, transform: Affine) {
        self.cmds.push(Cmd::Image {
            image: image.clone(),
            transform,
        });
    }

    pub fn draw_blurred_rounded_rect(
        &mut self,
        transform: Affine,
        rect: Rect,
        brush: Color,
        radius: f64,
        std_dev: f64,
    ) {
        self.cmds.push(Cmd::Blurred {
            transform,
            rect,
            color: brush,
            radius,
            std_dev,
        });
    }

    /// Appends `other`, every transform in it composed with `transform`.
    pub fn append(&mut self, other: &Self, transform: Option<Affine>) {
        self.cmds
            .push(Cmd::Append(Arc::new(other.cmds.clone()), transform));
    }

    /// The recorded commands, for a renderer that replays them itself.
    pub(crate) fn commands(&self) -> &[Cmd] {
        &self.cmds
    }

    /// Replays the recording into a vello scene, under `transform`.
    pub fn replay_vello(&self, scene: &mut Scene, transform: Affine) {
        replay(&self.cmds, scene, transform);
    }
}

fn replay(cmds: &[Cmd], scene: &mut Scene, base: Affine) {
    // `base` composes in front of every command, as `Scene::append` does for a child.
    let t = |x: Affine| {
        if base == Affine::IDENTITY {
            x
        } else {
            base * x
        }
    };
    for cmd in cmds {
        match cmd {
            Cmd::Fill {
                style,
                transform,
                paint,
                brush_transform,
                shape,
            } => {
                scene.fill(
                    *style,
                    t(*transform),
                    paint.brush(),
                    *brush_transform,
                    &**shape,
                );
            }
            Cmd::Stroke {
                style,
                transform,
                paint,
                brush_transform,
                shape,
            } => {
                scene.stroke(
                    style,
                    t(*transform),
                    paint.brush(),
                    *brush_transform,
                    &**shape,
                );
            }
            Cmd::PushLayer {
                blend,
                alpha,
                transform,
                clip,
            } => {
                scene.push_layer(*blend, *alpha, t(*transform), &**clip);
            }
            Cmd::PopLayer => scene.pop_layer(),
            Cmd::Image { image, transform } => scene.draw_image(image, t(*transform)),
            Cmd::Blurred {
                transform,
                rect,
                color,
                radius,
                std_dev,
            } => {
                scene.draw_blurred_rounded_rect(t(*transform), *rect, *color, *radius, *std_dev);
            }
            Cmd::Append(child, tr) => {
                let mut sub = Scene::new();
                replay(child, &mut sub, Affine::IDENTITY);
                scene.append(&sub, Some(base * tr.unwrap_or(Affine::IDENTITY)));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use vello::peniko::Mix;

    fn square(c: &mut Canvas) {
        c.fill(
            Fill::NonZero,
            Affine::IDENTITY,
            Color::from_rgba8(10, 20, 30, 255),
            None,
            &Rect::new(0.0, 0.0, 4.0, 4.0),
        );
    }

    fn direct(f: impl FnOnce(&mut Scene)) -> Vec<u32> {
        let mut s = Scene::new();
        f(&mut s);
        encoding_bytes(&s)
    }

    fn encoding_bytes(s: &Scene) -> Vec<u32> {
        let e = s.encoding();
        let mut v: Vec<u32> = e.path_tags.iter().map(|t| u32::from(t.0)).collect();
        v.extend_from_slice(&e.path_data);
        for t in &e.transforms {
            v.extend(t.matrix.iter().chain(&t.translation).map(|x| x.to_bits()));
        }
        v.extend_from_slice(&e.draw_data);
        v
    }

    fn replayed(c: &Canvas, t: Affine) -> Vec<u32> {
        let mut s = Scene::new();
        c.replay_vello(&mut s, t);
        encoding_bytes(&s)
    }

    #[test]
    fn fill_replays_like_the_scene_call() {
        let mut c = Canvas::new();
        square(&mut c);
        let want = direct(|s| {
            s.fill(
                Fill::NonZero,
                Affine::IDENTITY,
                Color::from_rgba8(10, 20, 30, 255),
                None,
                &Rect::new(0.0, 0.0, 4.0, 4.0),
            )
        });
        assert!(!want.is_empty());
        assert_eq!(replayed(&c, Affine::IDENTITY), want);
    }

    #[test]
    fn nested_append_composes_transforms() {
        let mut inner = Canvas::new();
        square(&mut inner);
        inner.push_layer(
            Mix::Clip,
            1.0,
            Affine::IDENTITY,
            &Rect::new(0.0, 0.0, 2.0, 2.0),
        );
        square(&mut inner);
        inner.pop_layer();
        let mut outer = Canvas::new();
        outer.append(&inner, Some(Affine::translate((5.0, 7.0))));

        // Same as appending the replayed child to a scene by hand...
        let mut child = Scene::new();
        inner.replay_vello(&mut child, Affine::IDENTITY);
        let want = direct(|s| s.append(&child, Some(Affine::translate((5.0, 7.0)))));
        assert_eq!(replayed(&outer, Affine::IDENTITY), want);

        // ...and the replay transform composes in front of the nested one.
        let moved = replayed(&outer, Affine::scale(2.0));
        let want = direct(|s| {
            s.append(
                &child,
                Some(Affine::scale(2.0) * Affine::translate((5.0, 7.0))),
            )
        });
        assert_eq!(moved, want);
        assert_ne!(moved, replayed(&outer, Affine::IDENTITY));
    }
}
