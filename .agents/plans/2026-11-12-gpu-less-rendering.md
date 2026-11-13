# GPU-less rendering Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** `cecilia --gui` (and `--screenshot`) runs in a VM with no GPU, while real GPUs keep being used by default.

**Architecture:** One `ui.renderer` setting (`auto` | `gpu` | `software` | `cpu`, overridable by the `CECILIA_RENDERER` env var, and by `-o ui.renderer=…` on the CLI) picks the backend. Phase 1 keeps vello+wgpu and adds software-adapter selection plus an `auto` fallback chain. Phase 2 adds a wgpu-free CPU backend (`vello_cpu` + `softbuffer`) behind a small `Canvas` trait that `draw.rs` paints through, because the target VM (Windows 11 QEMU/KVM, VirtIO display-only, see `TODO.md`) has no usable Vulkan, no lavapipe, and WARP crashes natively.

**Tech Stack:** vello 0.5 / wgpu (existing), `vello_cpu` + `softbuffer` (phase 2, new deps), winit 0.30.

**Spec:** none; requirements are the user request: run in a GPU-less VM, keep GPU acceleration, selectable by option/env var/fallback. Existing related code: `renderer_options()` and `render_context()` in `crates/cecilia-gui/src/lib.rs`, `CECILIA_CPU` (README.md:156, CHANGELOG.md:26), TODO.md:24.

## Global Constraints

- Default behaviour on a machine with a working GPU is unchanged (same adapter, same pixels).
- New setting follows the options pattern: row in `crates/cecilia-app/src/options.rs` (model on `popups.blur`, line ~403) **and** a row in `docs/OPTIONS.md` **and** `docs/config.example.toml`; tests fail if these drift.
- `CECILIA_CPU=1` keeps working (it only toggles vello's `use_cpu`).
- Conventional Commits, one commit per task; `CHANGELOG.md` entries under `[Unreleased]`; run `just ci` before pushing; `cargo clippy --workspace --all-targets` clean.
- A native crash (the Windows WARP access violation) cannot be caught: it must be avoided by not selecting that adapter, never by `catch_unwind`.
- The TUI is untouched (it already works on the VM).

## Review Focus

- No display / no adapter at all: `--gui` must fail with a message naming `ui.renderer`/`CECILIA_RENDERER`, not "Couldn't create wgpu surface". Test: `auto` attempt list ends in `cpu` once phase 2 exists; before that, the error text is asserted.
- `CECILIA_RENDERER` set to garbage (`foo`): warn once and use the configured value, do not panic.
- `ui.renderer=gpu` with no GPU: fail loudly, never silently fall back.
- Window resize and HiDPI scale on the CPU backend: buffer size must track `inner_size()`, zero-size (minimised) must not panic.
- `popups.blur = 0` and `> 0` both work on the CPU backend (the backdrop image path differs).

## File Structure

- `crates/cecilia-app/src/config.rs`, `options.rs`: `RendererMode` enum + `ui.renderer` option (owner of the setting; frontend independent).
- `crates/cecilia-gui/src/backend.rs` (new): resolving the mode (env > config) and the ordered attempt list. Pure functions, unit tested.
- `crates/cecilia-gui/src/lib.rs`: `Gpu` becomes a backend enum; window/surface creation walks the attempt list; `screenshot()` likewise.
- Phase 2: `crates/cecilia-gui/src/canvas.rs` (trait + impls), `cpu.rs` (CPU rendering, blur, softbuffer presenter); `draw.rs`, `blur.rs` adapt to the trait.

---

# Phase 1: software adapter + selection

### Task 1: `ui.renderer` setting and mode resolution

**Files:**
- Modify: `crates/cecilia-app/src/config.rs`, `crates/cecilia-app/src/options.rs`, `docs/OPTIONS.md`, `docs/config.example.toml`
- Create: `crates/cecilia-gui/src/backend.rs`; add `mod backend;` in `lib.rs`
- Test: unit tests in `backend.rs`; the existing options/docs drift tests

**Interfaces:**
- Produces (`cecilia-app`): `pub enum RendererMode { Auto, Gpu, Software, Cpu }` (serde lowercase, `Default = Auto`), field `ui.renderer: RendererMode` in the `[ui]` config struct, option name `ui.renderer`.
- Produces (`backend.rs`): `pub fn resolve_mode(env: Option<&str>, configured: RendererMode) -> RendererMode` and `pub enum Attempt { Gpu, Software, Cpu }` with `pub fn attempts(mode: RendererMode) -> Vec<Attempt>`.

- [ ] **Step 1: Write failing tests** in `backend.rs`:
  `resolve_mode(Some("software"), Auto) == Software`; `resolve_mode(Some("foo"), Gpu) == Gpu`; `resolve_mode(Some(""), Cpu) == Cpu`; `resolve_mode(None, Auto) == Auto`.
  `attempts(Auto) == [Gpu, Software]` (phase 2 appends `Cpu`); `attempts(Gpu) == [Gpu]`; `attempts(Software) == [Software]`.
- [ ] **Step 2:** `cargo test -p cecilia-gui backend::` fails (module missing).
- [ ] **Step 3:** Add the enum, config field, option row (values `auto|gpu|software|cpu`, GUI-only note; `cpu` documented as "phase 2, rejected until implemented" or omitted from the accepted values until Task 5), docs rows, and `backend.rs`. Env var name is `CECILIA_RENDERER`; an unknown value prints one `eprintln!` warning and is ignored.
- [ ] **Step 4:** `cargo test --workspace` passes (includes the OPTIONS.md / config drift tests).
- [ ] **Step 5:** Commit `feat(gui): add ui.renderer setting and CECILIA_RENDERER override`.

### Task 2: Software adapter selection and `auto` fallback

**Files:**
- Modify: `crates/cecilia-gui/src/lib.rs` (`render_context`, `renderer_options`, `resumed` window setup at ~line 250, `screenshot` at ~513)

**Interfaces:**
- Consumes: `Attempt`, `attempts`, `resolve_mode` from Task 1.
- Produces: `fn open_wgpu(cx: &mut RenderContext, attempt: Attempt, window: Arc<Window>, size) -> Result<(RenderSurface<'static>, Renderer, Blur)>` used by the window path; `Attempt::Software` = adapter with `wgpu::DeviceType::Cpu` (lavapipe/llvmpipe) if one is enumerated, else `force_fallback_adapter: true`; plus `use_cpu: true` in `RendererOptions`.

- [ ] **Step 1: Write failing test** `software_attempt_prefers_cpu_adapter` over a pure helper `fn pick_software<'a>(infos: &[AdapterInfo]) -> Option<usize>` (index of first `DeviceType::Cpu`; `None` if none). Run, see it fail.
- [ ] **Step 2:** Implement `pick_software`; build the software device by hand from `cx.instance` and push a `DeviceHandle` into `cx.devices` (check `vello::util` source for how `RenderContext::device` / `create_surface` choose among existing devices and make the surface use the pushed one; confirm by reading the vendored vello 0.5 source in `~/.cargo/registry/src/*/vello-0.5.1/src/util.rs`).
- [ ] **Step 3:** Walk `attempts(resolve_mode(env, app.config.file.ui.renderer))` when creating the window surface; first success wins, keep the last error. On all-fail, error text: `no usable renderer (tried gpu, software); set ui.renderer or CECILIA_RENDERER, install a software Vulkan driver (lavapipe), or use the terminal UI`. Same walk in `screenshot()` (it currently does `cx.device(None)`).
- [ ] **Step 4:** On this Linux box verify: `CECILIA_RENDERER=software target/release/cecilia -W scratch --screenshot /tmp/…/sw.png` (needs `mesa-vulkan-swrast`/lavapipe; if absent, `vulkaninfo --summary` shows it and the run must produce the clear error). `CECILIA_RENDERER=gpu` output must be pixel-identical to before the change (screenshot diff against a pre-change render).
- [ ] **Step 5:** Commit `feat(gui): software adapter and auto fallback for GPU-less machines`.

### Task 3: Phase 1 docs

**Files:** `README.md` (line ~136 and ~156), `CHANGELOG.md`, `TODO.md:24`, `docs/COMMANDS.md` only if `:set` listing there is generated.

- [ ] **Step 1:** README: describe `ui.renderer` / `CECILIA_RENDERER`, the fallback chain, lavapipe on Linux VMs; fold `CECILIA_CPU` into that paragraph.
- [ ] **Step 2:** CHANGELOG `[Unreleased]` entry; update the TODO item to point at phase 2.
- [ ] **Step 3:** `just ci` passes. Commit `docs: renderer selection for GPU-less machines`.

---

# Phase 2: wgpu-free CPU renderer

**Decision gate after Task 4.** The Windows VM can only be fixed by this phase, so if Task 4 shows `vello_cpu` cannot meet the needs below, stop and re-plan (fallback candidate: `tiny-skia`, already a dependency).

### Task 4: Spike `vello_cpu` and the `Canvas` seam

**Files:** scratch crate or `crates/cecilia-gui/examples/cpu_spike.rs` (delete afterwards); findings go in the Status section below.

- [ ] **Step 1:** Check `vello_cpu` (crates.io ≈0.4) depends on the same `kurbo`/`peniko` major versions as vello 0.5 (`cargo tree -d`); if not, note the conversion cost.
- [ ] **Step 2:** Prove it can do everything `draw.rs` asks of `vello::Scene`. Usage counts today: `fill` 98, `stroke` 43, `push_layer(Mix::Clip, …)`/`pop_layer` 6/8, `append` 2 (scene + transform), `draw_image` 1 (the lily image) plus the blurred backdrop image, `draw_blurred_rounded_rect` 1 (popup shadows). Text is already outlines (no glyph runs).
- [ ] **Step 3:** Record in Status: pass/fail per item and the plan for any gap (the shadow and the backdrop blur are the likely gaps; both may be approximated on the CPU).

### Task 5: `Canvas` trait and CPU backend

**Files:**
- Create: `crates/cecilia-gui/src/canvas.rs`, `crates/cecilia-gui/src/cpu.rs`
- Modify: `draw.rs` (every `&mut Scene` parameter becomes `&mut dyn Canvas` / generic), `blur.rs`, `lib.rs`, `crates/cecilia-gui/Cargo.toml`

**Interfaces:**
- Produces: `trait Canvas { fn fill(&mut self, …); fn stroke(&mut self, …); fn push_clip(&mut self, shape: &impl Shape); fn pop_layer(&mut self); fn draw_image(&mut self, …); fn blurred_rounded_rect(&mut self, …); fn append(&mut self, other: &Self, transform: Option<Affine>); }` with exact parameter types copied from the corresponding `vello::Scene` methods so the vello impl is a one-line forward each.
- Produces: `cpu::render(canvas: &CpuCanvas, width: u32, height: u32) -> Vec<u8>` (RGBA8) and `cpu::Presenter` (softbuffer surface over the winit window: `resize`, `present(&[u8])`).

- [ ] **Step 1 (refactor, behaviour-preserving):** introduce `Canvas` with only the vello impl, switch `draw.rs`/`blur.rs` to it. Verify: screenshots of 3 corpus scores (`test-corpus/Tabs`, a gabc, one with a picker open via `--keys`) are byte-identical before and after.
- [ ] **Step 2:** Test `cpu_renders_a_filled_rect`: a `CpuCanvas` fill of a known color gives the expected RGBA pixel at a known coordinate and the background elsewhere. Fail, then implement `CpuCanvas`.
- [ ] **Step 3:** CPU backdrop blur in `cpu.rs`: box/gaussian blur on the downscaled frame (`blur::DOWNSCALE`), same contract as the GPU `Blur::backdrop`. Test: a 2-color checker blurred has intermediate values and the same dimensions.
- [ ] **Step 4:** `Gpu` in `lib.rs` becomes `enum Backend { Wgpu {…}, Cpu { window, presenter } }`; `redraw`, `Resized` and `screenshot()` dispatch on it. Add `Attempt::Cpu`, accept `cpu` in the option, and set `attempts(Auto) == [Gpu, Software, Cpu]` (update Task 1's test). Skip a `DeviceType::Cpu` adapter on Windows in `Auto` (WARP crash), going straight to `Cpu`.
- [ ] **Step 5:** Verify headless: `CECILIA_RENDERER=cpu target/release/cecilia -W scratch --screenshot out.png song.gp5` works with `WGPU_BACKEND=none`-style environment (no adapter); compare visually against the GPU screenshot (view both PNGs). Verify window: `CECILIA_RENDERER=cpu cecilia --gui` resizes, minimises (zero size) and repaints on edit without panic.
- [ ] **Step 6:** Commit `feat(gui): wgpu-free CPU renderer`.

### Task 6: CI and docs

**Files:** `.github/workflows/ci.yml` (`render-smoke`), `README.md`, `CHANGELOG.md`, `TODO.md`

- [ ] **Step 1:** Add a `render-smoke` step running `CECILIA_RENDERER=cpu ./target/release/cecilia -W ci --screenshot render-cpu.png` and upload it; this finally gives a Windows GPU-less check (the runner segfaults in WARP today).
- [ ] **Step 2:** README, CHANGELOG, close the TODO.md:24 item (retest on the real Windows VM first: `ui.renderer cpu`).
- [ ] **Step 3:** `just ci` passes. Commit `docs: CPU renderer and CI smoke test`.

## Status

- [ ] Task 1  - [ ] Task 2  - [ ] Task 3  - [ ] Task 4 (gate)  - [ ] Task 5  - [ ] Task 6

Open questions: whether `vello_cpu` fits (Task 4); whether the Windows VM VirtIO display driver supports `softbuffer` (GDI, expected yes). Not planned: GPU selection by name/index, runtime renderer switching without restart.
