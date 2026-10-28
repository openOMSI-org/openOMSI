//! NVIDIA Neural Rendering availability, independent of spatial upscaling.
//!
//! Streamline 2.14.16 declares kFeatureDLSS_NR, but its public x64 archive contains
//! neither the NR plugin nor its configuration header. No inference API is guessed here.

#[cfg(feature = "neural-external")]
pub mod external;

/// Engine diagnostics, not NVIDIA quality modes. Saved settings use these stable names.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum TemporalDebug {
    #[default]
    Off,
    Motion,
    Depth,
    Validity,
}

impl TemporalDebug {
    pub fn from_setting(value: &str) -> Self {
        match value {
            "motion" => Self::Motion,
            "depth" => Self::Depth,
            "validity" => Self::Validity,
            _ => Self::Off,
        }
    }

    pub fn as_setting(self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::Motion => "motion",
            Self::Depth => "depth",
            Self::Validity => "validity",
        }
    }
}

/// Reasons established by this build. A vendor ID alone cannot certify a GPU or driver
/// for DLSS 5; that requires a successful query of the official runtime.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NeuralRenderingUnavailable {
    UnsupportedPlatform,
    UnsupportedBackend,
    UnsupportedVendor,
    MissingNeuralRenderingSdk,
}

impl std::fmt::Display for NeuralRenderingUnavailable {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::UnsupportedPlatform => "Unavailable: this integration targets Windows x64",
            Self::UnsupportedBackend => "Unavailable: this integration targets DirectX 12",
            Self::UnsupportedVendor => "Unavailable: NVIDIA GPU required",
            Self::MissingNeuralRenderingSdk => {
                "Unavailable: authorized NVIDIA NR runtime and feature contract required"
            }
        })
    }
}

/// Only reports prerequisites; never reports Ready based on filenames or a GPU name.
pub fn neural_rendering_unavailable(info: &wgpu::AdapterInfo) -> NeuralRenderingUnavailable {
    availability_for(
        cfg!(all(target_os = "windows", target_arch = "x86_64")),
        info.backend,
        info.vendor,
    )
}

fn availability_for(
    windows_x64: bool,
    backend: wgpu::Backend,
    vendor: u32,
) -> NeuralRenderingUnavailable {
    if !windows_x64 {
        NeuralRenderingUnavailable::UnsupportedPlatform
    } else if backend != wgpu::Backend::Dx12 {
        NeuralRenderingUnavailable::UnsupportedBackend
    } else if vendor != 0x10de {
        NeuralRenderingUnavailable::UnsupportedVendor
    } else {
        NeuralRenderingUnavailable::MissingNeuralRenderingSdk
    }
}

impl crate::Renderer {
    /// Current integration evidence, without inferring inference from installed files.
    pub fn neural_rendering_status(&self) -> String {
        #[cfg(feature = "neural-external")]
        {
            let d = self.external_neural_diagnostics();
            if d.state != external::State::Disabled {
                return format!(
                    "External prototype: {:?}; evaluations {}, compositions {}{}",
                    d.state,
                    d.evaluations,
                    d.compositions,
                    d.last_error
                        .as_ref()
                        .map(|s| format!("; {s}"))
                        .unwrap_or_default()
                );
            }
        }
        self.neural_rendering_unavailable().to_string()
    }

    pub fn neural_rendering_unavailable(&self) -> NeuralRenderingUnavailable {
        neural_rendering_unavailable(&self.device.adapter_info())
    }

    /// Explicit invalidation for map loads, camera cuts, teleports and capture sequences.
    /// Mirrors do not advance the main view's history.
    pub fn reset_temporal_history(&mut self, scene: &mut crate::Scene) {
        #[cfg(feature = "temporal-diagnostics")]
        scene.temporal.reset();
        #[cfg(not(feature = "temporal-diagnostics"))]
        let _ = scene;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prerequisites_never_claim_runtime_support() {
        use NeuralRenderingUnavailable::*;
        assert_eq!(
            availability_for(false, wgpu::Backend::Vulkan, 0x10de),
            UnsupportedPlatform
        );
        assert_eq!(
            availability_for(true, wgpu::Backend::Vulkan, 0x10de),
            UnsupportedBackend
        );
        assert_eq!(
            availability_for(true, wgpu::Backend::Dx12, 0x1002),
            UnsupportedVendor
        );
        assert_eq!(
            availability_for(true, wgpu::Backend::Dx12, 0x10de),
            MissingNeuralRenderingSdk
        );
    }

    #[test]
    fn diagnostic_names_round_trip_and_unknown_values_disable() {
        for mode in [
            TemporalDebug::Off,
            TemporalDebug::Motion,
            TemporalDebug::Depth,
            TemporalDebug::Validity,
        ] {
            assert_eq!(TemporalDebug::from_setting(mode.as_setting()), mode);
        }
        assert_eq!(TemporalDebug::from_setting("dlss5"), TemporalDebug::Off);
    }
}

/// Borrowed resources from the most recently encoded desktop diagnostic frame.
/// Validity is engine metadata, not an NVIDIA masking API. Unsupported coverage is
/// not certified as valid. No readback or native ownership transfer is performed.
#[cfg(feature = "temporal-diagnostics")]
pub struct TemporalBuffers<'a> {
    pub motion: &'a wgpu::Texture,
    pub validity: &'a wgpu::Texture,
    pub depth: &'a wgpu::Texture,
    pub size: (u32, u32),
}
