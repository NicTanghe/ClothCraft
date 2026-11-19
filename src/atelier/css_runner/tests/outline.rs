//! Basic gray avatar silhouette on the 2D board.

use super::super::app::create_runner_app;
use super::super::board::rendering::render_runner_2d_paths;
use super::super::state::CssRunnerState;
use super::support::selection_test_state;
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
        state.body_outline.details.is_empty(),
        "a single triangle has no interior details"
    );
    assert!(
        state
            .body_outline
            .contours
            .iter()
            .any(|ring| ring.len() >= 3),
        "a closed triangle avatar should expose its boundary"
    );
    let (paths, _) = render_runner_2d_paths(&state);
    let fill = paths.iter().find(|path| path.class == "body-fill").unwrap();
    let outline = paths
        .iter()
        .find(|path| path.class == "body-outline")
        .unwrap();
    assert!(fill.path.paint().fill.is_some());
    assert!(
        !fill.background && !outline.background,
        "body layers stay off the cached grid"
    );
    let filled = fill.path.geometry().bounds().unwrap();
    let center = outline.path.geometry().bounds().unwrap();
    let half_width = outline.path.paint().stroke_width * 0.5;
    for extension in [
        center.min_x - filled.min_x,
        center.min_y - filled.min_y,
        filled.max_x - center.max_x,
        filled.max_y - center.max_y,
    ] {
        assert!(
            (extension - half_width).abs() < 0.02 * half_width,
            "{extension} vs {half_width}"
        );
    }
}

#[test]
fn body_settings_control_all_layers_independently() {
    use clothcraft::avatar::SurfaceMesh;

    let _guard = super::super::super::worker::WORKER_TEST_LOCK
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let dir = tempfile::tempdir().unwrap();
    let mut state = selection_test_state(dir.path());
    let avatar =
        SurfaceMesh::load_obj(std::path::Path::new("assets/makehuman/mannequin.obj")).unwrap();
    state.body_outline = super::super::body_outline::build(&avatar, &state.project.template);
    assert!(
        !state.body_outline.details.is_empty(),
        "bundled mannequin should retain feature lines"
    );
    let mut app = app(state);
    let mut frame = 0;
    let mut click = |app: &mut OutlineApp, id: &str| {
        let rect = app.find_element_bounds_by_id(id).unwrap();
        assert!(cssimpler::renderer::dispatch_click(
            app.scene(),
            rect.x + rect.width * 0.5,
            rect.y + rect.height * 0.5
        ));
        frame += 1;
        app.frame(FrameInfo {
            delta: Duration::ZERO,
            frame_index: frame,
        });
    };
    let layers = |app: &OutlineApp| {
        let (paths, _) = render_runner_2d_paths(app.state());
        let fill = paths.iter().position(|p| p.class == "body-fill");
        let outline = paths.iter().position(|p| p.class == "body-outline");
        let details = paths.iter().position(|p| p.class == "body-details");
        if let (Some(fill), Some(outline)) = (fill, outline) {
            assert!(fill < outline, "outline must overlay the fill");
        }
        if let (Some(fill), Some(details)) = (fill, details) {
            assert!(fill < details, "detail lines must overlay the fill");
        }
        (fill.is_some(), outline.is_some(), details.is_some())
    };

    assert_eq!(layers(&app), (true, true, true));
    click(&mut app, "#settings-toggle");
    assert!(
        app.find_element_bounds_by_id("#settings-body-fill")
            .is_none()
    );
    click(&mut app, "#settings-body-section");
    click(&mut app, "#settings-body-fill");
    assert_eq!(layers(&app), (false, true, true));
    click(&mut app, "#settings-body-outline");
    assert_eq!(layers(&app), (false, false, true));
    click(&mut app, "#settings-body-fill");
    click(&mut app, "#settings-body-details");
    assert_eq!(layers(&app), (true, false, false));
    click(&mut app, "#settings-body-visible");
    assert_eq!(layers(&app), (false, false, false));
    click(&mut app, "#settings-body-visible");
    assert_eq!(layers(&app), (true, false, false));
    click(&mut app, "#settings-body-section");
    assert!(
        app.find_element_bounds_by_id("#settings-body-details")
            .is_none()
    );
    click(&mut app, "#settings-close");
    click(&mut app, "#body-outline-toggle");
    assert_eq!(layers(&app), (false, false, false));
    click(&mut app, "#body-outline-toggle");
    assert_eq!(layers(&app), (true, false, false));
}
