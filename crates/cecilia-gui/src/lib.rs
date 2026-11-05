//! Graphical frontend for cecilia: a winit window rendered with vello. Panes show engraved
//! notation from `cecilia-engrave`; all chrome (tabs, status and command lines) is drawn with the
//! same outline fonts.

mod design;
mod draw;
mod keys;

use std::num::NonZeroUsize;
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{Context, Result, anyhow};
use cecilia_app::App;
use cecilia_app::playback::Playback;
use vello::kurbo::Affine;
use vello::util::{RenderContext, RenderSurface};
use vello::{AaConfig, AaSupport, RenderParams, Renderer, RendererOptions, Scene, wgpu};
use winit::application::ApplicationHandler;
use winit::event::{ElementState, MouseButton, MouseScrollDelta, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::ModifiersState;
use winit::window::{CursorIcon, Window, WindowId};

pub use draw::Views;

struct Gpu {
    window: Arc<Window>,
    surface: RenderSurface<'static>,
    renderer: Renderer,
}

struct Gui {
    /// The peeking tracks were drawn last frame (one more redraw hides them).
    tracks_peek_drawn: bool,
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
    /// The pointer shape shown (resize arrows over draggable edges).
    pointer: draw::Pointer,
    /// The tracks panel was peeking when last drawn (redraw once it hides).
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
        tracks_peek_drawn: false,
        cx: RenderContext::new(),
        gpu: None,
        app,
        views: Views::default(),
        playback: Playback::default(),
        scene: Scene::new(),
        logical: Scene::new(),
        mods: ModifiersState::empty(),
        mouse: (0.0, 0.0),
        pointer: draw::Pointer::Default,
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
                    base_color: draw::background(),
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
            if let Some(effect) = cecilia_app::dialog::apply(&mut self.app, effect) {
                self.playback.apply(&mut self.app, effect);
            }
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
            .with_title("cecilia")
            .with_inner_size(winit::dpi::LogicalSize::new(1280.0, 860.0));
        // The app ID / WM class matches cecilia.desktop, so desktops show its name and icon.
        #[cfg(all(unix, not(target_os = "macos")))]
        let attrs = {
            use winit::platform::wayland::WindowAttributesExtWayland;
            use winit::platform::x11::WindowAttributesExtX11;
            let attrs = WindowAttributesExtWayland::with_name(attrs, "cecilia", "cecilia");
            WindowAttributesExtX11::with_name(attrs, "cecilia", "cecilia")
        };
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
            // Like :quit: refused (with a message) while files have unsaved changes.
            WindowEvent::CloseRequested => {
                self.app.run(cecilia_app::Command::Quit { force: false });
                self.after_input();
            }
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
                let was_over = self.views.over_tabline(self.mouse.1);
                self.mouse = (position.x / f, position.y / f);
                self.views.hover = self.mouse;
                if let Some(row) = self.views.palette_row_at(self.mouse.0, self.mouse.1) {
                    self.app.hover_palette(row);
                    self.after_input();
                }
                if self
                    .views
                    .drag_to(&mut self.app, self.mouse.0, self.mouse.1)
                {
                    self.after_input();
                }
                // Resize arrows over pane borders and panel edges.
                let pointer = self.views.pointer(&self.app, self.mouse.0, self.mouse.1);
                if pointer != self.pointer {
                    self.pointer = pointer;
                    if let Some(gpu) = &self.gpu {
                        gpu.window.set_cursor(match pointer {
                            draw::Pointer::Default => CursorIcon::Default,
                            draw::Pointer::ResizeColumns => CursorIcon::ColResize,
                            draw::Pointer::ResizeRows => CursorIcon::RowResize,
                        });
                    }
                }
                // Hover effects on the tabline (close buttons) need a redraw.
                if was_over || self.views.over_tabline(self.mouse.1) {
                    if let Some(gpu) = &self.gpu {
                        gpu.window.request_redraw();
                    }
                }
            }
            WindowEvent::CursorLeft { .. } => {
                self.views.hover = (-1.0, -1.0);
                if let Some(gpu) = &self.gpu {
                    gpu.window.request_redraw();
                }
            }
            WindowEvent::MouseInput {
                state: ElementState::Pressed,
                button: MouseButton::Left,
                ..
            } => {
                // Shift extends a selection; Ctrl or Alt selects whole bars.
                self.views.select_mods = (
                    self.mods.shift_key(),
                    self.mods.control_key() || self.mods.alt_key(),
                );
                self.views.click(&mut self.app, self.mouse.0, self.mouse.1);
                self.after_input();
            }
            WindowEvent::MouseInput {
                state: ElementState::Released,
                button: MouseButton::Left,
                ..
            } => {
                self.views.release(&mut self.app);
                self.after_input();
            }
            WindowEvent::MouseInput {
                state: ElementState::Pressed,
                button: MouseButton::Middle,
                ..
            } => {
                // Middle-click closes a buffer tab, as in bufferline.nvim.
                self.views
                    .middle_click(&mut self.app, self.mouse.0, self.mouse.1);
                self.after_input();
            }
            WindowEvent::MouseWheel { delta, .. } => {
                // How far the view moves right and down (the content moves the other way).
                let (mut dx, mut dy) = match delta {
                    MouseScrollDelta::LineDelta(x, y) => (-x as f64 * 60.0, -y as f64 * 60.0),
                    MouseScrollDelta::PixelDelta(p) => {
                        let f = self.scale_factor();
                        (-p.x / f, -p.y / f)
                    }
                };
                // Shift turns the vertical wheel sideways.
                if self.mods.shift_key() && dx == 0.0 {
                    (dx, dy) = (dy, 0.0);
                }
                let (mx, my) = self.mouse;
                if self.app.mode == cecilia_app::Mode::Picker {
                    // The wheel over a picker moves its selection.
                    self.app.picker_scroll(if dy > 0.0 { 1 } else { -1 });
                    self.after_input();
                    return;
                }
                if self
                    .views
                    .wheel(&self.app, mx, my, dx, dy, self.mods.control_key())
                {
                    if let Some(gpu) = &self.gpu {
                        gpu.window.request_redraw();
                    }
                } else if self.mods.control_key() {
                    // Zooms the pane under the mouse, not the focused one.
                    if let Some(id) = self.views.pane_at(mx, my) {
                        self.app
                            .zoom_pane(id, if dy < 0.0 { 1.15 } else { 1.0 / 1.15 });
                    }
                } else {
                    self.views.scroll(mx, my, dx, dy);
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
        if self.views.selecting() {
            // Dragging a selection near the pane's edge scrolls it, also while the pointer rests.
            if self.views.autoscroll(&mut self.app) {
                self.after_input();
            }
            event_loop.set_control_flow(ControlFlow::WaitUntil(
                Instant::now() + Duration::from_millis(16),
            ));
        } else if self.app.playing {
            if let Some((buffer, loc, _, _)) = self.playback.location(&self.app) {
                self.app.follow_playback(buffer, loc.bar, loc.tick);
            }
            if let Some(gpu) = &self.gpu {
                gpu.window.request_redraw();
            }
            event_loop.set_control_flow(ControlFlow::WaitUntil(
                Instant::now() + Duration::from_millis(16),
            ));
        } else if self.app.has_tool(cecilia_app::PaneKind::Tuner) {
            // The tuner's needle moves with what it hears.
            if let Some(gpu) = &self.gpu {
                gpu.window.request_redraw();
            }
            event_loop.set_control_flow(ControlFlow::WaitUntil(
                Instant::now() + Duration::from_millis(33),
            ));
        } else if self.app.tracks_peeking() || self.tracks_peek_drawn {
            // The tracks peeking after a change of track: redraw until they hide again.
            self.tracks_peek_drawn = self.app.tracks_peeking();
            if let Some(gpu) = &self.gpu {
                gpu.window.request_redraw();
            }
            event_loop.set_control_flow(ControlFlow::WaitUntil(
                Instant::now() + Duration::from_millis(100),
            ));
        } else if self.views.tooltip_pending() {
            // A toolbar tooltip waits to show.
            if let Some(gpu) = &self.gpu {
                gpu.window.request_redraw();
            }
            event_loop.set_control_flow(ControlFlow::WaitUntil(
                Instant::now() + Duration::from_millis(100),
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
pub fn screenshot(
    mut app: App,
    width: u32,
    height: u32,
    factor: f64,
    hover: Option<(f64, f64)>,
) -> Result<Vec<u8>> {
    let factor = factor.max(0.1);
    let mut cx = RenderContext::new();
    let dev_id = pollster::block_on(cx.device(None)).ok_or_else(|| anyhow!("no GPU adapter"))?;
    let dev = &cx.devices[dev_id];
    let (device, queue) = (&dev.device, &dev.queue);
    let mut renderer =
        Renderer::new(device, renderer_options()).map_err(|e| anyhow!("renderer: {e}"))?;
    let mut scene = Scene::new();
    let mut views = Views::default();
    views.hover = hover.unwrap_or((-1.0, -1.0));
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
    // Hovering a toolbar button: draw once and wait, so its tooltip shows.
    if hover.is_some() {
        let mut first = Scene::new();
        draw::build(
            &mut first,
            &mut app,
            &mut views,
            Some(&mut playback),
            None,
            width as f64 / factor,
            height as f64 / factor,
        );
        if views.tooltip_pending() {
            std::thread::sleep(std::time::Duration::from_millis(550));
        }
    }
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
                base_color: draw::background(),
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
