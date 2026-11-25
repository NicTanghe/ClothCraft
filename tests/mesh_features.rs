//! Static feature sizing preserves material geometry; these hints add no forces.
use clothcraft::{
    garment::{domain::InternalLine, meshing::MeshingSettings, polygon::PolygonPattern},
    mesh_resolution::{
        features::{self, MeshFeature},
        mesh::{Domain, Ring},
        preparation::{self, PhysicsSizing},
        sizing::{Feature, Source},
    },
};
use glam::DVec2;
use std::collections::BTreeMap;
fn plan() -> PhysicsSizing {
    PhysicsSizing {
        young_pa: [110000.0; 2],
        bending_nm: [1e-5; 2],
        axis_degrees: 0.0,
        boundary_ratio: 1.0,
        transition_m: 0.01,
        seed: 42,
        spacing_scale: 1.0,
        smooth_transitions: true,
        match_seams: false,
        sewn_edge_ratio: None,
        features: vec![],
    }
}
fn domain() -> Domain {
    Domain {
        outer: Ring {
            points: vec![
                DVec2::ZERO,
                DVec2::new(0.08, 0.0),
                DVec2::new(0.08, 0.12),
                DVec2::new(0.0, 0.12),
            ],
            edges: ["bottom", "right", "top", "left"]
                .map(str::to_owned)
                .to_vec(),
        },
        holes: vec![],
        lines: BTreeMap::from([(
            "fold".into(),
            vec![DVec2::new(0.04, 0.02), DVec2::new(0.04, 0.10)],
        )]),
    }
}
fn map(p: &PhysicsSizing) -> clothcraft::mesh_resolution::sizing::SizingMap {
    let mut d = domain();
    let named = d.paths().into_iter().collect();
    let extra = features::resolve(p, &named, &mut d).unwrap();
    p.map(&[]).unwrap().with_additional_sources(extra).unwrap()
}
#[test]
fn old_plans_default_to_original_spacing_and_transitions() {
    let mut json = serde_json::to_value(plan()).unwrap();
    let obj = json.as_object_mut().unwrap();
    obj.remove("spacing_scale");
    obj.remove("smooth_transitions");
    obj.remove("features");
    let restored: PhysicsSizing = serde_json::from_value(json).unwrap();
    assert_eq!(restored, plan());
    let mut fine = plan();
    fine.spacing_scale = 0.25;
    let mut coarse = plan();
    coarse.spacing_scale = 4.0;
    for (f, c) in fine.base().unwrap().into_iter().zip(coarse.base().unwrap()) {
        assert!((c / f - 16.0).abs() < 1e-12);
    }
    fine.spacing_scale = f64::NAN;
    assert!(fine.validate().is_err());
}
#[test]
fn gathering_ratio_and_added_modulus_only_refine_the_named_source() {
    let mut p = plan();
    p.features.push(MeshFeature::Gathering {
        path: "bottom".into(),
        sewn_rest_ratio: 0.5,
        added_young_pa: [0.0; 2],
    });
    let m = map(&p);
    let base = p.base().unwrap();
    assert_eq!(m.spacing(DVec2::new(0.04, 0.0)), base.map(|h| h * 0.5));
    assert_eq!(m.spacing(DVec2::new(0.04, 0.10)), base);
    if let MeshFeature::Gathering { added_young_pa, .. } = &mut p.features[0] {
        *added_young_pa = [110000.0; 2];
    }
    assert!(map(&p).spacing(DVec2::new(0.04, 0.0))[0] < base[0] * 0.5);
}
#[test]
fn pressure_is_unit_normalized_and_zero_matches_gathering() {
    let mut p = plan();
    p.features.push(MeshFeature::DownFilled {
        path: "bottom".into(),
        sewn_rest_ratio: 0.5,
        pressure_pa: 0.0,
        pressure_reference_pa: 1000.0,
    });
    let mut g = p.clone();
    g.features[0] = MeshFeature::Gathering {
        path: "bottom".into(),
        sewn_rest_ratio: 0.5,
        added_young_pa: [0.0; 2],
    };
    let x = DVec2::new(0.04, 0.005);
    assert_eq!(map(&p).spacing(x), map(&g).spacing(x));
    if let MeshFeature::DownFilled { pressure_pa, .. } = &mut p.features[0] {
        *pressure_pa = 1000.0;
    }
    let mut equivalent = p.clone();
    if let MeshFeature::DownFilled {
        pressure_pa,
        pressure_reference_pa,
        ..
    } = &mut equivalent.features[0]
    {
        *pressure_pa *= 1000.0;
        *pressure_reference_pa *= 1000.0;
    }
    assert_eq!(map(&p).spacing(x), map(&equivalent).spacing(x));
    for i in 0..100 {
        let x = DVec2::new(0.04, i as f64 * 0.0002);
        assert!(map(&p).spacing(x)[0] >= map(&g).spacing(x)[0]);
    }
}
#[test]
fn hard_transition_ball_bound_covers_a_jump() {
    let mut p = plan();
    p.smooth_transitions = false;
    let m = p
        .map(&[])
        .unwrap()
        .with_additional_sources(vec![Source {
            feature: Feature::Polyline {
                points: vec![DVec2::ZERO, DVec2::X],
            },
            spacing_m: [0.001; 2],
            doubling_distance_m: 0.01,
            exponent: 0.5,
        }])
        .unwrap();
    assert_eq!(m.spacing(DVec2::new(0.04, 0.005)), [0.001; 2]);
    assert_eq!(m.spacing(DVec2::new(0.04, 0.011)), p.base().unwrap());
    assert_eq!(m.lower_bound(DVec2::new(0.04, 0.011), 0.002), [0.001; 2]);
}
#[test]
fn fold_endpoints_are_directional_and_rails_are_true_constraints() {
    let mut p = plan();
    p.features.push(MeshFeature::Fold {
        path: "fold".into(),
        endpoints: true,
        parallel_lines: true,
        angle_degrees: 90.0,
        offset_m: 0.0005,
    });
    let mut d = domain();
    let named = d.paths().into_iter().collect();
    let extra = features::resolve(&p, &named, &mut d).unwrap();
    assert_eq!(d.lines.len(), 3);
    assert_eq!(extra.len(), 2);
    let m = p.map(&[]).unwrap().with_additional_sources(extra).unwrap();
    assert!(m.spacing(DVec2::new(0.04, 0.021))[0] < m.spacing(DVec2::new(0.04, 0.019))[0]);
    let original = domain();
    let lines = vec![InternalLine {
        id: "fold".into(),
        points: original.lines["fold"].clone(),
        tangents: vec![DVec2::ZERO; 2],
    }];
    let settings = MeshingSettings {
        physics_sizing: Some(p.clone()),
        ..Default::default()
    };
    let mesh = preparation::triangulate(
        &original.outer.points,
        &original.outer.edges,
        &[],
        &lines,
        &settings,
        &p,
    )
    .unwrap();
    let area: f64 = mesh
        .faces
        .iter()
        .map(|f| {
            let [a, b, c] = f.map(|i| mesh.uv[i]);
            (b - a).perp_dot(c - a) * 0.5
        })
        .sum();
    assert!((area - 0.08 * 0.12).abs() < 1e-12);
    for (id, path) in &d.lines {
        let ids = &mesh.boundaries[id];
        assert!(mesh.uv[ids[0]].distance(path[0]) < 1e-12);
        for pair in ids.windows(2) {
            assert!(
                mesh.faces
                    .iter()
                    .any(|f| f.contains(&pair[0]) && f.contains(&pair[1]))
            );
        }
    }
}
#[test]
fn missing_paths_and_invalid_parallel_offsets_fail_instead_of_dropping_hints() {
    assert!(
        preparation::triangulate(&[], &[], &[], &[], &MeshingSettings::default(), &plan()).is_err()
    );
    let mut p = plan();
    p.features.push(MeshFeature::Fold {
        path: "missing".into(),
        endpoints: true,
        parallel_lines: false,
        angle_degrees: 90.0,
        offset_m: 0.0005,
    });
    let mut d = domain();
    let named = d.paths().into_iter().collect();
    assert!(features::resolve(&p, &named, &mut d).is_err());
    p.features[0] = MeshFeature::Fold {
        path: "bottom".into(),
        endpoints: true,
        parallel_lines: true,
        angle_degrees: 90.0,
        offset_m: 0.0005,
    };
    assert!(features::resolve(&p, &named, &mut d).is_err());
    p.features[0] = MeshFeature::DownFilled {
        path: "bottom".into(),
        sewn_rest_ratio: 0.5,
        pressure_pa: 1000.0,
        pressure_reference_pa: 0.0,
    };
    assert!(p.validate().is_err());
}
#[test]
fn curved_named_boundary_hints_survive_compilation_and_preserve_mass() {
    let d = domain();
    let mut p = plan();
    p.features.push(MeshFeature::Gathering {
        path: "top".into(),
        sewn_rest_ratio: 0.5,
        added_young_pa: [0.0; 2],
    });
    let mut poly: PolygonPattern=serde_json::from_value(serde_json::json!({"points":d.outer.points,"edges":d.outer.edges,"subdivisions":0,"curves":{"top":[[0.06,0.13],[0.02,0.13]]}})).unwrap();
    let material = clothcraft::model::Material::default();
    let baseline = poly.build(&material).unwrap();
    poly.meshing = Some(MeshingSettings {
        physics_sizing: Some(p),
        ..Default::default()
    });
    let rebuilt = poly.build(&material).unwrap();
    assert!(
        (baseline.mesh.masses.iter().sum::<f64>() - rebuilt.mesh.masses.iter().sum::<f64>()).abs()
            < 1e-12
    );
    assert_eq!(
        baseline.boundaries.keys().collect::<Vec<_>>(),
        rebuilt.boundaries.keys().collect::<Vec<_>>()
    );
    let json = serde_json::to_string(&poly).unwrap();
    let loaded: PolygonPattern = serde_json::from_str(&json).unwrap();
    assert_eq!(loaded.build(&material).unwrap().mesh.uv, rebuilt.mesh.uv);
}

#[test]
fn coarse_fine_scale_changes_vertex_budget_without_changing_domain_area() {
    let d = domain();
    let build = |scale| {
        let mut p = plan();
        p.spacing_scale = scale;
        preparation::triangulate(
            &d.outer.points,
            &d.outer.edges,
            &[],
            &[],
            &MeshingSettings::default(),
            &p,
        )
        .unwrap()
    };
    let coarse = build(4.0);
    let fine = build(0.25);
    assert!(fine.uv.len() > coarse.uv.len() * 2);
    let area = |m: &clothcraft::garment::meshing::TriangulatedContour| -> f64 {
        m.faces
            .iter()
            .map(|f| {
                let [a, b, c] = f.map(|i| m.uv[i]);
                (b - a).perp_dot(c - a) * 0.5
            })
            .sum()
    };
    assert!((area(&coarse) - area(&fine)).abs() < 1e-12);
    assert_eq!(
        coarse.boundaries.keys().collect::<Vec<_>>(),
        fine.boundaries.keys().collect::<Vec<_>>()
    );
}
