//! Which renderer the GUI tries, and in what order.

use cecilia_app::config::RendererMode;
use vello::wgpu::{AdapterInfo, DeviceType};

/// One way of opening the GUI's renderer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Attempt {
    Gpu,
    Software,
    /// No graphics API: `vello_cpu` and, in a window, softbuffer.
    Cpu,
}

/// The renderer mode in force: the `CECILIA_RENDERER` value `env` when it names a mode, else
/// the configured one. An unknown non-empty value warns once and is ignored.
pub fn resolve_mode(env: Option<&str>, configured: RendererMode) -> RendererMode {
    match env {
        None | Some("") => configured,
        Some("auto") => RendererMode::Auto,
        Some("gpu") => RendererMode::Gpu,
        Some("software") => RendererMode::Software,
        Some("cpu") => RendererMode::Cpu,
        Some(other) => {
            eprintln!(
                "cecilia: ignoring CECILIA_RENDERER={other:?} (expected auto, gpu, software or cpu)"
            );
            configured
        }
    }
}

/// The renderers to try for `mode`, in order.
pub fn attempts(mode: RendererMode) -> Vec<Attempt> {
    match mode {
        RendererMode::Auto => vec![Attempt::Gpu, Attempt::Software, Attempt::Cpu],
        RendererMode::Gpu => vec![Attempt::Gpu],
        RendererMode::Software => vec![Attempt::Software],
        RendererMode::Cpu => vec![Attempt::Cpu],
    }
}

/// Index of the first software (CPU) adapter among `infos`, e.g. lavapipe or llvmpipe. With
/// `skip_dx12` (Windows) DX12 adapters are ignored: that is WARP, whose crashes cannot be caught.
pub fn pick_software(infos: &[AdapterInfo], skip_dx12: bool) -> Option<usize> {
    infos.iter().position(|i| {
        i.device_type == DeviceType::Cpu && !(skip_dx12 && i.backend == vello::wgpu::Backend::Dx12)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn info_on(name: &str, device_type: DeviceType, backend: vello::wgpu::Backend) -> AdapterInfo {
        AdapterInfo {
            name: name.into(),
            vendor: 0,
            device: 0,
            device_type,
            driver: String::new(),
            driver_info: String::new(),
            backend,
        }
    }

    fn info(name: &str, device_type: DeviceType) -> AdapterInfo {
        info_on(name, device_type, vello::wgpu::Backend::Vulkan)
    }

    #[test]
    fn software_attempt_prefers_cpu_adapter() {
        let infos = [
            info("NVIDIA", DeviceType::DiscreteGpu),
            info("llvmpipe", DeviceType::Cpu),
            info("other", DeviceType::Cpu),
        ];
        assert_eq!(pick_software(&infos, false), Some(1));
        assert_eq!(pick_software(&infos[..1], false), None);
        assert_eq!(pick_software(&[], false), None);
    }

    #[test]
    fn env_overrides_configured() {
        assert_eq!(
            resolve_mode(Some("software"), RendererMode::Auto),
            RendererMode::Software
        );
        assert_eq!(
            resolve_mode(Some("foo"), RendererMode::Gpu),
            RendererMode::Gpu
        );
        assert_eq!(
            resolve_mode(Some(""), RendererMode::Software),
            RendererMode::Software
        );
        assert_eq!(
            resolve_mode(Some("cpu"), RendererMode::Auto),
            RendererMode::Cpu
        );
        assert_eq!(resolve_mode(None, RendererMode::Auto), RendererMode::Auto);
    }

    #[test]
    fn attempts_per_mode() {
        assert_eq!(
            attempts(RendererMode::Auto),
            vec![Attempt::Gpu, Attempt::Software, Attempt::Cpu]
        );
        assert_eq!(attempts(RendererMode::Cpu), vec![Attempt::Cpu]);
        assert_eq!(attempts(RendererMode::Gpu), vec![Attempt::Gpu]);
        assert_eq!(attempts(RendererMode::Software), vec![Attempt::Software]);
    }

    #[test]
    fn software_attempt_skips_dx12_warp_when_asked() {
        use vello::wgpu::Backend::{Dx12, Vulkan};
        let infos = [
            info_on("WARP", DeviceType::Cpu, Dx12),
            info_on("lavapipe", DeviceType::Cpu, Vulkan),
        ];
        assert_eq!(pick_software(&infos, true), Some(1));
        assert_eq!(pick_software(&infos, false), Some(0));
        assert_eq!(pick_software(&infos[..1], true), None);
    }
}
