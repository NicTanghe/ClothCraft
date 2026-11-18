//! Native 3D view presets and projection controls.

use super::super::app::create_runner_app;
use super::super::state::CssRunnerState;
use super::support::selection_test_state;
use clothcraft::css_ui::load_studio_stylesheet;
use clothcraft::render::PresetView;
use cssimpler::renderer::FrameInfo;
use std::time::Duration;

type CameraApp = cssimpler::app::App<
    'static,
    CssRunnerState,
    fn(&mut CssRunnerState, FrameInfo) -> cssimpler::app::Invalidation,
    fn(&CssRunnerState) -> cssimpler::core::Node,
    cssimpler::app::Invalidation,
>;

#[allow(clippy::type_complexity)]
fn app(state: CssRunnerState) -> CameraApp {
    let stylesheet = Box::leak(Box::new(
        load_studio_stylesheet().expect("load studio stylesheet"),
    ));
    let mut app = create_runner_app(state, stylesheet);
    app.set_viewport(cssimpler::renderer::ViewportSize::new(1600, 980));
    app.frame(FrameInfo {
        delta: Duration::ZERO,
        frame_index: 0,
    });
    app
}

#[test]
fn view_menu_applies_presets_and_closes() {
    let _guard = super::super::super::worker::WORKER_TEST_LOCK
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let dir = tempfile::tempdir().unwrap();
    let mut app = app(selection_test_state(dir.path()));
    assert!(!app.state().camera_menu_open);
    let trigger = app
        .find_element_bounds_by_id("#camera-menu-trigger")
        .unwrap();
    assert!(cssimpler::renderer::dispatch_click(
        app.scene(),
        trigger.x + trigger.width * 0.5,
        trigger.y + trigger.height * 0.5
    ));
    app.frame(FrameInfo {
        delta: Duration::from_millis(16),
        frame_index: 1,
    });
    assert!(app.state().camera_menu_open);
    for view in PresetView::ALL {
        let id = format!("camera-preset-{}", format!("{view:?}").to_lowercase());
        assert!(app.find_element_bounds_by_id(&id).is_some(), "missing {id}");
    }
    let front = app
        .find_element_bounds_by_id("#camera-preset-front")
        .unwrap();
    assert!(cssimpler::renderer::dispatch_click(
        app.scene(),
        front.x + front.width * 0.5,
        front.y + front.height * 0.5
    ));
    app.frame(FrameInfo {
        delta: Duration::from_millis(16),
        frame_index: 2,
    });
    let (yaw, pitch) = PresetView::Front.angles();
    assert!((app.state().camera.yaw - yaw).abs() < 1e-6);
    assert!((app.state().camera.pitch - pitch).abs() < 1e-6);
    assert!(!app.state().camera_menu_open);
    assert_eq!(app.state().status_message, "3D view: Front");
}

#[test]
fn projection_toggle_writes_the_usd_camera() {
    use cssimpler::renderer::gpu::openusd_schemas::geom::{self, CameraSchema};
    let _guard = super::super::super::worker::WORKER_TEST_LOCK
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let dir = tempfile::tempdir().unwrap();
    let mut app = app(selection_test_state(dir.path()));
    let read = |state: &CssRunnerState| {
        let stage = state.usd_scene.as_ref().unwrap().stage();
        let camera = geom::Camera::get(stage, clothcraft::usd_scene::VIEWPORT_CAMERA_PATH)
            .unwrap()
            .unwrap();
        (
            camera.projection().unwrap().unwrap(),
            camera.vertical_aperture().unwrap().unwrap(),
        )
    };
    let (projection, aperture) = read(app.state());
    assert_eq!(projection, geom::Projection::Perspective);
    assert_eq!(aperture, 24.0);

    let trigger = app
        .find_element_bounds_by_id("#camera-menu-trigger")
        .unwrap();
    assert!(cssimpler::renderer::dispatch_click(
        app.scene(),
        trigger.x + trigger.width * 0.5,
        trigger.y + trigger.height * 0.5
    ));
    app.frame(FrameInfo {
        delta: Duration::from_millis(16),
        frame_index: 1,
    });
    let toggle = app.find_element_bounds_by_id("#camera-projection").unwrap();
    assert!(cssimpler::renderer::dispatch_click(
        app.scene(),
        toggle.x + toggle.width * 0.5,
        toggle.y + toggle.height * 0.5
    ));
    app.frame(FrameInfo {
        delta: Duration::from_millis(16),
        frame_index: 2,
    });
    assert!(app.state().camera.orthographic);
    assert!(!app.state().camera_menu_open);
    let (projection, aperture) = read(app.state());
    assert_eq!(projection, geom::Projection::Orthographic);
    let expected = app.state().camera.distance * (45.0_f32.to_radians() * 0.5).tan() * 20.0;
    assert!((aperture - expected).abs() < 1e-3, "{aperture} {expected}");

    // Toggling back restores the perspective aperture.
    app.state_mut()
        .actions
        .push(super::super::actions::RunnerAction::CameraMenu);
    app.state_mut()
        .actions
        .push(super::super::actions::RunnerAction::ProjectionToggle);
    app.state_mut().input_dirty = true;
    app.frame(FrameInfo {
        delta: Duration::from_millis(16),
        frame_index: 3,
    });
    let (projection, aperture) = read(app.state());
    assert_eq!(projection, geom::Projection::Perspective);
    assert_eq!(aperture, 24.0);
}
