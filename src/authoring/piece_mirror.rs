//! Optional left/right editing in a piece's material coordinates.
use super::{BoundaryRef, GarmentTemplate, PolygonPattern, SeamSpec, corner_tool::CornerRef};
use glam::DVec2;
use std::collections::{BTreeMap, BTreeSet};

// Tolerate small drafting/import discrepancies without pairing a different corner.
const PAIR_TOLERANCE_M: f64 = 0.00025;

pub(super) fn axis_x(polygon: &PolygonPattern) -> f64 {
    let (min, max) = polygon
        .points
        .iter()
        .fold((f64::INFINITY, f64::NEG_INFINITY), |(min, max), point| {
            (min.min(point.x), max.max(point.x))
        });
    min.midpoint(max)
}

pub(super) fn reflect(point: DVec2, axis: f64) -> DVec2 {
    DVec2::new(2. * axis - point.x, point.y)
}

pub(super) fn corner_partner(polygon: &PolygonPattern, axis: f64, corner: usize) -> Option<usize> {
    let nearest = |point: DVec2| {
        let mut candidates = polygon.points.iter().enumerate().filter_map(|(i, p)| {
            (p.distance_squared(point) <= PAIR_TOLERANCE_M.powi(2)).then_some(i)
        });
        let first = candidates.next()?;
        candidates.next().is_none().then_some(first)
    };
    let point = *polygon.points.get(corner)?;
    if (point.x - axis).abs() <= 1e-9 {
        return Some(corner);
    }
    let partner = nearest(reflect(point, axis))?;
    (partner != corner && nearest(reflect(polygon.points[partner], axis)) == Some(corner))
        .then_some(partner)
}

/// Reflection reverses contour winding, so the partner starts at the reflected end.
pub(super) fn edge_partner(polygon: &PolygonPattern, axis: f64, edge: usize) -> Option<usize> {
    let n = polygon.points.len();
    let start = corner_partner(polygon, axis, edge)?;
    let end = corner_partner(polygon, axis, (edge + 1) % n)?;
    ((end + 1) % n == start).then_some(end)
}

pub(super) fn boundary_partner<'a>(
    template: &'a GarmentTemplate,
    panel: &str,
    edge: &str,
) -> Option<&'a str> {
    let polygon = template
        .panels
        .iter()
        .find(|p| p.id == panel)?
        .polygon
        .as_ref()?;
    let edge = polygon.edges.iter().position(|name| name == edge)?;
    Some(&polygon.edges[edge_partner(polygon, axis_x(polygon), edge)?])
}

/// Reflect a new seam when every participating piece has mirror editing on.
/// A seam using the same spans after reflection is already present in the plan.
pub fn mirrored_seam(
    template: &GarmentTemplate,
    mirrored_pieces: &BTreeSet<String>,
    seam: &SeamSpec,
) -> Result<Option<SeamSpec>, String> {
    if !seam
        .references()
        .all(|side| mirrored_pieces.contains(&side.panel))
    {
        return Ok(None);
    }
    let mut mate = seam.clone();
    for side in mate.references_mut() {
        let edge = side
            .edge
            .as_deref()
            .ok_or("Mirror sewing requires named contour edges")?;
        let reflected = boundary_partner(template, &side.panel, edge)
            .ok_or_else(|| format!("No reflected boundary matches {}.{}", side.panel, edge))?;
        side.edge = Some(reflected.into());
        side.range = [1. - side.range[1], 1. - side.range[0]];
        // Reflection reverses winding, while the sewing traversal stays the same.
        side.reversed = !side.reversed;
    }
    let same_boundary = |a: &BoundaryRef, b: &BoundaryRef| a.panel == b.panel && a.edge == b.edge;
    let same_side = |a: bool, b: bool| {
        let mut ranges: Vec<_> = mate.side(b).collect();
        for range in seam.side(a) {
            let Some(index) = ranges.iter().position(|candidate| {
                same_boundary(range, candidate)
                    && range
                        .range
                        .iter()
                        .zip(candidate.range)
                        .all(|(a, b)| (a - b).abs() <= 1e-12)
            }) else {
                return false;
            };
            ranges.swap_remove(index);
        }
        ranges.is_empty()
    };
    if (same_side(false, false) && same_side(true, true))
        || (same_side(false, true) && same_side(true, false))
    {
        return Ok(None);
    }
    for a in seam.references() {
        if mate.references().any(|b| {
            same_boundary(a, b) && a.range[0].max(b.range[0]) < a.range[1].min(b.range[1]) - 1e-12
        }) {
            return Err("The reflected seam overlaps the original. Split the shared boundary before sewing.".into());
        }
    }
    Ok(Some(mate))
}

pub(super) fn mirror_corner_edits(
    template: &GarmentTemplate,
    enabled: &BTreeSet<String>,
    seeds: &[(CornerRef, DVec2)],
    preferred: &mut BTreeMap<CornerRef, DVec2>,
) {
    if enabled.is_empty() {
        return;
    }
    // The grabbed point is the first seed. Its movement wins if both sides
    // of a pair were explicitly selected. Include automatically moved dart points.
    let planned: Vec<_> = seeds
        .iter()
        .cloned()
        .chain(
            preferred
                .iter()
                .map(|(corner, delta)| (corner.clone(), *delta)),
        )
        .collect();
    let mut processed = BTreeSet::new();
    for (corner, delta) in planned {
        if !enabled.contains(&corner.panel) || processed.contains(&corner) {
            continue;
        }
        let Some(poly) = template
            .panels
            .iter()
            .find(|p| p.id == corner.panel)
            .and_then(|p| p.polygon.as_ref())
        else {
            continue;
        };
        let axis = axis_x(poly);
        let Some(partner) = corner_partner(poly, axis, corner.point) else {
            continue;
        };
        let delta = preferred.get(&corner).copied().unwrap_or(delta);
        if partner == corner.point {
            preferred.insert(corner.clone(), DVec2::new(0., delta.y));
        } else {
            let mate = CornerRef {
                panel: corner.panel.clone(),
                point: partner,
            };
            preferred.insert(corner.clone(), delta);
            preferred.insert(mate.clone(), DVec2::new(-delta.x, delta.y));
            processed.insert(mate);
        }
        processed.insert(corner);
    }
}

pub(super) fn include_corner_partners(
    template: &GarmentTemplate,
    enabled: &BTreeSet<String>,
    points: &mut Vec<(String, usize)>,
) {
    if enabled.is_empty() {
        return;
    }
    let partners: Vec<_> = points
        .iter()
        .filter_map(|(id, point)| {
            if !enabled.contains(id) {
                return None;
            }
            let polygon = template
                .panels
                .iter()
                .find(|p| p.id == *id)?
                .polygon
                .as_ref()?;
            Some((
                id.clone(),
                corner_partner(polygon, axis_x(polygon), *point)?,
            ))
        })
        .collect();
    for mate in partners {
        if !points.contains(&mate) {
            points.push(mate);
        }
    }
}

pub(super) fn mirror_curve_edits(
    polygon: &mut PolygonPattern,
    grabbed_handle: usize,
    changes: &mut Vec<(String, [DVec2; 2])>,
) {
    let axis = axis_x(polygon);
    let mut mirrored_edges = BTreeSet::new();
    for (i, (edge, mut controls)) in changes.clone().into_iter().enumerate() {
        if mirrored_edges.contains(&edge) {
            continue;
        }
        let Some(partner) = polygon
            .edges
            .iter()
            .position(|name| name == &edge)
            .and_then(|index| edge_partner(polygon, axis, index))
        else {
            continue;
        };
        let mate = polygon.edges[partner].clone();
        let target = if mate == edge {
            let handle = if i == 0 {
                grabbed_handle
            } else {
                1 - grabbed_handle
            };
            controls[1 - handle] = reflect(controls[handle], axis);
            controls
        } else {
            [reflect(controls[1], axis), reflect(controls[0], axis)]
        };
        polygon.curves.insert(mate.clone(), target);
        if let Some((_, previous)) = changes.iter_mut().find(|(name, _)| *name == mate) {
            *previous = target;
        } else {
            changes.push((mate.clone(), target));
        }
        mirrored_edges.insert(mate);
    }
}
