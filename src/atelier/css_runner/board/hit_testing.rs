//! Picking panels, edges, anchors and control handles.

use super::super::state::CssRunnerState;
use super::corners::{active_fillet_previews, fillet_drag_handle_local};
use super::geometry::{
    board_to_panel_pt, compute_2d_view_box_m, compute_2d_viewport_rects, dist_point_to_segment,
    panel_raw_points, panel_to_board_pt, point_in_poly_2d, sample_polygon_boundary,
};
use super::types::Hover2dTarget;
use clothcraft::garment::GarmentTemplate;
use glam::DVec2;

pub(in super::super) fn panel_order_selected_first(len: usize, selected: usize) -> Vec<usize> {
    let mut order = Vec::with_capacity(len);
    if selected < len {
        order.push(selected);
    }
    for i in 0..len {
        if i != selected {
            order.push(i);
        }
    }
    order
}

pub(in super::super) fn hit_test_2d(
    state: &CssRunnerState,
    board_m: DVec2,
) -> Option<Hover2dTarget> {
    let (_canvas, svg_rect) = compute_2d_viewport_rects(state);
    let (_vbx, _vby, view_span) = compute_2d_view_box_m(state);
    let px_m = view_span / (svg_rect.w.max(1.0) as f64);
    let hit_r = 10.0 * px_m;
    let template = state.active_template();

    // 1. Live Corner Fillet radius handles (including connected corners)
    for (panel_idx, edge_name, preview, _eff_r) in active_fillet_previews(state) {
        let knob_b = panel_to_board_pt(template, panel_idx, fillet_drag_handle_local(&preview));
        if board_m.distance(knob_b) <= hit_r * 1.35 {
            return Some(Hover2dTarget::CornerPreviewHandle {
                panel_idx,
                edge_name,
            });
        }
    }

    let order = panel_order_selected_first(template.panels.len(), state.selected_piece_index);

    // 2 & 3. Pick nearest among Bézier tangent handles and contour points (selected panel first)
    let mut best_handle: Option<(usize, usize, usize, f64)> = None;
    let mut best_point: Option<(usize, usize, f64)> = None;
    for &panel_idx in &order {
        let panel = &template.panels[panel_idx];
        if best_handle.is_none()
            && let Some(ref poly) = panel.polygon
        {
            for (edge_idx, edge_name) in poly.edges.iter().enumerate() {
                if let Some([c1, c2]) = poly.curves.get(edge_name) {
                    let d0 = board_m.distance(panel_to_board_pt(template, panel_idx, *c1));
                    if d0 <= hit_r * 1.25 && best_handle.is_none_or(|(_, _, _, bd)| d0 < bd) {
                        best_handle = Some((panel_idx, edge_idx, 0, d0));
                    }
                    let d1 = board_m.distance(panel_to_board_pt(template, panel_idx, *c2));
                    if d1 <= hit_r * 1.25 && best_handle.is_none_or(|(_, _, _, bd)| d1 < bd) {
                        best_handle = Some((panel_idx, edge_idx, 1, d1));
                    }
                }
            }
        }
        if best_point.is_none() {
            let pts = panel_raw_points(panel);
            for (pt_idx, pt) in pts.iter().enumerate() {
                let d = board_m.distance(panel_to_board_pt(template, panel_idx, *pt));
                if d <= hit_r && best_point.is_none_or(|(_, _, bd)| d < bd) {
                    best_point = Some((panel_idx, pt_idx, d));
                }
            }
        }
        if best_handle.is_some() || best_point.is_some() {
            break;
        }
    }
    match (best_handle, best_point) {
        (Some((hp, he, hi, hd)), Some((pp, pi, pd))) => {
            if pd <= hd * 1.15 {
                return Some(Hover2dTarget::Point {
                    panel_idx: pp,
                    point_idx: pi,
                });
            } else {
                return Some(Hover2dTarget::BezierHandle {
                    panel_idx: hp,
                    edge_idx: he,
                    handle_index: hi,
                });
            }
        }
        (Some((hp, he, hi, _)), None) => {
            return Some(Hover2dTarget::BezierHandle {
                panel_idx: hp,
                edge_idx: he,
                handle_index: hi,
            });
        }
        (None, Some((pp, pi, _))) => {
            return Some(Hover2dTarget::Point {
                panel_idx: pp,
                point_idx: pi,
            });
        }
        (None, None) => {}
    }

    // 4. Contour edges across panels
    let edge_hit_r = 8.0 * px_m;
    for &panel_idx in &order {
        let panel = &template.panels[panel_idx];
        if let Some(ref poly) = panel.polygon {
            let mut best: Option<(usize, f64)> = None;
            for edge_idx in 0..poly.edges.len() {
                if let Ok(sampled) = poly.edge_points(edge_idx) {
                    let mut min_d = f64::INFINITY;
                    for pair in sampled.windows(2) {
                        let a = panel_to_board_pt(template, panel_idx, pair[0]);
                        let b = panel_to_board_pt(template, panel_idx, pair[1]);
                        min_d = min_d.min(dist_point_to_segment(board_m, a, b));
                    }
                    if min_d <= edge_hit_r && best.is_none_or(|(_, bd)| min_d < bd) {
                        best = Some((edge_idx, min_d));
                    }
                }
            }
            if let Some((edge_idx, _)) = best {
                return Some(Hover2dTarget::Edge {
                    panel_idx,
                    edge_idx,
                });
            }
        }
    }

    // 5. Panel interiors
    for &panel_idx in &order {
        let panel = &template.panels[panel_idx];
        let ring: Vec<DVec2> = if let Some(ref poly) = panel.polygon {
            sample_polygon_boundary(poly)
                .into_iter()
                .map(|p| panel_to_board_pt(template, panel_idx, p))
                .collect()
        } else {
            panel_raw_points(panel)
                .into_iter()
                .map(|p| panel_to_board_pt(template, panel_idx, p))
                .collect()
        };
        if point_in_poly_2d(board_m, &ring) {
            return Some(Hover2dTarget::Panel(panel_idx));
        }
    }

    None
}

pub(in super::super) fn nearest_edge_anchor(
    template: &GarmentTemplate,
    board_m: DVec2,
    max_dist_m: f64,
) -> Option<(String, String, f64)> {
    let mut best: Option<(f64, String, String, f64)> = None;
    for (pidx, panel) in template.panels.iter().enumerate() {
        let Some(ref poly) = panel.polygon else {
            continue;
        };
        for (eidx, ename) in poly.edges.iter().enumerate() {
            let Ok(sampled) = poly.edge_points(eidx) else {
                continue;
            };
            let local = board_to_panel_pt(template, pidx, board_m);
            let mut total_len = 0.0_f64;
            let mut best_s = 0.0_f64;
            let mut min_d = f64::INFINITY;
            for pair in sampled.windows(2) {
                let seg = pair[1] - pair[0];
                let seg_l = seg.length();
                let t = if seg_l > 1e-12 {
                    ((local - pair[0]).dot(seg) / (seg_l * seg_l)).clamp(0.0, 1.0)
                } else {
                    0.0
                };
                let d = local.distance(pair[0] + seg * t);
                if d < min_d {
                    min_d = d;
                    best_s = total_len + seg_l * t;
                }
                total_len += seg_l;
            }
            if min_d <= max_dist_m && total_len > 1e-12 && best.as_ref().is_none_or(|b| min_d < b.0)
            {
                best = Some((
                    min_d,
                    panel.id.clone(),
                    ename.clone(),
                    (best_s / total_len).clamp(0.0, 1.0),
                ));
            }
        }
    }
    best.map(|(_, pid, ename, t)| (pid, ename, t))
}
