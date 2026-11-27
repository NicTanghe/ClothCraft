//! Prepare a rigid seam approach and validate its handoff to cloth contact.

use super::FittingScene;
use crate::authoring::{Edit, transform_placement};
use crate::garment::{CompiledGarment, GarmentTemplate, template::Placement};
use crate::model::State;
use crate::vbd::VbdSolver;
use glam::{DMat3, DQuat, DVec3, EulerRot};
use std::sync::Arc;

pub(crate) const APPROACH_FRAMES: u32 = 24;
// Allows numerical roundoff in aligned correspondences. This is not a
// physical clearance and is smaller than the contact activation distance.
const MEET_TOLERANCE_M: f64 = 1e-8;
const RIGID_TOLERANCE_M: f64 = 1e-6;

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

pub(crate) struct Approach {
    pub edit: Edit,
    pub panel_id: String,
    pub seam_id: String,
    pub joined_state: State,
    motion: RigidMotion,
}

impl Approach {
    pub fn transform(&self, frame: u32) -> impl Fn(DVec3) -> DVec3 + use<> {
        self.motion.transform(frame)
    }
}

struct RigidMotion {
    pivot: DVec3,
    translation: DVec3,
    rotation: DQuat,
}

impl RigidMotion {
    fn transform(&self, frame: u32) -> impl Fn(DVec3) -> DVec3 + use<> {
        let fraction = f64::from(frame) / f64::from(APPROACH_FRAMES);
        let t = fraction * fraction * (3.0 - 2.0 * fraction);
        let turn = DQuat::IDENTITY.slerp(self.rotation, t);
        let pivot = self.pivot;
        let translation = self.translation * t;
        move |start| pivot + turn * (start - pivot) + translation
    }
}

fn material_point(garment: &CompiledGarment, i: usize) -> DVec3 {
    let uv = garment.mesh.uv[i];
    DVec3::new(uv.x, -uv.y, 0.0)
}

/// Recover the displayed rigid pose, including a flat piece that has already
/// translated or rotated in the solver. A draped piece needs a different
/// assembly method because a rigid approach cannot flatten it.
fn displayed_placement(scene: &FittingScene, id: &str) -> Result<Placement, String> {
    let garment = &scene.garment;
    let panel = garment
        .panels
        .iter()
        .find(|panel| panel.id == id)
        .ok_or("Piece not found")?;
    let vertices = garment.mesh.triangles[panel.triangles.start].vertices;
    let local = vertices.map(|i| material_point(garment, i));
    let actual = vertices.map(|i| scene.state.x[i]);
    let basis = |p: [DVec3; 3]| {
        let x = (p[1] - p[0]).try_normalize()?;
        let z = x.cross(p[2] - p[0]).try_normalize()?;
        Some(DQuat::from_mat3(&DMat3::from_cols(x, z.cross(x), z)))
    };
    let rotation = (basis(actual).ok_or("The second piece has a degenerate face")?
        * basis(local)
            .expect("compiled material triangle")
            .conjugate())
    .normalize();
    let translation = actual[0] - rotation * local[0];
    if panel.vertices.clone().any(|i| {
        (rotation * material_point(garment, i) + translation).distance(scene.state.x[i])
            > RIGID_TOLERANCE_M
    }) {
        return Err("Reset or flatten the second piece before aligning its edge".into());
    }
    let (x, y, z) = rotation.to_euler(EulerRot::XYZ);
    Ok(Placement {
        translation,
        rotation_degrees: DVec3::new(x.to_degrees(), y.to_degrees(), z.to_degrees()),
    })
}

/// Prepare a single seam. All correspondence probes must meet in the final
/// pose, including the ends and both sides of each interior breakpoint.
pub(crate) fn prepare(
    scene: &FittingScene,
    source: &GarmentTemplate,
    edit: Edit,
) -> Result<Approach, String> {
    let spec = edit.template.seams.last().ok_or("Missing new seam")?;
    if edit.template.seams.len() != source.seams.len() + 1
        || !eligible(source, scene, &spec.a.panel, &spec.b.panel)
    {
        return Err("Choose two pieces and select an unsewn, unpinned piece second".into());
    }
    let id = spec.id.clone();
    prepare_seam(scene, edit, &id, false)
}

pub(crate) fn prepare_existing(
    scene: &FittingScene,
    template: &GarmentTemplate,
    id: &str,
) -> Result<Approach, String> {
    let edit = Edit {
        template: template.clone(),
        garment: (*scene.garment).clone(),
        selected: None,
        reset_simulation: true,
        new_point: None,
    };
    prepare_seam(scene, edit, id, true)
}

fn prepare_seam(
    scene: &FittingScene,
    mut edit: Edit,
    id: &str,
    existing: bool,
) -> Result<Approach, String> {
    let spec = edit
        .template
        .seams
        .iter()
        .find(|seam| seam.id == id)
        .ok_or("The selected seam no longer exists")?;
    if spec.a.panel == spec.b.panel || !spec.a_tail.is_empty() || !spec.b_tail.is_empty() {
        return Err("Join on Play needs one edge on each of two pieces".into());
    }
    let old_a = scene
        .garment
        .panels
        .iter()
        .find(|p| p.id == spec.a.panel)
        .expect("existing seam reference")
        .vertices
        .clone();
    let [a, b] = [&spec.a.panel, &spec.b.panel].map(|id| {
        edit.garment
            .panels
            .iter()
            .find(|p| &p.id == id)
            .expect("compiled seam reference")
    });
    if edit.garment.mesh.pins[b.vertices.clone()]
        .iter()
        .any(Option::is_some)
    {
        return Err("Release the pins on the second piece to move its edge".into());
    }
    let panel_id = b.id.clone();
    let start_placement = displayed_placement(scene, &panel_id)?;
    let seam = edit
        .garment
        .seams
        .iter()
        .find(|s| s.id == id)
        .expect("compiled seam");
    let base = edit.state_after(&scene.garment, &scene.state);
    if scene.state.x[old_a.clone()] != base.x[a.vertices.clone()]
        && scene.state.x[old_a.clone()]
            .iter()
            .zip(&scene.garment.arrangement_positions[old_a])
            .any(|(current, authored)| current.distance(*authored) > RIGID_TOLERANCE_M)
    {
        return Err(
            "The first edge would remesh and lose its current pose. Reset or use Sew".into(),
        );
    }
    if existing && seam.max_gap(&base.x) <= MEET_TOLERANCE_M {
        return Err("The selected edges already meet".into());
    }
    let mut start = base.clone();
    for i in b.vertices.clone() {
        start.x[i] = start_placement.transform(material_point(&edit.garment, i));
    }
    let guide = |index: usize| {
        seam.guides[index].map(|side| side.iter().map(|(i, w)| start.x[*i] * *w).sum::<DVec3>())
    };
    let [a0, b0] = guide(0);
    let [a1, b1] = guide(seam.guides.len() - 1);
    let axis = (a1 - a0)
        .try_normalize()
        .ok_or("The first seam edge has coincident ends")?;
    let from = (b1 - b0)
        .try_normalize()
        .ok_or("The second seam edge has coincident ends")?;
    let align = DQuat::from_rotation_arc(from, axis);

    // The chord sets direction, and every stitch sample contributes to the
    // translation and to the roll fit for matching curved edges.
    let pairs: Vec<_> = seam
        .samples
        .iter()
        .map(|sample| {
            let mut a = DVec3::ZERO;
            let mut b = DVec3::ZERO;
            for &(i, w) in &sample.point.terms {
                a += start.x[i] * w.max(0.0);
                b -= start.x[i] * w.min(0.0);
            }
            (a, b, sample.weight_m)
        })
        .collect();
    let (sum_a, sum_b) = pairs
        .iter()
        .fold((DVec3::ZERO, DVec3::ZERO), |(sa, sb), (pa, pb, w)| {
            (sa + pa * *w, sb + pb * *w)
        });
    let center_a = sum_a / seam.measure();
    let center_b = sum_b / seam.measure();
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

    let avatar = Some(scene.collider.surface.as_ref());
    let sewn = Arc::new(edit.garment.clone());
    let mut unsewn = edit.garment.clone();
    unsewn.seams.retain(|s| s.id != id);
    let unsewn = Arc::new(unsewn);
    let initially_joined = seam.max_gap(&start.x) <= MEET_TOLERANCE_M;
    let mut approach_world = if initially_joined {
        None
    } else {
        Some(
            VbdSolver::new(unsewn, avatar, &start)
                .map_err(|error| format!("The approach must start separated · {error}"))?
                .world,
        )
    };
    // Assembly moves the second piece. Preserve the gap quality of any
    // other sewn edges that share that piece.
    let neighboring: Vec<_> = edit
        .template
        .seams
        .iter()
        .filter(|s| s.id != id && s.references().any(|edge| edge.panel == panel_id))
        .filter_map(|s| edit.garment.seams.iter().find(|seam| seam.id == s.id))
        .map(|s| (s, s.max_gap(&start.x), s.integrated_gap_squared(&start.x)))
        .collect();
    let mut trial = start.clone();
    let mut compatible = false;
    let mut keeps_other_seams = false;
    for quarter_turn in [0.0, 1.0, -1.0, 2.0] {
        let rotation = if initially_joined {
            DQuat::IDENTITY
        } else {
            DQuat::from_axis_angle(axis, roll + quarter_turn * std::f64::consts::FRAC_PI_2) * align
        };
        let motion = RigidMotion {
            pivot: center_b,
            rotation,
            translation: if initially_joined {
                DVec3::ZERO
            } else {
                center_a - center_b
            },
        };
        let placement =
            transform_placement(&start_placement, center_b, motion.translation, rotation)?;
        for i in b.vertices.clone() {
            trial.x[i] = placement.transform(material_point(&edit.garment, i));
        }
        if seam.max_gap(&trial.x) > MEET_TOLERANCE_M {
            continue;
        }
        compatible = true;
        if neighboring.iter().any(|(other, gap, squared)| {
            other.max_gap(&trial.x) > gap + 1e-6
                || other.integrated_gap_squared(&trial.x) > squared + 1e-12
        }) {
            continue;
        }
        keeps_other_seams = true;
        // Exact seam contact is allowed only at handoff, using the same
        // local stitch exclusions that the actual solver uses.
        let Ok(mut final_solver) = VbdSolver::new(Arc::clone(&sewn), avatar, &trial) else {
            continue;
        };
        let mut reset = edit.garment.initial_state.x.clone();
        reset[b.vertices.clone()].copy_from_slice(&trial.x[b.vertices.clone()]);
        if final_solver
            .world
            .validate_start(&final_solver.world.positions(&reset))
            .is_err()
        {
            continue;
        }
        if let Some(world) = &mut approach_world
            && !(1..APPROACH_FRAMES).all(|frame| {
                let transform = motion.transform(frame);
                for i in b.vertices.clone() {
                    trial.x[i] = transform(start.x[i]);
                }
                world.validate_start(&world.positions(&trial.x)).is_ok()
            })
        {
            continue;
        }
        edit.template
            .panels
            .iter_mut()
            .find(|p| p.id == panel_id)
            .expect("compiled piece comes from template")
            .placement = placement;
        edit.selected = Some(panel_id.clone());
        edit.garment = edit.template.compile()?;
        let mut joined_state = edit.state_after(&scene.garment, &scene.state);
        let moved = edit
            .garment
            .panels
            .iter()
            .find(|p| p.id == panel_id)
            .expect("joined piece")
            .vertices
            .clone();
        joined_state.x[moved.clone()]
            .copy_from_slice(&edit.garment.arrangement_positions[moved.clone()]);
        joined_state.v[moved].fill(DVec3::ZERO);
        let seam = edit
            .garment
            .seams
            .iter()
            .find(|s| s.id == id)
            .expect("compiled seam");
        if seam.max_gap(&joined_state.x) > MEET_TOLERANCE_M {
            return Err("The full seam could not be preserved in the final mesh".into());
        }
        return Ok(Approach {
            edit,
            panel_id,
            seam_id: id.into(),
            joined_state,
            motion,
        });
    }
    Err(if keeps_other_seams {
        "The edge can meet, but the approach or joined pose has a collision. Reposition the pieces and try again"
    } else if compatible {
        "Joining this edge would pull other seams farther apart"
    } else {
        "The edges cannot meet along their full length with a rigid move. Choose matching edges or use Sew"
    }.into())
}
