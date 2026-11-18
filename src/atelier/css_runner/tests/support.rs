//! Shared headless fixtures and pointer-event helpers.

use super::super::super::{DocumentFiles, worker};
use super::super::board::geometry::{compute_2d_view_box_m, compute_2d_viewport_rect};
use super::super::board::native::BoardPath;
use super::super::state::CssRunnerState;
use clothcraft::fitting::FittingProject;
use clothcraft::garment::GarmentTemplate;
use clothcraft::garment::presets::GarmentKind;
use cssimpler::renderer::EngineEvent;
use glam::DVec2;
use std::fs;

pub(super) fn board_to_screen_px(state: &CssRunnerState, board_m: DVec2) -> (f32, f32) {
    let svg_rect = compute_2d_viewport_rect(state);
    let (vbx, vby, width, height) = compute_2d_view_box_m(state);
    let u = ((board_m.x - vbx) / width) as f32;
    let v = ((board_m.y - vby) / height) as f32;
    (svg_rect.x + u * svg_rect.w, svg_rect.y + v * svg_rect.h)
}

pub(super) fn pointer_move_ev(x: f32, y: f32) -> EngineEvent {
    EngineEvent::PointerMoved {
        position: cssimpler::renderer::PointerPosition { x, y },
        modifiers: cssimpler::renderer::KeyboardModifiers::default(),
    }
}

pub(super) fn pointer_btn_ev(down: bool) -> EngineEvent {
    pointer_btn_mod_ev(down, false, false)
}

pub(super) fn pointer_btn_mod_ev(down: bool, shift: bool, control: bool) -> EngineEvent {
    EngineEvent::PointerButton {
        button: cssimpler::renderer::PointerButton::Primary,
        state: if down {
            cssimpler::renderer::ButtonState::Pressed
        } else {
            cssimpler::renderer::ButtonState::Released
        },
        modifiers: cssimpler::renderer::KeyboardModifiers {
            shift,
            control,
            ..Default::default()
        },
    }
}

pub(super) fn count_class(nodes: &[BoardPath], target: &str) -> usize {
    nodes
        .iter()
        .filter(|p| p.class.split_whitespace().any(|c| c == target))
        .count()
}

pub(super) fn selection_test_state(dir: &std::path::Path) -> CssRunnerState {
    let avatar = dir.join("avatar.obj");
    fs::write(&avatar, "v 0 0 -10\nv 1 0 -10\nv 0 1 -10\nf 1 2 3\n").unwrap();
    let mut project = FittingProject::basic(GarmentKind::Tshirt, 0, avatar);
    project.template = GarmentTemplate::load(std::path::Path::new(
        "examples/templates/curved-panels.json",
    ))
    .unwrap();
    project.template.seams.clear();
    let scene = project.compile().unwrap();
    let files = DocumentFiles {
        output: dir.join("out"),
        source: None,
        modified: false,
    };
    let (tx, latest) = worker::start_cpu().unwrap();
    CssRunnerState::new(project, scene, files, tx, latest)
}
