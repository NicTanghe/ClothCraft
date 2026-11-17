//! USD camera and 3D navigation regressions.

use super::super::super::{DocumentFiles, worker};
use super::super::actions::ACT_RUN_CAM_RESET;
use super::super::app::create_runner_app;
use super::super::board::geometry::viewport_3d_rect;
use super::super::input::handle_runner_event;
use super::super::state::CssRunnerState;
use super::support::{pointer_btn_ev, pointer_move_ev};
use clothcraft::css_ui::load_studio_stylesheet;
use clothcraft::fitting::FittingProject;
use clothcraft::garment::presets::GarmentKind;
use cssimpler::renderer::{EngineEvent, FrameInfo};
use std::sync::atomic::Ordering;
use std::time::Duration;

#[test]
fn test_3d_camera_orbit_zoom_and_reset_usd() {
    let _guard = worker::WORKER_TEST_LOCK
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let stylesheet = load_studio_stylesheet().expect("load studio stylesheet");
    let dir = tempfile::tempdir().expect("tempdir");
    let avatar = dir.path().join("avatar.obj");
    std::fs::write(&avatar, "v 0 0 -10\nv 1 0 -10\nv 0 1 -10\nf 1 2 3\n").expect("write avatar");
    let project = FittingProject::basic(GarmentKind::Tshirt, 0, avatar);
    let scene = project.compile().expect("compile scene");
    let files = DocumentFiles {
        output: dir.path().join("out_cam"),
        source: None,
        modified: false,
    };
    let (tx, latest) = worker::start_cpu().expect("start cpu worker");
    let state = CssRunnerState::new(project, scene, files, tx, latest);
    let mut app = create_runner_app(state, &stylesheet);
    app.set_viewport(cssimpler::renderer::ViewportSize::new(1600, 980));
    app.frame(FrameInfo {
        delta: Duration::from_millis(16),
        frame_index: 0,
    });

    let read_xform = |st: &CssRunnerState| {
        st.usd_scene
            .as_ref()
            .unwrap()
            .stage()
            .attribute(format!(
                "{}.xformOp:transform",
                clothcraft::usd_scene::VIEWPORT_CAMERA_PATH
            ))
            .unwrap()
            .get::<cssimpler::renderer::gpu::openusd::gf::Matrix4d>()
            .unwrap()
            .unwrap()
            .0
    };
    let initial_xf = read_xform(app.state());
    let initial_yaw = app.state().camera.yaw;
    let initial_dist = app.state().camera.distance;

    let vp3d = viewport_3d_rect(app.state());
    let cx = vp3d.x + vp3d.w * 0.5;
    let cy = vp3d.y + vp3d.h * 0.5;
    handle_runner_event(app.state_mut(), &pointer_move_ev(cx, cy));
    handle_runner_event(app.state_mut(), &pointer_btn_ev(true));
    assert!(handle_runner_event(
        app.state_mut(),
        &pointer_move_ev(cx + 45.0, cy - 20.0)
    ));
    handle_runner_event(app.state_mut(), &pointer_btn_ev(false));
    assert!((app.state().camera.yaw - initial_yaw).abs() > 0.1);
    assert!(handle_runner_event(
        app.state_mut(),
        &EngineEvent::Scroll {
            delta: cssimpler::renderer::ScrollDelta::Lines { x: 0.0, y: 2.0 },
            modifiers: cssimpler::renderer::KeyboardModifiers::default(),
        }
    ));
    assert!((app.state().camera.distance - initial_dist).abs() > 0.1);
    assert_ne!(read_xform(app.state()), initial_xf);

    // Reset via ACT_RUN_CAM_RESET button in #rail-3d
    app.state_mut()
        .actions
        .fetch_or(ACT_RUN_CAM_RESET, Ordering::Relaxed);
    app.frame(FrameInfo {
        delta: Duration::from_millis(16),
        frame_index: 1,
    });
    assert!((app.state().camera.yaw - initial_yaw).abs() < 1e-6);
    assert!((app.state().camera.distance - initial_dist).abs() < 1e-6);
    assert_eq!(read_xform(app.state()), initial_xf);
}
