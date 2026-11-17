//! 2D and 3D tool rails.

use super::super::actions::{RunnerAction, queue_action};
use super::super::state::CssRunnerState;
use clothcraft::css_ui::icons::{
    icon_cam_reset, icon_corner_2d, icon_corner_select_2d, icon_dress_form, icon_edit_2d,
    icon_freehand_2d, icon_gizmo, icon_grab, icon_internal_2d, icon_measure_2d, icon_measure_3d,
    icon_pin, icon_polygon_2d, icon_rect_2d, icon_sew_2d, icon_sew_3d, icon_sidebar_toggle,
    icon_split_2d, icon_trim_2d,
};
use cssimpler::core::Node;
use cssimpler::ui;

pub(in super::super) fn build_runner_3d_rail(state: &CssRunnerState) -> Node {
    let gizmo_class = if state.active_3d_tool == "Gizmo" {
        "tool-btn active"
    } else {
        "tool-btn"
    };
    let grab_class = if state.active_3d_tool == "Grab" {
        "tool-btn active"
    } else {
        "tool-btn"
    };
    let sew3d_class = if state.active_3d_tool == "3D Sew" {
        "tool-btn active"
    } else {
        "tool-btn"
    };
    let pin_class = if state.active_3d_tool == "Pin" {
        "tool-btn active"
    } else {
        "tool-btn"
    };
    let msr_class = if state.active_3d_tool == "Measure" {
        "tool-btn active"
    } else {
        "tool-btn"
    };
    let avatar_class = if state.active_3d_tool == "Avatar Diagnostics" {
        "tool-btn active"
    } else {
        "tool-btn"
    };

    ui! {
        <aside id="rail-3d" class="tool-rail tool-rail-3d">
            <button class={gizmo_class} onclick_shared={queue_action(&state.actions, RunnerAction::ToolGizmo)} title="Select & Arrange [Gizmo] (1)">
                {icon_gizmo()}
                <span class="tool-label">"Gizmo"</span>
            </button>
            <button class={grab_class} onclick_shared={queue_action(&state.actions, RunnerAction::ToolGrab)} title="Grab Cloth (Pull & Drag in Simulation) (W)">
                {icon_grab()}
                <span class="tool-label">"Grab"</span>
            </button>
            <button class={sew3d_class} onclick_shared={queue_action(&state.actions, RunnerAction::ToolSew3d)} title="3D Segment Sewing (N)">
                {icon_sew_3d()}
                <span class="tool-label">"3D Sew"</span>
            </button>
            <button class={pin_class} onclick_shared={queue_action(&state.actions, RunnerAction::ToolPin)} title="Pin Cloth [Click vertices] (P)">
                {icon_pin()}
                <span class="tool-label">"Pin"</span>
            </button>
            <button class={msr_class} onclick_shared={queue_action(&state.actions, RunnerAction::ToolMeasure3d)} title="3D Tape Measure (T)">
                {icon_measure_3d()}
                <span class="tool-label">"Measure"</span>
            </button>
            <button id="avatar-diagnostics-btn" class={avatar_class} onclick_shared={queue_action(&state.actions, RunnerAction::ToolAvatarDiag)} title="Avatar & Dress Form Diagnostics (Shift+A)">
                {icon_dress_form()}
                <span class="tool-label">"Avatar"</span>
            </button>
            <button class="tool-btn" onclick_shared={queue_action(&state.actions, RunnerAction::CamReset)} title="Reset Camera View [Framing] (0)">
                {icon_cam_reset()}
                <span class="tool-label">"Reset"</span>
            </button>
            <button class="tool-btn" onclick_shared={queue_action(&state.actions, RunnerAction::DrawerToggle)} title="Toggle Asset & Piece Drawer (Tab)">
                {icon_sidebar_toggle()}
                <span class="tool-label">"Drawer"</span>
            </button>
        </aside>
    }
}

pub(in super::super) fn build_runner_2d_rail(state: &CssRunnerState) -> Node {
    if state.clean_view {
        return ui! { <aside id="rail-2d" class="tool-rail tool-rail-2d collapsed"></aside> };
    }

    let edit_class = if state.active_2d_tool == "Edit" {
        "tool-btn active"
    } else {
        "tool-btn"
    };
    let rect_class = if state.active_2d_tool == "Rect" {
        "tool-btn active"
    } else {
        "tool-btn"
    };
    let pen_class = if state.active_2d_tool == "Pen" {
        "tool-btn active"
    } else {
        "tool-btn"
    };
    let free_class = if state.active_2d_tool == "Freehand" {
        "tool-btn active"
    } else {
        "tool-btn"
    };
    let sew_class = if state.active_2d_tool == "Sew" {
        "tool-btn active"
    } else {
        "tool-btn"
    };
    let split_class = if state.active_2d_tool == "Split" {
        "tool-btn active"
    } else {
        "tool-btn"
    };
    let line_class = if state.active_2d_tool == "Internal Line" {
        "tool-btn active"
    } else {
        "tool-btn"
    };
    let tape_class = if state.active_2d_tool == "Measure" {
        "tool-btn active"
    } else {
        "tool-btn"
    };
    let trim_class = if state.active_2d_tool == "Trim" {
        "tool-btn active"
    } else {
        "tool-btn"
    };
    let corner_select_class = if state.active_2d_tool == "Corner Select" {
        "tool-btn active"
    } else {
        "tool-btn"
    };
    let corner_class = if state.active_2d_tool == "Corner" {
        "tool-btn active"
    } else {
        "tool-btn"
    };

    ui! {
        <aside id="rail-2d" class="tool-rail tool-rail-2d">
            <button class={edit_class} onclick_shared={queue_action(&state.actions, RunnerAction::ToolEdit2d)} title="Edit Point / Transform (Z / A)">
                {icon_edit_2d()}
                <span class="tool-label">"Edit"</span>
            </button>
            <button class={rect_class} onclick_shared={queue_action(&state.actions, RunnerAction::ToolRect2d)} title="Draw Rectangle Pattern (S)">
                {icon_rect_2d()}
                <span class="tool-label">"Rect"</span>
            </button>
            <button class={pen_class} onclick_shared={queue_action(&state.actions, RunnerAction::ToolPen2d)} title="Draw Polygon Pattern (H)">
                {icon_polygon_2d()}
                <span class="tool-label">"Pen"</span>
            </button>
            <button class={free_class} onclick_shared={queue_action(&state.actions, RunnerAction::ToolFree2d)} title="Freehand Pattern Curve (F)">
                {icon_freehand_2d()}
                <span class="tool-label">"Free"</span>
            </button>
            <button class={sew_class} onclick_shared={queue_action(&state.actions, RunnerAction::ToolSew2d)} title="Sew Seams (B)">
                {icon_sew_2d()}
                <span class="tool-label">"Sew"</span>
            </button>
            <button class={split_class} onclick_shared={queue_action(&state.actions, RunnerAction::ToolSplit2d)} title="Split / Add Point (X)">
                {icon_split_2d()}
                <span class="tool-label">"Split"</span>
            </button>
            <button class={line_class} onclick_shared={queue_action(&state.actions, RunnerAction::ToolInternal2d)} title="Internal Polygon / Line (G)">
                {icon_internal_2d()}
                <span class="tool-label">"Line"</span>
            </button>
            <button class={tape_class} onclick_shared={queue_action(&state.actions, RunnerAction::ToolMeasure2d)} title="2D Tape Measure (M)">
                {icon_measure_2d()}
                <span class="tool-label">"Tape"</span>
            </button>
            <button class={trim_class} onclick_shared={queue_action(&state.actions, RunnerAction::ToolTrim2d)} title="Trim (T)">
                {icon_trim_2d()}
                <span class="tool-label">"Trim"</span>
            </button>
            <button id="tool-corner-select-2d" class={corner_select_class} onclick_shared={queue_action(&state.actions, RunnerAction::ToolCornerSelect2d)} title="Select Corners (V / Shift+Click multi)">
                {icon_corner_select_2d()}
                <span class="tool-label">"Select Corner"</span>
            </button>
            <button id="tool-corner-round-2d" class={corner_class} onclick_shared={queue_action(&state.actions, RunnerAction::ToolCorner2d)} title="Corner Rounding Menu & Fillet (C)">
                {icon_corner_2d()}
                <span class="tool-label">"Corner"</span>
            </button>
        </aside>
    }
}
