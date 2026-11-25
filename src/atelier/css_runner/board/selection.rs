//! Shared corner/edge selection and sewn-piece highlights.

use super::super::state::CssRunnerState;
use super::corners::resolve_corner_edge;
use super::types::Hover2dTarget;
use clothcraft::authoring::corner_tool::{
    connected_corners, connected_edges, is_sewn_dart_point, is_sewn_dart_tip,
};
use clothcraft::garment::GarmentTemplate;

pub(in super::super) struct Connected2dHighlight {
    pub(in super::super) connected_panels: std::collections::BTreeSet<String>,
    pub(in super::super) selected_corners: std::collections::BTreeSet<(String, usize)>,
    pub(in super::super) hovered_corners: std::collections::BTreeSet<(String, usize)>,
    pub(in super::super) selected_edges: std::collections::BTreeSet<(String, String)>,
    pub(in super::super) hovered_edges: std::collections::BTreeSet<(String, String)>,
    pub(in super::super) weld_links: Vec<((String, usize), (String, usize))>,
}

pub(in super::super) fn effective_selected_points(state: &CssRunnerState) -> Vec<(String, usize)> {
    if state.selected_2d_points.len() > 1 {
        let mut pts = state.selected_2d_points.clone();
        if let Some(ref single) = state.selected_2d_point
            && !pts.contains(single)
        {
            pts.push(single.clone());
        }
        pts
    } else if let Some(ref single) = state.selected_2d_point {
        vec![single.clone()]
    } else {
        Vec::new()
    }
}

pub(in super::super) fn has_single_dart_point(state: &CssRunnerState) -> bool {
    state.selected_2d_points.len() <= 1
        && state
            .selected_2d_point
            .as_ref()
            .is_some_and(|(panel, point)| {
                is_sewn_dart_point(&state.project.template, panel, *point)
            })
}

pub(in super::super) fn has_single_slide_point(state: &CssRunnerState) -> bool {
    let template = &state.project.template;
    state.selected_2d_points.len() <= 1
        && state
            .selected_2d_point
            .as_ref()
            .is_some_and(|(panel, point)| {
                resolve_corner_edge(template, panel, *point).is_some()
                    && !is_sewn_dart_tip(template, panel, *point)
            })
}

pub(in super::super) fn batch_corners_for_points(
    template: &GarmentTemplate,
    points: &[(String, usize)],
) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for (pid, pt_idx) in points {
        if let Some(pair) = resolve_corner_edge(template, pid, *pt_idx)
            && !out.contains(&pair)
        {
            out.push(pair);
        }
    }
    out
}

pub(in super::super) fn compute_connected_2d_highlight(
    state: &CssRunnerState,
) -> Connected2dHighlight {
    let template = state.active_template();
    let primary_panel_id = template
        .panels
        .get(state.selected_piece_index)
        .map(|p| p.id.as_str())
        .unwrap_or("");
    let mut connected_panels = std::collections::BTreeSet::new();
    let mut selected_corners = std::collections::BTreeSet::new();
    let mut hovered_corners = std::collections::BTreeSet::new();
    let mut selected_edges = std::collections::BTreeSet::new();
    let mut hovered_edges = std::collections::BTreeSet::new();
    let mut weld_links = Vec::new();
    let mut seen_weld_links = std::collections::BTreeSet::new();

    let sel_pts = effective_selected_points(state);
    if !sel_pts.is_empty() {
        for (pid, pt_idx) in sel_pts {
            if pid != primary_panel_id {
                connected_panels.insert(pid.clone());
            }
            let corners = connected_corners(template, &pid, pt_idx);
            for c in corners {
                if c.panel != pid || c.point != pt_idx {
                    let a = (pid.clone(), pt_idx);
                    let b = (c.panel.clone(), c.point);
                    let key = if a <= b {
                        (a.clone(), b.clone())
                    } else {
                        (b.clone(), a.clone())
                    };
                    if seen_weld_links.insert(key) {
                        weld_links.push((a, b));
                    }
                }
                if c.panel != primary_panel_id {
                    connected_panels.insert(c.panel.clone());
                }
                selected_corners.insert((c.panel, c.point));
            }
        }
    } else if let Some((ref pid, ref ename)) = state.selected_2d_edge {
        for ce in connected_edges(template, pid, ename) {
            if ce.panel != primary_panel_id {
                connected_panels.insert(ce.panel.clone());
            }
            selected_edges.insert((ce.panel, ce.edge));
        }
    }

    match &state.hovered_2d {
        Some(Hover2dTarget::Point {
            panel_idx,
            point_idx,
        }) => {
            if let Some(hp) = template.panels.get(*panel_idx) {
                for c in connected_corners(template, &hp.id, *point_idx) {
                    hovered_corners.insert((c.panel, c.point));
                }
            }
        }
        Some(Hover2dTarget::CornerPreviewHandle {
            panel_idx,
            edge_name,
        }) => {
            if let Some(hp) = template.panels.get(*panel_idx)
                && let Some(poly) = hp.polygon.as_ref()
                && let Some(pt_idx) = poly.edges.iter().position(|e| e == edge_name)
            {
                for c in connected_corners(template, &hp.id, pt_idx) {
                    hovered_corners.insert((c.panel, c.point));
                }
            }
        }
        Some(Hover2dTarget::Edge {
            panel_idx,
            edge_idx,
        })
        | Some(Hover2dTarget::BezierHandle {
            panel_idx,
            edge_idx,
            ..
        }) => {
            if let Some(hp) = template.panels.get(*panel_idx)
                && let Some(poly) = hp.polygon.as_ref()
                && let Some(ename) = poly.edges.get(*edge_idx)
            {
                for ce in connected_edges(template, &hp.id, ename) {
                    hovered_edges.insert((ce.panel, ce.edge));
                }
            }
        }
        Some(Hover2dTarget::Panel(_)) | None => {}
    }

    Connected2dHighlight {
        connected_panels,
        selected_corners,
        hovered_corners,
        selected_edges,
        hovered_edges,
        weld_links,
    }
}

pub(in super::super) fn sanitize_2d_subselection(state: &mut CssRunnerState) {
    if state.selected_piece_index >= state.project.template.panels.len() {
        state.selected_piece_index = 0;
    }
    let point_valid = |pid: &str, pt_idx: usize| -> bool {
        state
            .project
            .template
            .panels
            .iter()
            .find(|p| p.id == pid)
            .and_then(|p| p.polygon.as_ref())
            .is_some_and(|poly| pt_idx < poly.points.len())
    };
    let mut cleaned = Vec::with_capacity(state.selected_2d_points.len());
    for (pid, pt_idx) in state.selected_2d_points.drain(..) {
        if point_valid(&pid, pt_idx) && !cleaned.contains(&(pid.clone(), pt_idx)) {
            cleaned.push((pid, pt_idx));
        }
    }
    state.selected_2d_points = cleaned;
    if let Some((ref pid, pt_idx)) = state.selected_2d_point {
        if !point_valid(pid, pt_idx) {
            state.sync_selected_point();
        } else if state.selected_2d_points.is_empty() {
            state.selected_2d_points.push((pid.clone(), pt_idx));
        }
    } else if !state.selected_2d_points.is_empty() {
        state.sync_selected_point();
    }
    if let Some((ref pid, ref edge_name)) = state.selected_2d_edge {
        let valid = state
            .project
            .template
            .panels
            .iter()
            .find(|p| &p.id == pid)
            .and_then(|p| p.polygon.as_ref())
            .is_some_and(|poly| poly.edges.iter().any(|e| e == edge_name));
        if !valid {
            state.selected_2d_edge = None;
        }
    }
}
