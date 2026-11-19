//! Filled front silhouette for the 2D pattern board.
//!
//! The prototype registers avatar world XY against the first panel's board
//! layout and 3D translation. The projection is independent of the 3D camera.

use clothcraft::avatar::SurfaceMesh;
use clothcraft::garment::GarmentTemplate;
use i_overlay::{
    core::fill_rule::FillRule, core::overlay_rule::OverlayRule, float::overlay::FloatOverlay,
};

/// Closed coverage contours in board millimetres. Holes have opposite winding
/// to exterior contours so a single native path fills the entire silhouette.
pub(super) fn build(surface: &SurfaceMesh, template: &GarmentTemplate) -> Vec<Vec<(f64, f64)>> {
    let Some(panel) = template.panels.first() else {
        return Vec::new();
    };
    let anchor_world = panel.placement.translation;
    let anchor_board = clothcraft::authoring::layout(template, 0);
    let projected: Vec<[f64; 2]> = surface
        .vertices
        .iter()
        .map(|p| {
            [
                anchor_board.x + (p.x - anchor_world.x),
                anchor_board.y - (p.y - anchor_world.y),
            ]
        })
        .collect();

    let mut triangles = Vec::with_capacity(surface.triangles.len());
    for &[ia, ib, ic] in &surface.triangles {
        let (Some(&a), Some(&b), Some(&c)) =
            (projected.get(ia), projected.get(ib), projected.get(ic))
        else {
            continue;
        };
        let area = (b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0]);
        if !area.is_finite()
            || area == 0.0
            || [a, b, c]
                .into_iter()
                .flatten()
                .any(|v| !((1000.0 * v) as f32).is_finite())
        {
            continue;
        }
        // Give every projected face the same winding so overlaps add coverage
        // even when the adjacent or occluding face points the other way.
        triangles.push(if area > 0.0 {
            vec![a, b, c]
        } else {
            vec![a, c, b]
        });
    }
    if triangles.is_empty() {
        return Vec::new();
    }
    FloatOverlay::with_subj(&triangles)
        .overlay(OverlayRule::Subject, FillRule::NonZero)
        .into_iter()
        .flatten()
        .map(|ring| {
            ring.into_iter()
                .map(|[x, y]| (x * 1000.0, y * 1000.0))
                .collect()
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use glam::DVec3;

    #[test]
    fn coverage_preserves_the_hole_and_unions_reversed_overlapping_faces() {
        // Four quads surround a 2x2 hole inside a 4x4 square. The last
        // triangle lies over one face, at a different depth and winding.
        let mut vertices = [
            [0., 0.],
            [4., 0.],
            [4., 4.],
            [0., 4.],
            [1., 1.],
            [3., 1.],
            [3., 3.],
            [1., 3.],
        ]
        .map(|[x, y]| DVec3::new(x, y, 0.))
        .to_vec();
        vertices.extend([
            vertices[0] + DVec3::Z,
            vertices[5] + DVec3::Z,
            vertices[1] + DVec3::Z,
        ]);
        let surface = SurfaceMesh {
            vertices,
            triangles: vec![
                [0, 1, 5],
                [0, 5, 4],
                [1, 2, 6],
                [1, 6, 5],
                [2, 3, 7],
                [2, 7, 6],
                [3, 0, 4],
                [3, 4, 7],
                [8, 9, 10],
            ],
        };
        let contours = build(&surface, &GarmentTemplate::two_panel_demo());
        assert_eq!(contours.len(), 2);
        let area = |ring: &[(f64, f64)]| {
            ring.iter()
                .zip(ring.iter().cycle().skip(1))
                .map(|(&(ax, ay), &(bx, by))| ax * by - ay * bx)
                .sum::<f64>()
                / 2.0
        };
        let a = area(&contours[0]);
        let b = area(&contours[1]);
        assert!(a * b < 0.0, "hole winding must oppose exterior winding");
        assert!(((a + b).abs() / 1e6 - 12.0).abs() < 1e-6);
    }

    #[test]
    fn empty_surface_and_empty_template_are_safe() {
        let surface = SurfaceMesh {
            vertices: vec![],
            triangles: vec![],
        };
        assert!(build(&surface, &GarmentTemplate::two_panel_demo()).is_empty());
        let mut template = GarmentTemplate::two_panel_demo();
        template.panels.clear();
        assert!(build(&surface, &template).is_empty());
    }
}
