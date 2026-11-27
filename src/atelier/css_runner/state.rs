//! Editor state and simulation/selection state helpers.

use super::super::viewport_wireframe::WireframeSettings;
use super::super::worker::{self, Latest, Work};
use super::super::{DocumentFiles, perf};
use super::actions::RunnerAction;
use super::board::geometry::fit_2d_board;
use super::board::types::{Drag2d, Hover2dTarget, ToolDraft2d};
use super::imports::ImportDialog;
use super::mesh_preparation;
use super::preferences::{AppPreferences, PreferenceStore};
use super::shadow_controls::{ShadowInput, ShadowLevels};
use clothcraft::authoring::corner_tool::CIRCLE_TENSION;
use clothcraft::authoring::{CornerRule, History};
use clothcraft::avatar::SurfaceMesh;
use clothcraft::avatar_import::StagedAvatar;
use clothcraft::css_ui::{Theme, ViewLayoutMode};
use clothcraft::fitting::{FittingProject, FittingScene};
use clothcraft::garment::GarmentTemplate;
use clothcraft::gpu_avbd::ContactMethod;
use clothcraft::render::Camera;
use clothcraft::usd_scene::StudioBackdropTheme;
use clothcraft::vbd::{VbdMetrics, VbdSettings};
use cssimpler::ActionQueue;
use cssimpler::app::UpdateHandle;
use cssimpler::frame_drag::FrameAlignedDrag;
use std::cell::{Cell, RefCell};
use std::collections::BTreeSet;
use std::rc::Rc;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex, mpsc};
use std::time::{Duration, Instant};

pub(super) const DEFAULT_2D_ZOOM_SPEED: f64 = 1.5;
pub(super) const MIN_2D_ZOOM_SPEED: f64 = 0.25;
pub(super) const MAX_2D_ZOOM_SPEED: f64 = 4.0;
/// Coalesce rapid control changes (for example a zoom-speed drag) into one write.
const PREFERENCE_SAVE_DEBOUNCE: Duration = Duration::from_millis(500);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum ObjectCategory {
    #[default]
    Pattern2D,
    Sewing,
    Form3D,
    Material,
    Environment,
}

pub struct CssRunnerState {
    pub(super) actions: ActionQueue<RunnerAction>,
    pub(super) action_batch: Vec<RunnerAction>,
    pub(super) update_handle: UpdateHandle,
    pub(super) frame_drag: FrameAlignedDrag,
    pub(super) input_dirty: bool,
    pub wireframe: bool,
    /// Stroke colour index into `viewport_wireframe::COLOURS`.
    pub wireframe_colour: usize,
    /// Stroke width in physical pixels.
    pub wireframe_width_px: f32,
    /// Shared with the GPU viewport, which applies the stroke on its next frame.
    pub viewport_wireframe: Arc<Mutex<WireframeSettings>>,
    pub(super) viewport_shadow_levels: Rc<Cell<ShadowLevels>>,
    pub(super) viewport_shadow_supported: Rc<Cell<bool>>,
    pub(super) shadow_input: Arc<Mutex<ShadowInput>>,
    /// Wakes the on-demand viewport when the wireframe toggle or style changes.
    pub viewport_redraw: Arc<Mutex<Option<cssimpler::renderer::gpu::GpuRedrawHandle>>>,
    pub(super) board_background: RefCell<Option<super::board::rendering::BoardBackgroundCache>>,
    pub(super) native_number_drag: bool,
    pub(super) number_drag: Arc<Mutex<super::input::number::NumberDragState>>,
    pub(super) numeric_text: Arc<Mutex<Option<(String, String)>>>,
    pub(super) numeric_input_active: Arc<AtomicBool>,
    pub smoke: bool,
    pub smoke_done: std::rc::Rc<std::cell::Cell<bool>>,
    pub usd_scene: Option<cssimpler::renderer::gpu::usd::UsdScene>,
    pub(super) usd_camera_scene: Option<cssimpler::renderer::gpu::usd::UsdScene>,
    pub import_dialog: Option<ImportDialog>,
    pub import_tolerance_mm: f64,
    pub import_mesh_mm: f64,
    /// Zero means the DXF header units are used.
    pub import_scale_mm_per_unit: f64,
    /// Keeps the imported avatar's canonical OBJ alive for Save.
    pub staged_avatar: Option<StagedAvatar>,
    /// Every imported avatar mesh for display; physics uses `scene.collider`.
    pub avatar_display: Option<Arc<SurfaceMesh>>,
    pub project: FittingProject,
    pub preview_template: Option<GarmentTemplate>,
    pub history: History,
    pub mesh_draft: mesh_preparation::MeshDraft,
    pub scene: FittingScene,
    pub initial_scene: FittingScene,
    pub files: DocumentFiles,
    pub settings: VbdSettings,
    pub worker_tx: mpsc::Sender<Work>,
    pub worker_latest: Latest,
    pub generation: u64,
    pub worker_pending: bool,
    pub playing: bool,
    pub(super) seam_assembly: Option<super::seam_assembly::SeamAssembly>,
    pub(super) selected_seam: Option<String>,
    pub(super) seam_to_join: Option<String>,
    pub(super) seam_mode_error: Option<String>,
    pub(super) seam_collision: Option<super::seam_assembly::CollisionPreview>,
    pub step_count: u64,
    pub step_ms: f64,
    pub last_metrics: Option<VbdMetrics>,
    pub camera: Camera,
    pub viewport_camera: Rc<RefCell<Camera>>,
    pub viewport_cube_hover: Rc<Cell<Option<glam::Vec3>>>,
    pub(super) gizmo: super::gizmo::Gizmo,
    pub(super) gizmo_overlay: Rc<Cell<super::super::placement_gizmo::Overlay>>,
    pub(super) gizmo_viewport: super::gizmo::ViewportInfo,
    pub(super) cube_pointer_present: bool,
    pub theme: Theme,
    pub shading: clothcraft::usd_scene::ShadingMode,
    pub show_body_outline: bool,
    pub(super) body_outline: super::body_outline::BodyOutline,
    pub(super) show_body_fill: bool,
    pub(super) show_body_border: bool,
    pub(super) show_body_details: bool,
    pub(super) body_settings_open: bool,
    pub layout_mode: ViewLayoutMode,
    pub active_category: ObjectCategory,
    pub active_3d_tool: String,
    pub active_2d_tool: String,
    pub clean_view: bool,
    pub command_palette_open: bool,
    pub(super) file_menu_open: bool,
    pub settings_open: bool,
    pub left_drawer_open: bool,
    pub selected_piece_index: usize,
    pub mirrored_pieces: BTreeSet<String>,
    /// Uncommitted Arrange field edit; see `placement`.
    pub placement_draft: Option<super::placement::PlacementDraft>,
    pub selected_2d_point: Option<(String, usize)>,
    pub selected_2d_points: Vec<(String, usize)>,
    pub mirror_dart_bases: bool,
    pub slide_on_edge: bool,
    pub selected_2d_edge: Option<(String, String)>,
    pub hovered_2d: Option<Hover2dTarget>,
    pub drag_2d: Option<Drag2d>,
    pub draft_2d: Option<ToolDraft2d>,
    pub status_message: String,
    pub pointer_down: bool,
    pub(super) last_2d_click: Option<(std::time::Instant, (f32, f32), Hover2dTarget)>,
    pub dragging_3d: bool,
    pub last_pointer_pos: (f32, f32),
    pub window_size: (f32, f32),
    pub gpu_sim: bool,
    pub contact_method: ContactMethod,
    pub solver_menu_open: bool,
    pub perf_test: bool,
    pub perf: Option<perf::PerfRun>,
    pub cadence: perf::Cadence,
    pub should_exit: bool,
    pub pin_mode_contour: bool,
    pub corner_mode_radius: bool,
    pub corner_radius_mm: f64,
    pub corner_tension: f64,
    pub corner_rule: CornerRule,
    pub corner_preview_active: bool,
    pub open_accordion_idx: usize,
    pub accordion_open: [bool; 6],
    pub(super) board_2d_fit: (glam::DVec2, f64),
    pub board_2d_pan: (f64, f64),
    pub board_2d_zoom: f64,
    pub board_2d_zoom_speed: f64,
    /// Where preferences are written. `None` keeps them session-local: smoke
    /// runs and the headless tests never touch the real user profile.
    preference_store: Option<PreferenceStore>,
    /// Last successfully loaded or saved snapshot, used to detect changes.
    saved_preferences: AppPreferences,
    preference_dirty_since: Option<Instant>,
    preference_error: Option<String>,
}

impl CssRunnerState {
    pub fn new(
        project: FittingProject,
        scene: FittingScene,
        files: DocumentFiles,
        worker_tx: mpsc::Sender<Work>,
        worker_latest: Latest,
    ) -> Self {
        let settings = project.simulation.clone().unwrap_or_default();
        let initial_scene = scene.clone();
        let mesh_draft = mesh_preparation::MeshDraft::from_panel(project.template.panels.first());
        let camera = clothcraft::usd_scene::default_scene_camera(
            &scene.state.x,
            Some(&scene.collider.surface),
        );
        let body_outline =
            super::body_outline::build(scene.collider.surface.as_ref(), &project.template);

        let board_2d_fit = fit_2d_board(&project.template);
        Self {
            actions: ActionQueue::new(),
            action_batch: Vec::new(),
            update_handle: UpdateHandle::default(),
            frame_drag: FrameAlignedDrag::new(),
            input_dirty: false,
            wireframe: false,
            wireframe_colour: super::super::viewport_wireframe::DEFAULT_COLOUR,
            wireframe_width_px: super::super::viewport_wireframe::DEFAULT_WIDTH_PX,
            viewport_wireframe: Arc::new(Mutex::new(WireframeSettings::default())),
            viewport_shadow_levels: Rc::new(Cell::new(ShadowLevels::default())),
            viewport_shadow_supported: Rc::new(Cell::new(true)),
            shadow_input: Arc::new(Mutex::new(ShadowInput::default())),
            viewport_redraw: Arc::new(Mutex::new(None)),
            board_background: RefCell::new(None),
            native_number_drag: false,
            number_drag: Arc::new(Mutex::new(Default::default())),
            numeric_text: Arc::new(Mutex::new(None)),
            numeric_input_active: Arc::new(AtomicBool::new(false)),
            smoke: false,
            smoke_done: Default::default(),
            usd_scene: None,
            usd_camera_scene: None,
            import_dialog: None,
            import_tolerance_mm: 0.1,
            import_mesh_mm: 25.0,
            import_scale_mm_per_unit: 0.0,
            staged_avatar: None,
            avatar_display: None,
            project,
            preview_template: None,
            history: History::default(),
            mesh_draft,
            scene,
            initial_scene,
            files,
            settings,
            worker_tx,
            worker_latest,
            generation: 1,
            worker_pending: false,
            playing: false,
            seam_assembly: None,
            selected_seam: None,
            seam_to_join: None,
            seam_mode_error: None,
            seam_collision: None,
            step_count: 0,
            step_ms: 0.0,
            last_metrics: None,
            viewport_camera: Rc::new(RefCell::new(camera.clone())),
            viewport_cube_hover: Rc::new(Cell::new(None)),
            gizmo: Default::default(),
            gizmo_overlay: Default::default(),
            gizmo_viewport: Default::default(),
            cube_pointer_present: false,
            camera,
            theme: Theme::Dark,
            shading: clothcraft::usd_scene::ShadingMode::Smooth,
            show_body_outline: true,
            body_outline,
            show_body_fill: true,
            show_body_border: true,
            show_body_details: true,
            body_settings_open: false,
            layout_mode: ViewLayoutMode::Split,
            active_category: ObjectCategory::Pattern2D,
            active_3d_tool: "Gizmo".to_string(),
            active_2d_tool: "Edit".to_string(),
            clean_view: false,
            command_palette_open: false,
            file_menu_open: false,
            settings_open: false,
            left_drawer_open: true,
            selected_piece_index: 0,
            mirrored_pieces: BTreeSet::new(),
            placement_draft: None,
            selected_2d_point: None,
            selected_2d_points: Vec::new(),
            mirror_dart_bases: true,
            slide_on_edge: false,
            selected_2d_edge: None,
            hovered_2d: None,
            drag_2d: None,
            draft_2d: None,
            status_message: "Ready".to_string(),
            pointer_down: false,
            last_2d_click: None,
            dragging_3d: false,
            last_pointer_pos: (0.0, 0.0),
            window_size: (1600.0, 980.0),
            gpu_sim: false,
            contact_method: ContactMethod::Ipc,
            solver_menu_open: false,
            perf_test: false,
            perf: None,
            cadence: Default::default(),
            should_exit: false,
            pin_mode_contour: true,
            corner_mode_radius: true,
            corner_radius_mm: 10.0,
            corner_tension: CIRCLE_TENSION,
            corner_rule: CornerRule::Smooth,
            corner_preview_active: false,
            open_accordion_idx: 0,
            accordion_open: [true, false, true, false, false, false],
            board_2d_fit,
            board_2d_pan: (0.0, 0.0),
            board_2d_zoom: 1.0,
            board_2d_zoom_speed: DEFAULT_2D_ZOOM_SPEED,
            preference_store: None,
            saved_preferences: AppPreferences::default(),
            preference_dirty_since: None,
            preference_error: None,
        }
    }

    pub(super) fn frame_all_2d(&mut self) {
        self.board_2d_fit = fit_2d_board(&self.project.template);
        self.board_2d_pan = (0.0, 0.0);
        self.board_2d_zoom = 1.0;
    }

    pub fn sync_selected_point(&mut self) {
        self.selected_2d_point = self.selected_2d_points.last().cloned();
    }

    pub fn set_single_2d_point(&mut self, panel_id: String, point_idx: usize) {
        self.selected_2d_points.clear();
        self.selected_2d_points.push((panel_id.clone(), point_idx));
        self.selected_2d_point = Some((panel_id, point_idx));
        self.selected_2d_edge = None;
    }

    pub fn toggle_2d_point(&mut self, panel_id: String, point_idx: usize) {
        if self.selected_2d_points.is_empty()
            && let Some(prev) = self.selected_2d_point.take()
        {
            self.selected_2d_points.push(prev);
        }
        if let Some(pos) = self
            .selected_2d_points
            .iter()
            .position(|(p, i)| p == &panel_id && *i == point_idx)
        {
            self.selected_2d_points.remove(pos);
        } else {
            self.selected_2d_points.push((panel_id, point_idx));
        }
        self.selected_2d_edge = None;
        self.sync_selected_point();
    }

    pub fn clear_2d_subselection(&mut self) {
        self.selected_2d_points.clear();
        self.selected_2d_point = None;
        self.selected_2d_edge = None;
        self.corner_preview_active = false;
    }

    /// Theme for the display-only studio cove.
    pub fn studio_backdrop_theme(&self) -> StudioBackdropTheme {
        match self.theme {
            Theme::Dark => StudioBackdropTheme::Dark,
            Theme::Light => StudioBackdropTheme::Light,
        }
    }

    /// Recolour the studio cove after a theme change.
    pub fn sync_usd_backdrop(&mut self) {
        let Some(scene) = &self.usd_scene else {
            return;
        };
        if let Err(error) =
            clothcraft::usd_scene::update_infinity_cove_color(scene, self.studio_backdrop_theme())
        {
            self.status_message = format!("Studio backdrop update error: {error}");
        }
    }

    /// Surface shown in the 3D viewport and used for framing. It includes
    /// imported meshes that are not part of the collision proxy.
    pub fn avatar_display_surface(&self) -> &SurfaceMesh {
        self.avatar_display
            .as_deref()
            .unwrap_or(&self.scene.collider.surface)
    }

    pub fn active_template(&self) -> &GarmentTemplate {
        self.preview_template
            .as_ref()
            .unwrap_or(&self.project.template)
    }

    pub fn send_control_state(&mut self, once: bool) {
        if self.playing || once {
            super::seam_assembly::clear_collision(self);
        }
        let once = if (self.playing || once)
            && self.seam_assembly.is_none()
            && let Some(id) = self.seam_to_join.clone()
        {
            super::seam_assembly::begin_existing(self, &id) && once
        } else {
            once
        };
        let assembling = if let Some(assembly) = &mut self.seam_assembly {
            assembly.step_requested |= once;
            if self.playing || once {
                self.update_handle.request_update();
            }
            true
        } else {
            false
        };
        let playing = self.playing && !assembling;
        let once = once && !assembling;
        let update = self.update_handle.clone();
        let work = Work {
            generation: self.generation,
            scene: self.scene.clone(),
            settings: self.settings.clone(),
            grab: None,
            gpu: self.gpu_sim,
            contact_method: self.contact_method,
            signal: worker::RepaintSignal::new(move || update.request_update()),
            playing,
            once,
        };
        if self.worker_tx.send(work).is_ok() && (playing || once) {
            self.worker_pending = true;
        }
    }

    pub fn trigger_worker_step(&mut self, once: bool) {
        self.send_control_state(once);
    }

    pub fn sync_usd_camera(&mut self) {
        self.viewport_camera.replace(self.camera.clone());
        if let Some(scene) = &self.usd_camera_scene
            && let Err(error) = clothcraft::usd_scene::update_viewport_camera(scene, &self.camera)
        {
            self.status_message = format!("USD camera error: {error}");
        }
    }

    pub fn sync_usd_points(&mut self) {
        if let Some(scene) = &self.usd_scene
            && let Err(error) = clothcraft::usd_scene::update_panel_viewport_positions(
                scene,
                &self.scene.garment,
                &self.scene.state,
                self.shading,
            )
        {
            self.status_message = format!("USD update error: {error}");
        }
    }

    pub(super) fn sync_usd_simulation_pose(&mut self) {
        if let Some(scene) = &self.usd_scene
            && let Err(error) = clothcraft::usd_scene::update_panel_viewport_pose(
                scene,
                &self.scene.garment,
                &self.scene.state,
            )
        {
            self.status_message = format!("USD pose update error: {error}");
        }
    }

    /// Current wireframe toggle, colour and width, clamped to supported ranges.
    pub fn wireframe_settings(&self) -> WireframeSettings {
        WireframeSettings {
            enabled: self.wireframe,
            colour: self.wireframe_colour,
            width_px: self.wireframe_width_px,
        }
        .normalized()
    }

    /// Apply the cloth-only wireframe toggle, colour and width to the viewport.
    ///
    /// The stroke lives in the GPU renderer, so the panel positions and the
    /// stage stay untouched; the viewport applies the settings on its next
    /// frame.
    pub fn sync_usd_wireframe(&mut self) {
        let settings = self.wireframe_settings();
        *self
            .viewport_wireframe
            .lock()
            .unwrap_or_else(|e| e.into_inner()) = settings;
        self.request_viewport_redraw();
    }

    pub(super) fn sync_view_cube_hover(&self) {
        let rect = super::board::geometry::viewport_3d_rect(self);
        let (x, y) = self.last_pointer_pos;
        let hovered = if self.cube_pointer_present
            && !self.pointer_down
            && !self.settings_open
            && !self.command_palette_open
            && !self.file_menu_open
            && !self.solver_menu_open
            && self.import_dialog.is_none()
            && rect.contains(x, y)
        {
            super::super::view_cube::pick(
                &self.camera,
                glam::Vec2::new(rect.w, rect.h),
                glam::Vec2::new(x - rect.x, y - rect.y),
            )
        } else {
            None
        };
        if self.viewport_cube_hover.replace(hovered) != hovered {
            self.request_viewport_redraw();
        }
    }

    pub(super) fn request_viewport_redraw(&self) {
        if let Some(handle) = self
            .viewport_redraw
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .as_ref()
        {
            handle.request_redraw();
        }
    }

    /// Load the per-user preferences and apply them to this editor instance.
    ///
    /// Values that belong to the document (geometry, fabrics, seams and solver
    /// settings) are left alone. The measurement precision is applied only to
    /// a fresh session; a project opened from disk keeps its own value.
    pub(super) fn attach_preferences(&mut self, store: PreferenceStore) {
        let preferences = store.load();
        self.apply_preferences(&preferences);
        self.saved_preferences = self.capture_preferences();
        self.preference_store = Some(store);
        self.preference_dirty_since = None;
        self.preference_error = None;
    }

    /// Current preference values, normalized to the ranges the controls allow.
    fn capture_preferences(&self) -> AppPreferences {
        AppPreferences {
            version: super::preferences::VERSION,
            theme: self.theme.into(),
            layout: self.layout_mode.into(),
            zoom_speed: self.board_2d_zoom_speed,
            body_visible: self.show_body_outline,
            body_fill: self.show_body_fill,
            body_outline: self.show_body_border,
            body_details: self.show_body_details,
            wireframe_enabled: self.wireframe,
            wireframe_colour: self.wireframe_colour,
            wireframe_width_px: self.wireframe_width_px,
            measurement_bits: self.project.template.measurement_precision(),
        }
        .sanitized()
    }

    fn apply_preferences(&mut self, preferences: &AppPreferences) {
        let preferences = preferences.clone().sanitized();
        self.theme = preferences.theme.into();
        self.sync_usd_backdrop();
        self.layout_mode = preferences.layout.into();
        self.board_2d_zoom_speed = preferences.zoom_speed;
        self.show_body_outline = preferences.body_visible;
        self.show_body_fill = preferences.body_fill;
        self.show_body_border = preferences.body_outline;
        self.show_body_details = preferences.body_details;
        self.wireframe = preferences.wireframe_enabled;
        self.wireframe_colour = preferences.wireframe_colour;
        self.wireframe_width_px = preferences.wireframe_width_px;
        if self.files.source.is_none() {
            self.project.template.measurement_bits = Some(preferences.measurement_bits);
        }
        self.sync_usd_wireframe();
    }

    pub(super) fn next_update_at(&self) -> Option<Instant> {
        let save_at = self
            .preference_dirty_since
            .map(|since| since + PREFERENCE_SAVE_DEBOUNCE);
        let sample_at = self.number_drag.lock().unwrap().next_sample_at();
        let assembly_at = self
            .seam_assembly
            .as_ref()
            .filter(|_| self.playing)
            .map(super::seam_assembly::SeamAssembly::deadline);
        save_at
            .into_iter()
            .chain(sample_at)
            .chain(assembly_at)
            .min()
    }

    /// Write changed preferences after a short debounce, or immediately when
    /// `force` is set (shutdown and tests).
    ///
    /// Returns `true` when the status line changed because the file could not
    /// be written, so the caller can repaint.
    pub(super) fn flush_preferences(&mut self, force: bool) -> bool {
        let Some(store) = self.preference_store.clone() else {
            return false;
        };
        let current = self.capture_preferences();
        if current == self.saved_preferences {
            self.preference_dirty_since = None;
            return false;
        }
        let now = Instant::now();
        let since = *self.preference_dirty_since.get_or_insert(now);
        if !force && now.saturating_duration_since(since) < PREFERENCE_SAVE_DEBOUNCE {
            return false;
        }
        match store.save(&current) {
            Ok(()) => {
                let recovered = self.preference_error.take().is_some();
                self.saved_preferences = current;
                self.preference_dirty_since = None;
                if recovered {
                    self.status_message = "Settings saved".to_string();
                }
                recovered
            }
            Err(error) => {
                self.preference_dirty_since = Some(now);
                if self.preference_error.as_deref() == Some(error.as_str()) {
                    return false;
                }
                self.preference_error = Some(error.clone());
                self.status_message = format!("Settings not saved: {error}");
                true
            }
        }
    }

    /// Re-apply the current shading mode to the garment and collision display meshes.
    pub fn sync_usd_shading(&mut self) {
        if let Some(scene) = &self.usd_scene
            && let Err(error) = clothcraft::usd_scene::apply_viewport_shading(
                scene,
                &self.scene.garment,
                &self.scene.state,
                Some(self.avatar_display_surface()),
                self.shading,
            )
        {
            self.status_message = format!("USD shading error: {error}");
        }
    }

    /// Transfer the last displayed accepted x/v/time; discard in-flight results
    /// by generation. Playback pauses; rest geometry and document history stay intact.
    pub fn select_solver(&mut self, gpu: bool) {
        self.select_solver_method(gpu, ContactMethod::Ipc);
    }
    pub fn select_solver_method(&mut self, gpu: bool, method: ContactMethod) {
        self.solver_menu_open = false;
        if self.gpu_sim == gpu && self.contact_method == method {
            return;
        }
        super::seam_assembly::cancel_motion(self);
        self.gpu_sim = gpu;
        self.contact_method = method;
        self.generation = self.generation.wrapping_add(1);
        self.playing = false;
        self.worker_pending = false;
        self.last_metrics = None;
        self.step_ms = 0.0;
        self.status_message =
            "Solver changed; paused at displayed state. Press Play or Step.".into();
        self.send_control_state(false);
    }

    pub fn reset_simulation(&mut self) {
        super::seam_assembly::cancel_motion(self);
        super::seam_assembly::clear_collision(self);
        self.seam_mode_error = None;
        self.files.modified |= self.scene.state.time != self.initial_scene.state.time
            || self.scene.state.x != self.initial_scene.state.x
            || self.scene.state.v != self.initial_scene.state.v;
        self.generation = self.generation.wrapping_add(1);
        self.scene = self.initial_scene.clone();
        self.step_count = 0;
        self.step_ms = 0.0;
        self.last_metrics = None;
        self.worker_pending = false;
        self.playing = false;
        self.status_message = "Reset simulation state".to_string();
        self.sync_usd_points();
        self.send_control_state(false);
    }
}

impl Drop for CssRunnerState {
    fn drop(&mut self) {
        // Normal window close and process exit both drop the state; writing here
        // keeps changes made inside the debounce window.
        let _ = self.flush_preferences(true);
    }
}
