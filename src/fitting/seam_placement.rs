//! Rigid seam placement before the solver takes over.

use super::FittingScene;
use crate::authoring::{Edit, transform_placement};
use crate::garment::GarmentTemplate;
use crate::garment::seam::StitchGuide;
use crate::vbd::VbdSolver;
use glam::{DQuat, DVec3};
use std::sync::Arc;

/// A piece with other seams or world constraints already has a placement
/// relationship that this single-seam operation cannot move independently.
pub(crate) fn eligible(
    template: &GarmentTemplate,
    scene: &FittingScene,
    first: &str,
    moving: &str,
) -> bool {
    first != moving
        && !template
            .seams
            .iter()
            .any(|seam| seam.references().any(|side| side.panel == moving))
        && scene.garment.panels.iter().any(|panel| {
            panel.id == moving
                && scene.garment.mesh.pins[panel.vertices.clone()]
                    .iter()
                    .all(Option::is_none)
        })
}

/// Position the second piece of a single new seam. Reuse the compiled stitch
/// correspondences, including reversed orientation and eased arclength.
pub(crate) fn prepare(
    scene: &FittingScene,
    source: &GarmentTemplate,
    mut edit: Edit,
) -> Result<(Edit, [f64; 2]), String> {
    let spec = edit.template.seams.last().ok_or("Missing new seam")?;
    if edit.template.seams.len() != source.seams.len() + 1
        || !spec.a_tail.is_empty()
        || !spec.b_tail.is_empty()
        || !eligible(source, scene, &spec.a.panel, &spec.b.panel)
    {
        return Err("Choose two pieces and select an unsewn, unpinned piece second".into());
    }
    let base = edit.state_after(&scene.garment, &scene.state);
    let [a, b] = [&spec.a.panel, &spec.b.panel].map(|id| {
        edit.garment
            .panels
            .iter()
            .find(|panel| &panel.id == id)
            .expect("compiled seam owns both pieces")
    });
    let panel = edit
        .template
        .panels
        .iter_mut()
        .find(|panel| panel.id == b.id)
        .expect("compiled panel comes from the template");
    let seam = edit.garment.seams.last().expect("compiled new seam");
    let rest = &edit.garment.arrangement_positions;
    let guide = |stitch: &StitchGuide| {
        let point =
            |side: usize, x: &[DVec3]| stitch[side].iter().map(|(i, w)| x[*i] * *w).sum::<DVec3>();
        [point(0, &base.x), point(1, rest)]
    };
    let [a0, b0] = guide(seam.guides.first().expect("seam has endpoint guides"));
    let [a1, b1] = guide(seam.guides.last().expect("seam has endpoint guides"));
    let axis = (a1 - a0)
        .try_normalize()
        .ok_or("The first seam edge has coincident ends")?;
    let from = (b1 - b0)
        .try_normalize()
        .ok_or("The second seam edge has coincident ends")?;
    let align = DQuat::from_rotation_arc(from, axis);

    let pairs: Vec<_> = seam
        .samples
        .iter()
        .map(|sample| {
            let mut first = DVec3::ZERO;
            let mut second = DVec3::ZERO;
            for &(i, weight) in &sample.point.terms {
                if b.vertices.contains(&i) {
                    second -= weight * rest[i];
                } else {
                    first += weight * base.x[i];
                }
            }
            (first, second, sample.weight_m)
        })
        .collect();
    let (sum_a, sum_b) = pairs
        .iter()
        .fold((DVec3::ZERO, DVec3::ZERO), |(sa, sb), (pa, pb, w)| {
            (sa + *w * pa, sb + *w * pb)
        });
    let center_a = sum_a / seam.measure();
    let center_b = sum_b / seam.measure();

    // After matching chord directions, fit the remaining roll using the
    // whole seam. Straight edges have no preferred roll.
    let (sin, cos) = pairs.iter().fold((0.0, 0.0), |(sin, cos), (pa, pb, w)| {
        let x = (align * (pb - center_b)).reject_from_normalized(axis);
        let y = (pa - center_a).reject_from_normalized(axis);
        (sin + *w * axis.dot(x.cross(y)), cos + *w * x.dot(y))
    });
    let roll = if sin.hypot(cos) > 1e-14 {
        sin.atan2(cos)
    } else {
        0.0
    };
    let panel_center = base.x[a.vertices.clone()].iter().sum::<DVec3>() / a.vertices.len() as f64;
    let mut normal = axis
        .cross(center_a - panel_center)
        .try_normalize()
        .unwrap_or_else(|| axis.any_orthonormal_vector());
    if normal.dot(center_b - center_a) < 0.0 {
        normal = -normal;
    }

    // Keep a small positive clearance at handoff. Test both sides and a few
    // alternative rolls so folded or overlapping poses can be rejected.
    const GAP_M: f64 = 0.003;
    let old_gap = seam.max_gap(&base.x);
    let old_squared = seam.integrated_gap_squared(&base.x);
    let mut candidates = Vec::new();
    let mut trial = base.clone();
    for quarter_turn in [0.0, 1.0, -1.0, 2.0] {
        let rotation =
            DQuat::from_axis_angle(axis, roll + quarter_turn * std::f64::consts::FRAC_PI_2) * align;
        for side in [1.0, -1.0] {
            let delta = center_a + normal * (side * GAP_M) - center_b;
            let placement = transform_placement(&panel.placement, center_b, delta, rotation)?;
            for i in b.vertices.clone() {
                let uv = edit.garment.mesh.uv[i];
                trial.x[i] = placement.transform(DVec3::new(uv.x, -uv.y, 0.0));
            }
            let gap = seam.max_gap(&trial.x);
            let squared = seam.integrated_gap_squared(&trial.x);
            if gap + 1e-6 < old_gap
                && squared < old_squared
                && trial.x[b.vertices.clone()] != rest[b.vertices.clone()]
            {
                let rms_m = (squared / seam.measure()).sqrt();
                candidates.push((
                    (rms_m * 1e6).round(),
                    gap,
                    placement,
                    trial.x[b.vertices.clone()].to_vec(),
                ));
            }
        }
    }
    if candidates.is_empty() {
        return Err("Alignment would not reduce the seam gap".into());
    }
    // At equal fit to the nearest micrometre, prefer the fitted roll and
    // the side that the moving piece was already on.
    candidates.sort_by(|a, b| a.0.total_cmp(&b.0));

    // Use the usual solver startup gate, without the new seam's local
    // contact exclusions, before publishing either the seam or the placement.
    let mut collision_garment = edit.garment.clone();
    collision_garment.seams.pop();
    let collision_garment = Arc::new(collision_garment);
    let avatar = Some(scene.collider.surface.as_ref());
    let mut reset = edit.garment.initial_state.clone();
    let needs_reset_check = base.x[..b.vertices.start] != reset.x[..b.vertices.start]
        || base.x[b.vertices.end..] != reset.x[b.vertices.end..];
    for (_, gap, placement, points) in candidates {
        trial.x[b.vertices.clone()].copy_from_slice(&points);
        if let Ok(mut solver) = VbdSolver::new(Arc::clone(&collision_garment), avatar, &trial) {
            if needs_reset_check {
                // The solver world includes the avatar as a fixed suffix.
                reset.x[b.vertices.clone()].copy_from_slice(&points);
                let mut all = reset.x.clone();
                all.extend_from_slice(&scene.collider.surface.vertices);
                if solver.world.validate_start(&all).is_err() {
                    continue;
                }
            }
            panel.placement = placement;
            edit.selected = Some(b.id.clone());
            edit.garment = edit.template.compile()?;
            return Ok((edit, [old_gap, gap]));
        }
    }
    Err("No separated placement found. Move the second piece near the seam or choose Sew".into())
}
