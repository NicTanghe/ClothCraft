//! Settings controls, modal input ownership and wheel sensitivity regressions.

use super::super::super::viewport_wireframe::{DEFAULT_WIDTH_PX, MAX_WIDTH_PX, MIN_WIDTH_PX};
use super::super::super::worker;
use super::super::actions::{RunnerUiOp, apply_runner_ui_op};
use super::super::app::create_runner_app;
use super::super::board::geometry::{
    compute_2d_viewport_rect, screen_to_board_m, viewport_3d_rect,
};
use super::super::input::handle_runner_event;
use super::super::preferences::{AppPreferences, PreferenceStore};
use super::super::state::{DEFAULT_2D_ZOOM_SPEED, MAX_2D_ZOOM_SPEED, MIN_2D_ZOOM_SPEED};
use super::support::{pointer_btn_ev, pointer_move_ev, selection_test_state};
use clothcraft::css_ui::{Theme, ViewLayoutMode, load_studio_stylesheet};
use cssimpler::renderer::{
    EngineEvent, FrameInfo, KeyIdentity, KeyboardEvent, KeyboardModifiers, ScrollDelta,
};
use std::time::Duration;

#[test]
fn settings_controls_close_and_do_not_edit_the_board() {
    let _guard = worker::WORKER_TEST_LOCK
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let dir = tempfile::tempdir().unwrap();
    let state = selection_test_state(dir.path());
    let document = serde_json::to_string(&state.project).unwrap();
    let stylesheet = load_studio_stylesheet().unwrap();
    let mut app = create_runner_app(state, &stylesheet);
    let mut frame = 0;
    app.set_viewport(cssimpler::renderer::ViewportSize::new(1600, 980));
    app.frame(FrameInfo {
        delta: Duration::ZERO,
        frame_index: frame,
    });
    macro_rules! click {
        ($id:expr) => {{
            let b = app.find_element_bounds_by_id($id).unwrap();
            let (x, y) = (b.x + b.width * 0.5, b.y + b.height * 0.5);
            app.handle_engine_event(&pointer_move_ev(x, y));
            app.handle_engine_event(&pointer_btn_ev(true));
            assert!(
                cssimpler::renderer::dispatch_click(app.scene(), x, y),
                "{}",
                $id
            );
            app.handle_engine_event(&pointer_btn_ev(false));
            frame += 1;
            app.frame(FrameInfo {
                delta: Duration::ZERO,
                frame_index: frame,
            });
        }};
    }
    for (w, h) in [(1600, 980), (1280, 800)] {
        app.state_mut().window_size = (w as f32, h as f32);
        app.set_viewport(cssimpler::renderer::ViewportSize::new(w, h));
        frame += 1;
        app.frame(FrameInfo {
            delta: Duration::ZERO,
            frame_index: frame,
        });
        let file = app
            .find_element_bounds_by_id("#runner-file-toggle")
            .unwrap();
        let settings = app.find_element_bounds_by_id("#settings-toggle").unwrap();
        assert!(settings.x >= file.x + file.width);
        assert!(settings.x + settings.width <= w as f32);
        assert!(app.find_element_bounds_by_id("#settings-panel").is_none());
        click!("#settings-toggle");
        assert!(app.state().settings_open);
        for id in [
            "#settings-panel",
            "#settings-close",
            "#zoom-speed-inc",
            "#zoom-speed-dec",
            "#zoom-speed-reset",
            "#wireframe-colour",
            "#wireframe-width-dec",
            "#wireframe-width-value",
            "#wireframe-width-inc",
            "#wireframe-reset",
        ] {
            let b = app.find_element_bounds_by_id(id).unwrap();
            assert!(
                b.width > 10.0
                    && b.height > 10.0
                    && b.x >= 0.0
                    && b.y >= 0.0
                    && b.x + b.width <= w as f32
                    && b.y + b.height <= h as f32,
                "{id}: {b:?}"
            );
        }
        click!("#zoom-speed-inc");
        assert_eq!(
            app.state().board_2d_zoom_speed,
            DEFAULT_2D_ZOOM_SPEED + 0.25
        );
        click!("#zoom-speed-dec");
        assert_eq!(app.state().board_2d_zoom_speed, DEFAULT_2D_ZOOM_SPEED);
        click!("#zoom-speed-inc");
        click!("#zoom-speed-reset");
        assert_eq!(app.state().board_2d_zoom_speed, DEFAULT_2D_ZOOM_SPEED);
        // A click on the noninteractive header stays inside the settings modal.
        click!("#settings-title");
        assert!(app.state().settings_open);
        let board = compute_2d_viewport_rect(app.state());
        let (x, y) = (board.x + board.w * 0.5, board.y + board.h * 0.5);
        let zoom_before = app.state().board_2d_zoom;
        let tool_before = app.state().active_2d_tool.clone();
        app.handle_engine_event(&pointer_move_ev(x, y));
        app.handle_engine_event(&EngineEvent::Scroll {
            delta: ScrollDelta::Lines { x: 0.0, y: 1.0 },
            modifiers: KeyboardModifiers::default(),
        });
        app.handle_engine_event(&pointer_btn_ev(true));
        app.handle_engine_event(&pointer_btn_ev(false));
        assert!(!app.state().pointer_down);
        assert!(app.state().drag_2d.is_none());
        assert_eq!(app.state().board_2d_zoom, zoom_before);
        app.handle_engine_event(&EngineEvent::Key(KeyboardEvent {
            logical_key: KeyIdentity::Character("s".into()),
            physical_key: None,
            location: cssimpler::renderer::KeyLocation::Standard,
            state: cssimpler::renderer::ButtonState::Pressed,
            repeat: false,
            modifiers: KeyboardModifiers::default(),
        }));
        assert_eq!(app.state().active_2d_tool, tool_before);
        assert!(app.handle_engine_event(&EngineEvent::Key(KeyboardEvent {
            logical_key: KeyIdentity::Named("Escape".into()),
            physical_key: None,
            location: cssimpler::renderer::KeyLocation::Standard,
            state: cssimpler::renderer::ButtonState::Pressed,
            repeat: false,
            modifiers: KeyboardModifiers::default(),
        })));
        frame += 1;
        app.frame(FrameInfo {
            delta: Duration::ZERO,
            frame_index: frame,
        });
        assert!(!app.state().settings_open);
        click!("#settings-toggle");
        // Click outside the panel, over the underlying tool rail.
        assert!(cssimpler::renderer::dispatch_click(
            app.scene(),
            10.0,
            500.0
        ));
        frame += 1;
        app.frame(FrameInfo {
            delta: Duration::ZERO,
            frame_index: frame,
        });
        assert!(!app.state().settings_open);
        click!("#settings-toggle");
        click!("#settings-close");
        assert!(!app.state().settings_open);
    }
    assert_eq!(
        serde_json::to_string(&app.state().project).unwrap(),
        document
    );
    assert!(!app.state().files.modified);
    assert!(!app.state().history.can_undo());

    click!("#settings-toggle");
    for expected in [10, 16, 8] {
        click!("#measurement-precision");
        assert_eq!(
            app.state().project.template.measurement_precision(),
            expected
        );
        assert!(app.state().history.can_undo());
    }
}

#[test]
fn zoom_speed_affects_both_scroll_units_preserves_anchor_and_keeps_limits() {
    let _guard = worker::WORKER_TEST_LOCK
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let dir = tempfile::tempdir().unwrap();
    let mut state = selection_test_state(dir.path());
    let board = compute_2d_viewport_rect(&state);
    let (x, y) = (board.x + board.w * 0.3, board.y + board.h * 0.6);
    handle_runner_event(&mut state, &pointer_move_ev(x, y));
    for (speed, expected, delta) in [
        (1.0, 1.03, ScrollDelta::Lines { x: 0.0, y: 1.0 }),
        (
            DEFAULT_2D_ZOOM_SPEED,
            1.045,
            ScrollDelta::Lines { x: 0.0, y: 1.0 },
        ),
        (
            DEFAULT_2D_ZOOM_SPEED,
            1.045,
            ScrollDelta::Pixels { x: 0.0, y: 40.0 },
        ),
        (3.0, 1.09, ScrollDelta::Lines { x: 0.0, y: 1.0 }),
    ] {
        state.board_2d_zoom = 1.0;
        state.board_2d_pan = (0.0, 0.0);
        state.board_2d_zoom_speed = speed;
        let anchor = screen_to_board_m(&state, x, y).unwrap();
        // Scroll paints are deferred to the frame-aligned update.
        assert!(!handle_runner_event(
            &mut state,
            &EngineEvent::Scroll {
                delta,
                modifiers: KeyboardModifiers::default()
            }
        ));
        assert!((state.board_2d_zoom - expected).abs() < 1e-7);
        assert!(screen_to_board_m(&state, x, y).unwrap().distance(anchor) < 1e-6);
    }
    apply_runner_ui_op(&mut state, RunnerUiOp::Adjust2dZoomSpeed(100.0));
    assert_eq!(state.board_2d_zoom_speed, MAX_2D_ZOOM_SPEED);
    apply_runner_ui_op(&mut state, RunnerUiOp::Adjust2dZoomSpeed(-100.0));
    assert_eq!(state.board_2d_zoom_speed, MIN_2D_ZOOM_SPEED);
    apply_runner_ui_op(&mut state, RunnerUiOp::Adjust2dZoomSpeed(f64::NAN));
    assert_eq!(state.board_2d_zoom_speed, MIN_2D_ZOOM_SPEED);
    apply_runner_ui_op(&mut state, RunnerUiOp::Reset2dZoomSpeed);
    for (start, delta, expected) in [(9.99, 10000.0, 10.0), (0.251, -10000.0, 0.25)] {
        state.board_2d_zoom = start;
        assert!(!handle_runner_event(
            &mut state,
            &EngineEvent::Scroll {
                delta: ScrollDelta::Lines { x: 0.0, y: delta },
                modifiers: KeyboardModifiers::default()
            }
        ));
        assert_eq!(state.board_2d_zoom, expected);
    }
    let camera = viewport_3d_rect(&state);
    handle_runner_event(
        &mut state,
        &pointer_move_ev(camera.x + 20.0, camera.y + 20.0),
    );
    state.camera.distance = 5.0;
    state.board_2d_zoom_speed = MAX_2D_ZOOM_SPEED;
    assert!(!handle_runner_event(
        &mut state,
        &EngineEvent::Scroll {
            delta: ScrollDelta::Lines { x: 0.0, y: 1.0 },
            modifiers: KeyboardModifiers::default()
        }
    ));
    assert!((state.camera.distance - 4.8).abs() < 1e-6);
}

#[test]
fn wireframe_colour_and_thickness_reach_the_viewport() {
    let dir = tempfile::tempdir().unwrap();
    let mut state = selection_test_state(dir.path());
    macro_rules! shared {
        () => {
            *state
                .viewport_wireframe
                .lock()
                .unwrap_or_else(|e| e.into_inner())
        };
    }

    // Defaults: off, first colour, 2 px.
    state.sync_usd_wireframe();
    assert!(!shared!().enabled);
    assert_eq!(shared!().colour, 0);
    assert_eq!(shared!().width_px, DEFAULT_WIDTH_PX);

    apply_runner_ui_op(&mut state, RunnerUiOp::CycleWireframeColour);
    assert_eq!(shared!().colour, 1);
    apply_runner_ui_op(&mut state, RunnerUiOp::AdjustWireframeWidth(1.0));
    assert_eq!(shared!().width_px, 3.0);
    apply_runner_ui_op(&mut state, RunnerUiOp::AdjustWireframeWidth(-100.0));
    assert_eq!(shared!().width_px, MIN_WIDTH_PX);
    apply_runner_ui_op(&mut state, RunnerUiOp::AdjustWireframeWidth(100.0));
    assert_eq!(shared!().width_px, MAX_WIDTH_PX);
    apply_runner_ui_op(&mut state, RunnerUiOp::AdjustWireframeWidth(f32::NAN));
    assert_eq!(shared!().width_px, MAX_WIDTH_PX);

    // Turning the overlay on keeps the chosen style.
    apply_runner_ui_op(&mut state, RunnerUiOp::ToggleWireframe);
    assert!(shared!().enabled);
    assert_eq!(shared!().colour, 1);
    assert_eq!(shared!().width_px, MAX_WIDTH_PX);

    // Reset restores colour and width but not the toggle.
    apply_runner_ui_op(&mut state, RunnerUiOp::ResetWireframeStyle);
    assert!(shared!().enabled);
    assert_eq!(shared!().colour, 0);
    assert_eq!(shared!().width_px, DEFAULT_WIDTH_PX);
}

#[test]
fn settings_survive_a_restart_through_the_preference_file() {
    let _guard = worker::WORKER_TEST_LOCK
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("preferences/settings.ron");
    let mut state = selection_test_state(dir.path());
    state.attach_preferences(PreferenceStore::at(&path));

    state.theme = Theme::Light;
    state.layout_mode = ViewLayoutMode::TwoDOnly;
    state.board_2d_zoom_speed = 3.25;
    state.show_body_fill = false;
    state.show_body_border = false;
    state.wireframe = true;
    state.wireframe_colour = 3;
    state.wireframe_width_px = 6.0;
    state.project.template.measurement_bits = Some(16);
    assert_eq!(state.next_update_at(), None);
    state.flush_preferences(false);
    assert!(state.next_update_at().is_some());
    assert!(!path.exists());
    state.flush_preferences(true);
    assert_eq!(state.next_update_at(), None);
    assert!(path.exists(), "flush should create {}", path.display());

    // A second session starts from the defaults and then loads the file.
    let mut restored = selection_test_state(dir.path());
    restored.attach_preferences(PreferenceStore::at(&path));
    assert_eq!(restored.theme, Theme::Light);
    assert_eq!(restored.layout_mode, ViewLayoutMode::TwoDOnly);
    assert_eq!(restored.board_2d_zoom_speed, 3.25);
    assert!(!restored.show_body_fill);
    assert!(!restored.show_body_border);
    assert!(restored.show_body_outline);
    assert!(restored.wireframe);
    assert_eq!(restored.wireframe_colour, 3);
    assert_eq!(restored.wireframe_width_px, 6.0);
    assert_eq!(restored.project.template.measurement_precision(), 16);
}

#[test]
fn opened_documents_keep_their_own_measurement_precision() {
    let _guard = worker::WORKER_TEST_LOCK
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let dir = tempfile::tempdir().unwrap();
    let store = PreferenceStore::at(dir.path().join("settings.ron"));
    store
        .save(&AppPreferences {
            zoom_speed: 2.0,
            measurement_bits: 16,
            ..AppPreferences::default()
        })
        .unwrap();

    let mut state = selection_test_state(dir.path());
    state.files.source = Some(dir.path().join("garment.json"));
    state.project.template.measurement_bits = Some(10);
    state.attach_preferences(store);

    // The document's own value wins; unrelated preferences still apply.
    assert_eq!(state.project.template.measurement_precision(), 10);
    assert_eq!(state.board_2d_zoom_speed, 2.0);
}
