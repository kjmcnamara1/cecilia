//! Which renderer the GUI tries, and in what order.

use cecilia_app::config::RendererMode;

/// One way of opening the GUI's renderer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)] // Task 2 wires these into the GUI start-up.
pub enum Attempt {
    Gpu,
    Software,
}

/// The renderer mode in force: the `CECILIA_RENDERER` value `env` when it names a mode, else
/// the configured one. An unknown non-empty value warns once and is ignored.
#[allow(dead_code)] // Task 2 wires it.
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
#[allow(dead_code)] // Task 2 wires it.
pub fn attempts(mode: RendererMode) -> Vec<Attempt> {
    match mode {
        RendererMode::Auto => vec![Attempt::Gpu, Attempt::Software],
        RendererMode::Gpu => vec![Attempt::Gpu],
        RendererMode::Software => vec![Attempt::Software],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
