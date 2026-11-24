//! Picking panels, edges, anchors and control handles.

use super::super::state::CssRunnerState;
use super::corners::{active_fillet_previews, fillet_drag_handle_local};
use super::geometry::{
    board_to_panel_pt, compute_2d_view_box_m, compute_2d_viewport_rect, dist_point_to_segment,
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
    let svg_rect = compute_2d_viewport_rect(state);
    let (_vbx, _vby, view_span, _) = compute_2d_view_box_m(state);
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

pub(in super::super) fn measurement_pick(
    template: &GarmentTemplate,
    board: DVec2,
    tolerance: f64,
    selected: usize,
    owner: Option<&str>,
) -> Option<(
    usize,
    clothcraft::garment::polygon::MeasurementAnchor,
    DVec2,
)> {
    use clothcraft::garment::polygon::MeasurementAnchor;
    for idx in panel_order_selected_first(template.panels.len(), selected) {
        let panel = &template.panels[idx];
        if owner.is_some_and(|id| id != panel.id) {
            continue;
        }
        let Some(poly) = &panel.polygon else { continue };
        let local = board_to_panel_pt(template, idx, board);
        // Scan sampled segments once. Resolve a named station only for the winner.
        let mut nearest: Option<(&str, f64)> = None;
        let mut consider = |edge, points: &[DVec2]| {
            for pair in points.windows(2) {
                let distance = dist_point_to_segment(local, pair[0], pair[1]);
                if distance <= tolerance && nearest.is_none_or(|(_, d)| distance < d) {
                    nearest = Some((edge, distance));
                }
            }
        };
        for (i, edge) in poly.edges.iter().enumerate() {
            if let Ok(points) = poly.edge_points(i) {
                consider(edge.as_str(), &points);
            }
        }
        for hole in &poly.holes {
            for (i, edge) in hole.edges.iter().enumerate() {
                consider(
                    edge.as_str(),
                    &[hole.points[i], hole.points[(i + 1) % hole.points.len()]],
                );
            }
        }
        for line in &poly.lines {
            if let Ok(points) = line.samples() {
                consider(line.id.as_str(), &points);
            }
        }
        let anchor = if let Some((edge, _)) = nearest {
            let (station, _) = poly.measurement_anchor_at_point(edge, local).ok()?;
            let (edge, t) = station.station()?;
            if !poly.curves.contains_key(edge) && !poly.lines.iter().any(|l| l.id == edge) {
                MeasurementAnchor::fixed(edge.into(), t, template.measurement_precision()).ok()?
            } else {
                station
            }
        } else if point_in_poly_2d(local, &sample_polygon_boundary(poly))
            && !poly
                .holes
                .iter()
                .any(|h| point_in_poly_2d(local, &h.points))
        {
            poly.interior_anchor(local)?
        } else {
            continue;
        };
        let resolved = poly.measurement_anchor_position(&anchor).ok()?;
        return Some((idx, anchor, panel_to_board_pt(template, idx, resolved)));
    }
    None
}

#[cfg(test)]
mod measurement_tests {
    use super::*;
    use clothcraft::garment::polygon::MeasurementAnchor;

    #[test]
    fn rotated_board_pick_resolves_material_coordinates() {
        let mut template = GarmentTemplate::load(std::path::Path::new(
            "examples/templates/curved-panels.usda",
        ))
        .unwrap();
        template.panels[0].layout_rotation_degrees = 57.;
        template.panels[0].layout_2d = Some(DVec2::new(4., -3.));
        let local = DVec2::new(0.18, 0.25);
        let board = panel_to_board_pt(&template, 0, local);
        let owner = &template.panels[0].id;
        let (idx, anchor, projected) =
            measurement_pick(&template, board, 0.001, 0, Some(owner)).unwrap();
        assert_eq!(idx, 0);
        assert!(matches!(anchor, MeasurementAnchor::Barycentric { .. }));
        let poly = template.panels[0].polygon.as_ref().unwrap();
        assert!(
            poly.measurement_anchor_position(&anchor)
                .unwrap()
                .distance(local)
                < 1e-12
        );
        assert!(projected.distance(board) < 1e-12);
        assert!(
            measurement_pick(&template, board + DVec2::splat(3.), 0.001, 0, Some(owner)).is_none()
        );
    }
}
