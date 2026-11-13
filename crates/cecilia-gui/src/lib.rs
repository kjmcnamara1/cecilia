//! Graphical frontend for cecilia: a winit window rendered with vello. Panes show engraved
//! notation from `cecilia-engrave`; all chrome (tabs, status and command lines) is drawn with the
//! same outline fonts.

mod backend;
mod blur;
mod canvas;
mod design;
mod draw;
mod keys;
mod mono;

use std::num::NonZeroUsize;
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{Context, Result, anyhow};
use canvas::Canvas;
use cecilia_app::App;
use cecilia_app::playback::Playback;
use vello::kurbo::Affine;
use vello::util::RenderContext;
use vello::{AaSupport, Renderer, RendererOptions, wgpu};
use winit::application::ApplicationHandler;
use winit::event::{ElementState, MouseButton, MouseScrollDelta, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::ModifiersState;
use winit::window::{CursorIcon, Window, WindowId};

pub use draw::Views;

struct Gpu {
    window: Arc<Window>,
    surface: Surf,
    renderer: Renderer,
    blur: blur::Blur,
}

struct Gui {
    /// The peeking tracks were drawn last frame (one more redraw hides them).
    tracks_peek_drawn: bool,
    notifications_drawn: bool,
    gpu: Option<Gpu>,
    app: App,
    views: Views,
    playback: Playback,
    scene: Canvas,
    /// The frame in logical units, before scaling to the window's pixels, and its popups.
    logical: Canvas,
    popups: Canvas,
    mods: ModifiersState,
    mouse: (f64, f64),
    /// The pointer shape shown (resize arrows over draggable edges).
    pointer: draw::Pointer,
    /// The tracks panel was peeking when last drawn (redraw once it hides).
    last_config_check: Instant,
    error: Option<anyhow::Error>,
}

/// `CECILIA_CPU=1` runs vello's shader stages on the CPU instead of the GPU's compute shaders,
/// for drivers (virtual machines, software renderers) where those crash or are missing. The
/// result is still presented through wgpu.
///
/// `software` (the software-adapter attempt) forces that on: lavapipe-like drivers rarely run
/// vello's compute shaders reliably, and the CPU path is the one that works there.
fn renderer_options(software: bool) -> RendererOptions {
    RendererOptions {
        use_cpu: software
            || std::env::var_os("CECILIA_CPU").is_some_and(|v| !v.is_empty() && v != "0"),
        antialiasing_support: AaSupport::area_only(),
        num_init_threads: NonZeroUsize::new(1),
        pipeline_cache: None,
    }
}

/// The wgpu context. On Windows, wgpu's default (all backends) can pick DX12 or its WARP software
/// renderer, which crashes or draws nothing on some laptops, while Vulkan works. So unless
/// `WGPU_BACKEND` says otherwise, Vulkan is used when it has an adapter, else everything.
fn render_context() -> RenderContext {
    #[cfg(windows)]
    if std::env::var_os("WGPU_BACKEND").is_none() {
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
            backends: wgpu::Backends::VULKAN,
            ..Default::default()
        });
        if !instance
            .enumerate_adapters(wgpu::Backends::VULKAN)
            .is_empty()
        {
            return RenderContext {
                instance,
                devices: Vec::new(),
            };
        }
    }
    RenderContext::new()
}

/// The window surface with the device that draws to it, from either renderer attempt. vello's
/// `RenderSurface` cannot be built for a device we create ourselves (its adapter is private),
/// so both attempts end up in this struct.
struct Surf {
    surface: wgpu::Surface<'static>,
    config: wgpu::SurfaceConfiguration,
    device: wgpu::Device,
    queue: wgpu::Queue,
    /// Vello draws here; it is blitted to the swapchain image.
    target_view: wgpu::TextureView,
    blitter: wgpu::util::TextureBlitter,
}

impl Surf {
    fn new(
        surface: wgpu::Surface<'static>,
        adapter: &wgpu::Adapter,
        device: wgpu::Device,
        queue: wgpu::Queue,
        size: winit::dpi::PhysicalSize<u32>,
    ) -> Result<Self> {
        let format = surface
            .get_capabilities(adapter)
            .formats
            .into_iter()
            .find(|f| {
                matches!(
                    f,
                    wgpu::TextureFormat::Rgba8Unorm | wgpu::TextureFormat::Bgra8Unorm
                )
            })
            .ok_or_else(|| anyhow!("surface: no supported format"))?;
        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            width: size.width.max(1),
            height: size.height.max(1),
            present_mode: wgpu::PresentMode::AutoVsync,
            desired_maximum_frame_latency: 2,
            alpha_mode: wgpu::CompositeAlphaMode::Auto,
            view_formats: vec![],
        };
        let target_view = Self::target_view(&device, config.width, config.height);
        surface.configure(&device, &config);
        let blitter = wgpu::util::TextureBlitter::new(&device, format);
        Ok(Self {
            surface,
            config,
            device,
            queue,
            target_view,
            blitter,
        })
    }

    fn target_view(device: &wgpu::Device, width: u32, height: u32) -> wgpu::TextureView {
        device
            .create_texture(&wgpu::TextureDescriptor {
                label: None,
                size: wgpu::Extent3d {
                    width,
                    height,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                usage: wgpu::TextureUsages::STORAGE_BINDING | wgpu::TextureUsages::TEXTURE_BINDING,
                format: wgpu::TextureFormat::Rgba8Unorm,
                view_formats: &[],
            })
            .create_view(&wgpu::TextureViewDescriptor::default())
    }

    fn resize(&mut self, width: u32, height: u32) {
        self.config.width = width.max(1);
        self.config.height = height.max(1);
        self.target_view = Self::target_view(&self.device, self.config.width, self.config.height);
        self.surface.configure(&self.device, &self.config);
    }
}

/// Device and queue for `adapter`, with the optional features vello benefits from.
fn request_device(adapter: &wgpu::Adapter) -> Result<(wgpu::Device, wgpu::Queue)> {
    let maybe = wgpu::Features::CLEAR_TEXTURE | wgpu::Features::PIPELINE_CACHE;
    pollster::block_on(adapter.request_device(
        &wgpu::DeviceDescriptor {
            label: None,
            required_features: adapter.features() & maybe,
            required_limits: wgpu::Limits::default(),
            memory_hints: wgpu::MemoryHints::default(),
        },
        None,
    ))
    .map_err(|e| anyhow!("device: {e}"))
}

/// The first software (CPU) adapter of `instance` that can present to `surface` (any, without
/// one), such as lavapipe or llvmpipe, with a device on it. Never panics: no adapter or a
/// failing device is an `Err`.
fn software_device(
    instance: &wgpu::Instance,
    surface: Option<&wgpu::Surface<'_>>,
) -> Result<(wgpu::Adapter, wgpu::Device, wgpu::Queue)> {
    let adapters: Vec<_> = instance
        .enumerate_adapters(wgpu::Backends::all())
        .into_iter()
        .filter(|a| surface.is_none_or(|s| a.is_surface_supported(s)))
        .collect();
    let infos: Vec<_> = adapters.iter().map(|a| a.get_info()).collect();
    let i = backend::pick_software(&infos, cfg!(windows))
        .ok_or_else(|| anyhow!("no software adapter found"))?;
    let adapter = adapters
        .into_iter()
        .nth(i)
        .expect("index from pick_software");
    let (device, queue) = request_device(&adapter)?;
    Ok((adapter, device, queue))
}

/// Opens the renderer for one `attempt`: the window's surface and device, vello's renderer and
/// the blur pipeline. The GPU attempt uses vello's own device selection; the software one
/// picks the adapter itself.
fn open_wgpu(
    attempt: backend::Attempt,
    window: Arc<Window>,
    size: winit::dpi::PhysicalSize<u32>,
) -> Result<(Surf, Renderer, blur::Blur)> {
    let software = attempt == backend::Attempt::Software;
    let surf = if software {
        let instance = RenderContext::new().instance;
        let surface = instance
            .create_surface(window)
            .map_err(|e| anyhow!("surface: {e}"))?;
        let (adapter, device, queue) = software_device(&instance, Some(&surface))?;
        Surf::new(surface, &adapter, device, queue, size)?
    } else {
        let mut cx = render_context();
        let rs = pollster::block_on(cx.create_surface(
            window,
            size.width.max(1),
            size.height.max(1),
            wgpu::PresentMode::AutoVsync,
        ))
        .map_err(|e| anyhow!("surface: {e}"))?;
        let dev = &cx.devices[rs.dev_id];
        Surf {
            surface: rs.surface,
            config: rs.config,
            device: dev.device.clone(),
            queue: dev.queue.clone(),
            target_view: rs.target_view,
            blitter: rs.blitter,
        }
    };
    let blur = blur::Blur::new(&surf.device);
    let renderer = Renderer::new(&surf.device, renderer_options(software))
        .map_err(|e| anyhow!("renderer: {e}"))?;
    Ok((surf, renderer, blur))
}

/// The error for when every attempt in `tried` failed; `last` is the final attempt's error.
fn no_renderer(tried: &[backend::Attempt], last: anyhow::Error) -> anyhow::Error {
    let names: Vec<_> = tried
        .iter()
        .map(|a| match a {
            backend::Attempt::Gpu => "gpu",
            backend::Attempt::Software => "software",
        })
        .collect();
    last.context(format!(
        "no usable renderer (tried {}); set ui.renderer or CECILIA_RENDERER, install a software \
         Vulkan driver (lavapipe), or use the terminal UI",
        names.join(", ")
    ))
}

/// The renderer modes to try: `CECILIA_RENDERER` over `ui.renderer`.
fn renderer_attempts(app: &App) -> Vec<backend::Attempt> {
    let env = std::env::var("CECILIA_RENDERER").ok();
    backend::attempts(backend::resolve_mode(
        env.as_deref(),
        app.config.file.ui.renderer,
    ))
}

/// Most bytes a screenshot may take (RGBA, plus the readback's padding).
const MAX_IMAGE_BYTES: u64 = 1 << 30;

/// The screenshot size limited to what the GPU can make a texture of (`max_dimension`) and to a
/// memory budget, keeping the aspect ratio; None for an empty or absurd request.
fn clamp_image_size(width: u32, height: u32, max_dimension: u32) -> Option<(u32, u32)> {
    if width == 0 || height == 0 || max_dimension == 0 {
        return None;
    }
    let mut k = f64::min(1.0, f64::from(max_dimension) / f64::from(width.max(height)));
    let bytes = |k: f64| (f64::from(width) * k).floor() * (f64::from(height) * k).floor() * 4.0;
    if bytes(k) > MAX_IMAGE_BYTES as f64 {
        k *= (MAX_IMAGE_BYTES as f64 / bytes(k)).sqrt();
    }
    let size = (
        ((f64::from(width) * k).floor() as u32).max(1),
        ((f64::from(height) * k).floor() as u32).max(1),
    );
    // 256-aligned rows of 4 bytes per pixel must fit in u32.
    (u64::from(size.0) * 4 + 255 < u64::from(u32::MAX)).then_some(size)
}

/// Opens the GUI window and runs until the user quits.
pub fn run(app: App) -> Result<()> {
    let event_loop = EventLoop::new().context("cannot open a window (no display?)")?;
    let mut gui = Gui {
        tracks_peek_drawn: false,
        notifications_drawn: false,
        gpu: None,
        app,
        views: Views::default(),
        playback: Playback::default(),
        scene: Canvas::new(),
        logical: Canvas::new(),
        popups: Canvas::new(),
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
        self.popups.reset();
        let play = self.playback.location(&self.app);
        let blur_px = self.app.config.file.popups.blur;
        self.views.set_backdrop((blur_px > 0.0).then(|| {
            (
                gpu.blur.backdrop((width, height)),
                f64::from(blur::DOWNSCALE) / factor,
            )
        }));
        draw::build(
            &mut self.logical,
            &mut self.popups,
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
        let mut popups = Canvas::new();
        popups.append(&self.popups, Some(Affine::scale(factor)));
        gpu.blur.render(
            &mut gpu.renderer,
            &gpu.surface.device,
            &gpu.surface.queue,
            &self.scene,
            &popups,
            &gpu.surface.target_view,
            draw::background(),
            (width, height),
            blur_px * factor,
        )?;
        let frame = match gpu.surface.surface.get_current_texture() {
            Ok(frame) => frame,
            // The surface no longer matches the window (resized, minimized, device reset):
            // configure it again and draw the next frame instead of reporting an error.
            Err(wgpu::SurfaceError::Outdated | wgpu::SurfaceError::Lost) => {
                gpu.surface.resize(width, height);
                gpu.window.request_redraw();
                return Ok(());
            }
            Err(wgpu::SurfaceError::Timeout) => {
                gpu.window.request_redraw();
                return Ok(());
            }
            Err(e) => return Err(anyhow!("surface texture: {e}")),
        };
        let mut encoder =
            gpu.surface
                .device
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("blit"),
                });
        gpu.surface.blitter.copy(
            &gpu.surface.device,
            &mut encoder,
            &gpu.surface.target_view,
            &frame
                .texture
                .create_view(&wgpu::TextureViewDescriptor::default()),
        );
        gpu.surface.queue.submit([encoder.finish()]);
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
            let attempts = renderer_attempts(&self.app);
            let mut failure = None;
            let mut opened = None;
            for (n, &attempt) in attempts.iter().enumerate() {
                match open_wgpu(attempt, window.clone(), size) {
                    Ok(o) => {
                        opened = Some(o);
                        break;
                    }
                    Err(e) => failure = Some((n, e)),
                }
            }
            let Some((surface, renderer, blur)) = opened else {
                let (n, e) = failure.expect("at least one renderer attempt");
                return Err(no_renderer(&attempts[..=n], e));
            };
            Ok(Gpu {
                window,
                surface,
                renderer,
                blur,
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
                    gpu.surface.resize(size.width, size.height);
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
                // Hover effects on the tabline (close buttons) and dashboard buttons need a
                // redraw.
                if (was_over
                    || self.views.over_tabline(self.mouse.1)
                    || self.app.dashboard_visible()
                    || self.app.viewer().is_some())
                    && let Some(gpu) = &self.gpu
                {
                    gpu.window.request_redraw();
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
        } else if self.app.notifications_active() || self.notifications_drawn {
            // Notification cards: redraw (their bars drain) until the last one is gone.
            self.notifications_drawn = self.app.notifications_active();
            if let Some(gpu) = &self.gpu {
                gpu.window.request_redraw();
            }
            event_loop.set_control_flow(ControlFlow::WaitUntil(
                Instant::now() + Duration::from_millis(33),
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
    let attempts = renderer_attempts(&app);
    let mut failure = None;
    let mut opened = None;
    for (n, &attempt) in attempts.iter().enumerate() {
        let result = match attempt {
            backend::Attempt::Gpu => {
                let mut cx = render_context();
                pollster::block_on(cx.device(None))
                    .map(|id| (cx.devices[id].device.clone(), cx.devices[id].queue.clone()))
                    .ok_or_else(|| anyhow!("no GPU adapter"))
            }
            backend::Attempt::Software => {
                let instance = RenderContext::new().instance;
                software_device(&instance, None).map(|(_, device, queue)| (device, queue))
            }
        };
        match result {
            Ok(o) => {
                opened = Some((o, attempt == backend::Attempt::Software));
                break;
            }
            Err(e) => failure = Some((n, e)),
        }
    }
    let Some(((device, queue), software)) = opened else {
        let (n, e) = failure.expect("at least one renderer attempt");
        return Err(no_renderer(&attempts[..=n], e));
    };
    let (device, queue) = (&device, &queue);
    let (width, height) = clamp_image_size(width, height, device.limits().max_texture_dimension_2d)
        .ok_or_else(|| anyhow!("screenshot size {width}x{height} is too large"))?;
    let mut renderer =
        Renderer::new(device, renderer_options(software)).map_err(|e| anyhow!("renderer: {e}"))?;
    let mut scene = Canvas::new();
    let mut blur = blur::Blur::new(device);
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
    let mut logical = Canvas::new();
    let mut popups = Canvas::new();
    let blur_px = app.config.file.popups.blur;
    views.set_backdrop((blur_px > 0.0).then(|| {
        (
            blur.backdrop((width, height)),
            f64::from(blur::DOWNSCALE) / factor,
        )
    }));
    // Hovering a toolbar button: draw once and wait, so its tooltip shows.
    if hover.is_some() {
        let mut first = Canvas::new();
        draw::build(
            &mut first,
            &mut Canvas::new(),
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
        &mut popups,
        &mut app,
        &mut views,
        Some(&mut playback),
        None,
        width as f64 / factor,
        height as f64 / factor,
    );
    scene.append(&logical, Some(Affine::scale(factor)));
    let mut popups_dev = Canvas::new();
    popups_dev.append(&popups, Some(Affine::scale(factor)));
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
    blur.render(
        &mut renderer,
        device,
        queue,
        &scene,
        &popups_dev,
        &view,
        draw::background(),
        (width, height),
        blur_px * factor,
    )?;
    let row = (width * 4).div_ceil(256) * 256; // fits: clamp_image_size bounds the size
    let buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("readback"),
        size: u64::from(row) * u64::from(height),
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
        let start = y as usize * row as usize;
        pixels.extend_from_slice(&data[start..start + (width * 4) as usize]);
    }
    let size = tiny_skia::IntSize::from_wh(width, height).ok_or_else(|| anyhow!("bad size"))?;
    let pixmap = tiny_skia::Pixmap::from_vec(pixels, size).ok_or_else(|| anyhow!("bad pixmap"))?;
    pixmap.encode_png().map_err(|e| anyhow!("png: {e}"))
}

#[cfg(test)]
mod tests {
    use super::clamp_image_size;

    #[test]
    fn screenshot_size_is_clamped_to_the_gpu_limit() {
        assert_eq!(clamp_image_size(1400, 900, 16384), Some((1400, 900)));
        // 16x scale on a monitor with a 16384 px texture limit.
        let (w, h) = clamp_image_size(22400, 14400, 16384).unwrap();
        assert!(w <= 16384 && h <= 16384 && w > 16000);
        assert_eq!(clamp_image_size(0, 900, 16384), None);
        // Huge requests are cut to the memory budget instead of overflowing.
        let (w, h) = clamp_image_size(u32::MAX, u32::MAX, u32::MAX).unwrap();
        assert!(u64::from(w) * u64::from(h) * 4 <= super::MAX_IMAGE_BYTES);
    }
}
