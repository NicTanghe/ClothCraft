//! Cloth-only wireframe for the native USD viewport.
//!
//! cssimpler strokes polygon-cage edges per USD mesh, so the runner enables the
//! stroke on the garment panel meshes and leaves every other display mesh
//! (collider, avatars) shaded only. This replaces the former `/ClothWire`
//! ribbon mesh, which had to be re-authored into the stage on every step.

use super::css_runner::shadow_controls::ShadowLevels;
use super::placement_gizmo::GizmoRenderer;
use cssimpler::renderer::gpu::usd::{
    UsdMaterial, UsdRenderer, UsdSurface, Wireframe, WireframeMode, WireframeStyle,
};
use cssimpler::renderer::{
    self, EngineEvent,
    gpu::{GpuContext, GpuViewportFrame, GpuViewportInfo, GpuViewportRenderer},
};
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::{Arc, Mutex};

/// Garment panel meshes are authored under this scope, one mesh per panel.
pub const GARMENT_SCOPE: &str = "/World/Garment/";

/// Stroke colours offered by the settings panel, in linear RGB. `Ink` is the
/// colour the former ribbon overlay used.
pub const COLOURS: [(&str, [f32; 3]); 5] = [
    ("Ink", [0.09, 0.11, 0.15]),
    ("White", [0.92, 0.94, 0.97]),
    ("Cyan", [0.10, 0.85, 1.00]),
    ("Amber", [1.00, 0.72, 0.15]),
    ("Red", [1.00, 0.25, 0.25]),
];

pub const DEFAULT_COLOUR: usize = 0;
pub const DEFAULT_WIDTH_PX: f32 = 2.0;
pub const MIN_WIDTH_PX: f32 = 1.0;
pub const MAX_WIDTH_PX: f32 = 8.0;

/// View-only cloth wireframe state shared between the settings panel and the
/// GPU viewport.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WireframeSettings {
    pub enabled: bool,
    pub colour: usize,
    pub width_px: f32,
}

impl Default for WireframeSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            colour: DEFAULT_COLOUR,
            width_px: DEFAULT_WIDTH_PX,
        }
    }
}

impl WireframeSettings {
    /// Clamp the colour index and width into the supported ranges.
    pub fn normalized(self) -> Self {
        Self {
            colour: self.colour.min(COLOURS.len() - 1),
            width_px: if self.width_px.is_finite() {
                self.width_px.clamp(MIN_WIDTH_PX, MAX_WIDTH_PX)
            } else {
                DEFAULT_WIDTH_PX
            },
            ..self
        }
    }

    pub fn colour_name(self) -> &'static str {
        COLOURS[self.colour.min(COLOURS.len() - 1)].0
    }

    /// Linear-RGB colour, for a CSS preview swatch.
    pub fn colour_rgb(self) -> [f32; 3] {
        COLOURS[self.colour.min(COLOURS.len() - 1)].1
    }

    /// A solid band: the core and outline colours are the same, so the stroke
    /// is one colour across its full width.
    pub fn stroke(self) -> WireframeStyle {
        let rgb = self.colour_rgb();
        WireframeStyle {
            core: rgb,
            outline: rgb,
            width_px: self.normalized().width_px,
        }
    }
}

/// True for the garment panel meshes the cloth wireframe covers.
pub fn is_garment_model(path: &str) -> bool {
    path.starts_with(GARMENT_SCOPE)
}

/// Enable or clear the stroke on every garment panel mesh of `renderer`.
///
/// Colliders, avatars and other display meshes are never touched, so the
/// wireframe stays cloth-only.
pub fn apply(renderer: &mut UsdRenderer, settings: WireframeSettings) {
    let settings = settings.normalized();
    let paths: Vec<String> = renderer
        .models()
        .iter()
        .filter(|path| is_garment_model(path))
        .cloned()
        .collect();
    for path in &paths {
        if settings.enabled {
            renderer.set_model_wireframe(
                path,
                Wireframe {
                    mode: WireframeMode::Edges,
                    style: settings.stroke(),
                },
            );
        } else {
            renderer.clear_model_wireframe(path);
        }
    }
}

/// Applies the shared settings to a renderer when they change.
///
/// The model count is part of the key, so a scene that gains or loses panels is
/// stroked again while unchanged frames do no work.
pub struct WireframeSync {
    settings: Arc<Mutex<WireframeSettings>>,
    applied: Option<(WireframeSettings, usize)>,
}

impl WireframeSync {
    pub fn new(settings: Arc<Mutex<WireframeSettings>>) -> Self {
        Self {
            settings,
            applied: None,
        }
    }

    pub fn sync(&mut self, renderer: &mut UsdRenderer) {
        let settings = *self.settings.lock().unwrap_or_else(|e| e.into_inner());
        let key = (settings, renderer.models().len());
        if self.applied == Some(key) {
            return;
        }
        apply(renderer, settings);
        self.applied = Some(key);
    }
}

/// The runner's GPU viewport: the USD renderer plus the shared wireframe state.
pub struct RunnerViewport {
    renderer: UsdRenderer,
    sync: WireframeSync,
    cube: super::view_cube::ViewCube,
    camera: Rc<RefCell<clothcraft::render::Camera>>,
    cube_hover: Rc<Cell<Option<glam::Vec3>>>,
    shadow_levels: Rc<Cell<ShadowLevels>>,
    gizmo: GizmoRenderer,
}

impl RunnerViewport {
    pub fn new(
        renderer: UsdRenderer,
        settings: Arc<Mutex<WireframeSettings>>,
        cube: super::view_cube::ViewCube,
        camera: Rc<RefCell<clothcraft::render::Camera>>,
        cube_hover: Rc<Cell<Option<glam::Vec3>>>,
        shadow_levels: Rc<Cell<ShadowLevels>>,
        gizmo: GizmoRenderer,
    ) -> Self {
        Self {
            renderer,
            sync: WireframeSync::new(settings),
            cube,
            camera,
            cube_hover,
            shadow_levels,
            gizmo,
        }
    }
}

impl GpuViewportRenderer for RunnerViewport {
    fn resize(&mut self, gpu: &GpuContext, info: GpuViewportInfo) -> renderer::Result<()> {
        self.gizmo.resize(info);
        self.renderer.resize(gpu, info)
    }
    fn event(&mut self, event: &EngineEvent) -> bool {
        self.renderer.event(event)
    }
    fn render(&mut self, gpu: &GpuContext, frame: GpuViewportFrame<'_>) -> renderer::Result<()> {
        self.sync.sync(&mut self.renderer);
        if let Some(settings) = self.renderer.shadow_settings() {
            let levels = self.shadow_levels.get();
            self.renderer
                .set_shadow_filter_radius_in_stage_units(Some(levels.filter_radius_m()))?;
            let updated = levels.apply(settings);
            if settings != updated {
                self.renderer.set_shadow_settings(updated)?;
            }
        }
        if self.renderer.supports_shadow_mapping() {
            let unassigned_panels: Vec<_> = self
                .renderer
                .models()
                .iter()
                .filter(|path| {
                    is_garment_model(path) && self.renderer.model_material(path).is_none()
                })
                .cloned()
                .collect();
            for path in unassigned_panels {
                self.renderer.set_model_material(
                    &path,
                    UsdMaterial {
                        surface: UsdSurface::Fabric,
                        // Garment UVs are in metres. Keep the weave fine enough
                        // to fade from view at fitting distance.
                        weave_frequency: 256.0,
                        ..UsdMaterial::default()
                    },
                )?;
            }
        }
        self.renderer.render(
            gpu,
            GpuViewportFrame {
                encoder: &mut *frame.encoder,
                ..frame
            },
        )?;
        let camera = self.camera.borrow();
        self.gizmo.draw(
            gpu,
            GpuViewportFrame {
                encoder: &mut *frame.encoder,
                ..frame
            },
            &camera,
        );
        self.cube.draw(gpu, frame, &camera, self.cube_hover.get());
        Ok(())
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

    #[test]
    fn settings_clamp_colour_and_width() {
        let settings = WireframeSettings {
            enabled: true,
            colour: 99,
            width_px: 100.0,
        }
        .normalized();
        assert_eq!(settings.colour, COLOURS.len() - 1);
        assert_eq!(settings.width_px, MAX_WIDTH_PX);
        assert_eq!(settings.colour_name(), COLOURS[COLOURS.len() - 1].0);

        let settings = WireframeSettings {
            width_px: f32::NAN,
            ..WireframeSettings::default()
        }
        .normalized();
        assert_eq!(settings.width_px, DEFAULT_WIDTH_PX);

        // A solid band keeps one colour across the full width.
        let stroke = WireframeSettings {
            enabled: true,
            colour: 2,
            width_px: 4.0,
        }
        .stroke();
        assert_eq!(stroke.core, stroke.outline);
        assert_eq!(stroke.core, COLOURS[2].1);
        assert_eq!(stroke.width_px, 4.0);
    }
}
