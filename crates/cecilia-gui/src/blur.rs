//! Backdrop blur for popups: the frame without its popups is rendered to a texture, shrunk and
//! gaussian blurred on the GPU, and handed to vello as an image (`Renderer::override_image`) that
//! the popups fill their shape with before their translucent surface.

use crate::canvas::Canvas;
use anyhow::{Result, anyhow};
use vello::kurbo::Affine;
use vello::peniko::{Image, ImageFormat};
use vello::wgpu::{self, util::DeviceExt};
use vello::{AaConfig, RenderParams, Renderer, Scene};

/// The blurred copy is this many times smaller than the window: cheaper to blur, and a blur
/// hides the lost detail.
pub const DOWNSCALE: u32 = 4;

const FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;

const SHADER: &str = "
struct U { step: vec2<f32>, sigma: f32, radius: i32 }
@group(0) @binding(0) var src: texture_2d<f32>;
@group(0) @binding(1) var smp: sampler;
@group(0) @binding(2) var<uniform> u: U;
struct VO { @builtin(position) pos: vec4<f32>, @location(0) uv: vec2<f32> }
@vertex fn vs(@builtin(vertex_index) i: u32) -> VO {
    let p = vec2<f32>(f32((i << 1u) & 2u), f32(i & 2u));
    var o: VO;
    o.pos = vec4<f32>(p * 2.0 - 1.0, 0.0, 1.0);
    o.uv = vec2<f32>(p.x, 1.0 - p.y);
    return o;
}
@fragment fn fs(v: VO) -> @location(0) vec4<f32> {
    var sum = vec4<f32>(0.0);
    var total = 0.0;
    for (var i = -u.radius; i <= u.radius; i++) {
        let w = exp(-0.5 * f32(i * i) / (u.sigma * u.sigma));
        sum += w * textureSampleLevel(src, smp, v.uv + u.step * f32(i), 0.0);
        total += w;
    }
    return sum / total;
}
";

/// The blur pipeline and the textures of the current window size.
pub struct Blur {
    pipeline: wgpu::RenderPipeline,
    sampler: wgpu::Sampler,
    size: (u32, u32),
    /// What the popups fill with: its pixels are never used, vello copies the blurred texture over
    /// them, but its size and identity are.
    placeholder: Option<Image>,
    targets: Option<Targets>,
}

struct Targets {
    /// The frame without popups, at full size.
    base: wgpu::Texture,
    /// The shrunk frame; ends up holding the blurred result.
    low: wgpu::Texture,
    scratch: wgpu::Texture,
}

fn texture(
    device: &wgpu::Device,
    (width, height): (u32, u32),
    usage: wgpu::TextureUsages,
) -> wgpu::Texture {
    device.create_texture(&wgpu::TextureDescriptor {
        label: Some("blur"),
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: FORMAT,
        usage,
        view_formats: &[],
    })
}

impl Blur {
    pub fn new(device: &wgpu::Device) -> Self {
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("blur"),
            source: wgpu::ShaderSource::Wgsl(SHADER.into()),
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("blur"),
            layout: None,
            vertex: wgpu::VertexState {
                module: &module,
                entry_point: Some("vs"),
                compilation_options: Default::default(),
                buffers: &[],
            },
            fragment: Some(wgpu::FragmentState {
                module: &module,
                entry_point: Some("fs"),
                compilation_options: Default::default(),
                targets: &[Some(FORMAT.into())],
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        Self {
            pipeline,
            sampler,
            size: (0, 0),
            placeholder: None,
            targets: None,
        }
    }

    fn low_size((width, height): (u32, u32)) -> (u32, u32) {
        (
            width.div_ceil(DOWNSCALE).max(1),
            height.div_ceil(DOWNSCALE).max(1),
        )
    }

    /// The image popups fill with for a window of `size` pixels (drawn at `DOWNSCALE` pixels per
    /// image pixel).
    pub fn backdrop(&mut self, size: (u32, u32)) -> Image {
        if self.size != size || self.placeholder.is_none() {
            self.size = size;
            self.targets = None;
            let (w, h) = Self::low_size(size);
            self.placeholder = Some(Image::new(
                vec![0u8; (w * h * 4) as usize].into(),
                ImageFormat::Rgba8,
                w,
                h,
            ));
        }
        self.placeholder.clone().expect("set above")
    }

    /// One pass of the pipeline: `src` blurred along `step` (in uv units; with `radius` 0 a plain
    /// resample) into `dst`.
    fn pass(
        &self,
        device: &wgpu::Device,
        encoder: &mut wgpu::CommandEncoder,
        src: &wgpu::Texture,
        dst: &wgpu::Texture,
        step: [f32; 2],
        sigma: f32,
    ) {
        let radius = if sigma > 0.0 {
            (sigma * 3.0).ceil() as i32
        } else {
            0
        };
        let mut data = Vec::with_capacity(16);
        for v in [step[0], step[1], sigma.max(0.01)] {
            data.extend_from_slice(&v.to_ne_bytes());
        }
        data.extend_from_slice(&radius.to_ne_bytes());
        let uniform = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("blur"),
            contents: &data,
            usage: wgpu::BufferUsages::UNIFORM,
        });
        let src_view = src.create_view(&Default::default());
        let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("blur"),
            layout: &self.pipeline.get_bind_group_layout(0),
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&src_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&self.sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: uniform.as_entire_binding(),
                },
            ],
        });
        let dst_view = dst.create_view(&Default::default());
        let mut rpass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("blur"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &dst_view,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
        });
        rpass.set_pipeline(&self.pipeline);
        rpass.set_bind_group(0, &group, &[]);
        rpass.draw(0..3, 0..1);
    }

    /// Renders `base` then `popups` to `target` (`size` pixels). With popups and a `sigma` (in
    /// pixels of the target), `base` is first rendered and blurred so that the popups, which fill
    /// with [`Blur::backdrop`], show it blurred.
    #[allow(clippy::too_many_arguments)]
    pub fn render(
        &mut self,
        renderer: &mut Renderer,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        base: &Canvas,
        popups: &Canvas,
        target: &wgpu::TextureView,
        base_color: vello::peniko::Color,
        size: (u32, u32),
        sigma: f64,
    ) -> Result<()> {
        let params = RenderParams {
            base_color,
            width: size.0,
            height: size.1,
            antialiasing_method: AaConfig::Area,
        };
        let draw = |renderer: &mut Renderer, scene: &Scene, view: &wgpu::TextureView| {
            renderer
                .render_to_texture(device, queue, scene, view, &params)
                .map_err(|e| anyhow!("render: {e}"))
        };
        let (mut base_scene, mut popups_scene) = (Scene::new(), Scene::new());
        base.replay_vello(&mut base_scene, Affine::IDENTITY);
        popups.replay_vello(&mut popups_scene, Affine::IDENTITY);
        let (base, popups) = (&base_scene, &popups_scene);
        let blurred = sigma > 0.0
            && !popups.encoding().is_empty()
            && self.size == size
            && self.placeholder.is_some();
        let mut full = base.clone();
        full.append(popups, None);
        if !blurred {
            return draw(renderer, &full, target);
        }
        let low_size = Self::low_size(size);
        let targets = self.targets.get_or_insert_with(|| Targets {
            base: texture(
                device,
                size,
                wgpu::TextureUsages::STORAGE_BINDING | wgpu::TextureUsages::TEXTURE_BINDING,
            ),
            low: texture(
                device,
                low_size,
                wgpu::TextureUsages::RENDER_ATTACHMENT
                    | wgpu::TextureUsages::TEXTURE_BINDING
                    | wgpu::TextureUsages::COPY_SRC,
            ),
            scratch: texture(
                device,
                low_size,
                wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            ),
        });
        draw(
            renderer,
            base,
            &targets.base.create_view(&Default::default()),
        )?;
        let sigma = ((sigma / f64::from(DOWNSCALE)) as f32).max(0.5);
        let (lw, lh) = (low_size.0 as f32, low_size.1 as f32);
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("blur"),
        });
        let this = &*self;
        let t = this.targets.as_ref().expect("set above");
        this.pass(device, &mut encoder, &t.base, &t.low, [0.0, 0.0], 0.0);
        this.pass(
            device,
            &mut encoder,
            &t.low,
            &t.scratch,
            [1.0 / lw, 0.0],
            sigma,
        );
        this.pass(
            device,
            &mut encoder,
            &t.scratch,
            &t.low,
            [0.0, 1.0 / lh],
            sigma,
        );
        queue.submit([encoder.finish()]);
        let placeholder = this.placeholder.as_ref().expect("checked above");
        renderer.override_image(
            placeholder,
            Some(wgpu::TexelCopyTextureInfoBase {
                texture: t.low.clone(),
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            }),
        );
        let done = draw(renderer, &full, target);
        renderer.override_image(placeholder, None);
        done
    }
}
