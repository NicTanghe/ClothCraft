//! Basic DOM view cube in the native 3D viewport.

use super::super::app::create_runner_app;
use super::super::state::CssRunnerState;
use super::support::selection_test_state;
use clothcraft::css_ui::load_studio_stylesheet;
use clothcraft::render::PresetView;
use cssimpler::renderer::FrameInfo;
use std::time::Duration;

type CubeApp = cssimpler::app::App<
    'static,
    CssRunnerState,
    fn(&mut CssRunnerState, FrameInfo) -> cssimpler::app::Invalidation,
    fn(&CssRunnerState) -> cssimpler::core::Node,
    cssimpler::app::Invalidation,
>;

#[allow(clippy::type_complexity)]
fn app(state: CssRunnerState) -> CubeApp {
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
fn view_cube_has_every_face_and_clicking_applies_presets() {
    let _guard = super::super::super::worker::WORKER_TEST_LOCK
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let dir = tempfile::tempdir().unwrap();
    let mut app = app(selection_test_state(dir.path()));
    let cube = app.find_element_bounds_by_id("#view-cube").unwrap();
    let viewport = app
        .find_element_bounds_by_id("#runner-viewport-3d-scene")
        .unwrap();
    assert!(cube.width >= 100.0 && cube.height >= 70.0);
    assert!(cube.y >= viewport.y && cube.y + cube.height <= viewport.y + viewport.height + 1.0);
    for id in [
        "view-cube-top",
        "view-cube-bottom",
        "view-cube-front",
        "view-cube-back",
        "view-cube-left",
        "view-cube-right",
        "view-cube-iso",
    ] {
        assert!(
            app.find_element_bounds_by_id(&format!("#{id}")).is_some(),
            "{id}"
        );
    }

    let front = app.find_element_bounds_by_id("#view-cube-front").unwrap();
    assert!(cssimpler::renderer::dispatch_click(
        app.scene(),
        front.x + front.width * 0.5,
        front.y + front.height * 0.5
    ));
    app.frame(FrameInfo {
        delta: Duration::from_millis(16),
        frame_index: 1,
    });
    let (yaw, pitch) = PresetView::Front.angles();
    assert!((app.state().camera.yaw - yaw).abs() < 1e-6);
    assert!((app.state().camera.pitch - pitch).abs() < 1e-6);
    assert_eq!(app.state().status_message, "3D view: Front");

    let top = app.find_element_bounds_by_id("#view-cube-top").unwrap();
    assert!(cssimpler::renderer::dispatch_click(
        app.scene(),
        top.x + top.width * 0.5,
        top.y + top.height * 0.5
    ));
    app.frame(FrameInfo {
        delta: Duration::from_millis(16),
        frame_index: 2,
    });
    let (yaw, pitch) = PresetView::Top.angles();
    assert!((app.state().camera.yaw - yaw).abs() < 1e-6);
    assert!((app.state().camera.pitch - pitch).abs() < 1e-6);
}
