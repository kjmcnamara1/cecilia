//! Which renderer the GUI tries, and in what order.

use cecilia_app::config::RendererMode;
use vello::wgpu::{AdapterInfo, DeviceType};

/// One way of opening the GUI's renderer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Attempt {
    Gpu,
    Software,
}

/// The renderer mode in force: the `CECILIA_RENDERER` value `env` when it names a mode, else
/// the configured one. An unknown non-empty value warns once and is ignored.
pub fn resolve_mode(env: Option<&str>, configured: RendererMode) -> RendererMode {
    match env {
        None | Some("") => configured,
        Some("auto") => RendererMode::Auto,
        Some("gpu") => RendererMode::Gpu,
        Some("software") => RendererMode::Software,
        Some(other) => {
            eprintln!(
                "cecilia: ignoring CECILIA_RENDERER={other:?} (expected auto, gpu or software)"
            );
            configured
        }
    }
}

/// The renderers to try for `mode`, in order.
pub fn attempts(mode: RendererMode) -> Vec<Attempt> {
    match mode {
        RendererMode::Auto => vec![Attempt::Gpu, Attempt::Software],
        RendererMode::Gpu => vec![Attempt::Gpu],
        RendererMode::Software => vec![Attempt::Software],
    }
}

/// Index of the first software (CPU) adapter among `infos`, e.g. lavapipe or llvmpipe.
pub fn pick_software(infos: &[AdapterInfo]) -> Option<usize> {
    infos.iter().position(|i| i.device_type == DeviceType::Cpu)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn info(name: &str, device_type: DeviceType) -> AdapterInfo {
        AdapterInfo {
            name: name.into(),
            vendor: 0,
            device: 0,
            device_type,
            driver: String::new(),
            driver_info: String::new(),
            backend: vello::wgpu::Backend::Vulkan,
        }
    }

    #[test]
    fn software_attempt_prefers_cpu_adapter() {
        let infos = [
            info("NVIDIA", DeviceType::DiscreteGpu),
            info("llvmpipe", DeviceType::Cpu),
            info("other", DeviceType::Cpu),
        ];
        assert_eq!(pick_software(&infos), Some(1));
        assert_eq!(pick_software(&infos[..1]), None);
        assert_eq!(pick_software(&[]), None);
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
        assert_eq!(resolve_mode(None, RendererMode::Auto), RendererMode::Auto);
    }

    #[test]
    fn attempts_per_mode() {
        assert_eq!(
            attempts(RendererMode::Auto),
            vec![Attempt::Gpu, Attempt::Software]
        );
        assert_eq!(attempts(RendererMode::Gpu), vec![Attempt::Gpu]);
        assert_eq!(attempts(RendererMode::Software), vec![Attempt::Software]);
    }
}
