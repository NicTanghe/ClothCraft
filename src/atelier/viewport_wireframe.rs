//! Cloth-only wireframe for the native USD viewport.
//!
//! cssimpler strokes polygon-cage edges per USD mesh, so the runner enables the
//! stroke on the garment panel meshes and leaves every other display mesh
//! (collider, avatars) shaded only. This replaces the former `/ClothWire`
//! ribbon mesh, which had to be re-authored into the stage on every step.

use cssimpler::renderer::gpu::usd::{UsdRenderer, Wireframe, WireframeMode, WireframeStyle};
use cssimpler::renderer::{
    self, EngineEvent,
    gpu::{GpuContext, GpuViewportFrame, GpuViewportInfo, GpuViewportRenderer},
};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

/// Garment panel meshes are authored under this scope, one mesh per panel.
pub const GARMENT_SCOPE: &str = "/World/Garment/";

/// Solid stroke matching the former dark ribbon overlay.
const CLOTH_STROKE: WireframeStyle = WireframeStyle {
    core: [0.09, 0.11, 0.15],
    outline: [0.09, 0.11, 0.15],
    width_px: 2.0,
};

/// True for the garment panel meshes the cloth wireframe covers.
pub fn is_garment_model(path: &str) -> bool {
    path.starts_with(GARMENT_SCOPE)
}

/// Enable or clear the stroke on every garment panel mesh of `renderer`.
///
/// Colliders, avatars and other display meshes are never touched, so the
/// wireframe stays cloth-only.
pub fn apply(renderer: &mut UsdRenderer, enabled: bool) {
    let paths: Vec<String> = renderer
        .models()
        .iter()
        .filter(|path| is_garment_model(path))
        .cloned()
        .collect();
    for path in &paths {
        if enabled {
            renderer.set_model_wireframe(
                path,
                Wireframe {
                    mode: WireframeMode::Edges,
                    style: CLOTH_STROKE,
                },
            );
        } else {
            renderer.clear_model_wireframe(path);
        }
    }
}

/// Applies the shared toggle to a renderer when it changes.
///
/// The model count is part of the key, so a scene that gains or loses panels is
/// stroked again while unchanged frames do no work.
pub struct WireframeSync {
    enabled: Arc<AtomicBool>,
    applied: Option<(bool, usize)>,
}

impl WireframeSync {
    pub fn new(enabled: Arc<AtomicBool>) -> Self {
        Self {
            enabled,
            applied: None,
        }
    }

    pub fn sync(&mut self, renderer: &mut UsdRenderer) {
        let enabled = self.enabled.load(Ordering::Relaxed);
        let key = (enabled, renderer.models().len());
        if self.applied == Some(key) {
            return;
        }
        apply(renderer, enabled);
        self.applied = Some(key);
    }
}

/// The runner's GPU viewport: the USD renderer plus the shared wireframe toggle.
pub struct RunnerViewport {
    renderer: UsdRenderer,
    sync: WireframeSync,
}

impl RunnerViewport {
    pub fn new(renderer: UsdRenderer, wireframe: Arc<AtomicBool>) -> Self {
        Self {
            renderer,
            sync: WireframeSync::new(wireframe),
        }
    }
}

impl GpuViewportRenderer for RunnerViewport {
    fn resize(&mut self, gpu: &GpuContext, info: GpuViewportInfo) -> renderer::Result<()> {
        self.renderer.resize(gpu, info)
    }
    fn event(&mut self, event: &EngineEvent) -> bool {
        self.renderer.event(event)
    }
    fn render(&mut self, gpu: &GpuContext, frame: GpuViewportFrame<'_>) -> renderer::Result<()> {
        self.sync.sync(&mut self.renderer);
        self.renderer.render(gpu, frame)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_garment_panel_models_are_eligible() {
        assert!(is_garment_model("/World/Garment/panel_front"));
        assert!(is_garment_model("/World/Garment/panel_back"));
        for other in [
            "/Collider",
            "/World/Avatar/body",
            "/World/Garment",
            "/ClothWire",
        ] {
            assert!(!is_garment_model(other), "{other} must stay shaded only");
        }
    }
}
