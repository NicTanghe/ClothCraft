//! Basic gray avatar silhouette on the 2D board.

use super::super::app::create_runner_app;
use super::super::board::rendering::render_runner_2d_paths;
use super::super::state::CssRunnerState;
use super::support::{count_class, selection_test_state};
use clothcraft::css_ui::load_studio_stylesheet;
use cssimpler::renderer::FrameInfo;
use std::time::Duration;

type OutlineApp = cssimpler::app::App<
    'static,
    CssRunnerState,
    fn(&mut CssRunnerState, FrameInfo) -> cssimpler::app::Invalidation,
    fn(&CssRunnerState) -> cssimpler::core::Node,
    cssimpler::app::Invalidation,
>;

#[allow(clippy::type_complexity)]
fn app(state: CssRunnerState) -> OutlineApp {
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
fn body_outline_uses_the_uncached_drawing_layer() {
    let _guard = super::super::super::worker::WORKER_TEST_LOCK
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let dir = tempfile::tempdir().unwrap();
    let state = selection_test_state(dir.path());
    assert!(state.show_body_outline);
    assert!(
        state.body_outline.len() >= 3,
        "a closed triangle avatar should expose its boundary"
    );
    let (paths, _) = render_runner_2d_paths(&state);
    let outline = paths
        .iter()
        .find(|path| path.class.split_whitespace().any(|c| c == "body-outline"))
        .expect("body outline path");
    assert!(
        !outline.background,
        "silhouette stays on the uncached drawing layer"
    );
}

#[test]
fn body_outline_toggle_hides_and_shows_the_overlay() {
    let _guard = super::super::super::worker::WORKER_TEST_LOCK
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let dir = tempfile::tempdir().unwrap();
    let mut app = app(selection_test_state(dir.path()));
    assert_eq!(
        count_class(&render_runner_2d_paths(app.state()).0, "body-outline"),
        1
    );
    let toggle = app
        .find_element_bounds_by_id("#body-outline-toggle")
        .unwrap();
    assert!(cssimpler::renderer::dispatch_click(
        app.scene(),
        toggle.x + toggle.width * 0.5,
        toggle.y + toggle.height * 0.5
    ));
    app.frame(FrameInfo {
        delta: Duration::from_millis(16),
        frame_index: 1,
    });
    assert!(!app.state().show_body_outline);
    assert_eq!(
        count_class(&render_runner_2d_paths(app.state()).0, "body-outline"),
        0
    );
    let toggle = app
        .find_element_bounds_by_id("#body-outline-toggle")
        .unwrap();
    assert!(cssimpler::renderer::dispatch_click(
        app.scene(),
        toggle.x + toggle.width * 0.5,
        toggle.y + toggle.height * 0.5
    ));
    app.frame(FrameInfo {
        delta: Duration::from_millis(16),
        frame_index: 2,
    });
    assert!(app.state().show_body_outline);
    assert_eq!(
        count_class(&render_runner_2d_paths(app.state()).0, "body-outline"),
        1
    );
}
