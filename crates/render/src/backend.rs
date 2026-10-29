use serde::{Deserialize, Serialize};

/// Selects how projected geometry is rasterized.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RenderBackend {
    /// Deterministic CPU tile rasterizer. This is the default and the only
    /// backend on builds without the `gpu` feature.
    #[default]
    Cpu,
    /// Offscreen wgpu rasterizer. Fails with `BACKEND_UNAVAILABLE` when the
    /// build or host cannot provide an adapter.
    Gpu,
    /// Use the GPU for large meshes and the CPU otherwise, falling back to the
    /// CPU when GPU initialization fails.
    Auto,
}

impl RenderBackend {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Cpu => "cpu",
            Self::Gpu => "gpu",
            Self::Auto => "auto",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "cpu" => Some(Self::Cpu),
            "gpu" => Some(Self::Gpu),
            "auto" => Some(Self::Auto),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::RenderBackend;

    #[test]
    fn parses_stable_names() {
        for backend in [RenderBackend::Cpu, RenderBackend::Gpu, RenderBackend::Auto] {
            assert_eq!(RenderBackend::parse(backend.as_str()), Some(backend));
        }
        assert_eq!(RenderBackend::parse(""), None);
        assert_eq!(RenderBackend::parse("GPU"), None);
    }

    #[test]
    fn serializes_as_lowercase_names() {
        assert_eq!(
            serde_json::to_string(&RenderBackend::Cpu).unwrap(),
            "\"cpu\""
        );
        assert_eq!(
            serde_json::to_string(&RenderBackend::Gpu).unwrap(),
            "\"gpu\""
        );
        assert_eq!(
            serde_json::to_string(&RenderBackend::Auto).unwrap(),
            "\"auto\""
        );
    }
}
