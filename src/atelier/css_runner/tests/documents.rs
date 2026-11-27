//! Native File menu and editable USD project workflows.

use super::super::super::{DocumentFiles, worker};
use super::super::actions::RunnerAction;
use super::super::app::create_runner_app;
use super::super::input::handle_runner_event;
use super::super::state::CssRunnerState;
use super::support::{pointer_btn_ev, pointer_move_ev};
use clothcraft::css_ui::load_studio_stylesheet;
use clothcraft::fitting::FittingProject;
use clothcraft::garment::presets::GarmentKind;
use clothcraft::usd_scene::viewport_panel_path;
use cssimpler::renderer::{
    ButtonState, EngineEvent, FrameInfo, KeyIdentity, KeyLocation, KeyboardEvent,
    KeyboardModifiers, ViewportSize, dispatch_click,
};
use std::time::Duration;

fn save_test_state(dir: &std::path::Path, source: Option<std::path::PathBuf>) -> CssRunnerState {
    let avatar = dir.join("avatar.obj");
    std::fs::write(&avatar, "v 0 0 -10\nv 1 0 -10\nv 0 1 -10\nf 1 2 3\n").expect("write avatar");
    let project = FittingProject::basic(GarmentKind::Tshirt, 1, avatar);
    let scene = project.compile().expect("compile scene");
    let files = DocumentFiles {
        output: dir.join("out"),
        source,
        modified: false,
    };
    let (tx, latest) = worker::start_cpu().expect("start cpu worker");
    CssRunnerState::new(project, scene, files, tx, latest)
}

#[test]
fn save_writes_the_opened_project_and_reopens_it() {
    let _guard = worker::WORKER_TEST_LOCK
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let dir = tempfile::tempdir().expect("tempdir");
    let source = dir.path().join("garment.usda");
    let mut state = save_test_state(dir.path(), Some(source.clone()));
    state.project.save(&source).expect("seed source document");
    state.project.template.panels[0].name = "Renamed by save test".into();

    assert_eq!(state.save_target(), Some(source.as_path()));
    state.files.modified = true;
    state.save_project();
    assert_eq!(state.status_message, format!("Saved {}", source.display()));
    assert!(!state.files.modified);
    assert!(
        !dir.path().join("out/fitting-project.usda").exists(),
        "save must not fall back to the output directory when a source exists"
    );
    let reopened = FittingProject::load(&source).expect("reopen saved project");
    assert_eq!(reopened.template.panels[0].name, "Renamed by save test");
    assert_eq!(
        reopened.state.as_ref().map(|s| s.x.len()),
        Some(state.scene.state.x.len())
    );
    assert!(reopened.simulation.is_some());
    assert!(
        reopened.compile().is_ok(),
        "portable avatar asset must resolve"
    );
}

#[test]
fn save_as_sets_the_destination_for_later_saves() {
    let _guard = worker::WORKER_TEST_LOCK
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let dir = tempfile::tempdir().expect("tempdir");
    let mut state = save_test_state(dir.path(), None);
    state.scene.state.time = 0.25;
    state.scene.state.v[0].x = 0.125;
    let expected = dir.path().join("new-project.usd");
    assert_eq!(state.save_target(), None);
    state.save_project_to_path(&expected);
    assert_eq!(state.save_target(), Some(expected.as_path()));
    state.scene.state.time = 0.5;
    state.save_project();
    let text = std::fs::read_to_string(&expected).expect("read Save As destination");
    assert!(text.starts_with("#usda "));
    let reopened = FittingProject::load(&expected).expect("reopen saved USD");
    let scene = reopened
        .compile()
        .expect("compile reopened project with portable avatar");
    assert_eq!(scene.state.x, state.scene.state.x);
    assert_eq!(scene.state.v, state.scene.state.v);
    assert_eq!(scene.state.time, state.scene.state.time);
}

#[test]
fn rejected_save_leaves_the_previous_document_intact() {
    let _guard = worker::WORKER_TEST_LOCK
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let dir = tempfile::tempdir().expect("tempdir");
    let source = dir.path().join("garment.usda");
    let mut state = save_test_state(dir.path(), Some(source.clone()));
    state.project.save(&source).expect("seed source document");
    let before = std::fs::read(&source).expect("read source");

    state.scene.state.x[0].x = f64::NAN;
    state.save_project();
    assert!(
        state.status_message.starts_with("Save error for ")
            && state.status_message.contains(&source.display().to_string()),
        "{}",
        state.status_message
    );
    assert_eq!(std::fs::read(&source).expect("reread source"), before);
}

#[test]
fn open_replaces_the_document_and_the_shared_viewport() {
    let _guard = worker::WORKER_TEST_LOCK
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let dir = tempfile::tempdir().unwrap();
    let mut state = save_test_state(dir.path(), None);
    let old_panel = viewport_panel_path(&state.project.template.panels[0].id);
    state.history.record(state.project.template.clone());
    state.files.modified = true;
    state.playing = true;
    let avatar = dir.path().join("other.obj");
    std::fs::write(&avatar, "v 0 0 -20\nv 2 0 -20\nv 0 2 -20\nf 1 2 3\n").unwrap();
    let mut project = FittingProject::basic(GarmentKind::Dress, 1, avatar);
    project.template.seams.clear();
    project.template.panels.truncate(1);
    project.template.panels[0].id = "opened-panel".into();
    let mut saved_state = project.compile().unwrap().state;
    saved_state.time = 0.25;
    saved_state.v[0].x = 0.125;
    let settings = clothcraft::vbd::VbdSettings {
        dt: 1.0 / 120.0,
        ..Default::default()
    };
    project.simulation = Some(settings.clone());
    project.state = Some(saved_state.clone());
    let path = dir.path().join("other.usda");
    project.save(&path).unwrap();
    let stylesheet = load_studio_stylesheet().unwrap();
    let mut app = create_runner_app(state, &stylesheet);
    let viewport = app.state().usd_scene.clone().unwrap();
    let stage = viewport.stage();
    assert!(stage.prim(&old_panel).unwrap().is_valid().unwrap());

    let state = app.state_mut();
    let generation = state.generation;
    state.open_project_from_path(&path).unwrap();
    assert_eq!(state.files.source.as_deref(), Some(path.as_path()));
    assert!(!state.files.modified && !state.playing && !state.worker_pending);
    assert_eq!(state.generation, generation + 1);
    assert!(!state.history.can_undo());
    assert_eq!(state.scene.state.x, saved_state.x);
    assert_eq!(state.scene.state.v, saved_state.v);
    assert_eq!(state.scene.state.time, saved_state.time);
    assert_eq!(state.settings.dt, settings.dt);
    assert_eq!(state.scene.collider.surface.vertices[0].z, -20.0);
    assert!(!stage.prim(&old_panel).unwrap().is_valid().unwrap());
    assert!(
        stage
            .prim(viewport_panel_path("opened-panel"))
            .unwrap()
            .is_valid()
            .unwrap()
    );

    let invalid = dir.path().join("invalid.usda");
    std::fs::write(&invalid, "invalid document").unwrap();
    assert!(state.open_project_from_path(&invalid).is_err());
    assert_eq!(state.save_target(), Some(path.as_path()));
    assert_eq!(state.generation, generation + 1);
}

#[test]
fn file_dropdown_routes_commands_and_dismisses_without_editing() {
    let _guard = worker::WORKER_TEST_LOCK
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let dir = tempfile::tempdir().unwrap();
    let stylesheet = load_studio_stylesheet().unwrap();
    let mut app = create_runner_app(save_test_state(dir.path(), None), &stylesheet);
    let mut frame = FrameInfo {
        delta: Duration::ZERO,
        frame_index: 0,
    };
    app.set_viewport(ViewportSize::new(1024, 768));
    app.state_mut().window_size = (1024.0, 768.0);
    app.frame(frame);
    assert!(app.find_element_bounds_by_id("#runner-file-menu").is_none());

    let toggle = app
        .find_element_bounds_by_id("#runner-file-toggle")
        .unwrap();
    assert!(dispatch_click(
        app.scene(),
        toggle.x + toggle.width / 2.0,
        toggle.y + toggle.height / 2.0
    ));
    frame.frame_index += 1;
    app.frame(frame);
    let menu = app.find_element_bounds_by_id("#runner-file-menu").unwrap();
    assert!(menu.y >= toggle.y + toggle.height);
    for (id, expected) in [
        ("#runner-open-btn", RunnerAction::Open),
        ("#runner-save-btn", RunnerAction::Save),
        ("#runner-save-as-btn", RunnerAction::SaveAs),
        ("#runner-export-btn", RunnerAction::ExportSvg),
    ] {
        let item = app.find_element_bounds_by_id(id).unwrap();
        assert!(dispatch_click(
            app.scene(),
            item.x + item.width / 2.0,
            item.y + item.height / 2.0
        ));
        // Take the command before update would invoke the native picker.
        let mut actions = Vec::new();
        app.state().actions.drain_into(&mut actions);
        assert_eq!(actions, [expected]);
    }

    let (x, y) = (500.0, 400.0);
    app.handle_engine_event(&pointer_move_ev(x, y));
    app.handle_engine_event(&pointer_btn_ev(true));
    app.handle_engine_event(&pointer_btn_ev(false));
    assert!(!app.state().pointer_down);
    assert!(dispatch_click(app.scene(), x, y));
    frame.frame_index += 1;
    app.frame(frame);
    assert!(app.find_element_bounds_by_id("#runner-file-menu").is_none());
    assert!(!app.state().files.modified);

    for (key, shift, expected) in [
        ("o", false, RunnerAction::Open),
        ("s", false, RunnerAction::Save),
        ("S", true, RunnerAction::SaveAs),
    ] {
        assert!(handle_runner_event(
            app.state_mut(),
            &EngineEvent::Key(KeyboardEvent {
                logical_key: KeyIdentity::Character(key.into()),
                physical_key: None,
                location: KeyLocation::Standard,
                state: ButtonState::Pressed,
                repeat: false,
                modifiers: KeyboardModifiers {
                    control: true,
                    shift,
                    ..Default::default()
                },
            })
        ));
        let mut actions = Vec::new();
        app.state().actions.drain_into(&mut actions);
        assert_eq!(actions, [expected]);
    }
}
