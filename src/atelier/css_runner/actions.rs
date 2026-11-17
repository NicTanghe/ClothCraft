//! Queued UI actions, operation dispatch and frame updates.

use super::board::interaction::set_active_2d_tool;
use super::editing::{round_selected_corner, toggle_selected_curve, undo_redo_2d};
use super::state::{CssRunnerState, ObjectCategory};
use clothcraft::authoring::CornerRule;
use clothcraft::css_ui::ViewLayoutMode;
use clothcraft::fitting::FittingScene;
use clothcraft::garment::presets::{GarmentKind, basic as basic_garment};
use clothcraft::gpu_avbd::ContactMethod;
use cssimpler::app::Invalidation;
use cssimpler::renderer::FrameInfo;
use std::fs;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

pub const ACT_RUN_PLAY_PAUSE: u64 = 1 << 0;

pub const ACT_RUN_STEP: u64 = 1 << 1;

pub const ACT_RUN_RESET: u64 = 1 << 2;

pub const ACT_RUN_SAVE: u64 = 1 << 3;

pub const ACT_RUN_EXPORT: u64 = 1 << 4;

pub const ACT_RUN_CLEAN_VIEW: u64 = 1 << 5;

pub const ACT_RUN_PALETTE_TOGGLE: u64 = 1 << 6;

pub const ACT_RUN_THEME_TOGGLE: u64 = 1 << 7;

pub const ACT_RUN_VIEW_SPLIT: u64 = 1 << 8;

pub const ACT_RUN_VIEW_3D: u64 = 1 << 9;

pub const ACT_RUN_VIEW_2D: u64 = 1 << 10;

pub const ACT_RUN_DRAWER_TOGGLE: u64 = 1 << 11;

pub const ACT_RUN_TAB_PATTERN: u64 = 1 << 12;

pub const ACT_RUN_TAB_SEWING: u64 = 1 << 13;

pub const ACT_RUN_TAB_FORM: u64 = 1 << 14;

pub const ACT_RUN_TAB_MATERIAL: u64 = 1 << 15;

pub const ACT_RUN_TAB_ENV: u64 = 1 << 16;

pub const ACT_RUN_CAM_RESET: u64 = 1 << 17;

pub const ACT_RUN_PIECE_NEXT: u64 = 1 << 18;

pub const ACT_RUN_TOOL_SELECT: u64 = 1 << 19;

pub const ACT_RUN_TOOL_SEW: u64 = 1 << 20;

pub const ACT_RUN_TOOL_MEASURE: u64 = 1 << 21;

pub const ACT_RUN_TOOL_CORNER: u64 = 1 << 22;

pub const ACT_RUN_TOOL_GIZMO: u64 = 1 << 23;

pub const ACT_RUN_TOOL_GRAB: u64 = 1 << 24;

pub const ACT_RUN_TOOL_SEW3D: u64 = 1 << 25;

pub const ACT_RUN_TOOL_PIN: u64 = 1 << 26;

pub const ACT_RUN_TOOL_MEASURE3D: u64 = 1 << 27;

pub const ACT_RUN_TOOL_AVATAR_DIAG: u64 = 1 << 28;

pub const ACT_RUN_TOOL_EDIT2D: u64 = 1 << 29;

pub const ACT_RUN_TOOL_RECT2D: u64 = 1 << 30;

pub const ACT_RUN_TOOL_PEN2D: u64 = 1 << 31;

pub const ACT_RUN_TOOL_FREE2D: u64 = 1 << 32;

pub const ACT_RUN_TOOL_SEW2D: u64 = 1 << 33;

pub const ACT_RUN_TOOL_SPLIT2D: u64 = 1 << 34;

pub const ACT_RUN_TOOL_INTERNAL2D: u64 = 1 << 35;

pub const ACT_RUN_TOOL_MEASURE2D: u64 = 1 << 36;

pub const ACT_RUN_TOOL_TRIM2D: u64 = 1 << 37;

pub const ACT_RUN_TOOL_CORNER2D: u64 = 1 << 38;

pub const ACT_RUN_PIN_MODE_TOGGLE: u64 = 1 << 39;

pub const ACT_RUN_CORNER_MODE_TOGGLE: u64 = 1 << 40;

pub const ACT_RUN_ACC_0: u64 = 1 << 41;

pub const ACT_RUN_ACC_1: u64 = 1 << 42;

pub const ACT_RUN_ACC_2: u64 = 1 << 43;

pub const ACT_RUN_ACC_3: u64 = 1 << 44;

pub const ACT_RUN_ACC_4: u64 = 1 << 45;

pub const ACT_RUN_ACC_5: u64 = 1 << 46;

pub const ACT_RUN_ACCORDION_TOGGLE: u64 = 1 << 47;

pub const ACT_RUN_PIECE_0: u64 = 1 << 48;

pub const ACT_RUN_PIECE_1: u64 = 1 << 49;

pub const ACT_RUN_PIECE_2: u64 = 1 << 50;

pub const ACT_RUN_PIECE_3: u64 = 1 << 51;

pub const ACT_RUN_SOLVER_MENU: u64 = 1 << 52;

pub const ACT_RUN_SOLVER_GPU: u64 = 1 << 53;

pub const ACT_RUN_SOLVER_CPU: u64 = 1 << 54;

pub const ACT_RUN_SOLVER_AL: u64 = 1 << 55;

pub const ACT_RUN_2D_ZOOM_IN: u64 = 1 << 56;

pub const ACT_RUN_2D_ZOOM_OUT: u64 = 1 << 57;

pub const ACT_RUN_2D_FIT: u64 = 1 << 58;

pub const ACT_RUN_2D_NEXT_PIECE: u64 = 1 << 59;

pub const ACT_RUN_TOOL_CORNER_SELECT2D: u64 = 1 << 60;

#[derive(Clone, Debug)]
pub enum RunnerUiOp {
    SetCornerMode(bool),
    SetCornerRule(CornerRule),
    AdjustCornerRadiusMm(f64),
    AdjustCornerTension(f64),
    ApplyCornerRoundSelected,
    ToggleSelectedCurve(bool),
    Undo2d,
    Redo2d,
    SelectPiece(usize),
    LoadPreset(GarmentKind),
    StatusNote(String),
}

pub fn update_runner(state: &mut CssRunnerState, frame: FrameInfo) -> Invalidation {
    // 1. Poll simulation worker result
    let done = state
        .worker_latest
        .lock()
        .ok()
        .and_then(|mut guard| guard.take());
    let mut had_result = false;
    if let Some(done) = done
        && done.generation == state.generation
    {
        state.worker_pending = false;
        match done.result {
            Ok((new_state, metrics)) => {
                had_result = true;
                state.scene.state = new_state;
                state.step_count += 1;
                state.step_ms = metrics.step_ms;
                let residual = metrics.barrier_al.as_ref().map_or_else(
                    || {
                        metrics
                            .residual_rms_n
                            .map_or("N/A".into(), |r| format!("{r:.1e} N RMS"))
                    },
                    |al| {
                        format!(
                            "{:.1e} N max, gap {:.1e} m",
                            al.original_force_max_n, al.primal_m
                        )
                    },
                );
                state.status_message = format!(
                    "Step {}: min dist {:.3} mm, residual {}, {:.1} ms",
                    state.step_count,
                    metrics.minimum_distance_m * 1000.,
                    residual,
                    metrics.step_ms
                );
                state.last_metrics = Some(metrics);
                state.sync_usd_points();
            }
            Err(e) => {
                state.playing = false;
                state.status_message = format!("Worker error: {e}");
                state.send_control_state(false);
            }
        }
    }

    state.cadence.update(
        std::time::Instant::now(),
        state.scene.state.time,
        state.generation,
        state.playing,
    );

    if let Some(perf) = state.perf.as_mut() {
        let dt = state.settings.dt;
        if let Some(report) = perf.tick(state.scene.state.time, frame.frame_index, dt, true) {
            let _ = fs::create_dir_all(&state.files.output);
            let perf_path = state.files.output.join("ui-perf.json");
            let _ = fs::write(
                &perf_path,
                serde_json::to_string_pretty(&report).unwrap_or_default(),
            );
            state.should_exit = true;
        } else if perf.expired() {
            state.should_exit = true;
        }
    }

    // 2. Process pending UI actions
    let actions = state.actions.swap(0, Ordering::Relaxed);
    let mut prev_piece = state.selected_piece_index;

    if actions & ACT_RUN_SOLVER_MENU != 0 {
        state.solver_menu_open = !state.solver_menu_open;
    }
    if actions & ACT_RUN_SOLVER_GPU != 0 {
        state.select_solver(true);
    }
    if actions & ACT_RUN_SOLVER_AL != 0 {
        state.select_solver_method(true, ContactMethod::BarrierAl);
    }
    if actions & ACT_RUN_SOLVER_CPU != 0 {
        state.select_solver(false);
    }
    if actions & ACT_RUN_PLAY_PAUSE != 0 {
        state.playing = !state.playing;
        state.send_control_state(false);
    }
    if actions & ACT_RUN_STEP != 0 {
        state.send_control_state(true);
    }
    if actions & ACT_RUN_RESET != 0 {
        state.reset_simulation();
    }
    if actions & ACT_RUN_SAVE != 0 {
        state.save_project();
    }
    if actions & ACT_RUN_EXPORT != 0 {
        state.export_artifacts();
    }
    if actions & ACT_RUN_CLEAN_VIEW != 0 {
        state.clean_view = !state.clean_view;
    }
    if actions & ACT_RUN_PALETTE_TOGGLE != 0 {
        state.command_palette_open = !state.command_palette_open;
    }
    if actions & ACT_RUN_THEME_TOGGLE != 0 {
        state.theme = state.theme.toggled();
    }
    if actions & ACT_RUN_VIEW_SPLIT != 0 {
        state.layout_mode = ViewLayoutMode::Split;
    }
    if actions & ACT_RUN_VIEW_3D != 0 {
        state.layout_mode = ViewLayoutMode::ThreeDOnly;
    }
    if actions & ACT_RUN_VIEW_2D != 0 {
        state.layout_mode = ViewLayoutMode::TwoDOnly;
    }
    if actions & ACT_RUN_DRAWER_TOGGLE != 0 {
        state.left_drawer_open = !state.left_drawer_open;
    }
    if actions & ACT_RUN_TAB_PATTERN != 0 {
        state.active_category = ObjectCategory::Pattern2D;
    }
    if actions & ACT_RUN_TAB_SEWING != 0 {
        state.active_category = ObjectCategory::Sewing;
    }
    if actions & ACT_RUN_TAB_FORM != 0 {
        state.active_category = ObjectCategory::Form3D;
    }
    if actions & ACT_RUN_TAB_MATERIAL != 0 {
        state.active_category = ObjectCategory::Material;
    }
    if actions & ACT_RUN_TAB_ENV != 0 {
        state.active_category = ObjectCategory::Environment;
    }
    if actions & ACT_RUN_CAM_RESET != 0 {
        state.camera = clothcraft::usd_scene::default_scene_camera(
            &state.scene.state.x,
            Some(&state.scene.collider.surface),
        );
        state.sync_usd_camera();
        state.status_message = "Reset 3D camera".to_string();
    }
    if actions & ACT_RUN_PIECE_NEXT != 0 {
        let count = state.project.template.panels.len().max(1);
        state.selected_piece_index = (state.selected_piece_index + 1) % count;
    }
    if actions & ACT_RUN_TOOL_GIZMO != 0 || actions & ACT_RUN_TOOL_SELECT != 0 {
        state.active_3d_tool = "Gizmo".to_string();
        state.status_message = "3D Tool: Gizmo — drag 3D viewport to orbit".to_string();
    }
    if actions & ACT_RUN_TOOL_GRAB != 0 {
        state.active_3d_tool = "Grab".to_string();
        state.status_message = "3D Tool: Grab".to_string();
    }
    if actions & ACT_RUN_TOOL_SEW3D != 0 {
        state.active_3d_tool = "3D Sew".to_string();
        state.active_category = ObjectCategory::Sewing;
        state.status_message = "3D Tool: 3D Sew (inspect seams in Object Browser)".to_string();
    }
    if actions & ACT_RUN_TOOL_PIN != 0 {
        state.active_3d_tool = "Pin".to_string();
        state.accordion_open[3] = true;
        state.open_accordion_idx = 3;
        state.status_message = "3D Tool: Pin".to_string();
    }
    if actions & ACT_RUN_TOOL_MEASURE3D != 0 {
        state.active_3d_tool = "Measure".to_string();
        state.status_message = "3D Tool: 3D Measure".to_string();
    }
    if actions & ACT_RUN_TOOL_AVATAR_DIAG != 0 {
        state.active_3d_tool = "Avatar Diagnostics".to_string();
        state.active_category = ObjectCategory::Environment;
        state.accordion_open[5] = true;
        state.open_accordion_idx = 5;
        state.status_message = "3D Tool: Avatar Diagnostics".to_string();
    }
    if actions & ACT_RUN_TOOL_EDIT2D != 0 {
        set_active_2d_tool(state, "Edit");
    }
    if actions & ACT_RUN_TOOL_RECT2D != 0 {
        set_active_2d_tool(state, "Rect");
    }
    if actions & ACT_RUN_TOOL_PEN2D != 0 {
        set_active_2d_tool(state, "Pen");
    }
    if actions & ACT_RUN_TOOL_FREE2D != 0 {
        set_active_2d_tool(state, "Freehand");
    }
    if actions & ACT_RUN_TOOL_SEW2D != 0 || actions & ACT_RUN_TOOL_SEW != 0 {
        set_active_2d_tool(state, "Sew");
    }
    if actions & ACT_RUN_TOOL_SPLIT2D != 0 {
        set_active_2d_tool(state, "Split");
    }
    if actions & ACT_RUN_TOOL_INTERNAL2D != 0 {
        set_active_2d_tool(state, "Internal Line");
    }
    if actions & ACT_RUN_TOOL_MEASURE2D != 0 || actions & ACT_RUN_TOOL_MEASURE != 0 {
        set_active_2d_tool(state, "Measure");
    }
    if actions & ACT_RUN_TOOL_TRIM2D != 0 {
        set_active_2d_tool(state, "Trim");
    }
    if actions & ACT_RUN_TOOL_CORNER_SELECT2D != 0 {
        set_active_2d_tool(state, "Corner Select");
    }
    if actions & ACT_RUN_TOOL_CORNER2D != 0 || actions & ACT_RUN_TOOL_CORNER != 0 {
        set_active_2d_tool(state, "Corner");
    }
    if actions & ACT_RUN_PIN_MODE_TOGGLE != 0 {
        state.pin_mode_contour = !state.pin_mode_contour;
    }
    if actions & ACT_RUN_CORNER_MODE_TOGGLE != 0 {
        state.corner_mode_radius = !state.corner_mode_radius;
        state.corner_preview_active = true;
    }
    if actions & ACT_RUN_ACC_0 != 0 {
        state.accordion_open[0] = !state.accordion_open[0];
        state.open_accordion_idx = 0;
    }
    if actions & ACT_RUN_ACC_1 != 0 {
        state.accordion_open[1] = !state.accordion_open[1];
        state.open_accordion_idx = 1;
    }
    if actions & ACT_RUN_ACC_2 != 0 {
        state.accordion_open[2] = !state.accordion_open[2];
        state.open_accordion_idx = 2;
    }
    if actions & ACT_RUN_ACC_3 != 0 {
        state.accordion_open[3] = !state.accordion_open[3];
        state.open_accordion_idx = 3;
    }
    if actions & ACT_RUN_ACC_4 != 0 {
        state.accordion_open[4] = !state.accordion_open[4];
        state.open_accordion_idx = 4;
    }
    if actions & ACT_RUN_ACC_5 != 0 {
        state.accordion_open[5] = !state.accordion_open[5];
        state.open_accordion_idx = 5;
    }
    if actions & ACT_RUN_ACCORDION_TOGGLE != 0
        && (actions
            & (ACT_RUN_ACC_0
                | ACT_RUN_ACC_1
                | ACT_RUN_ACC_2
                | ACT_RUN_ACC_3
                | ACT_RUN_ACC_4
                | ACT_RUN_ACC_5)
            == 0)
    {
        state.open_accordion_idx = (state.open_accordion_idx + 1) % 6;
        state.accordion_open[state.open_accordion_idx] =
            !state.accordion_open[state.open_accordion_idx];
    }
    if actions & ACT_RUN_PIECE_0 != 0 && !state.project.template.panels.is_empty() {
        state.selected_piece_index = 0;
    }
    if actions & ACT_RUN_PIECE_1 != 0 && state.project.template.panels.len() > 1 {
        state.selected_piece_index = 1;
    }
    if actions & ACT_RUN_PIECE_2 != 0 && state.project.template.panels.len() > 2 {
        state.selected_piece_index = 2;
    }
    if actions & ACT_RUN_PIECE_3 != 0 && state.project.template.panels.len() > 3 {
        state.selected_piece_index = 3;
    }
    if actions & ACT_RUN_2D_ZOOM_IN != 0 {
        state.board_2d_zoom = (state.board_2d_zoom * 1.25).clamp(0.25, 10.0);
    }
    if actions & ACT_RUN_2D_ZOOM_OUT != 0 {
        state.board_2d_zoom = (state.board_2d_zoom / 1.25).clamp(0.25, 10.0);
    }
    if actions & ACT_RUN_2D_FIT != 0 {
        state.board_2d_pan = (0.0, 0.0);
        state.board_2d_zoom = 1.0;
    }
    if actions & ACT_RUN_2D_NEXT_PIECE != 0 {
        let count = state.project.template.panels.len().max(1);
        state.selected_piece_index = (state.selected_piece_index + 1) % count;
        if let Some(p) = state
            .project
            .template
            .panels
            .get(state.selected_piece_index)
        {
            state.status_message = format!("Selected piece: {} ({})", p.name, p.id);
        }
    }
    if state.selected_piece_index != prev_piece {
        prev_piece = state.selected_piece_index;
        let _ = prev_piece;
        state.clear_2d_subselection();
    }

    let ui_ops: Vec<RunnerUiOp> = state
        .pending_ui_ops
        .lock()
        .map(|mut g| std::mem::take(&mut *g))
        .unwrap_or_default();
    let had_ui_ops = !ui_ops.is_empty();
    for op in ui_ops {
        apply_runner_ui_op(state, op);
    }

    if actions != 0 || had_ui_ops || (had_result && !state.playing) {
        Invalidation::Structure
    } else if had_result {
        Invalidation::Paint
    } else {
        Invalidation::Clean
    }
}

pub(super) fn queue_action(
    actions: &Arc<AtomicU64>,
    action: u64,
) -> impl Fn() + Send + Sync + 'static {
    let actions = Arc::clone(actions);
    move || {
        actions.fetch_or(action, Ordering::Relaxed);
    }
}

pub(super) fn queue_ui_op(
    ops: &Arc<Mutex<Vec<RunnerUiOp>>>,
    op: RunnerUiOp,
) -> impl Fn() + Send + Sync + 'static {
    let ops = Arc::clone(ops);
    move || {
        if let Ok(mut guard) = ops.lock() {
            guard.push(op.clone());
        }
    }
}

pub(super) fn apply_runner_ui_op(state: &mut CssRunnerState, op: RunnerUiOp) {
    match op {
        RunnerUiOp::SetCornerMode(radius) => {
            state.corner_mode_radius = radius;
            state.corner_preview_active = true;
            state.status_message = if radius {
                "Corner mode: Radius (circular fillet)".into()
            } else {
                format!("Corner mode: Shape (tension {:.2})", state.corner_tension)
            };
        }
        RunnerUiOp::SetCornerRule(rule) => {
            state.corner_rule = rule;
            state.corner_preview_active = true;
            state.status_message = format!("Continuity rule: {:?}", rule);
        }
        RunnerUiOp::AdjustCornerRadiusMm(delta) => {
            state.corner_radius_mm = (state.corner_radius_mm + delta).clamp(1.0, 250.0);
            state.corner_preview_active = true;
            state.status_message = format!("Corner radius: {:.1} mm", state.corner_radius_mm);
        }
        RunnerUiOp::AdjustCornerTension(delta) => {
            state.corner_tension = (state.corner_tension + delta).clamp(0.05, 1.5);
            state.corner_mode_radius = false;
            state.corner_preview_active = true;
            state.status_message = format!("Corner tension: {:.2}", state.corner_tension);
        }
        RunnerUiOp::ApplyCornerRoundSelected => {
            round_selected_corner(state);
        }
        RunnerUiOp::ToggleSelectedCurve(curved) => {
            toggle_selected_curve(state, curved);
        }
        RunnerUiOp::Undo2d => {
            undo_redo_2d(state, false);
        }
        RunnerUiOp::Redo2d => {
            undo_redo_2d(state, true);
        }
        RunnerUiOp::SelectPiece(idx) => {
            if let Some(p) = state.project.template.panels.get(idx) {
                let msg = format!("Selected piece: {} ({})", p.name, p.id);
                state.selected_piece_index = idx;
                state.clear_2d_subselection();
                state.status_message = msg;
            }
        }
        RunnerUiOp::LoadPreset(kind) => {
            let subdivisions = state
                .project
                .template
                .panels
                .first()
                .and_then(|p| p.polygon.as_ref())
                .map_or(1, |poly| poly.subdivisions);
            let next_template = basic_garment(kind, subdivisions);
            match next_template.compile() {
                Ok(compiled) => {
                    let old = state.project.template.clone();
                    state.history.record(old);
                    state.preview_template = None;
                    state.drag_2d = None;
                    state.draft_2d = None;
                    state.hovered_2d = None;
                    state.selected_piece_index = 0;
                    state.clear_2d_subselection();
                    state.board_2d_pan = (0.0, 0.0);
                    state.board_2d_zoom = 1.0;

                    let next_garment = Arc::new(compiled);
                    let next_state = next_garment.initial_state.clone();
                    state.project.template = next_template;
                    state.project.state = None;
                    state.project.mesh_signature = None;
                    state.generation = state.generation.wrapping_add(1);
                    state.worker_pending = false;
                    state.playing = false;
                    state.step_count = 0;
                    state.scene = FittingScene {
                        garment: Arc::clone(&next_garment),
                        collider: Arc::clone(&state.scene.collider),
                        state: next_state.clone(),
                        quasistatic: state.scene.quasistatic,
                        next_dt: state.scene.next_dt,
                    };
                    state.initial_scene = FittingScene {
                        garment: next_garment,
                        collider: Arc::clone(&state.scene.collider),
                        state: next_state,
                        quasistatic: state.scene.quasistatic,
                        next_dt: state.scene.next_dt,
                    };
                    state.camera = clothcraft::usd_scene::default_scene_camera(
                        &state.scene.state.x,
                        Some(&state.scene.collider.surface),
                    );
                    if let Some(ref usd) = state.usd_scene {
                        let _ = clothcraft::usd_scene::update_panel_viewport_mesh(
                            usd,
                            &state.scene.garment,
                            &state.scene.state,
                        );
                    }
                    state.sync_usd_camera();
                    state.send_control_state(false);
                    state.status_message = format!("Loaded {} preset", kind.name());
                }
                Err(err) => {
                    state.status_message = format!("Preset compile error: {err}");
                }
            }
        }
        RunnerUiOp::StatusNote(msg) => {
            state.status_message = msg;
        }
    }
}
