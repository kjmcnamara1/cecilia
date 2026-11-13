//! Graphical frontend for cecilia: a winit window rendered with vello. Panes show engraved
//! notation from `cecilia-engrave`; all chrome (tabs, status and command lines) is drawn with the
//! same outline fonts.

mod backend;
mod blur;
mod canvas;
mod cpu;
mod design;
mod draw;
mod keys;
mod mono;

use std::num::{NonZeroU32, NonZeroUsize};
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

/// The wgpu renderer: vello on a GPU (or software Vulkan) device, blitted to the window.
struct Gpu {
    window: Arc<Window>,
    surface: Surf,
    renderer: Renderer,
    blur: blur::Blur,
}

/// The CPU renderer's window: frames are drawn by `cpu::render` and presented with softbuffer, so
/// no graphics API is involved.
struct CpuWindow {
    window: Arc<Window>,
    surface: softbuffer::Surface<Arc<Window>, Arc<Window>>,
    /// The size softbuffer's buffer is set to.
    size: (u32, u32),
    /// The backdrop placeholder popups fill with, for the size it was made for.
    placeholder: Option<((u32, u32), vello::peniko::Image)>,
}

impl CpuWindow {
    fn new(window: Arc<Window>) -> Result<Self> {
        let context = softbuffer::Context::new(window.clone())
            .map_err(|e| anyhow!("softbuffer context: {e}"))?;
        let surface = softbuffer::Surface::new(&context, window.clone())
            .map_err(|e| anyhow!("softbuffer surface: {e}"))?;
        Ok(Self {
            window,
            surface,
            size: (0, 0),
            placeholder: None,
        })
    }

    /// Sizes the buffer to the window; a zero size (minimised) leaves it alone, as nothing is
    /// drawn then.
    fn resize(&mut self, width: u32, height: u32) -> Result<()> {
        if let (Some(w), Some(h)) = (NonZeroU32::new(width), NonZeroU32::new(height))
            && self.size != (width, height)
        {
            self.surface
                .resize(w, h)
                .map_err(|e| anyhow!("softbuffer resize: {e}"))?;
            self.size = (width, height);
        }
        Ok(())
    }

    fn placeholder(&mut self, size: (u32, u32)) -> vello::peniko::Image {
        match &self.placeholder {
            Some((s, image)) if *s == size => image.clone(),
            _ => {
                let image = blur::placeholder(size);
                self.placeholder = Some((size, image.clone()));
                image
            }
        }
    }

    /// Shows `rgba` (opaque RGBA8, `size` pixels) in the window.
    fn present(&mut self, rgba: &[u8], size: (u32, u32)) -> Result<()> {
        self.resize(size.0, size.1)?;
        let mut buffer = self
            .surface
            .buffer_mut()
            .map_err(|e| anyhow!("softbuffer buffer: {e}"))?;
        for (dst, px) in buffer.iter_mut().zip(rgba.as_chunks::<4>().0) {
            *dst = u32::from(px[0]) << 16 | u32::from(px[1]) << 8 | u32::from(px[2]);
        }
        buffer
            .present()
            .map_err(|e| anyhow!("softbuffer present: {e}"))
    }
}

/// What draws the window's frames.
enum Backend {
    Wgpu(Box<Gpu>),
    Cpu(CpuWindow),
}

impl Backend {
    fn window(&self) -> &Arc<Window> {
        match self {
            Backend::Wgpu(g) => &g.window,
            Backend::Cpu(c) => &c.window,
        }
    }
}

struct Gui {
    /// The peeking tracks were drawn last frame (one more redraw hides them).
    tracks_peek_drawn: bool,
    notifications_drawn: bool,
    backend: Option<Backend>,
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

    /// Configures the swapchain: the last step of opening a window, so a failing attempt never
    /// leaves a configured surface behind.
    fn configure(&self) {
        self.surface.configure(&self.device, &self.config);
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

/// Runs `f` with validation errors on `device` captured, returning the first as an `Err`
/// (wgpu's default handler would panic instead).
fn validated<T>(device: &wgpu::Device, f: impl FnOnce() -> Result<T>) -> Result<T> {
    device.push_error_scope(wgpu::ErrorFilter::Validation);
    let result = f();
    match pollster::block_on(device.pop_error_scope()) {
        Some(e) => Err(anyhow!("wgpu validation error: {e}")),
        None => result,
    }
}

/// Reports wgpu errors after the renderer is chosen instead of panicking mid-session.
fn report_wgpu_errors(device: &wgpu::Device) {
    device.on_uncaptured_error(Box::new(|e| eprintln!("cecilia: wgpu error: {e}")));
}

/// The blur pipeline and vello renderer for `device`.
fn make_renderer(device: &wgpu::Device, software: bool) -> Result<(blur::Blur, Renderer)> {
    validated(device, || {
        let blur = blur::Blur::new(device);
        let renderer = Renderer::new(device, renderer_options(software))
            .map_err(|e| anyhow!("renderer: {e}"))?;
        Ok((blur, renderer))
    })
}

/// The adapter the GPU attempt uses (`WGPU_ADAPTER_NAME` / `WGPU_POWER_PREF` honoured) with a
/// device on it. A CPU-type adapter (WARP, llvmpipe, lavapipe) is refused before a device is
/// made, unless `WGPU_ADAPTER_NAME` names one: those belong to the software attempt.
fn gpu_device(
    instance: &wgpu::Instance,
    surface: Option<&wgpu::Surface<'_>>,
) -> Result<(wgpu::Adapter, wgpu::Device, wgpu::Queue)> {
    let adapter = pollster::block_on(wgpu::util::initialize_adapter_from_env_or_default(
        instance, surface,
    ))
    .ok_or_else(|| anyhow!("no GPU adapter"))?;
    let info = adapter.get_info();
    if backend::reject_for_gpu(&info, std::env::var_os("WGPU_ADAPTER_NAME").is_some()) {
        return Err(anyhow!(
            "only a software adapter ({}) was found; use ui.renderer software or cpu",
            info.name
        ));
    }
    let (device, queue) = request_device(&adapter)?;
    Ok((adapter, device, queue))
}

/// Opens the renderer for one `attempt`: the window's surface and device, vello's renderer and
/// the blur pipeline. The surface is configured last.
fn open_wgpu(
    attempt: backend::Attempt,
    window: Arc<Window>,
    size: winit::dpi::PhysicalSize<u32>,
) -> Result<(Surf, Renderer, blur::Blur)> {
    let software = attempt == backend::Attempt::Software;
    let instance = if software {
        RenderContext::new().instance
    } else {
        render_context().instance
    };
    let surface = instance
        .create_surface(window)
        .map_err(|e| anyhow!("surface: {e}"))?;
    let (adapter, device, queue) = if software {
        software_device(&instance, Some(&surface))?
    } else {
        gpu_device(&instance, Some(&surface))?
    };
    let (blur, renderer) = make_renderer(&device, software)?;
    let surf = Surf::new(surface, &adapter, device, queue, size)?;
    validated(&surf.device, || {
        surf.configure();
        Ok(())
    })?;
    report_wgpu_errors(&surf.device);
    Ok((surf, renderer, blur))
}

/// A wgpu device with the renderer and blur pipeline for `--screenshot`, for one attempt.
fn open_headless(attempt: backend::Attempt) -> Result<Headless> {
    let software = attempt == backend::Attempt::Software;
    let (device, queue) = if software {
        let (_, d, q) = software_device(&RenderContext::new().instance, None)?;
        (d, q)
    } else {
        let (_, d, q) = gpu_device(&render_context().instance, None)?;
        (d, q)
    };
    let (blur, renderer) = make_renderer(&device, software)?;
    report_wgpu_errors(&device);
    Ok((blur, renderer, device, queue))
}

type Headless = (blur::Blur, Renderer, wgpu::Device, wgpu::Queue);

/// The error for when every attempt in `tried` failed; `last` is the final attempt's error.
fn no_renderer(tried: &[backend::Attempt], last: anyhow::Error) -> anyhow::Error {
    let names: Vec<_> = tried
        .iter()
        .map(|a| match a {
            backend::Attempt::Gpu => "gpu",
            backend::Attempt::Software => "software",
            backend::Attempt::Cpu => "cpu",
        })
        .collect();
    let advice = if tried.contains(&backend::Attempt::Cpu) {
        "set ui.renderer or CECILIA_RENDERER to try another renderer, or use the terminal UI"
    } else {
        "set ui.renderer or CECILIA_RENDERER to cpu (needs no graphics driver) or auto, or use \
         the terminal UI"
    };
    last.context(format!(
        "no usable renderer (tried {}); {advice}",
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

/// Builds the frame into `scene` (device pixels) and returns its popups, also in device pixels:
/// the UI is laid out in logical units (`logical`, `popups_logical`) and scaled by `factor`.
/// `backdrop` is the placeholder popups fill with to show the blurred page (None without blur).
#[allow(clippy::too_many_arguments)]
fn build_frame(
    app: &mut App,
    views: &mut Views,
    playback: &mut Playback,
    follow_playback: bool,
    [logical, popups_logical, scene]: [&mut Canvas; 3],
    (width, height): (u32, u32),
    factor: f64,
    backdrop: Option<vello::peniko::Image>,
) -> Canvas {
    let play = if follow_playback {
        playback.location(app)
    } else {
        None
    };
    views.set_backdrop(backdrop.map(|image| (image, f64::from(blur::DOWNSCALE) / factor)));
    logical.reset();
    popups_logical.reset();
    draw::build(
        logical,
        popups_logical,
        app,
        views,
        Some(playback),
        play,
        width as f64 / factor,
        height as f64 / factor,
    );
    scene.reset();
    scene.append(logical, Some(Affine::scale(factor)));
    let mut popups = Canvas::new();
    popups.append(popups_logical, Some(Affine::scale(factor)));
    popups
}

/// Opens the GUI window and runs until the user quits.
pub fn run(app: App) -> Result<()> {
    let event_loop = EventLoop::new().context("cannot open a window (no display?)")?;
    let mut gui = Gui {
        tracks_peek_drawn: false,
        notifications_drawn: false,
        backend: None,
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
        let Some(backend) = self.backend.as_mut() else {
            return Ok(());
        };
        let window = backend.window().clone();
        let (width, height) = match backend {
            Backend::Wgpu(gpu) => (gpu.surface.config.width, gpu.surface.config.height),
            Backend::Cpu(_) => {
                let size = window.inner_size();
                (size.width, size.height)
            }
        };
        // Minimised: nothing to draw into.
        if width == 0 || height == 0 {
            return Ok(());
        }
        // Lay out in logical units so the UI keeps its size on HiDPI and fractionally scaled
        // monitors, then scale the frame to the surface's pixels.
        let factor = window.scale_factor().max(0.1);
        let blur_px = self.app.config.file.popups.blur;
        let backdrop = (blur_px > 0.0).then(|| match backend {
            Backend::Wgpu(gpu) => gpu.blur.backdrop((width, height)),
            Backend::Cpu(cpu) => cpu.placeholder((width, height)),
        });
        let popups = build_frame(
            &mut self.app,
            &mut self.views,
            &mut self.playback,
            true,
            [&mut self.logical, &mut self.popups, &mut self.scene],
            (width, height),
            factor,
            backdrop.clone(),
        );
        let gpu = match backend {
            Backend::Wgpu(gpu) => gpu,
            Backend::Cpu(cpu) => {
                let pixels = cpu::render(
                    &self.scene,
                    &popups,
                    (width, height),
                    draw::background(),
                    blur_px * factor,
                    backdrop.as_ref(),
                )?;
                return cpu.present(&pixels, (width, height));
            }
        };
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
        self.window().map_or(1.0, |w| w.scale_factor().max(0.1))
    }

    fn window(&self) -> Option<&Arc<Window>> {
        self.backend.as_ref().map(Backend::window)
    }

    fn request_redraw(&self) {
        if let Some(window) = self.window() {
            window.request_redraw();
        }
    }

    fn after_input(&mut self) {
        for effect in self.app.take_effects() {
            if let Some(effect) = cecilia_app::dialog::apply(&mut self.app, effect) {
                self.playback.apply(&mut self.app, effect);
            }
        }
        self.request_redraw();
    }
}

impl ApplicationHandler for Gui {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.backend.is_some() {
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
        let result = (|| -> Result<Backend> {
            let window = Arc::new(event_loop.create_window(attrs)?);
            let size = window.inner_size();
            let attempts = renderer_attempts(&self.app);
            let mut failure = None;
            let mut opened = None;
            for (n, &attempt) in attempts.iter().enumerate() {
                let result = match attempt {
                    backend::Attempt::Cpu => CpuWindow::new(window.clone()).map(Backend::Cpu),
                    _ => {
                        open_wgpu(attempt, window.clone(), size).map(|(surface, renderer, blur)| {
                            Backend::Wgpu(Box::new(Gpu {
                                window: window.clone(),
                                surface,
                                renderer,
                                blur,
                            }))
                        })
                    }
                };
                match result {
                    Ok(o) => {
                        opened = Some(o);
                        break;
                    }
                    Err(e) => failure = Some((n, e)),
                }
            }
            opened.ok_or_else(|| {
                let (n, e) = failure.expect("at least one renderer attempt");
                no_renderer(&attempts[..=n], e)
            })
        })();
        match result {
            Ok(backend) => {
                backend.window().request_redraw();
                self.backend = Some(backend);
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
                match self.backend.as_mut() {
                    Some(Backend::Wgpu(gpu)) => gpu.surface.resize(size.width, size.height),
                    Some(Backend::Cpu(cpu)) => {
                        if let Err(e) = cpu.resize(size.width, size.height) {
                            self.app.error(format!("{e:#}"));
                        }
                    }
                    None => {}
                }
                self.request_redraw();
            }
            WindowEvent::ScaleFactorChanged { .. } => {
                // Moved to a monitor with another scale; the new size follows as `Resized`.
                self.request_redraw();
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
                    if let Some(window) = self.window() {
                        window.set_cursor(match pointer {
                            draw::Pointer::Default => CursorIcon::Default,
                            draw::Pointer::ResizeColumns => CursorIcon::ColResize,
                            draw::Pointer::ResizeRows => CursorIcon::RowResize,
                        });
                    }
                }
                // Hover effects on the tabline (close buttons) and dashboard buttons need a
                // redraw.
                if was_over
                    || self.views.over_tabline(self.mouse.1)
                    || self.app.dashboard_visible()
                    || self.app.viewer().is_some()
                {
                    self.request_redraw();
                }
            }
            WindowEvent::CursorLeft { .. } => {
                self.views.hover = (-1.0, -1.0);
                self.request_redraw();
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
                    self.request_redraw();
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
            self.request_redraw();
            event_loop.set_control_flow(ControlFlow::WaitUntil(
                Instant::now() + Duration::from_millis(16),
            ));
        } else if self.app.has_tool(cecilia_app::PaneKind::Tuner) {
            // The tuner's needle moves with what it hears.
            self.request_redraw();
            event_loop.set_control_flow(ControlFlow::WaitUntil(
                Instant::now() + Duration::from_millis(33),
            ));
        } else if self.app.tracks_peeking() || self.tracks_peek_drawn {
            // The tracks peeking after a change of track: redraw until they hide again.
            self.tracks_peek_drawn = self.app.tracks_peeking();
            self.request_redraw();
            event_loop.set_control_flow(ControlFlow::WaitUntil(
                Instant::now() + Duration::from_millis(100),
            ));
        } else if self.app.notifications_active() || self.notifications_drawn {
            // Notification cards: redraw (their bars drain) until the last one is gone.
            self.notifications_drawn = self.app.notifications_active();
            self.request_redraw();
            event_loop.set_control_flow(ControlFlow::WaitUntil(
                Instant::now() + Duration::from_millis(33),
            ));
        } else if self.views.tooltip_pending() {
            // A toolbar tooltip waits to show.
            self.request_redraw();
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
    app: App,
    width: u32,
    height: u32,
    factor: f64,
    hover: Option<(f64, f64)>,
) -> Result<Vec<u8>> {
    let factor = factor.max(0.1);
    let attempts = renderer_attempts(&app);
    let mut failure = None;
    // The first attempt that opens: a wgpu device, or None for the CPU renderer.
    let mut opened = None;
    for (n, &attempt) in attempts.iter().enumerate() {
        let result = match attempt {
            backend::Attempt::Cpu => Ok(None),
            _ => open_headless(attempt).map(Some),
        };
        match result {
            Ok(o) => {
                opened = Some(o);
                break;
            }
            Err(e) => failure = Some((n, e)),
        }
    }
    let Some(gpu) = opened else {
        let (n, e) = failure.expect("at least one renderer attempt");
        return Err(no_renderer(&attempts[..=n], e));
    };
    screenshot_with(app, width, height, factor, hover, gpu)
}

/// [`screenshot`] on a wgpu device, or on the CPU renderer without one.
fn screenshot_with(
    mut app: App,
    width: u32,
    height: u32,
    factor: f64,
    hover: Option<(f64, f64)>,
    mut gpu: Option<Headless>,
) -> Result<Vec<u8>> {
    let max_dimension = gpu
        .as_ref()
        .map_or(16384, |(.., d, _)| d.limits().max_texture_dimension_2d);
    let (width, height) = clamp_image_size(width, height, max_dimension)
        .ok_or_else(|| anyhow!("screenshot size {width}x{height} is too large"))?;
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
    let blur_px = app.config.file.popups.blur;
    let placeholder = (blur_px > 0.0).then(|| match gpu.as_mut() {
        Some((blur, ..)) => blur.backdrop((width, height)),
        None => blur::placeholder((width, height)),
    });
    let mut canvases = [Canvas::new(), Canvas::new(), Canvas::new()];
    let [logical, popups, scene] = &mut canvases;
    let mut frame = |app: &mut App, views: &mut Views, playback: &mut Playback| {
        build_frame(
            app,
            views,
            playback,
            false,
            [&mut *logical, &mut *popups, &mut *scene],
            (width, height),
            factor,
            placeholder.clone(),
        )
    };
    // Hovering a toolbar button: draw once and wait, so its tooltip shows.
    if hover.is_some() {
        frame(&mut app, &mut views, &mut playback);
        if views.tooltip_pending() {
            std::thread::sleep(std::time::Duration::from_millis(550));
        }
    }
    let popups_dev = frame(&mut app, &mut views, &mut playback);
    let Some((mut blur, mut renderer, device, queue)) = gpu else {
        let pixels = cpu::render(
            scene,
            &popups_dev,
            (width, height),
            draw::background(),
            blur_px * factor,
            placeholder.as_ref(),
        )?;
        let size = tiny_skia::IntSize::from_wh(width, height).ok_or_else(|| anyhow!("bad size"))?;
        let pixmap =
            tiny_skia::Pixmap::from_vec(pixels, size).ok_or_else(|| anyhow!("bad pixmap"))?;
        return pixmap.encode_png().map_err(|e| anyhow!("png: {e}"));
    };
    let (device, queue) = (&device, &queue);
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
        scene,
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
