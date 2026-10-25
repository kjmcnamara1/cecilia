//! Graphical frontend for muse: a winit window rendered with vello. Panes show engraved
//! notation from `muse-engrave`; all chrome (tabs, status and command lines) is drawn with the
//! same outline fonts.

mod draw;
mod keys;

use std::num::NonZeroUsize;
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{Context, Result, anyhow};
use muse_app::App;
use muse_app::playback::Playback;
use vello::kurbo::Affine;
use vello::util::{RenderContext, RenderSurface};
use vello::{AaConfig, AaSupport, RenderParams, Renderer, RendererOptions, Scene, wgpu};
use winit::application::ApplicationHandler;
use winit::event::{ElementState, MouseButton, MouseScrollDelta, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::ModifiersState;
use winit::window::{Window, WindowId};

pub use draw::Views;

struct Gpu {
    window: Arc<Window>,
    surface: RenderSurface<'static>,
    renderer: Renderer,
}

struct Gui {
    cx: RenderContext,
    gpu: Option<Gpu>,
    app: App,
    views: Views,
    playback: Playback,
    scene: Scene,
    /// The frame in logical units, before scaling to the window's pixels.
    logical: Scene,
    mods: ModifiersState,
    mouse: (f64, f64),
    last_config_check: Instant,
    error: Option<anyhow::Error>,
}

fn renderer_options() -> RendererOptions {
    RendererOptions {
        use_cpu: false,
        antialiasing_support: AaSupport::area_only(),
        num_init_threads: NonZeroUsize::new(1),
        pipeline_cache: None,
    }
}

/// Opens the GUI window and runs until the user quits.
pub fn run(app: App) -> Result<()> {
    let event_loop = EventLoop::new().context("cannot open a window (no display?)")?;
    let mut gui = Gui {
        cx: RenderContext::new(),
        gpu: None,
        app,
        views: Views::default(),
        playback: Playback::default(),
        scene: Scene::new(),
        logical: Scene::new(),
        mods: ModifiersState::empty(),
        mouse: (0.0, 0.0),
        last_config_check: Instant::now(),
        error: None,
    };
    event_loop.run_app(&mut gui)?;
    match gui.error {
        Some(e) => Err(e),
        None => Ok(()),
    }
}

impl Gui {
    fn redraw(&mut self) -> Result<()> {
        let Some(gpu) = self.gpu.as_mut() else {
            return Ok(());
        };
        let (width, height) = (gpu.surface.config.width, gpu.surface.config.height);
        // Lay out in logical units so the UI keeps its size on HiDPI and fractionally scaled
        // monitors, then scale the frame to the surface's pixels.
        let factor = gpu.window.scale_factor().max(0.1);
        self.logical.reset();
        let play = self.playback.location(&self.app);
        draw::build(
            &mut self.logical,
            &mut self.app,
            &mut self.views,
            Some(&mut self.playback),
            play,
            width as f64 / factor,
            height as f64 / factor,
        );
        self.scene.reset();
        self.scene
            .append(&self.logical, Some(Affine::scale(factor)));
        let dev = &self.cx.devices[gpu.surface.dev_id];
        gpu.renderer
            .render_to_texture(
                &dev.device,
                &dev.queue,
                &self.scene,
                &gpu.surface.target_view,
                &RenderParams {
                    base_color: draw::BACKGROUND,
                    width,
                    height,
                    antialiasing_method: AaConfig::Area,
                },
            )
            .map_err(|e| anyhow!("render: {e}"))?;
        let frame = gpu
            .surface
            .surface
            .get_current_texture()
            .context("surface texture")?;
        let mut encoder = dev
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("blit"),
            });
        gpu.surface.blitter.copy(
            &dev.device,
            &mut encoder,
            &gpu.surface.target_view,
            &frame
                .texture
                .create_view(&wgpu::TextureViewDescriptor::default()),
        );
        dev.queue.submit([encoder.finish()]);
        frame.present();
        Ok(())
    }

    /// Physical pixels per logical unit of the window (1.5 on a 150% scaled monitor).
    fn scale_factor(&self) -> f64 {
        self.gpu
            .as_ref()
            .map_or(1.0, |g| g.window.scale_factor().max(0.1))
    }

    fn after_input(&mut self) {
        for effect in self.app.take_effects() {
            self.playback.apply(&mut self.app, effect);
        }
        if let Some(gpu) = &self.gpu {
            gpu.window.request_redraw();
        }
    }
}

impl ApplicationHandler for Gui {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.gpu.is_some() {
            return;
        }
        let attrs = Window::default_attributes()
            .with_title("muse")
            .with_inner_size(winit::dpi::LogicalSize::new(1280.0, 860.0));
        let result = (|| -> Result<Gpu> {
            let window = Arc::new(event_loop.create_window(attrs)?);
            let size = window.inner_size();
            let surface = pollster::block_on(self.cx.create_surface(
                window.clone(),
                size.width.max(1),
                size.height.max(1),
                wgpu::PresentMode::AutoVsync,
            ))
            .map_err(|e| anyhow!("surface: {e}"))?;
            let dev = &self.cx.devices[surface.dev_id];
            let renderer = Renderer::new(&dev.device, renderer_options())
                .map_err(|e| anyhow!("renderer: {e}"))?;
            Ok(Gpu {
                window,
                surface,
                renderer,
            })
        })();
        match result {
            Ok(gpu) => {
                gpu.window.request_redraw();
                self.gpu = Some(gpu);
            }
            Err(e) => {
                self.error = Some(e);
                event_loop.exit();
            }
        }
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Resized(size) => {
                if let Some(gpu) = self.gpu.as_mut() {
                    self.cx
                        .resize_surface(&mut gpu.surface, size.width.max(1), size.height.max(1));
                    gpu.window.request_redraw();
                }
            }
            WindowEvent::ScaleFactorChanged { .. } => {
                // Moved to a monitor with another scale; the new size follows as `Resized`.
                if let Some(gpu) = &self.gpu {
                    gpu.window.request_redraw();
                }
            }
            WindowEvent::RedrawRequested => {
                if let Err(e) = self.redraw() {
                    self.app.error(format!("{e:#}"));
                }
            }
            WindowEvent::ModifiersChanged(m) => self.mods = m.state(),
            WindowEvent::KeyboardInput { event, .. } if event.state == ElementState::Pressed => {
                if let Some(key) = keys::convert(&event, self.mods) {
                    self.app.handle_key(key);
                    self.after_input();
                }
            }
            WindowEvent::CursorMoved { position, .. } => {
                // Logical coordinates, as the frame is laid out in.
                let f = self.scale_factor();
                self.mouse = (position.x / f, position.y / f);
            }
            WindowEvent::MouseInput {
                state: ElementState::Pressed,
                button: MouseButton::Left,
                ..
            } => {
                self.views.click(&mut self.app, self.mouse.0, self.mouse.1);
                self.after_input();
            }
            WindowEvent::MouseWheel { delta, .. } => {
                let dy = match delta {
                    MouseScrollDelta::LineDelta(_, y) => y as f64 * 60.0,
                    MouseScrollDelta::PixelDelta(p) => p.y / self.scale_factor(),
                };
                if self.mods.control_key() {
                    self.app.run(if dy > 0.0 {
                        muse_app::Command::ZoomIn
                    } else {
                        muse_app::Command::ZoomOut
                    });
                } else {
                    self.views
                        .scroll(&self.app, self.mouse.0, self.mouse.1, -dy);
                }
                self.after_input();
            }
            _ => {}
        }
        if self.app.quit {
            event_loop.exit();
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        self.playback.sync(&mut self.app);
        if self.app.playing {
            if let Some(gpu) = &self.gpu {
                gpu.window.request_redraw();
            }
            event_loop.set_control_flow(ControlFlow::WaitUntil(
                Instant::now() + Duration::from_millis(16),
            ));
        } else {
            event_loop.set_control_flow(ControlFlow::WaitUntil(
                Instant::now() + Duration::from_millis(500),
            ));
        }
        if self.last_config_check.elapsed() > Duration::from_secs(1) {
            self.last_config_check = Instant::now();
            self.app.poll_config();
            self.after_input();
        }
    }
}

/// Renders one frame offscreen to a PNG (for tests and documentation), `width` × `height`
/// physical pixels at a display scale factor (1.5 = a 150% scaled monitor).
pub fn screenshot(mut app: App, width: u32, height: u32, factor: f64) -> Result<Vec<u8>> {
    let factor = factor.max(0.1);
    let mut cx = RenderContext::new();
    let dev_id = pollster::block_on(cx.device(None)).ok_or_else(|| anyhow!("no GPU adapter"))?;
    let dev = &cx.devices[dev_id];
    let (device, queue) = (&dev.device, &dev.queue);
    let mut renderer =
        Renderer::new(device, renderer_options()).map_err(|e| anyhow!("renderer: {e}"))?;
    let mut scene = Scene::new();
    let mut views = Views::default();
    // Load the focused buffer's audio so the waveform shows.
    let mut playback = Playback::default();
    let buffer = app.pane().buffer;
    playback.ensure_audio(&app, buffer);
    for _ in 0..200 {
        playback.sync(&mut app);
        if playback.loaded_audio(buffer).is_some() || !playback.audio_loading(buffer) {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    let mut logical = Scene::new();
    draw::build(
        &mut logical,
        &mut app,
        &mut views,
        Some(&mut playback),
        None,
        width as f64 / factor,
        height as f64 / factor,
    );
    scene.append(&logical, Some(Affine::scale(factor)));
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("screenshot"),
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::STORAGE_BINDING | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    renderer
        .render_to_texture(
            device,
            queue,
            &scene,
            &view,
            &RenderParams {
                base_color: draw::BACKGROUND,
                width,
                height,
                antialiasing_method: AaConfig::Area,
            },
        )
        .map_err(|e| anyhow!("render: {e}"))?;
    let row = (width * 4).div_ceil(256) * 256;
    let buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("readback"),
        size: (row * height) as u64,
        usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("copy"),
    });
    encoder.copy_texture_to_buffer(
        texture.as_image_copy(),
        wgpu::TexelCopyBufferInfo {
            buffer: &buffer,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(row),
                rows_per_image: None,
            },
        },
        wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
    );
    queue.submit([encoder.finish()]);
    let slice = buffer.slice(..);
    slice.map_async(wgpu::MapMode::Read, |_| {});
    device.poll(wgpu::Maintain::Wait);
    let data = slice.get_mapped_range();
    let mut pixels = Vec::with_capacity((width * height * 4) as usize);
    for y in 0..height {
        let start = (y * row) as usize;
        pixels.extend_from_slice(&data[start..start + (width * 4) as usize]);
    }
    let size = tiny_skia::IntSize::from_wh(width, height).ok_or_else(|| anyhow!("bad size"))?;
    let pixmap = tiny_skia::Pixmap::from_vec(pixels, size).ok_or_else(|| anyhow!("bad pixmap"))?;
    pixmap.encode_png().map_err(|e| anyhow!("png: {e}"))
}
