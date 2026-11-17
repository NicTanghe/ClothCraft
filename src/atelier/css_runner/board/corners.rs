//! Connected fillet limits and live corner preview geometry.

use super::super::state::CssRunnerState;
use super::selection::{batch_corners_for_points, effective_selected_points};
use super::types::{Drag2d, Hover2dTarget};
use clothcraft::authoring::corner_tool::{
    BatchFilletValidation, CIRCLE_TENSION, FilletPreview, validate_fillet_batch,
    validate_fillet_preview,
};
use clothcraft::garment::{GarmentTemplate, curves};
use glam::DVec2;

pub(in super::super) fn clamp_fillet_radius_m(requested_mm: f64, max_radius_m: f64) -> Option<f64> {
    let max_allowed = max_radius_m * 0.98;
    if !max_allowed.is_finite() || max_allowed <= 1e-4 {
        return None;
    }
    let min_allowed = 0.0005_f64.min(max_allowed * 0.5);
    Some((requested_mm / 1000.0).clamp(min_allowed, max_allowed))
}

pub(in super::super) fn resolve_corner_edge(
    template: &GarmentTemplate,
    panel_id: &str,
    point_idx: usize,
) -> Option<(String, String)> {
    template
        .panels
        .iter()
        .find(|p| p.id == panel_id)
        .and_then(|p| p.polygon.as_ref())
        .and_then(|poly| poly.edges.get(point_idx))
        .map(|edge| (panel_id.to_owned(), edge.clone()))
}

pub(in super::super) fn connected_fillet_max_radius_m(
    template: &GarmentTemplate,
    panel_id: &str,
    edge_name: &str,
) -> Result<f64, String> {
    Ok(validate_fillet_preview(template, panel_id, edge_name, 1e-4, CIRCLE_TENSION)?.max_radius_m)
}

pub(in super::super) fn connected_batch_fillet_max_radius_m(
    template: &GarmentTemplate,
    corners: &[(String, String)],
) -> Result<BatchFilletValidation, String> {
    validate_fillet_batch(template, corners, 1e-4, CIRCLE_TENSION)
}

pub(in super::super) fn active_fillet_previews(
    state: &CssRunnerState,
) -> Vec<(usize, String, FilletPreview, f64)> {
    let template = state.active_template();
    let sel_pts = effective_selected_points(state);
    let dragging_corner = matches!(state.drag_2d, Some(Drag2d::CornerRadius { .. }));
    let preview_selected = state.corner_preview_active && !sel_pts.is_empty();
    if state.active_2d_tool != "Corner" && !preview_selected && !dragging_corner {
        return Vec::new();
    }
    let seed_corners: Vec<(String, String)> = match &state.drag_2d {
        Some(Drag2d::CornerRadius {
            panel_id,
            edge_name,
            batch_corners,
            ..
        }) => {
            if !batch_corners.is_empty() {
                batch_corners.clone()
            } else {
                vec![(panel_id.clone(), edge_name.clone())]
            }
        }
        _ if state.active_2d_tool != "Corner" => batch_corners_for_points(template, &sel_pts),
        _ => match &state.hovered_2d {
            Some(Hover2dTarget::CornerPreviewHandle {
                panel_idx,
                edge_name,
            }) => {
                let hovered_corner = template.panels.get(*panel_idx).and_then(|p| {
                    p.polygon.as_ref().and_then(|poly| {
                        poly.edges
                            .iter()
                            .position(|e| e == edge_name)
                            .map(|pt_idx| (p.id.clone(), pt_idx, edge_name.clone()))
                    })
                });
                if let Some((pid, pt_idx, ename)) = hovered_corner {
                    if sel_pts.len() > 1 && sel_pts.contains(&(pid.clone(), pt_idx)) {
                        batch_corners_for_points(template, &sel_pts)
                    } else {
                        vec![(pid, ename)]
                    }
                } else {
                    Vec::new()
                }
            }
            Some(Hover2dTarget::Point {
                panel_idx,
                point_idx,
            }) => {
                if let Some(p) = template.panels.get(*panel_idx) {
                    if sel_pts.len() > 1 && sel_pts.contains(&(p.id.clone(), *point_idx)) {
                        batch_corners_for_points(template, &sel_pts)
                    } else if let Some(pair) = resolve_corner_edge(template, &p.id, *point_idx) {
                        vec![pair]
                    } else {
                        Vec::new()
                    }
                } else {
                    Vec::new()
                }
            }
            _ => batch_corners_for_points(template, &sel_pts),
        },
    };
    if seed_corners.is_empty() {
        return Vec::new();
    }
    let Ok(base_val) = validate_fillet_batch(template, &seed_corners, 1e-4, CIRCLE_TENSION) else {
        return Vec::new();
    };
    let Some(eff_r) = clamp_fillet_radius_m(state.corner_radius_mm, base_val.max_radius_m) else {
        return Vec::new();
    };
    let tension = if state.corner_mode_radius {
        CIRCLE_TENSION
    } else {
        state.corner_tension
    };
    let Ok(val) = validate_fillet_batch(template, &seed_corners, eff_r, tension) else {
        return Vec::new();
    };

    let mut out = Vec::with_capacity(val.corners.len());
    for bc in val.corners {
        if let Some(c_pidx) = template.panels.iter().position(|p| p.id == bc.panel) {
            let mut prev = bc.preview;
            prev.max_radius_m = val.max_radius_m;
            let ename = prev.outgoing_edge.clone();
            out.push((c_pidx, ename, prev, eff_r));
        }
    }
    out
}

pub(in super::super) fn fillet_drag_handle_local(preview: &FilletPreview) -> DVec2 {
    curves::cubic(
        [
            preview.start,
            preview.control_1,
            preview.control_2,
            preview.end,
        ],
        0.5,
    )
}
