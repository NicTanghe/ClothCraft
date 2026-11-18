//! Transfer interior measurement points between authored contour revisions.
use super::domain::inside;
use super::polygon::{Corner, MeasurementAnchor, PolygonPattern};
use glam::DVec2;
use spade::{ConstrainedDelaunayTriangulation, Point2, Triangulation};
use std::collections::BTreeMap;

/// Faces contain indices into the authored contour for this transaction only.
struct Frames(Vec<[usize; 3]>);

impl Frames {
    fn new(points: &[DVec2], indices: impl Iterator<Item = usize>) -> Self {
        let mut mesh = ConstrainedDelaunayTriangulation::<Point2<f64>>::new();
        let mut authored = Vec::new();
        let mut handles = vec![None; points.len()];
        for i in indices {
            let p = points[i];
            if let Ok(vertex) = mesh.insert(Point2::new(p.x, p.y))
                && vertex.index() == authored.len()
            {
                authored.push(i);
                handles[i] = Some(vertex);
            }
        }
        // Respect surviving contour segments across concavities. Curved sides
        // use their authored chord here; the sampled outline checks material.
        for i in 0..handles.len() {
            if let (Some(a), Some(b)) = (handles[i], handles[(i + 1) % handles.len()])
                && mesh.can_add_constraint(a, b)
            {
                mesh.add_constraint(a, b);
            }
        }
        Self(
            mesh.inner_faces()
                .map(|face| face.vertices().map(|v| authored[v.index()]))
                .collect(),
        )
    }

    fn containing(&self, points: &[DVec2], p: DVec2) -> Option<([usize; 3], [f64; 3])> {
        self.0.iter().find_map(|&indices| {
            let [a, b, c] = indices.map(|i| points[i]);
            let det = (b - a).perp_dot(c - a);
            let scale = a
                .distance_squared(b)
                .max(b.distance_squared(c))
                .max(c.distance_squared(a));
            if det.abs() <= 1e-8 * scale {
                return None;
            }
            let u = (p - a).perp_dot(c - a) / det;
            let v = (b - a).perp_dot(p - a) / det;
            let weights = [1. - u - v, u, v];
            weights
                .iter()
                .all(|w| *w >= -1e-12)
                .then_some((indices, weights))
        })
    }
}

fn anchor(poly: &PolygonPattern, indices: [usize; 3], weights: [f64; 3]) -> MeasurementAnchor {
    MeasurementAnchor::Barycentric {
        vertices: Box::new(indices.map(|i| Corner {
            edge: poly.edges[i].clone(),
            end: false,
        })),
        weights,
    }
}

pub(super) fn containing_anchor(poly: &PolygonPattern, p: DVec2) -> Option<MeasurementAnchor> {
    let frames = Frames::new(&poly.points, 0..poly.points.len());
    let (indices, weights) = frames.containing(&poly.points, p)?;
    Some(anchor(poly, indices, weights))
}

pub(crate) fn rebind(old: &PolygonPattern, new: &mut PolygonPattern) -> Result<(), String> {
    if (old.points == new.points
        && old.edges == new.edges
        && old.curves == new.curves
        && old
            .holes
            .iter()
            .map(|h| &h.points)
            .eq(new.holes.iter().map(|h| &h.points)))
        || !new.measurements.iter().any(|m| {
            [&m.a, &m.b]
                .iter()
                .any(|a| matches!(a, MeasurementAnchor::Barycentric { .. }))
        })
    {
        return Ok(());
    }
    let by_name: BTreeMap<_, _> = new.edges.iter().enumerate().map(|(i, e)| (e, i)).collect();
    let next: Vec<_> = old.edges.iter().map(|e| by_name.get(e).copied()).collect();
    let survivors: Vec<_> = (0..next.len()).filter(|&i| next[i].is_some()).collect();
    // Normalizing a panel can translate every surviving corner in material space.
    // A componentwise median recovers this common offset when a minority move.
    let median = |axis: usize| {
        let mut shifts: Vec<_> = survivors
            .iter()
            .map(|&i| new.points[next[i].unwrap()][axis] - old.points[i][axis])
            .collect();
        shifts.sort_by(f64::total_cmp);
        shifts.get(shifts.len() / 2).copied().unwrap_or(0.)
    };
    let offset = DVec2::new(median(0), median(1));
    let scale = old
        .points
        .iter()
        .map(|p| p.distance(old.points[0]))
        .fold(0., f64::max);
    let stable: Vec<_> = survivors
        .iter()
        .copied()
        .filter(|&i| {
            (new.points[next[i].unwrap()] - old.points[i]).distance(offset) <= 1e-12 * scale
        })
        .collect();
    let stationary = Frames::new(&old.points, stable.iter().copied());
    let shared = Frames::new(&old.points, survivors.iter().copied());
    let current = Frames::new(&new.points, 0..new.points.len());
    let outline = new.outer_points()?;
    let mut records = new.measurements.clone();
    for m in &mut records {
        let Some(previous) = old.measurements.iter().find(|r| r.id == m.id) else {
            continue;
        };
        for (before, after) in [(&previous.a, &mut m.a), (&previous.b, &mut m.b)] {
            let MeasurementAnchor::Barycentric { vertices, .. } = before else {
                continue;
            };
            // A removed corner may still be named by the end of an incoming
            // edge which now ends at another vertex. Compare corner identities.
            let same_corners = if let MeasurementAnchor::Barycentric {
                vertices: updated, ..
            } = after
            {
                vertices.iter().zip(updated.iter()).all(|(v, n)| {
                    let Some(i) = old.edges.iter().position(|e| e == &v.edge) else {
                        return false;
                    };
                    let old_corner = (i + usize::from(v.end)) % old.edges.len();
                    by_name.get(&n.edge).is_some_and(|&j| {
                        next[old_corner] == Some((j + usize::from(n.end)) % new.edges.len())
                    })
                })
            } else {
                false
            };
            let p = old.measurement_anchor_position(before)?;
            let transport = |ids: [usize; 3], w: [f64; 3]| {
                let [a, b, c] = ids.map(|i| new.points[next[i].unwrap()]);
                a + (b - a) * w[1] + (c - a) * w[2]
            };
            let fixed = stationary.containing(&old.points, p);
            let target = if let Some((ids, w)) = fixed {
                transport(ids, w)
            } else if same_corners && let Ok(q) = new.measurement_anchor_position(after) {
                q
            } else if let Some((ids, w)) = shared.containing(&old.points, p) {
                transport(ids, w)
            } else {
                return Err("No surviving material triangle for measurement endpoint".into());
            };
            if !inside(target, &outline) || new.holes.iter().any(|h| inside(target, &h.points)) {
                return Err("Edit would remove material at a measurement endpoint".into());
            }
            if let Some((ids, w)) = fixed
                && stable.len() < survivors.len()
            {
                *after = anchor(new, ids.map(|i| next[i].unwrap()), w);
            } else if let Some((ids, w)) = current.containing(&new.points, target) {
                *after = anchor(new, ids, w);
            } else {
                *after = new
                    .interior_anchor(target)
                    .ok_or("No usable measurement reference triangle")?;
            }
        }
    }
    new.measurements = records;
    Ok(())
}
