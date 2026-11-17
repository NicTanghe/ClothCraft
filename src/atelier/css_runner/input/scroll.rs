//! Pointer-anchored board zoom and 3D camera zoom.

use super::super::board::geometry::{
    compute_2d_view_box_m, compute_2d_viewport_rects, viewport_3d_rect,
};
use super::super::state::CssRunnerState;
use cssimpler::renderer::EngineEvent;

pub(in super::super) fn handle_scroll(state: &mut CssRunnerState, event: &EngineEvent) -> bool {
    match event {
        EngineEvent::Scroll { delta, .. } => {
            if state.command_palette_open || state.solver_menu_open {
                return false;
            }
            let dy = match delta {
                cssimpler::renderer::ScrollDelta::Lines { y, .. } => *y * 0.2,
                cssimpler::renderer::ScrollDelta::Pixels { y, .. } => *y * 0.005,
            };
            let (px, py) = state.last_pointer_pos;
            let (canvas_2d, svg_2d) = compute_2d_viewport_rects(state);
            let rect_3d = viewport_3d_rect(state);
            if canvas_2d.contains(px, py) {
                let (vbx, vby, old_span) = compute_2d_view_box_m(state);
                let u = if svg_2d.w > 0.0 {
                    ((px - svg_2d.x) / svg_2d.w) as f64 - 0.5
                } else {
                    0.0
                };
                let v = if svg_2d.h > 0.0 {
                    ((py - svg_2d.y) / svg_2d.h) as f64 - 0.5
                } else {
                    0.0
                };
                let anchor_x = vbx + (0.5 + u) * old_span;
                let anchor_y = vby + (0.5 + v) * old_span;
                let center_x = vbx + 0.5 * old_span - state.board_2d_pan.0;
                let center_y = vby + 0.5 * old_span - state.board_2d_pan.1;

                let old_zoom = state.board_2d_zoom.clamp(0.25, 10.0);
                let factor = (1.0 + dy as f64 * 0.15).clamp(0.5, 2.0);
                let new_zoom = (old_zoom * factor).clamp(0.25, 10.0);
                let new_span = old_span * (old_zoom / new_zoom);
                state.board_2d_zoom = new_zoom;
                if svg_2d.contains(px, py) {
                    state.board_2d_pan.0 = anchor_x - center_x - u * new_span;
                    state.board_2d_pan.1 = anchor_y - center_y - v * new_span;
                }
                return true;
            } else if rect_3d.contains(px, py) {
                state.camera.distance = (state.camera.distance - dy).clamp(0.5, 15.0);
                state.sync_usd_camera();
                return true;
            }
            false
        }
        _ => false,
    }
}
