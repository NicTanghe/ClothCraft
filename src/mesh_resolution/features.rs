//! Authored, static sizing hints. These change discretization, never forces or rest shape.
use super::{
    mesh::Domain,
    preparation::PhysicsSizing,
    sizing::{Calibration, Fabric, Feature, Source},
};
use glam::DVec2;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum MeshFeature {
    Fold {
        path: String,
        endpoints: bool,
        parallel_lines: bool,
        angle_degrees: f64,
        offset_m: f64,
    },
    Gathering {
        path: String,
        sewn_rest_ratio: f64,
        added_young_pa: [f64; 2],
    },
    DownFilled {
        path: String,
        sewn_rest_ratio: f64,
        pressure_pa: f64,
        pressure_reference_pa: f64,
    },
}
impl MeshFeature {
    pub fn path(&self) -> &str {
        match self {
            Self::Fold { path, .. }
            | Self::Gathering { path, .. }
            | Self::DownFilled { path, .. } => path,
        }
    }
    pub fn validate(&self) -> Result<(), String> {
        if self.path().is_empty() || self.path().len() > 80 {
            return Err("Choose an existing named boundary or internal line".into());
        }
        let ratio_ok = |r: f64| r.is_finite() && (0.01..=1.0).contains(&r);
        match self {
            Self::Fold { angle_degrees, offset_m, .. } if !angle_degrees.is_finite() || !(1.0..=180.0).contains(angle_degrees) || !offset_m.is_finite() || !(0.0001..=0.01).contains(offset_m) => Err("Fold angle must be 1..180 degrees; parallel offset 0.1..10 mm".into()),
            Self::Gathering { sewn_rest_ratio, added_young_pa, .. } if !ratio_ok(*sewn_rest_ratio) || added_young_pa.iter().any(|e| !e.is_finite() || *e < 0.0) => Err("Gathering needs sewn/rest ratio 0.01..1 and nonnegative added modulus in Pa".into()),
            Self::DownFilled { sewn_rest_ratio, pressure_pa, pressure_reference_pa, .. } if !ratio_ok(*sewn_rest_ratio) || !pressure_pa.is_finite() || *pressure_pa < 0.0 || !pressure_reference_pa.is_finite() || *pressure_reference_pa <= 0.0 || pressure_pa / pressure_reference_pa > 2.0 => Err("Down-fill sizing needs ratio 0.01..1, positive pressure reference and pressure/reference 0..2".into()),
            _ => Ok(())
        }
    }
}

/// Resolve stable document paths every compilation; generated rails are temporary constraints.
pub fn resolve(
    plan: &PhysicsSizing,
    named: &BTreeMap<String, Vec<DVec2>>,
    domain: &mut Domain,
) -> Result<Vec<Source>, String> {
    let base = plan.base()?;
    let fabric = Fabric {
        young_pa: plan.young_pa,
        bending_nm: plan.bending_nm,
        axis_degrees: plan.axis_degrees,
    };
    let mut sources = Vec::new();
    let mut used: BTreeSet<String> = domain.paths().into_iter().map(|(id, _)| id).collect();
    used.extend(named.keys().cloned());
    for (i, hint) in plan.features.iter().enumerate() {
        hint.validate()?;
        let path = named
            .get(hint.path())
            .ok_or_else(|| format!("Mesh feature path '{}' no longer exists", hint.path()))?;
        if path.len() < 2
            || path.iter().any(|p| !p.is_finite())
            || path.windows(2).any(|w| w[0].distance(w[1]) < 1e-9)
        {
            return Err("Mesh feature requires a finite, nonzero path".into());
        }
        let source = |spacing_m, exponent, transition| Source {
            feature: Feature::Polyline {
                points: path.clone(),
            },
            spacing_m,
            doubling_distance_m: transition,
            exponent,
        };
        match hint {
            MeshFeature::Fold {
                endpoints,
                parallel_lines,
                angle_degrees,
                offset_m,
                ..
            } => {
                if *endpoints {
                    let end = path.len() - 1;
                    for (origin, direction) in [
                        (path[0], path[1] - path[0]),
                        (path[end], path[end - 1] - path[end]),
                    ] {
                        sources.push(Source {
                            feature: Feature::DirectedPoint { origin, direction },
                            spacing_m: base.map(|h| h * angle_degrees / 360.0),
                            doubling_distance_m: plan.transition_m,
                            exponent: 2.0 / 3.0,
                        });
                    }
                }
                if *parallel_lines {
                    if !domain.lines.contains_key(hint.path()) {
                        return Err(
                            "Parallel fold lines require an internal line with room on both sides"
                                .into(),
                        );
                    }
                    for sign in [-1.0, 1.0] {
                        let rail = offset_path(path, sign * offset_m)?;
                        let mut id =
                            format!("mesh_fold_{i}_{}", if sign < 0.0 { "a" } else { "b" });
                        while used.contains(&id) {
                            id.push('_');
                        }
                        used.insert(id.clone());
                        domain.lines.insert(id, rail);
                    }
                }
            }
            MeshFeature::Gathering {
                sewn_rest_ratio,
                added_young_pa,
                ..
            } => {
                let spacing = fabric
                    .shirring(
                        &Calibration::default(),
                        *added_young_pa,
                        *sewn_rest_ratio,
                        1.0,
                    )?
                    .map(|h| h * plan.spacing_scale);
                sources.push(source(spacing, 0.5, plan.transition_m));
            }
            MeshFeature::DownFilled {
                sewn_rest_ratio,
                pressure_pa,
                pressure_reference_pa,
                ..
            } => {
                let spacing = fabric
                    .shirring(&Calibration::default(), [0.0; 2], *sewn_rest_ratio, 1.0)?
                    .map(|h| h * plan.spacing_scale);
                // Independent unit-consistent analogue, not the paper's dimensional power fit.
                let pressure_factor = 2_f64.powf(pressure_pa / pressure_reference_pa);
                sources.push(source(spacing, 0.5, plan.transition_m / pressure_factor));
            }
        }
    }
    // Crossing/outside rails fail explicitly, without moving or clipping the authored geometry.
    domain.validate()?;
    Ok(sources)
}

fn offset_path(path: &[DVec2], offset: f64) -> Result<Vec<DVec2>, String> {
    if path.len() > 64 {
        return Err("Parallel fold paths support at most 64 sampled points".into());
    }
    let normal = |v: DVec2| {
        let v = v.normalize();
        DVec2::new(-v.y, v.x)
    };
    path.iter()
        .enumerate()
        .map(|(i, p)| {
            let before = normal(path[i.max(1)] - path[i.saturating_sub(1)]);
            let after = normal(path[(i + 1).min(path.len() - 1)] - path[i.min(path.len() - 2)]);
            let bisector = before + after;
            let denominator = bisector.dot(after);
            if denominator < 0.5 {
                return Err(
                    "Fold turn is too sharp for parallel constraints; simplify the line".into(),
                );
            }
            Ok(*p + bisector * (offset / denominator))
        })
        .collect()
}
