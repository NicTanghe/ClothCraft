//! Basic gray body silhouette for the 2D pattern board.
//!
//! This is an exploratory projection, not a fitted 2D/3D mapping. Avatar world
//! XY is placed using the first panel's board layout and 3D translation, so the
//! outline moves and zooms with the board but does not claim to align with every
//! panel's placement. Back faces are ignored; an edge between a front-facing and
//! a back-facing projected triangle is treated as a silhouette segment.

use clothcraft::avatar::SurfaceMesh;
use clothcraft::garment::GarmentTemplate;
use std::collections::HashMap;

/// Visible silhouette edges in board millimetres.
pub(super) fn build(surface: &SurfaceMesh, template: &GarmentTemplate) -> Vec<[(f64, f64); 2]> {
    let Some(panel) = template.panels.first() else {
        return Vec::new();
    };
    // Fixed front projection: world X, and world Y flipped because board Y grows
    // downward. Anchored to the first panel's board placement; no camera fitting
    // and no panel-by-panel alignment.
    let anchor_world = panel.placement.translation;
    let anchor_board = clothcraft::authoring::layout(template, 0) * 1000.0;
    let projected: Vec<(f64, f64)> = surface
        .vertices
        .iter()
        .map(|p| {
            (
                anchor_board.x + (p.x - anchor_world.x) * 1000.0,
                anchor_board.y - (p.y - anchor_world.y) * 1000.0,
            )
        })
        .collect();

    // Edge -> up to two adjacent triangle orientations. A sign change is a
    // silhouette; a single use is an open boundary. More than two uses are
    // non-manifold and ignored rather than guessed at.
    let mut edges: HashMap<(usize, usize), [Option<i8>; 2]> = HashMap::new();
    for triangle in &surface.triangles {
        let [ia, ib, ic] = *triangle;
        let (Some(a), Some(b), Some(c)) = (projected.get(ia), projected.get(ib), projected.get(ic))
        else {
            continue;
        };
        let area = (b.0 - a.0) * (c.1 - a.1) - (b.1 - a.1) * (c.0 - a.0);
        if !area.is_finite() || area.abs() < 1e-9 {
            continue;
        }
        let sign = if area > 0.0 { 1 } else { -1 };
        for (i, j) in [(ia, ib), (ib, ic), (ic, ia)] {
            let key = (i.min(j), i.max(j));
            let slot = edges.entry(key).or_insert([None, None]);
            if slot[0].is_none() {
                slot[0] = Some(sign);
            } else if slot[1].is_none() {
                slot[1] = Some(sign);
            }
        }
    }

    let mut segments = Vec::new();
    for ((i, j), signs) in edges {
        let visible = match signs {
            [Some(_), None] => true,
            [Some(a), Some(b)] => a != b,
            _ => false,
        };
        if visible && let (Some(a), Some(b)) = (projected.get(i), projected.get(j)) {
            segments.push([*a, *b]);
        }
    }
    // Stable output keeps the headless board tests deterministic.
    segments.sort_by(|a, b| {
        (a[0].0, a[0].1, a[1].0, a[1].1)
            .partial_cmp(&(b[0].0, b[0].1, b[1].0, b[1].1))
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    segments.truncate(20_000);
    segments
}

#[cfg(test)]
mod tests {
    use super::*;
    use glam::DVec3;

    fn template() -> GarmentTemplate {
        GarmentTemplate::two_panel_demo()
    }

    #[test]
    fn front_and_back_faces_produce_the_outer_silhouette() {
        // Two triangles sharing an edge, folded so one faces the viewer and one
        // faces away. The shared edge is a silhouette; the other four are open.
        let surface = SurfaceMesh {
            vertices: vec![
                DVec3::new(0., 0., 0.),
                DVec3::new(1., 0., 0.),
                DVec3::new(0., 1., 0.),
                DVec3::new(1., 1., -1.),
            ],
            triangles: vec![[0, 1, 2], [1, 0, 3]],
        };
        let segments = build(&surface, &template());
        assert_eq!(segments.len(), 5);
        assert!(segments.iter().all(|[a, b]| a != b));
    }

    #[test]
    fn empty_surface_and_empty_template_are_safe() {
        let surface = SurfaceMesh {
            vertices: vec![],
            triangles: vec![],
        };
        assert!(build(&surface, &template()).is_empty());
        let mut template = template();
        template.panels.clear();
        assert!(build(&surface, &template).is_empty());
    }
}
