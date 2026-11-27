//! The 2D Sew tool waits for an explicit easing choice before committing.

use super::super::super::{DocumentFiles, worker};
use super::super::actions::{RunnerUiOp, apply_runner_ui_op};
use super::super::board::geometry::panel_edge_length_m;
use super::super::board::rendering::render_runner_2d_paths;
use super::super::board::types::ToolDraft2d;
use super::super::editing::{
    commit_authoring_command, confirm_pending_sew, sew_lengths_equal, undo_redo_2d,
};
use super::super::input::handle_runner_event;
use super::super::state::CssRunnerState;
use super::support::{count_class, selection_test_state};
use clothcraft::authoring::Command;
use clothcraft::fitting::FittingProject;
use clothcraft::garment::GarmentTemplate;
use clothcraft::garment::presets::GarmentKind;
use cssimpler::renderer::EngineEvent;
use glam::DVec2;

fn named_key(name: &str) -> EngineEvent {
    EngineEvent::Key(cssimpler::renderer::KeyboardEvent {
        logical_key: cssimpler::renderer::KeyIdentity::Named(name.into()),
        physical_key: None,
        location: cssimpler::renderer::KeyLocation::Standard,
        state: cssimpler::renderer::ButtonState::Pressed,
        repeat: false,
        modifiers: cssimpler::renderer::KeyboardModifiers::default(),
    })
}

type EdgeLen = (String, String, f64);

/// One pair of boundaries on different panels whose lengths are equal (as the
/// seam compiler measures equality) and one pair that clearly differs.
fn sew_pairs(template: &GarmentTemplate) -> ((EdgeLen, EdgeLen), (EdgeLen, EdgeLen)) {
    let mut edges = Vec::new();
    for panel in &template.panels {
        if let Some(poly) = &panel.polygon {
            for edge in &poly.edges {
                if let Some(len) = panel_edge_length_m(panel, edge) {
                    edges.push((panel.id.clone(), edge.clone(), len));
                }
            }
        }
    }
    let mut equal = None;
    let mut unequal = None;
    for (i, x) in edges.iter().enumerate() {
        for y in &edges[i + 1..] {
            if x.0 == y.0 {
                continue;
            }
            if sew_lengths_equal(x.2, y.2) {
                equal.get_or_insert((x.clone(), y.clone()));
            } else if (x.2 - y.2).abs() > 1e-3 {
                unequal.get_or_insert((x.clone(), y.clone()));
            }
        }
    }
    (
        equal.expect("fixture has an equal-length pair"),
        unequal.expect("fixture has an unequal-length pair"),
    )
}

fn pending_sew(pair: &(EdgeLen, EdgeLen)) -> ToolDraft2d {
    ToolDraft2d::SewConfirm {
        a: (pair.0.0.clone(), pair.0.1.clone()),
        b: (pair.1.0.clone(), pair.1.1.clone()),
        reverse: false,
        a_len_m: pair.0.2,
        b_len_m: pair.1.2,
    }
}

#[test]
fn native_sewing_requires_an_explicit_easing_choice() {
    let _guard = worker::WORKER_TEST_LOCK
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let dir = tempfile::tempdir().expect("tempdir");
    let avatar = dir.path().join("avatar.obj");
    std::fs::write(&avatar, "v 0 0 -10\nv 1 0 -10\nv 0 1 -10\nf 1 2 3\n").expect("write avatar");
    let mut project = FittingProject::basic(GarmentKind::Tshirt, 1, avatar);
    // Free every boundary so the fixture's own seams cannot overlap the test seams.
    project.template.seams.clear();
    let scene = project.compile().expect("compile scene");
    let files = DocumentFiles {
        output: dir.path().join("out"),
        source: None,
        modified: false,
    };
    let (tx, latest) = worker::start_cpu().expect("start cpu worker");
    let mut state = CssRunnerState::new(project, scene, files, tx, latest);
    let (equal, unequal) = sew_pairs(&state.project.template);

    // Unequal lengths: plain confirmation (button or Enter) is refused and
    // keeps the draft; nothing is committed.
    state.draft_2d = Some(pending_sew(&unequal));
    assert!(!confirm_pending_sew(&mut state, false));
    assert!(handle_runner_event(&mut state, &named_key("Enter")));
    assert!(state.project.template.seams.is_empty());
    assert!(matches!(
        state.draft_2d,
        Some(ToolDraft2d::SewConfirm { .. })
    ));
    assert!(state.status_message.contains("Sew with easing"));

    // Escape cancels: document and history unchanged.
    assert!(handle_runner_event(&mut state, &named_key("Escape")));
    assert!(state.draft_2d.is_none());
    assert!(state.project.template.seams.is_empty());
    assert!(
        !undo_redo_2d(&mut state, false),
        "cancel must not create history"
    );

    // Cancel button behaves the same way.
    state.draft_2d = Some(pending_sew(&unequal));
    apply_runner_ui_op(&mut state, RunnerUiOp::CancelSew);
    assert!(state.draft_2d.is_none());
    assert!(state.project.template.seams.is_empty());

    // Explicit easing commits one eased seam as one undo entry.
    state.draft_2d = Some(pending_sew(&unequal));
    apply_runner_ui_op(&mut state, RunnerUiOp::ConfirmSew { ease: true });
    assert_eq!(
        state.project.template.seams.len(),
        1,
        "{}",
        state.status_message
    );
    assert!(state.project.template.seams[0].allow_ease);
    assert!(state.status_message.contains("easing"));
    assert!(undo_redo_2d(&mut state, false));
    assert!(state.project.template.seams.is_empty());

    // Equal lengths: Enter sews without easing.
    state.draft_2d = Some(pending_sew(&equal));
    assert!(handle_runner_event(&mut state, &named_key("Enter")));
    assert_eq!(
        state.project.template.seams.len(),
        1,
        "{}",
        state.status_message
    );
    assert!(!state.project.template.seams[0].allow_ease);
    assert!(state.draft_2d.is_none());
}

#[test]
fn mirror_sewing_previews_both_pairs_and_checks_reflected_easing_before_commit() {
    let _guard = worker::WORKER_TEST_LOCK
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let dir = tempfile::tempdir().unwrap();
    let mut state = selection_test_state(dir.path());
    let fabric = state.project.template.panels[0].fabric.clone();
    assert!(commit_authoring_command(
        &mut state,
        Command::Polygon {
            points: vec![
                DVec2::ZERO,
                DVec2::new(0.3, 0.),
                DVec2::new(0.3, 0.2),
                DVec2::new(0., 0.2),
            ],
            tangents: vec![],
            meshing: Default::default(),
            fabric,
            replace: true,
        },
        "Created test panel",
    ));
    let id = state.project.template.panels[0].id.clone();
    assert!(commit_authoring_command(
        &mut state,
        Command::Duplicate { panel: id },
        "Duplicated test panel",
    ));
    let panel = state.project.template.panels[1].id.clone();
    assert!(commit_authoring_command(
        &mut state,
        Command::Anchor {
            panel,
            anchor: 3,
            position: DVec2::new(0., 0.2001),
        },
        "Adjusted the reflected boundary",
    ));
    let side = |i: usize| {
        let panel = &state.project.template.panels[i];
        let edge = panel.polygon.as_ref().unwrap().edges[1].clone();
        let len = panel_edge_length_m(panel, &edge).unwrap();
        (panel.id.clone(), edge, len)
    };
    let pair = (side(0), side(1));
    assert!(sew_lengths_equal(pair.0.2, pair.1.2));
    state
        .mirrored_pieces
        .extend([pair.0.0.clone(), pair.1.0.clone()]);
    state.draft_2d = Some(pending_sew(&pair));
    let (paths, _) = render_runner_2d_paths(&state);
    assert_eq!(count_class(&paths, "tool-draft-sew"), 2);
    assert_eq!(count_class(&paths, "tool-draft-sew-reflected"), 2);
    assert!(!confirm_pending_sew(&mut state, false));
    assert!(state.status_message.contains("Sew with easing"));
    assert!(state.project.template.seams.is_empty());
    assert!(state.draft_2d.is_some());
    assert!(
        confirm_pending_sew(&mut state, true),
        "{}",
        state.status_message
    );
    assert_eq!(state.project.template.seams.len(), 2);
    assert!(state.project.template.seams[1].allow_ease);
    assert!(state.draft_2d.is_none());
    assert!(undo_redo_2d(&mut state, false));
    assert!(state.project.template.seams.is_empty());
}

/// Two equal straight edges with opposing boundary directions and a large
/// placement gap. The second piece starts rotated, as in a rough arrangement.
fn alignment_fixture(dir: &std::path::Path) -> (CssRunnerState, ToolDraft2d) {
    use clothcraft::garment::template::Placement;
    use glam::DVec3;

    let mut state = selection_test_state(dir);
    let fabric = state.project.template.panels[0].fabric.clone();
    assert!(commit_authoring_command(
        &mut state,
        Command::Polygon {
            points: vec![
                DVec2::ZERO,
                DVec2::new(0.3, 0.0),
                DVec2::new(0.3, 0.2),
                DVec2::new(0.0, 0.2),
            ],
            tangents: vec![],
            meshing: Default::default(),
            fabric,
            replace: true,
        },
        "Created rectangle",
    ));
    let first = state.project.template.panels[0].id.clone();
    assert!(commit_authoring_command(
        &mut state,
        Command::Duplicate { panel: first },
        "Duplicated rectangle",
    ));
    let second = state.project.template.panels[1].id.clone();
    assert!(commit_authoring_command(
        &mut state,
        Command::Place {
            panel: second,
            placement: Placement {
                translation: DVec3::new(0.8, 1.5, 0.7),
                rotation_degrees: DVec3::new(0.0, 0.0, 35.0),
            },
        },
        "Roughly placed second rectangle",
    ));
    let side = |idx: usize, edge: usize| {
        let panel = &state.project.template.panels[idx];
        (
            panel.id.clone(),
            panel.polygon.as_ref().unwrap().edges[edge].clone(),
        )
    };
    let draft = ToolDraft2d::SewConfirm {
        a: side(0, 1),
        b: side(1, 3),
        reverse: true,
        a_len_m: 0.2,
        b_len_m: 0.2,
    };
    (state, draft)
}

#[test]
fn align_and_sew_moves_the_whole_edge_before_the_first_solver_step() {
    use super::super::actions::{RunnerAction, apply_runner_action, update_runner};
    use clothcraft::fitting::seam_placement::APPROACH_FRAMES;
    use clothcraft::usd_scene::{
        VIEWPORT_GARMENT_PATH, garment_stage_usda, viewport_panel_path, viewport_scene_from_usda,
    };
    use clothcraft::vbd::VbdSolver;
    use cssimpler::renderer::{FrameInfo, gpu::openusd::gf};
    use std::{
        sync::{Arc, mpsc},
        time::Duration,
    };

    let _guard = worker::WORKER_TEST_LOCK
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    let dir = tempfile::tempdir().unwrap();
    let (mut state, draft) = alignment_fixture(dir.path());
    let authored = state.project.template.clone();
    let first_range = state.scene.garment.panels[0].vertices.clone();
    let first_before = state.scene.state.x[first_range.clone()].to_vec();
    state.draft_2d = Some(draft.clone());
    assert!(confirm_pending_sew(&mut state, false));
    let ordinary_gap = state.scene.garment.max_seam_gap(&state.scene.state.x);
    assert!(ordinary_gap > 0.5);
    assert!(undo_redo_2d(&mut state, false));

    let usd = viewport_scene_from_usda(
        &garment_stage_usda(&state.scene.garment, &state.scene.state).unwrap(),
    )
    .unwrap()
    .with_deforming_meshes(VIEWPORT_GARMENT_PATH)
    .unwrap();
    let panel_id = &authored.panels[1].id;
    let path = format!("{}.points", viewport_panel_path(panel_id));
    let read_points = || {
        usd.stage()
            .attribute(path.as_str())
            .unwrap()
            .get::<Vec<gf::Vec3f>>()
            .unwrap()
            .unwrap()
    };
    let initial_points = read_points();
    state.usd_scene = Some(usd.clone());

    // Observe the real transport controls without racing a background solve.
    let (send, receive) = mpsc::channel();
    state.worker_tx = send;
    state.draft_2d = Some(draft);
    apply_runner_ui_op(&mut state, RunnerUiOp::AlignAndSew { ease: false });
    assert!(state.seam_assembly.is_some(), "{}", state.status_message);
    assert_eq!(read_points(), initial_points);
    assert!(state.project.template.seams.is_empty());
    assert!(receive.try_iter().all(|work| !work.playing && !work.once));

    let frame = |index| FrameInfo {
        frame_index: index,
        delta: Duration::from_nanos(16_666_667),
    };
    apply_runner_action(&mut state, RunnerAction::PlayPause, false);
    for index in 1..=4 {
        update_runner(&mut state, frame(index));
    }
    let moved = read_points();
    assert_ne!(moved, initial_points);
    assert_eq!(state.scene.state.x[first_range.clone()], first_before);
    assert!(state.project.template.seams.is_empty());
    assert!(receive.try_iter().all(|work| !work.playing && !work.once));

    apply_runner_action(&mut state, RunnerAction::PlayPause, false);
    update_runner(&mut state, frame(5));
    assert_eq!(read_points(), moved);
    apply_runner_action(&mut state, RunnerAction::Step, false);
    update_runner(&mut state, frame(6));
    assert_ne!(read_points(), moved);
    assert!(receive.try_iter().all(|work| !work.playing && !work.once));

    apply_runner_action(&mut state, RunnerAction::PlayPause, false);
    for index in 7..=u64::from(APPROACH_FRAMES) + 1 {
        update_runner(&mut state, frame(index));
        assert!(state.project.template.seams.is_empty());
    }
    let meeting = read_points();
    update_runner(&mut state, frame(u64::from(APPROACH_FRAMES) + 2));
    let first_work = receive
        .try_iter()
        .find(|work| work.playing)
        .expect("worker starts at handoff");
    assert!(
        first_work
            .scene
            .garment
            .max_seam_gap(&first_work.scene.state.x)
            < 1e-8
    );
    assert!(state.seam_assembly.is_none());
    assert!(state.draft_2d.is_none());
    assert_eq!(state.project.template.seams.len(), 1);
    assert_eq!(state.scene.state.x[first_range], first_before);
    assert_eq!(
        state.project.template.panels[1]
            .polygon
            .as_ref()
            .unwrap()
            .points,
        authored.panels[1].polygon.as_ref().unwrap().points
    );
    let final_points = read_points();
    assert_eq!(meeting.len(), final_points.len());
    assert!(
        meeting
            .iter()
            .zip(&final_points)
            .all(|(a, b)| { ((a.x - b.x).abs() + (a.y - b.y).abs() + (a.z - b.z).abs()) < 1e-6 })
    );

    let mut solver = VbdSolver::new(
        Arc::clone(&state.scene.garment),
        Some(&state.scene.collider.surface),
        &state.scene.state,
    )
    .unwrap();
    let mut simulated = state.scene.state.clone();
    let metrics = solver.step(&mut simulated, &state.settings, None).unwrap();
    assert!(metrics.max_seam_gap_m < 0.004);
    assert!(simulated.time > state.scene.state.time);

    assert!(undo_redo_2d(&mut state, false));
    assert!(state.project.template.seams.is_empty());
    assert_eq!(
        state.project.template.panels[1].placement.translation,
        authored.panels[1].placement.translation
    );
    assert!(undo_redo_2d(&mut state, true));
    assert!(state.scene.garment.max_seam_gap(&state.scene.state.x) < 1e-8);
}

#[test]
fn align_and_sew_leaves_the_draft_when_startup_detects_an_intersection() {
    use clothcraft::avatar::{MeshCollider, SurfaceMesh};
    use std::sync::Arc;

    let _guard = worker::WORKER_TEST_LOCK
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    let dir = tempfile::tempdir().unwrap();
    let (mut state, draft) = alignment_fixture(dir.path());
    // A large fixed triangle slices through the stationary first panel.
    let avatar = dir.path().join("crossing.obj");
    std::fs::write(&avatar, "v -0.1 0 0\nv -0.1 3 0\nv -0.1 1.5 2\nf 1 2 3\n").unwrap();
    let surface = Arc::new(SurfaceMesh::load_obj(&avatar).unwrap());
    state.scene.collider =
        Arc::new(MeshCollider::new(surface, state.project.contact_stiffness).unwrap());
    let before = state.scene.state.x.clone();
    state.draft_2d = Some(draft);
    apply_runner_ui_op(&mut state, RunnerUiOp::AlignAndSew { ease: false });
    assert!(
        state
            .status_message
            .contains("The approach must start separated"),
        "{}",
        state.status_message
    );
    assert!(matches!(
        state.draft_2d,
        Some(ToolDraft2d::SewConfirm { .. })
    ));
    assert!(state.project.template.seams.is_empty());
    assert_eq!(state.scene.state.x, before);
    assert!(confirm_pending_sew(&mut state, false));
}

#[test]
fn join_on_play_rejects_a_close_fit_that_cannot_meet() {
    let _guard = worker::WORKER_TEST_LOCK
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    let dir = tempfile::tempdir().unwrap();
    let (mut state, mut draft) = alignment_fixture(dir.path());
    let ToolDraft2d::SewConfirm { b, b_len_m, .. } = &mut draft else {
        unreachable!();
    };
    assert!(commit_authoring_command(
        &mut state,
        Command::Anchor {
            panel: b.0.clone(),
            anchor: 3,
            position: DVec2::new(0.0, 0.199),
        },
        "Shortened second edge by 1 mm",
    ));
    *b_len_m = 0.199;
    state.draft_2d = Some(draft);
    apply_runner_ui_op(&mut state, RunnerUiOp::AlignAndSew { ease: true });
    assert!(
        state
            .status_message
            .contains("cannot meet along their full length"),
        "{}",
        state.status_message
    );
    assert!(state.seam_assembly.is_none());
    assert!(state.draft_2d.is_some());
    assert!(state.project.template.seams.is_empty());
}

#[test]
fn join_on_play_rejects_a_collision_between_start_and_contact() {
    use clothcraft::avatar::{MeshCollider, SurfaceMesh};
    use std::sync::Arc;

    let _guard = worker::WORKER_TEST_LOCK
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    let dir = tempfile::tempdir().unwrap();
    let (mut state, draft) = alignment_fixture(dir.path());
    // The wall lies between the pieces. Their start and joined poses clear
    // it, but the displayed rigid approach would pass through it.
    let avatar = dir.path().join("wall.obj");
    std::fs::write(
        &avatar,
        "v 0.5 -5 -5\nv 0.5 5 -5\nv 0.5 5 5\nv 0.5 -5 5\nf 1 2 3\nf 1 3 4\n",
    )
    .unwrap();
    let surface = Arc::new(SurfaceMesh::load_obj(&avatar).unwrap());
    state.scene.collider =
        Arc::new(MeshCollider::new(surface, state.project.contact_stiffness).unwrap());
    state.draft_2d = Some(draft);
    apply_runner_ui_op(&mut state, RunnerUiOp::AlignAndSew { ease: false });
    assert!(
        state
            .status_message
            .contains("approach or joined pose has a collision"),
        "{}",
        state.status_message
    );
    assert!(state.seam_assembly.is_none());
    assert!(state.draft_2d.is_some());
    assert!(state.project.template.seams.is_empty());
}
