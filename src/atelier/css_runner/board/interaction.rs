//! Tool switching and transient draft initialization.

use super::super::state::{CssRunnerState, ObjectCategory};

pub(in super::super) fn set_active_2d_tool(state: &mut CssRunnerState, tool: &str) {
    state.active_2d_tool = tool.to_string();
    state.last_2d_click = None;
    state.drag_2d = None;
    state.draft_2d = None;
    state.preview_template = None;
    match tool {
        "Edit" => {
            state.status_message =
                "2D Tool: Edit — double-click edges to split; Shift-drag to select points".into();
        }
        "Corner Select" => {
            state.accordion_open[1] = true;
            state.open_accordion_idx = 1;
            state.status_message =
                "2D Tool: Select Corners — click or Shift+click corners to multi-select".into();
        }
        "Rect" => {
            state.corner_preview_active = false;
            state.status_message = "2D Tool: Rect — drag on board to create rectangle".into();
        }
        "Pen" => {
            state.corner_preview_active = false;
            state.status_message =
                "2D Tool: Pen — click vertices on board, Enter or click start to close".into();
        }
        "Freehand" => {
            state.corner_preview_active = false;
            state.status_message = "2D Tool: Freehand — drag on board to sketch contour".into();
        }
        "Sew" => {
            state.corner_preview_active = false;
            state.active_category = ObjectCategory::Sewing;
            state.status_message = "2D Tool: Sew — click two edges to create a seam".into();
        }
        "Split" => {
            state.corner_preview_active = false;
            state.status_message =
                "2D Tool: Split — click a boundary edge to insert a point".into();
        }
        "Internal Line" => {
            state.corner_preview_active = false;
            state.status_message =
                "2D Tool: Internal Line — click points inside piece, Enter to finish".into();
        }
        "Measure" => {
            state.corner_preview_active = false;
            state.status_message =
                "2D Tool: Measure — click two points on board to measure distance".into();
        }
        "Trim" => {
            state.corner_preview_active = false;
            state.status_message =
                "2D Tool: Trim — click corner, sewn edge, or internal line to remove".into();
        }
        "Corner" => {
            state.corner_preview_active = true;
            state.accordion_open[1] = true;
            state.open_accordion_idx = 1;
            state.status_message =
                "2D Tool: Corner Rounding Menu — adjust settings to preview or click/drag corner to round"
                    .into();
        }
        _ => {}
    }
}
