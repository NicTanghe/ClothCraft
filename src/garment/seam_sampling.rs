//! Synchronize rest-space boundary stations before the final triangulation.
use super::{
    seam::{SeamSpec, common_stations_where, interpolate},
    template::{CompiledGarment, GarmentTemplate},
};
use crate::mesh_resolution::preparation::SewnRanges;
use glam::DVec2;
use std::collections::{BTreeMap, BTreeSet};

/// Ranges for the sizing field include seams whose mates use interpolated stitches.
pub(super) fn sewn_ranges(template: &GarmentTemplate) -> BTreeMap<&str, SewnRanges> {
    let mut panels = BTreeMap::<_, SewnRanges>::new();
    for reference in template.seams.iter().flat_map(SeamSpec::references) {
        if let Some(edge) = &reference.edge
            && let [start, end] = reference.range
            && 0. <= start
            && start < end
            && end <= 1.
        {
            panels
                .entry(reference.panel.as_str())
                .or_default()
                .entry(edge.clone())
                .or_default()
                .push(reference.range);
        }
    }
    panels
}

pub(super) fn plan(
    template: &GarmentTemplate,
    garment: &CompiledGarment,
    retarget: bool,
) -> Result<BTreeMap<String, Vec<DVec2>>, String> {
    let polygons: BTreeMap<_, _> = template
        .panels
        .iter()
        .enumerate()
        .filter_map(|(i, p)| Some((p.id.as_str(), (i, p.polygon.as_ref()?))))
        .collect();
    let mut seeds = BTreeMap::<usize, Vec<DVec2>>::new();
    let mut replaced = BTreeSet::new();
    let owner = |v| {
        garment
            .panels
            .partition_point(|panel| panel.vertices.end <= v)
    };
    for spec in &template.seams {
        let references: Vec<_> = spec.references().collect();
        if !references
            .iter()
            .all(|r| polygons.contains_key(r.panel.as_str()))
        {
            continue;
        }
        let plans = references.iter().filter_map(|r| {
            let (i, polygon) = polygons[r.panel.as_str()];
            let plan = polygon.meshing.as_ref()?.physics_sizing.as_ref()?;
            plan.match_seams.then_some((i, plan))
        });
        if plans.clone().next().is_none() {
            continue;
        }

        let [a, b] = template.seam_curves(garment, spec)?;
        let knots: Vec<_> = std::iter::once([0., 0.])
            .chain(spec.notches.iter().copied())
            .chain(std::iter::once([1., 1.]))
            .collect();
        let mut spacing = f64::INFINITY;
        if retarget {
            for (i, plan) in plans {
                if let Some(ratio) = plan.sewn_edge_ratio {
                    let mut plan = plan.clone();
                    let material = &garment.materials[garment.panels[i].material];
                    plan.bending_nm = [material.bend_warp, material.bend_weft];
                    let [warp, weft] = plan.base()?;
                    spacing = spacing.min(ratio * warp.min(weft));
                }
            }
        }

        let mut removable = BTreeSet::new();
        if spacing.is_finite() {
            for curve in [&a, &b] {
                curve.interior_vertices(&mut |vertex| {
                    removable.insert(vertex);
                });
            }
            // The flattened constraint's authored vertices must survive coarsening.
            for reference in &references {
                let (i, polygon) = polygons[reference.panel.as_str()];
                let name = reference
                    .edge
                    .as_deref()
                    .ok_or("Polygon seam requires a named edge")?;
                let points = polygon
                    .named_boundary(name)
                    .ok_or("Missing authored seam path")?;
                let mut vertices = garment.panels[i].boundaries[name].iter();
                for point in points {
                    let vertex = vertices
                        .find(|&&v| garment.mesh.uv[v].distance(point) <= 1e-9)
                        .ok_or("Mesh lost an authored seam vertex")?;
                    removable.remove(vertex);
                }
            }
            replaced.extend(removable.iter().copied());
            for &vertex in &removable {
                seeds.entry(owner(vertex)).or_default();
            }
        }

        let mut stations = common_stations_where(&a, &b, &knots, &|v| !removable.contains(&v));
        if spacing.is_finite() {
            let limit: usize = references
                .iter()
                .map(|r| r.panel.as_str())
                .collect::<BTreeSet<_>>()
                .into_iter()
                .map(|id| polygons[id].1.meshing_settings().max_vertices)
                .sum();
            let anchors = std::mem::take(&mut stations);
            for pair in anchors.windows(2) {
                let [start, end] = [pair[0], pair[1]];
                let length_a = a.length * (end - start);
                let length_b =
                    b.length * (interpolate(&knots, end, 0) - interpolate(&knots, start, 0));
                let count = ((length_a.max(length_b) / spacing).ceil() as usize).max(1);
                if count > limit.saturating_sub(stations.len()) {
                    return Err("Matched seam stations exceed panel vertex budget".into());
                }
                stations
                    .extend((0..count).map(|i| start + (end - start) * i as f64 / count as f64));
            }
            stations.push(*anchors.last().unwrap());
        }
        for s in stations {
            let t = interpolate(&knots, s, 0);
            // A chain joint can have two separate points at the same station.
            for (curve, station) in [(&a, s), (&b, t)] {
                let left = curve.point_limit(station, true);
                let right = curve.point(station);
                for weights in std::iter::once(left).chain((right != left).then_some(right)) {
                    let [(i, wi), (j, wj)] = weights;
                    let p = garment.mesh.uv[i] * wi + garment.mesh.uv[j] * wj;
                    let existing = [i, j]
                        .into_iter()
                        .any(|v| !removable.contains(&v) && p.distance(garment.mesh.uv[v]) <= 1e-9);
                    if !existing {
                        let owner = owner(i);
                        let points = seeds.entry(owner).or_default();
                        if points.len()
                            >= polygons[garment.panels[owner].id.as_str()]
                                .1
                                .meshing_settings()
                                .max_vertices
                        {
                            return Err("Matched seam stations exceed panel vertex budget".into());
                        }
                        points.push(p);
                    }
                }
            }
        }
    }
    Ok(seeds
        .into_iter()
        .map(|(owner, mut points)| {
            let panel = &garment.panels[owner];
            points.extend(
                panel
                    .boundaries
                    .values()
                    .flatten()
                    .filter(|v| !replaced.contains(v))
                    .map(|&v| garment.mesh.uv[v]),
            );
            (panel.id.clone(), points)
        })
        .collect())
}
