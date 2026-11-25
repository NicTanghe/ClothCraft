//! Board layout, metric geometry and coordinate mapping.

use super::super::state::CssRunnerState;
use super::types::Rect2d;
use clothcraft::authoring;
use clothcraft::css_ui::ViewLayoutMode;
use clothcraft::garment::GarmentTemplate;
use clothcraft::garment::curves::cubic;
use clothcraft::garment::polygon::PolygonPattern;
use clothcraft::garment::template::Panel;
use glam::DVec2;

/// Responsive row count and bar height `(rows, height_px)` for `#cad-2d-bar` given `#viewport-2d` width.
pub fn cad_2d_bar_layout(vp2d_w: f32) -> (usize, f32) {
    if vp2d_w >= 1300.0 {
        (1, 28.0)
    } else if vp2d_w >= 700.0 {
        (2, 52.0)
    } else if vp2d_w >= 420.0 {
        (3, 75.0)
    } else if vp2d_w >= 300.0 {
        (4, 98.0)
    } else {
        (7, 167.0)
    }
}

/// Exact CSS layout width and x-origin `(vp2d_x, vp2d_w)` for `#viewport-2d`.
pub fn viewport_2d_span_x(state: &CssRunnerState) -> (f32, f32) {
    let win_w = state.window_size.0.max(100.0);
    let left_w = 48.0_f32
        + if state.left_drawer_open && !state.clean_view {
            240.0
        } else {
            0.0
        };
    let right_w = if state.clean_view {
        0.0_f32
    } else {
        48.0 + 300.0
    };
    let stage_w = (win_w - left_w - right_w).max(1.0);
    match state.layout_mode {
        ViewLayoutMode::TwoDOnly => (left_w, stage_w),
        ViewLayoutMode::ThreeDOnly => (0.0, 0.0),
        ViewLayoutMode::Split => {
            let half = stage_w * 0.5;
            (left_w + half + 1.0, (half - 1.0).max(1.0))
        }
    }
}

/// Exact CSS layout rect for the full 2D pattern board.
pub fn compute_2d_viewport_rect(state: &CssRunnerState) -> Rect2d {
    let win_h = state.window_size.1.max(100.0);
    let body_y = 40.0_f32;
    let body_h = (win_h - 40.0 - 26.0).max(1.0);
    if matches!(state.layout_mode, ViewLayoutMode::ThreeDOnly) {
        return Rect2d::default();
    }
    let (vp2d_x, vp2d_w) = viewport_2d_span_x(state);

    // `.viewport-header` (32px) + `.cad-2d-bar` (`bar_h`)
    let (_rows, bar_h) = cad_2d_bar_layout(vp2d_w);
    let top_chrome_h = 32.0_f32 + bar_h;
    Rect2d {
        x: vp2d_x,
        y: body_y + top_chrome_h,
        w: vp2d_w,
        h: (body_h - top_chrome_h).max(1.0),
    }
}

pub(in super::super) fn board_status_rect(canvas: Rect2d) -> Rect2d {
    // Match the 26px buttons, 4px gap and offset in `.board-status-bar`.
    Rect2d {
        x: canvas.x + 8.0,
        y: canvas.y + 8.0,
        w: 56.0,
        h: 26.0,
    }
}

/// Exact CSS layout rect for `#viewport-3d`'s `.canvas-area` (for 3D camera orbit gating).
pub fn viewport_3d_rect(state: &CssRunnerState) -> Rect2d {
    let win_w = state.window_size.0.max(100.0);
    let win_h = state.window_size.1.max(100.0);
    let body_y = 40.0_f32;
    let body_h = (win_h - 40.0 - 26.0).max(1.0);

    let left_w = 48.0_f32
        + if state.left_drawer_open && !state.clean_view {
            240.0
        } else {
            0.0
        };
    let right_w = if state.clean_view {
        0.0_f32
    } else {
        48.0 + 300.0
    };
    let stage_w = (win_w - left_w - right_w).max(1.0);

    let vp3d_w = match state.layout_mode {
        ViewLayoutMode::TwoDOnly => return Rect2d::default(),
        ViewLayoutMode::ThreeDOnly => stage_w,
        ViewLayoutMode::Split => stage_w * 0.5,
    };

    Rect2d {
        x: left_w,
        y: body_y + 32.0,
        w: vp3d_w,
        h: (body_h - 32.0).max(1.0),
    }
}

pub(in super::super) fn panel_raw_points(panel: &Panel) -> Vec<DVec2> {
    if let Some(ref poly) = panel.polygon
        && !poly.points.is_empty()
    {
        return poly.points.clone();
    }
    let w = panel.pattern.width.max(0.1);
    let h = panel.pattern.height.max(0.1);
    vec![
        DVec2::ZERO,
        DVec2::new(w, 0.0),
        DVec2::new(w, h),
        DVec2::new(0.0, h),
    ]
}

pub(in super::super) fn panel_to_board_pt(
    template: &GarmentTemplate,
    idx: usize,
    local: DVec2,
) -> DVec2 {
    let origin = authoring::layout(template, idx);
    let rot = template.panels[idx].layout_rotation_degrees;
    origin + authoring::rotate_2d(local, rot)
}

pub(in super::super) fn board_to_panel_pt(
    template: &GarmentTemplate,
    idx: usize,
    board: DVec2,
) -> DVec2 {
    let origin = authoring::layout(template, idx);
    let rot = template.panels[idx].layout_rotation_degrees;
    authoring::rotate_2d(board - origin, -rot)
}

/// Bounds of the cubic itself, invariant under subdivision of its control polygon.
fn cubic_bounds(p: [DVec2; 4]) -> (DVec2, DVec2) {
    let mut min = p[0].min(p[3]);
    let mut max = p[0].max(p[3]);
    let [d0, d1, d2] = [p[1] - p[0], p[2] - p[1], p[3] - p[2]];
    for axis in 0..2 {
        // The coordinate derivative divided by three is a*t² + b*t + c.
        let a = (d0 - 2.0 * d1 + d2)[axis];
        let b = (2.0 * (d1 - d0))[axis];
        let c = d0[axis];
        let roots = if a == 0.0 {
            [-c / b, f64::NAN]
        } else {
            let discriminant = b.mul_add(b, -4.0 * a * c).sqrt();
            // Choose the sign that avoids cancellation, then use the product of roots.
            let q = -0.5 * (b + discriminant.copysign(b));
            [q / a, c / q]
        };
        for t in roots {
            if t > 0.0 && t < 1.0 {
                let point = cubic(p, t);
                min = min.min(point);
                max = max.max(point);
            }
        }
    }
    (min, max)
}

/// Centre and span of a fitted view, captured independently of subsequent edits.
pub(in super::super) fn fit_2d_board(template: &GarmentTemplate) -> (DVec2, f64) {
    let mut min = DVec2::splat(f64::INFINITY);
    let mut max = DVec2::splat(f64::NEG_INFINITY);

    for (idx, panel) in template.panels.iter().enumerate() {
        let points = panel_raw_points(panel);
        let to_board = |p| panel_to_board_pt(template, idx, p);
        for &point in &points {
            let point = to_board(point);
            min = min.min(point);
            max = max.max(point);
        }
        if let Some(poly) = &panel.polygon {
            for (edge_idx, edge) in poly.edges.iter().enumerate() {
                if let Some(&[c1, c2]) = poly.curves.get(edge) {
                    let end = (edge_idx + 1) % points.len();
                    let (lo, hi) =
                        cubic_bounds([points[edge_idx], c1, c2, points[end]].map(to_board));
                    min = min.min(lo);
                    max = max.max(hi);
                }
            }
        }
    }

    if !min.is_finite() {
        min = DVec2::ZERO;
        max = DVec2::splat(0.6);
    }

    let span_x = (max.x - min.x).max(0.2);
    let span_y = (max.y - min.y).max(0.2);
    let max_span = span_x.max(span_y).max(0.4);
    let margin = (max_span * 0.12).max(0.06);
    ((min + max) * 0.5, max_span + 2.0 * margin)
}

/// Metric board viewBox `(min_x_m, min_y_m, width_m, height_m)` for the current camera.
pub fn compute_2d_view_box_m(state: &CssRunnerState) -> (f64, f64, f64, f64) {
    let (center, span) = state.board_2d_fit;

    let zoom = state.board_2d_zoom.clamp(0.25, 10.0);
    let rect = compute_2d_viewport_rect(state);
    let width = f64::from(rect.w.max(1.0));
    let height = f64::from(rect.h.max(1.0));
    let metres_per_pixel = span / (zoom * width.min(height));
    let width = width * metres_per_pixel;
    let height = height * metres_per_pixel;
    let vb_min_x = center.x + state.board_2d_pan.0 - width * 0.5;
    let vb_min_y = center.y + state.board_2d_pan.1 - height * 0.5;
    (vb_min_x, vb_min_y, width, height)
}

pub fn screen_to_board_m(state: &CssRunnerState, px: f32, py: f32) -> Option<DVec2> {
    let rect = compute_2d_viewport_rect(state);
    if !rect.contains(px, py) {
        return None;
    }
    screen_to_board_unclamped_m(state, px, py)
}

pub fn screen_to_board_unclamped_m(state: &CssRunnerState, px: f32, py: f32) -> Option<DVec2> {
    let rect = compute_2d_viewport_rect(state);
    if rect.w <= 0.0 || rect.h <= 0.0 {
        return None;
    }
    let (x, y, width, height) = compute_2d_view_box_m(state);
    let u = f64::from(px - rect.x) / f64::from(rect.w);
    let v = f64::from(py - rect.y) / f64::from(rect.h);
    Some(DVec2::new(x + u * width, y + v * height))
}

pub(in super::super) fn dist_point_to_segment(p: DVec2, a: DVec2, b: DVec2) -> f64 {
    let ab = b - a;
    let len_sq = ab.length_squared();
    if len_sq <= 1e-18 {
        return p.distance(a);
    }
    let t = ((p - a).dot(ab) / len_sq).clamp(0.0, 1.0);
    p.distance(a + ab * t)
}

pub(in super::super) fn point_in_poly_2d(p: DVec2, ring: &[DVec2]) -> bool {
    if ring.len() < 3 {
        return false;
    }
    let mut inside = false;
    let mut j = ring.len() - 1;
    for i in 0..ring.len() {
        let a = ring[i];
        let b = ring[j];
        if ((a.y > p.y) != (b.y > p.y)) && (p.x < (b.x - a.x) * (p.y - a.y) / (b.y - a.y) + a.x) {
            inside = !inside;
        }
        j = i;
    }
    inside
}

pub(in super::super) fn sample_polygon_boundary(poly: &PolygonPattern) -> Vec<DVec2> {
    let mut ring = Vec::new();
    for edge_idx in 0..poly.edges.len() {
        if let Ok(pts) = poly.edge_points(edge_idx)
            && pts.len() >= 2
        {
            ring.extend_from_slice(&pts[..pts.len() - 1]);
        }
    }
    ring
}

pub(in super::super) fn panel_edge_length_m(panel: &Panel, edge_name: &str) -> Option<f64> {
    let poly = panel.polygon.as_ref()?;
    let idx = poly.edges.iter().position(|e| e == edge_name)?;
    let pts = poly.edge_points(idx).ok()?;
    Some(pts.windows(2).map(|pair| pair[0].distance(pair[1])).sum())
}
