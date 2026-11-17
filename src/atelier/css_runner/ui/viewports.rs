//! Stage, viewport hosts and responsive CAD toolbar.

use super::super::actions::{
    ACT_RUN_2D_FIT, ACT_RUN_2D_NEXT_PIECE, ACT_RUN_2D_ZOOM_IN, ACT_RUN_2D_ZOOM_OUT, RunnerUiOp,
    queue_action, queue_ui_op,
};
use super::super::board::geometry::{
    cad_2d_bar_layout, compute_2d_viewport_rects, viewport_2d_span_x,
};
use super::super::board::native::board_node;
use super::super::board::rendering::render_runner_2d_paths;
use super::super::board::selection::{compute_connected_2d_highlight, effective_selected_points};
use super::super::state::CssRunnerState;
use clothcraft::authoring::CornerRule;
use cssimpler::core::Node;
use cssimpler::ui;

pub(in super::super) fn build_runner_stage(state: &CssRunnerState) -> Node {
    let stage_class = format!("stage {}", state.layout_mode.stage_class());

    ui! {
        <main id="viewport-stage" class={stage_class}>
            {build_runner_3d_viewport(state)}
            {build_runner_2d_viewport(state)}
        </main>
    }
}

pub(in super::super) fn build_runner_3d_viewport(_state: &CssRunnerState) -> Node {
    ui! {
        <div id="viewport-3d" class="viewport-container">
            <div class="viewport-header">
                <span class="viewport-title">"3D Garment & Avatar"</span>
                <span class="viewport-sub">"USD native 3D viewport (VBD + OGC)"</span>
            </div>
            <div class="canvas-area">
                <div id="runner-viewport-3d-scene" class="viewport"></div>
            </div>
        </div>
    }
}

pub fn build_runner_2d_viewport(state: &CssRunnerState) -> Node {
    let (paths, view_box) = render_runner_2d_paths(state);
    let (_canvas_rect, svg_rect) = compute_2d_viewport_rects(state);
    let board = board_node(paths, view_box, svg_rect.w, svg_rect.h);
    let connected = compute_connected_2d_highlight(state);

    let panels = &state.project.template.panels;
    let selected_panel = panels.get(state.selected_piece_index);
    let selected_name = selected_panel.map_or("None", |p| p.name.as_str());
    let (w_mm, h_mm) = selected_panel.map_or((0.0, 0.0), |p| {
        let (w, h) = if let Some(poly) = &p.polygon {
            let min_x = poly
                .points
                .iter()
                .map(|pt| pt.x)
                .fold(f64::INFINITY, f64::min);
            let max_x = poly
                .points
                .iter()
                .map(|pt| pt.x)
                .fold(f64::NEG_INFINITY, f64::max);
            let min_y = poly
                .points
                .iter()
                .map(|pt| pt.y)
                .fold(f64::INFINITY, f64::min);
            let max_y = poly
                .points
                .iter()
                .map(|pt| pt.y)
                .fold(f64::NEG_INFINITY, f64::max);
            (max_x - min_x, max_y - min_y)
        } else {
            (p.pattern.width, p.pattern.height)
        };
        (w * 1000.0, h * 1000.0)
    });

    let zoom_pct = (state.board_2d_zoom * 100.0).round() as i32;
    let linked_str = if connected.connected_panels.is_empty() {
        String::new()
    } else {
        let ids: Vec<&str> = connected
            .connected_panels
            .iter()
            .map(|s| s.as_str())
            .collect();
        format!(" +{}", ids.join(","))
    };
    let sel_pts = effective_selected_points(state);
    let has_selected_pts = !sel_pts.is_empty();
    let sub_detail = if sel_pts.len() > 1 {
        format!(" · {} corners selected{linked_str}", sel_pts.len())
    } else if let Some((pid, pt_idx)) = sel_pts.into_iter().next() {
        let edge_label = state
            .project
            .template
            .panels
            .iter()
            .find(|p| p.id == pid)
            .and_then(|p| p.polygon.as_ref())
            .and_then(|poly| poly.edges.get(pt_idx))
            .map_or("?", |e| e.as_str());
        format!(" · Corner #{pt_idx} ({edge_label}){linked_str}")
    } else if let Some((ref _pid, ref ename)) = state.selected_2d_edge {
        format!(" · Edge {ename}{linked_str}")
    } else {
        String::new()
    };
    let sub_text = format!(
        "Tool: {} · Zoom: {}% · Selected: {} ({:.0}×{:.0} mm){}",
        state.active_2d_tool, zoom_pct, selected_name, w_mm, h_mm, sub_detail
    );
    let corner_menu_active =
        state.active_2d_tool == "Corner" || (state.corner_preview_active && has_selected_pts);
    let corner_group_class = if corner_menu_active {
        "cad-2d-group corner-menu-active"
    } else {
        "cad-2d-group"
    };
    let corner_row_class = if corner_menu_active {
        "cad-2d-row corner-menu-active"
    } else {
        "cad-2d-row"
    };

    let radius_mode_class = if state.corner_mode_radius {
        "cad-2d-pill active"
    } else {
        "cad-2d-pill"
    };
    let shape_mode_class = if !state.corner_mode_radius {
        "cad-2d-pill active"
    } else {
        "cad-2d-pill"
    };
    let sharp_class = if state.corner_rule == CornerRule::Sharp {
        "cad-2d-pill active"
    } else {
        "cad-2d-pill"
    };
    let smooth_class = if state.corner_rule == CornerRule::Smooth {
        "cad-2d-pill active"
    } else {
        "cad-2d-pill"
    };
    let sym_class = if state.corner_rule == CornerRule::Symmetric {
        "cad-2d-pill active"
    } else {
        "cad-2d-pill"
    };
    let undo_label = if state.history.can_undo() {
        "Undo"
    } else {
        "Undo ·"
    };
    let redo_label = if state.history.can_redo() {
        "Redo"
    } else {
        "Redo ·"
    };

    let undo_btn = ui! {
        <button id="cad-undo-btn" class="cad-2d-pill" onclick_shared={queue_ui_op(&state.pending_ui_ops, RunnerUiOp::Undo2d)} title="Undo (Ctrl+Z)">{undo_label}</button>
    };
    let redo_btn = ui! {
        <button id="cad-redo-btn" class="cad-2d-pill" onclick_shared={queue_ui_op(&state.pending_ui_ops, RunnerUiOp::Redo2d)} title="Redo (Ctrl+Shift+Z)">{redo_label}</button>
    };
    let mode_radius_btn = ui! {
        <button id="cad-corner-radius-mode" class={radius_mode_class} onclick_shared={queue_ui_op(&state.pending_ui_ops, RunnerUiOp::SetCornerMode(true))} title="Circular fillet mode">"Radius"</button>
    };
    let mode_shape_btn = ui! {
        <button id="cad-corner-shape-mode" class={shape_mode_class} onclick_shared={queue_ui_op(&state.pending_ui_ops, RunnerUiOp::SetCornerMode(false))} title="Shaped tension fillet mode">"Shape"</button>
    };
    let radius_dec_btn = ui! {
        <button id="cad-radius-dec" class="cad-2d-pill" onclick_shared={queue_ui_op(&state.pending_ui_ops, RunnerUiOp::AdjustCornerRadiusMm(-2.0))} title="Decrease corner radius">"-2"</button>
    };
    let radius_val = ui! {
        <span id="cad-radius-readout" class="cad-2d-value">{format!("{:.1} mm", state.corner_radius_mm)}</span>
    };
    let radius_inc_btn = ui! {
        <button id="cad-radius-inc" class="cad-2d-pill" onclick_shared={queue_ui_op(&state.pending_ui_ops, RunnerUiOp::AdjustCornerRadiusMm(2.0))} title="Increase corner radius">"+2"</button>
    };
    let tension_dec_btn = ui! {
        <button id="cad-tension-dec" class="cad-2d-pill" onclick_shared={queue_ui_op(&state.pending_ui_ops, RunnerUiOp::AdjustCornerTension(-0.05))} title="Decrease tension">"T-"</button>
    };
    let tension_val = ui! {
        <span id="cad-tension-readout" class="cad-2d-value">{format!("{:.2}", state.corner_tension)}</span>
    };
    let tension_inc_btn = ui! {
        <button id="cad-tension-inc" class="cad-2d-pill" onclick_shared={queue_ui_op(&state.pending_ui_ops, RunnerUiOp::AdjustCornerTension(0.05))} title="Increase tension">"T+"</button>
    };
    let apply_round_btn = ui! {
        <button id="cad-apply-round-btn" class="cad-2d-pill primary" onclick_shared={queue_ui_op(&state.pending_ui_ops, RunnerUiOp::ApplyCornerRoundSelected)} title="Round selected corner (Enter)">"Round"</button>
    };
    let rule_sharp_btn = ui! {
        <button id="cad-rule-sharp" class={sharp_class} onclick_shared={queue_ui_op(&state.pending_ui_ops, RunnerUiOp::SetCornerRule(CornerRule::Sharp))} title="Sharp independent handles">"Sharp"</button>
    };
    let rule_smooth_btn = ui! {
        <button id="cad-rule-smooth" class={smooth_class} onclick_shared={queue_ui_op(&state.pending_ui_ops, RunnerUiOp::SetCornerRule(CornerRule::Smooth))} title="Smooth G1 collinear handles">"Smooth"</button>
    };
    let rule_sym_btn = ui! {
        <button id="cad-rule-sym" class={sym_class} onclick_shared={queue_ui_op(&state.pending_ui_ops, RunnerUiOp::SetCornerRule(CornerRule::Symmetric))} title="Symmetric equal-length handles">"Sym"</button>
    };
    let curve_on_btn = ui! {
        <button id="cad-curve-on-btn" class="cad-2d-pill" onclick_shared={queue_ui_op(&state.pending_ui_ops, RunnerUiOp::ToggleSelectedCurve(true))} title="Convert selected boundary/corner to cubic Bézier">"Curve"</button>
    };
    let curve_off_btn = ui! {
        <button id="cad-curve-off-btn" class="cad-2d-pill" onclick_shared={queue_ui_op(&state.pending_ui_ops, RunnerUiOp::ToggleSelectedCurve(false))} title="Straighten selected boundary/corner">"Straight"</button>
    };

    let (_vp2d_x, vp2d_w) = viewport_2d_span_x(state);
    let (cad_rows, _bar_h) = cad_2d_bar_layout(vp2d_w);
    let cad_bar = match cad_rows {
        1 => ui! {
            <div id="cad-2d-bar" class="cad-2d-bar">
                <div class="cad-2d-group">
                    {undo_btn}
                    {redo_btn}
                </div>
                <div class={corner_group_class}>
                    <span class="cad-2d-label">"Corner"</span>
                    {mode_radius_btn}
                    {mode_shape_btn}
                    {radius_dec_btn}
                    {radius_val}
                    {radius_inc_btn}
                    {tension_dec_btn}
                    {tension_val}
                    {tension_inc_btn}
                    {apply_round_btn}
                </div>
                <div class="cad-2d-group">
                    <span class="cad-2d-label">"Rule"</span>
                    {rule_sharp_btn}
                    {rule_smooth_btn}
                    {rule_sym_btn}
                    {curve_on_btn}
                    {curve_off_btn}
                </div>
            </div>
        },
        2 => ui! {
            <div id="cad-2d-bar" class="cad-2d-bar rows-2">
                <div class="cad-2d-row">
                    {undo_btn}
                    {redo_btn}
                    <span class="cad-2d-label">"Rule"</span>
                    {rule_sharp_btn}
                    {rule_smooth_btn}
                    {rule_sym_btn}
                    {curve_on_btn}
                    {curve_off_btn}
                </div>
                <div class={corner_row_class}>
                    <span class="cad-2d-label">"Corner"</span>
                    {mode_radius_btn}
                    {mode_shape_btn}
                    {radius_dec_btn}
                    {radius_val}
                    {radius_inc_btn}
                    {tension_dec_btn}
                    {tension_val}
                    {tension_inc_btn}
                    {apply_round_btn}
                </div>
            </div>
        },
        3 => ui! {
            <div id="cad-2d-bar" class="cad-2d-bar rows-3">
                <div class={corner_row_class}>
                    {undo_btn}
                    {redo_btn}
                    {mode_radius_btn}
                    {mode_shape_btn}
                    {apply_round_btn}
                </div>
                <div class={corner_row_class}>
                    {radius_dec_btn}
                    {radius_val}
                    {radius_inc_btn}
                    {tension_dec_btn}
                    {tension_val}
                    {tension_inc_btn}
                </div>
                <div class="cad-2d-row">
                    {rule_sharp_btn}
                    {rule_smooth_btn}
                    {rule_sym_btn}
                    {curve_on_btn}
                    {curve_off_btn}
                </div>
            </div>
        },
        4 => ui! {
            <div id="cad-2d-bar" class="cad-2d-bar rows-4">
                <div class={corner_row_class}>
                    {undo_btn}
                    {redo_btn}
                    {mode_radius_btn}
                    {mode_shape_btn}
                </div>
                <div class={corner_row_class}>
                    {radius_dec_btn}
                    {radius_val}
                    {radius_inc_btn}
                    {apply_round_btn}
                </div>
                <div class={corner_row_class}>
                    {tension_dec_btn}
                    {tension_val}
                    {tension_inc_btn}
                    {curve_on_btn}
                    {curve_off_btn}
                </div>
                <div class="cad-2d-row">
                    {rule_sharp_btn}
                    {rule_smooth_btn}
                    {rule_sym_btn}
                </div>
            </div>
        },
        _ => ui! {
            <div id="cad-2d-bar" class="cad-2d-bar rows-7">
                <div class="cad-2d-row">{undo_btn}{redo_btn}</div>
                <div class={corner_row_class}>{mode_radius_btn}{mode_shape_btn}</div>
                <div class={corner_row_class}>{radius_dec_btn}{radius_val}{radius_inc_btn}</div>
                <div class={corner_row_class}>{apply_round_btn}</div>
                <div class={corner_row_class}>{tension_dec_btn}{tension_val}{tension_inc_btn}</div>
                <div class="cad-2d-row">{curve_on_btn}{curve_off_btn}</div>
                <div class="cad-2d-row">{rule_sharp_btn}{rule_smooth_btn}{rule_sym_btn}</div>
            </div>
        },
    };

    ui! {
        <div id="viewport-2d" class="viewport-container">
            <div class="viewport-header">
                <div class="viewport-header-left">
                    <span class="viewport-title">"2D Pattern Board"</span>
                    <span class="viewport-sub">{sub_text}</span>
                </div>
                <div class="viewport-tools">
                    <button class="viewport-tool-btn" onclick_shared={queue_action(&state.actions, ACT_RUN_2D_ZOOM_IN)} title="Zoom In (+)">"+"</button>
                    <button class="viewport-tool-btn" onclick_shared={queue_action(&state.actions, ACT_RUN_2D_ZOOM_OUT)} title="Zoom Out (-)">"-"</button>
                    <button class="viewport-tool-btn" onclick_shared={queue_action(&state.actions, ACT_RUN_2D_FIT)} title="Fit Board (0)">"Fit"</button>
                    <button class="viewport-tool-btn" onclick_shared={queue_action(&state.actions, ACT_RUN_2D_NEXT_PIECE)} title="Cycle Piece (])">"Next Piece ▾"</button>
                </div>
            </div>
            {cad_bar}
            <div class="canvas-area">
                {board}
            </div>
        </div>
    }
}
