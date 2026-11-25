//! Synchronize rest-space boundary stations before the final triangulation.
use super::{
    seam::{common_stations, interpolate},
    template::{CompiledGarment, GarmentTemplate},
};
use glam::DVec2;
use std::collections::BTreeMap;

pub(super) fn plan(
    template: &GarmentTemplate,
    garment: &CompiledGarment,
) -> Result<BTreeMap<String, Vec<DVec2>>, String> {
    let polygons: BTreeMap<_, _> = template
        .panels
        .iter()
        .filter_map(|p| {
            let settings = p.polygon.as_ref()?.meshing_settings();
            Some((
                p.id.as_str(),
                (
                    settings.physics_sizing.is_some_and(|plan| plan.match_seams),
                    settings.max_vertices,
                ),
            ))
        })
        .collect();
    let mut additions = BTreeMap::<usize, Vec<DVec2>>::new();
    for spec in &template.seams {
        if !spec
            .references()
            .all(|r| polygons.contains_key(r.panel.as_str()))
            || !spec.references().any(|r| polygons[r.panel.as_str()].0)
        {
            continue;
        }
        let [a, b] = template.seam_curves(garment, spec)?;
        let knots: Vec<_> = std::iter::once([0., 0.])
            .chain(spec.notches.iter().copied())
            .chain(std::iter::once([1., 1.]))
            .collect();
        for s in common_stations(&a, &b, &knots) {
            let t = interpolate(&knots, s, 0);
            // A chain joint can have two separate points at the same station.
            for (curve, station) in [(&a, s), (&b, t)] {
                let left = curve.point_limit(station, true);
                let right = curve.point(station);
                for weights in std::iter::once(left).chain((right != left).then_some(right)) {
                    let [(i, wi), (j, wj)] = weights;
                    let p = garment.mesh.uv[i] * wi + garment.mesh.uv[j] * wj;
                    if p.distance(garment.mesh.uv[i])
                        .min(p.distance(garment.mesh.uv[j]))
                        <= 1e-9
                    {
                        continue;
                    }
                    let owner = garment
                        .panels
                        .partition_point(|panel| panel.vertices.end <= i);
                    let extra = additions.entry(owner).or_default();
                    if extra.len() >= polygons[garment.panels[owner].id.as_str()].1 {
                        return Err("Matched seam stations exceed panel vertex budget".into());
                    }
                    extra.push(p);
                }
            }
        }
    }
    Ok(additions
        .into_iter()
        .map(|(owner, mut extra)| {
            let panel = &garment.panels[owner];
            extra.extend(
                panel
                    .boundaries
                    .values()
                    .flatten()
                    .map(|&v| garment.mesh.uv[v]),
            );
            (panel.id.clone(), extra)
        })
        .collect())
}
