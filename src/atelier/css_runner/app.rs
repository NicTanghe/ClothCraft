//! Native application creation and runner entry point.

use super::super::{DocumentFiles, RunOptions, perf, usd_smoke, viewport_wireframe, worker};
use super::actions::update_runner;
use super::input::handle_runner_event;
use super::preferences::PreferenceStore;
use super::state::CssRunnerState;
use super::ui::shell::build_runner_ui;
use clothcraft::css_ui::load_studio_stylesheet;
use clothcraft::fitting::{FittingProject, FittingScene};
use clothcraft::gpu_avbd::ContactMethod;
use cssimpler::app::{App, Invalidation, RenderMode};
use cssimpler::core::Node;
use cssimpler::renderer::gpu::usd::{UsdRenderer, UsdShadowSettings, UsdStudioLighting};
use cssimpler::renderer::{FrameInfo, WindowConfig};
use cssimpler::style::Stylesheet;

pub(super) fn fitting_viewport_usda(
    scene: &FittingScene,
    surface: &clothcraft::avatar::SurfaceMesh,
    theme: clothcraft::usd_scene::StudioBackdropTheme,
) -> Result<String, String> {
    use clothcraft::usd_scene;

    let usda = usd_scene::garment_stage_usda(&scene.garment, &scene.state)?;
    let usda = usd_scene::append_viewport_surface(usda, surface);
    Ok(usd_scene::append_infinity_cove(usda, theme))
}

#[allow(clippy::type_complexity)]
pub fn create_runner_app<'a>(
    mut state: CssRunnerState,
    stylesheet: &'a Stylesheet,
) -> App<
    'a,
    CssRunnerState,
    fn(&mut CssRunnerState, FrameInfo) -> Invalidation,
    fn(&CssRunnerState) -> Node,
    Invalidation,
> {
    use clothcraft::usd_scene;

    let usda = fitting_viewport_usda(
        &state.scene,
        state.avatar_display_surface(),
        state.studio_backdrop_theme(),
    )
    .expect("publish garment viewport geometry");
    let usd_scene = usd_scene::viewport_scene_from_usda(&usda)
        .and_then(|scene| {
            scene
                .with_deforming_meshes(usd_scene::VIEWPORT_GARMENT_PATH)
                .map_err(|error| error.to_string())
        })
        .expect("create USD viewport scene");
    let camera_scene =
        usd_scene::viewport_camera_scene(&state.camera).expect("create USD viewport camera");
    let camera_stage = camera_scene.stage().clone();
    state.usd_camera_scene = Some(camera_scene);
    state.usd_scene = Some(usd_scene.clone());
    state.viewport_camera.replace(state.camera.clone());
    let camera = state.viewport_camera.clone();
    let cube_hover = state.viewport_cube_hover.clone();
    let gizmo_overlay = state.gizmo_overlay.clone();
    let gizmo_viewport = state.gizmo_viewport.clone();

    let smoke = state.smoke;
    let smoke_done = state.smoke_done.clone();
    let smoke_output = state.files.output.join("ui.png");
    // Clone the viewport handles before `state` moves into the application.
    let wireframe = state.viewport_wireframe.clone();
    let shadow_levels = state.viewport_shadow_levels.clone();
    let shadow_supported = state.viewport_shadow_supported.clone();
    let redraw_slot = state.viewport_redraw.clone();
    let update = state.update_handle.clone();
    let continuous = smoke || state.perf_test;
    let app = App::new(
        state,
        stylesheet,
        update_runner as fn(&mut CssRunnerState, FrameInfo) -> Invalidation,
        build_runner_ui as fn(&CssRunnerState) -> Node,
    )
    .with_event_handler(handle_runner_event)
    .with_render_mode(RenderMode::OnInvalidation)
    .with_frame_aligned_input(true)
    .with_update_handle(update)
    .with_update_deadline(CssRunnerState::next_update_at)
    .with_continuous_updates(continuous)
    .with_exit_when(|state| state.smoke_done.get() || state.should_exit);

    app.with_gpu_viewport(
        "runner-viewport-3d-scene",
        Default::default(),
        move |init| {
            let redraw = init.redraw.clone();
            *redraw_slot.lock().unwrap_or_else(|e| e.into_inner()) = Some(redraw.clone());
            let cube = super::super::view_cube::ViewCube::new(init.gpu, init.info.format);
            let gizmo = super::super::placement_gizmo::GizmoRenderer::new(
                init.gpu,
                init.info,
                gizmo_overlay,
                gizmo_viewport,
            );
            let mut renderer = UsdRenderer::new_with_authored_camera(
                init,
                usd_scene,
                camera_stage,
                usd_scene::VIEWPORT_CAMERA_PATH,
            )?;
            shadow_supported.set(renderer.supports_shadow_mapping());
            if shadow_supported.get() {
                renderer.set_studio_lighting(Some(UsdStudioLighting {
                    key_intensity: 0.76,
                    key_tint: [1.06, 0.99, 0.87],
                    toward_fill: [0.70, 0.45, 0.55],
                    fill_intensity: 0.34,
                    fill_tint: [0.84, 0.94, 1.08],
                    ambient: 0.24,
                    floor: false,
                    shadows: Some(shadow_levels.get().apply(UsdShadowSettings::default())),
                    ..UsdStudioLighting::default()
                }))?;
            }
            let viewport = viewport_wireframe::RunnerViewport::new(
                renderer,
                wireframe,
                cube,
                camera,
                cube_hover,
                shadow_levels,
                gizmo,
            );
            if smoke {
                Ok(Box::new(usd_smoke::SmokeViewport {
                    viewport,
                    output: smoke_output,
                    done: smoke_done,
                    redraw,
                    frames: 0,
                }))
            } else {
                Ok(Box::new(viewport))
            }
        },
    )
}

pub(super) fn init_gpu_device() -> Result<(wgpu::Device, wgpu::Queue), String> {
    clothcraft::gpu_avbd::request_device()
}

pub fn run(
    project: FittingProject,
    scene: FittingScene,
    files: DocumentFiles,
    options: RunOptions,
) -> Result<(), String> {
    let RunOptions {
        barrier_al,
        smoke,
        perf_test,
        gpu_sim,
    } = options;
    let stylesheet =
        Box::leak(Box::new(load_studio_stylesheet().map_err(|e| {
            format!("Failed to load Studio V2 stylesheet: {e}")
        })?));

    let (tx, latest) = if gpu_sim {
        let (device, queue) = init_gpu_device()?;
        worker::start(device, queue).map_err(|e| format!("Failed to start GPU worker: {e}"))?
    } else {
        worker::start_switchable().map_err(|e| format!("Failed to start CPU worker: {e}"))?
    };

    let mut state = CssRunnerState::new(project, scene, files, tx, latest);
    state.native_number_drag = !smoke;
    state.smoke = smoke;
    state.perf_test = perf_test;
    state.gpu_sim = gpu_sim;
    state.contact_method = if barrier_al {
        ContactMethod::BarrierAl
    } else {
        ContactMethod::Ipc
    };
    // Smoke and performance runs keep deterministic defaults and never write to
    // the real user profile.
    if !smoke && !perf_test {
        match PreferenceStore::user() {
            Ok(store) => state.attach_preferences(store),
            Err(error) => state.status_message = format!("Settings can't be saved: {error}"),
        }
    }
    if perf_test {
        state.perf = Some(perf::PerfRun::new());
        state.playing = true;
        state.send_control_state(false);
    }
    let smoke_done = state.smoke_done.clone();
    let app = create_runner_app(state, stylesheet);
    let config = WindowConfig::new("ClothCraft | Simulation Studio V2", 1600, 980);
    app.run(config)
        .map_err(|e| format!("CSSimpler application error: {e}"))?;
    if smoke && !smoke_done.get() {
        return Err("Native smoke closed before USD capture completed".into());
    }
    Ok(())
}
