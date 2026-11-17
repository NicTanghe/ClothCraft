//! Board layout, metric geometry and coordinate mapping.

use super::super::state::CssRunnerState;
use super::types::Rect2d;
use clothcraft::authoring;
use clothcraft::css_ui::ViewLayoutMode;
use clothcraft::garment::GarmentTemplate;
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

/// Exact CSS layout rects for `#viewport-2d`'s `.canvas-area` and square `#board-2d`.
pub fn compute_2d_viewport_rects(state: &CssRunnerState) -> (Rect2d, Rect2d) {
    let win_h = state.window_size.1.max(100.0);
    let body_y = 40.0_f32;
    let body_h = (win_h - 40.0 - 26.0).max(1.0);
    if matches!(state.layout_mode, ViewLayoutMode::ThreeDOnly) {
        return (Rect2d::default(), Rect2d::default());
    }
    let (vp2d_x, vp2d_w) = viewport_2d_span_x(state);

    // `.viewport-header` (32px) + `.cad-2d-bar` (`bar_h`)
    let (_rows, bar_h) = cad_2d_bar_layout(vp2d_w);
    let top_chrome_h = 32.0_f32 + bar_h;
    let canvas_rect = Rect2d {
        x: vp2d_x,
        y: body_y + top_chrome_h,
        w: vp2d_w,
        h: (body_h - top_chrome_h).max(1.0),
    };
    let sq = canvas_rect.w.min(canvas_rect.h).floor().max(1.0);
    let svg_rect = Rect2d {
        x: canvas_rect.x + (canvas_rect.w - sq) * 0.5,
        y: canvas_rect.y + (canvas_rect.h - sq) * 0.5,
        w: sq,
        h: sq,
    };
    (canvas_rect, svg_rect)
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

/// Square 2D board viewBox `(min_x_m, min_y_m, span_m)` derived from `template` and pan/zoom.
pub fn compute_2d_view_box_m(state: &CssRunnerState) -> (f64, f64, f64) {
    let template = &state.project.template;
    let mut g_min_x = f64::INFINITY;
    let mut g_max_x = f64::NEG_INFINITY;
    let mut g_min_y = f64::INFINITY;
    let mut g_max_y = f64::NEG_INFINITY;

    for (idx, panel) in template.panels.iter().enumerate() {
        for pt in panel_raw_points(panel) {
            let b = panel_to_board_pt(template, idx, pt);
            g_min_x = g_min_x.min(b.x);
            g_max_x = g_max_x.max(b.x);
            g_min_y = g_min_y.min(b.y);
            g_max_y = g_max_y.max(b.y);
        }
        if let Some(ref poly) = panel.polygon {
            for [c1, c2] in poly.curves.values() {
                for c in [c1, c2] {
                    let b = panel_to_board_pt(template, idx, *c);
                    g_min_x = g_min_x.min(b.x);
                    g_max_x = g_max_x.max(b.x);
                    g_min_y = g_min_y.min(b.y);
                    g_max_y = g_max_y.max(b.y);
                }
            }
        }
    }

    if !g_min_x.is_finite() {
        g_min_x = 0.0;
        g_max_x = 0.6;
        g_min_y = 0.0;
        g_max_y = 0.6;
    }

    let span_x = (g_max_x - g_min_x).max(0.2);
    let span_y = (g_max_y - g_min_y).max(0.2);
    let max_span = span_x.max(span_y).max(0.4);
    let center_x = (g_min_x + g_max_x) * 0.5;
    let center_y = (g_min_y + g_max_y) * 0.5;
    let margin = (max_span * 0.12).max(0.06);
    let half_span = max_span * 0.5 + margin;

    let zoom = state.board_2d_zoom.clamp(0.25, 10.0);
    let view_span = (half_span * 2.0) / zoom;
    let vb_min_x = center_x + state.board_2d_pan.0 - view_span * 0.5;
    let vb_min_y = center_y + state.board_2d_pan.1 - view_span * 0.5;
    (vb_min_x, vb_min_y, view_span)
}

pub fn screen_to_board_m(state: &CssRunnerState, px: f32, py: f32) -> Option<DVec2> {
    let (_canvas, svg_rect) = compute_2d_viewport_rects(state);
    if !svg_rect.contains(px, py) {
        return None;
    }
    let (vb_x, vb_y, span) = compute_2d_view_box_m(state);
    let u = ((px - svg_rect.x) / svg_rect.w) as f64;
    let v = ((py - svg_rect.y) / svg_rect.h) as f64;
    Some(DVec2::new(vb_x + u * span, vb_y + v * span))
}

pub fn screen_to_board_unclamped_m(state: &CssRunnerState, px: f32, py: f32) -> Option<DVec2> {
    let (_canvas, svg_rect) = compute_2d_viewport_rects(state);
    if svg_rect.w <= 0.0 || svg_rect.h <= 0.0 {
        return None;
    }
    let (vb_x, vb_y, span) = compute_2d_view_box_m(state);
    let u = ((px - svg_rect.x) / svg_rect.w) as f64;
    let v = ((py - svg_rect.y) / svg_rect.h) as f64;
    Some(DVec2::new(vb_x + u * span, vb_y + v * span))
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
