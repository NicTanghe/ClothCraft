//! Simulation-side static preparation, independent of native rendering.
use clothcraft::{
    fitting::FittingProject,
    garment::{
        GarmentTemplate,
        meshing::MeshingSettings,
        polygon::PolygonPattern,
        presets::{self, GarmentKind},
        seam::{BoundaryCurve, BoundaryRef, SeamSpec, Side},
    },
    mesh_resolution::preparation,
    vbd::{VbdSettings, VbdSolver},
};
use glam::{DVec2, DVec3};
use std::{collections::BTreeMap, sync::Arc};
fn fixture() -> GarmentTemplate {
    let mut t = presets::basic(GarmentKind::Tshirt, 0);
    t.panels.truncate(1);
    t.seams.clear();
    t.mirror_links.clear();
    let p = &mut t.panels[0];
    p.pins.clear();
    p.pin_points.clear();
    p.pattern.width = 0.16;
    p.pattern.height = 0.20;
    p.polygon = Some(PolygonPattern {
        points: vec![
            DVec2::ZERO,
            DVec2::new(0.16, 0.),
            DVec2::new(0.16, 0.20),
            DVec2::new(0., 0.20),
        ],
        edges: ["a", "b", "c", "d"].map(str::to_owned).to_vec(),
        subdivisions: 0,
        meshing: Some(MeshingSettings {
            target_edge_m: 0.04,
            ..Default::default()
        }),
        curves: BTreeMap::new(),
        holes: vec![],
        lines: vec![],
        target_lengths: BTreeMap::new(),
        seam_allowances: BTreeMap::new(),
        notches: BTreeMap::new(),
        measurements: Vec::new(),
    });
    t
}
fn prepare(t: &GarmentTemplate) -> GarmentTemplate {
    preparation::configure(
        t,
        &t.panels.iter().map(|p| p.id.clone()).collect::<Vec<_>>(),
        [110000.; 2],
        0.5,
        0.04,
        true,
    )
    .unwrap()
}

#[test]
fn prepared_seam_matches_trimmed_reversed_and_eased_stations() {
    let project = preparation::simulation_demo("unused.obj".into()).unwrap();
    let mut template = project.template;
    // The mating edge has a different mesh density and the seam has two
    // different local easing ratios. Only the skirt uses physics sizing.
    template.panels[1]
        .polygon
        .as_mut()
        .unwrap()
        .meshing
        .as_mut()
        .unwrap()
        .physics_sizing = None;
    template.seams[0].a.range = [0.15, 0.8];
    template.seams[0].b.range = [0.1, 0.9];
    template.seams[0].notches = vec![[0.35, 0.55]];
    let garment = template.compile().unwrap();
    assert_eq!(garment.mesh.uv, template.compile().unwrap().mesh.uv);
    let mut unsewn = template.clone();
    unsewn.seams.clear();
    let unsewn = unsewn.compile().unwrap();
    for (before, after) in unsewn.panels.iter().zip(&garment.panels) {
        let before = &before.boundaries["left"];
        let after = &after.boundaries["left"];
        assert_eq!(before.len(), after.len());
        for (&a, &b) in before.iter().zip(after) {
            assert!(unsewn.mesh.uv[a].distance(garment.mesh.uv[b]) < 1e-9);
        }
    }
    let seam = &template.seams[0];
    let curve = |panel: usize, r: &BoundaryRef, reverse| {
        BoundaryCurve::new(
            &garment.mesh,
            garment.panels[panel].boundaries[r.edge.as_ref().unwrap()].clone(),
            r.range,
            reverse,
        )
        .unwrap()
    };
    let a = curve(0, &seam.a, false);
    let b = curve(1, &seam.b, true);
    let mut a_stations = a.breakpoints();
    let mut b_stations = b.breakpoints();
    // Trim endpoints can appear twice through floating-point round-off.
    a_stations.dedup_by(|s, t| (*s - *t).abs() < 1e-8);
    b_stations.dedup_by(|s, t| (*s - *t).abs() < 1e-8);
    assert!(a_stations.len() > 3);
    assert_eq!(a_stations.len(), b_stations.len());
    assert!(a_stations.iter().any(|s| (s - 0.35).abs() < 1e-8));
    for (s, t) in a_stations.into_iter().zip(b_stations) {
        let mapped = if s <= 0.35 {
            0.55 * s / 0.35
        } else {
            0.55 + 0.45 * (s - 0.35) / 0.65
        };
        assert!((mapped - t).abs() < 1e-8, "{s} maps to {mapped}, got {t}");
        for (panel, point) in [a.point(s), b.point(t)].into_iter().enumerate() {
            let &(vertex, weight) = point.iter().max_by(|a, b| a.1.total_cmp(&b.1)).unwrap();
            assert!((weight - 1.).abs() < 1e-8, "station lies between vertices");
            assert!(garment.panels[panel].vertices.contains(&vertex));
        }
    }
}

#[test]
fn prepared_seam_matches_a_chain_joint_to_a_vertex_on_the_other_panel() {
    let mut template = fixture();
    let mut second = template.panels[0].clone();
    second.id = "second".into();
    let mut mate = second.clone();
    mate.id = "mate".into();
    template.panels.extend([second, mate]);
    let reference = |panel: &str, end| BoundaryRef {
        panel: panel.into(),
        edge: Some("a".into()),
        side: Side::Top,
        range: [0., end],
        reversed: false,
    };
    template.seams.push(SeamSpec {
        id: "chain".into(),
        a: reference(&template.panels[0].id, 0.37),
        a_tail: vec![reference("second", 0.63)],
        b: reference("mate", 1.),
        b_tail: vec![],
        reverse_b: false,
        notches: vec![],
        stiffness: 50_000.,
        allow_ease: false,
    });
    let garment = prepare(&template).compile().unwrap();
    for (panel, station) in [(0, 0.37), (1, 0.63), (2, 0.37)] {
        let vertex = garment.panels[panel].boundaries["a"]
            .iter()
            .find(|&&v| (garment.mesh.uv[v].x / 0.16 - station).abs() < 1e-8)
            .expect("seam joint must be a vertex on this panel");
        assert!(garment.panels[panel].vertices.contains(vertex));
    }
}

#[test]
fn older_preparation_plans_keep_their_saved_drape() {
    let directory = tempfile::tempdir().unwrap();
    let avatar = directory.path().join("avatar.obj");
    std::fs::write(&avatar, "v 0 0 -10\nv 1 0 -10\nv 0 1 -10\nf 1 2 3\n").unwrap();
    let mut project = preparation::simulation_demo(avatar).unwrap();
    for panel in &mut project.template.panels {
        panel
            .polygon
            .as_mut()
            .unwrap()
            .meshing
            .as_mut()
            .unwrap()
            .physics_sizing
            .as_mut()
            .unwrap()
            .match_seams = false;
    }
    let scene = project.compile().unwrap();
    let mut drape = scene.state;
    drape.time = 0.25;
    project.state = Some(drape.clone());
    let path = directory.path().join("legacy-prepared.usda");
    project.save(&path).unwrap();
    assert!(
        !std::fs::read_to_string(&path)
            .unwrap()
            .contains("match_seams")
    );
    let loaded = FittingProject::load(&path).unwrap().compile().unwrap();
    assert_eq!(loaded.garment.mesh.uv, scene.garment.mesh.uv);
    assert_eq!(loaded.state.x, drape.x);
    assert_eq!(loaded.state.time, drape.time);
}

#[test]
fn physical_compile_preserves_material_geometry_mass_seams_and_boundary_identity() {
    let mut t = fixture();
    let mut p = t.panels[0].clone();
    p.id = "other".into();
    p.placement.translation.x += 0.3;
    t.panels.push(p);
    let boundary = |p: &str| BoundaryRef {
        panel: p.into(),
        edge: Some("a".into()),
        side: Side::Top,
        range: [0.2, 0.8],
        reversed: false,
    };
    t.seams.push(SeamSpec {
        id: "sew".into(),
        a: boundary(&t.panels[0].id),
        b: boundary("other"),
        a_tail: vec![],
        b_tail: vec![],
        reverse_b: true,
        allow_ease: false,
        notches: vec![],
        stiffness: 50000.,
    });
    let original = serde_json::to_value(&t).unwrap();
    let before = t.compile().unwrap();
    let ready = prepare(&t);
    let after = ready.compile().unwrap();
    assert_eq!(original, serde_json::to_value(&t).unwrap());
    assert_ne!(before.mesh.uv.len(), after.mesh.uv.len());
    assert!(
        (before.mesh.masses.iter().sum::<f64>() - after.mesh.masses.iter().sum::<f64>()).abs()
            < 1e-12
    );
    assert!((before.seams[0].measure() - after.seams[0].measure()).abs() < 1e-12);
    for (a, b) in before.panels.iter().zip(&after.panels) {
        assert_eq!(a.id, b.id);
        assert_eq!(
            a.boundaries.keys().collect::<Vec<_>>(),
            b.boundaries.keys().collect::<Vec<_>>()
        );
        for name in a.boundaries.keys() {
            let length = |g: &clothcraft::garment::CompiledGarment, ids: &Vec<usize>| {
                ids.windows(2)
                    .map(|w| g.mesh.uv[w[0]].distance(g.mesh.uv[w[1]]))
                    .sum::<f64>()
            };
            assert!(
                (length(&before, &a.boundaries[name]) - length(&after, &b.boundaries[name])).abs()
                    < 1e-12
            );
        }
    }
    assert!(after.mesh.hinges.len() > before.mesh.hinges.len());
    for (panel, range) in ready.panels.iter().zip(&after.panels) {
        let settings = panel.polygon.as_ref().unwrap().meshing_settings();
        let quality = clothcraft::garment::meshing::quality(
            &after.mesh.triangles[range.triangles.clone()],
            &settings,
        );
        assert!(quality.min_metric_angle_degrees >= 24.999, "{quality:?}");
        assert!(
            quality.max_metric_triangle_area <= 3_f64.sqrt() / 4. + 1e-9,
            "{quality:?}"
        );
    }
}
#[test]
fn prepared_mesh_roundtrips_and_rebinds_fabric_and_grain() {
    let prepared = prepare(&fixture());
    let a = prepared.compile().unwrap();
    let read: GarmentTemplate =
        serde_json::from_slice(&serde_json::to_vec(&prepared).unwrap()).unwrap();
    assert_eq!(a.mesh.uv, read.compile().unwrap().mesh.uv);
    let mut changed = read;
    changed.panels[0].grain_degrees += 37.;
    changed.fabrics[0].properties.bend_warp *= 4.;
    let b = changed.compile().unwrap();
    assert_ne!(a.mesh.uv, b.mesh.uv);
    assert!((a.mesh.masses.iter().sum::<f64>() - b.mesh.masses.iter().sum::<f64>()).abs() < 1e-12);
}
#[test]
fn legacy_defaults_stay_unchanged_and_invalid_preparation_is_atomic() {
    let t = fixture();
    let before = serde_json::to_value(&t).unwrap();
    assert!(!before.to_string().contains("physics_sizing"));
    let ids = vec![t.panels[0].id.clone()];
    assert!(preparation::configure(&t, &ids, [f64::NAN; 2], 0.5, 0.04, true).is_err());
    let mut bad = prepare(&t);
    bad.panels[0]
        .polygon
        .as_mut()
        .unwrap()
        .meshing
        .as_mut()
        .unwrap()
        .max_vertices = 64;
    assert!(bad.compile().is_err());
    assert_eq!(before, serde_json::to_value(&t).unwrap());
    let restored = preparation::configure(&prepare(&t), &ids, [1.; 2], 1., 0.04, false).unwrap();
    assert_eq!(
        t.compile().unwrap().mesh.uv,
        restored.compile().unwrap().mesh.uv
    );
}
#[test]
fn fitting_cache_includes_preparation_and_reset_rebuilds_same_mesh() {
    let avatar =
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("assets/makehuman/mannequin.obj");
    let mut p = FittingProject::basic(GarmentKind::Tshirt, 0, avatar);
    p.template = prepare(&fixture());
    p.prearrange = false;
    let scene = p.compile().unwrap();
    p.state = Some(scene.state);
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("prepared.json");
    p.save(&path).unwrap();
    let mut read = FittingProject::load(&path).unwrap();
    let uv = read.compile().unwrap().garment.mesh.uv.clone();
    read.template.panels[0]
        .polygon
        .as_mut()
        .unwrap()
        .meshing
        .as_mut()
        .unwrap()
        .physics_sizing
        .as_mut()
        .unwrap()
        .young_pa = [220000.; 2];
    assert!(
        read.compile()
            .err()
            .unwrap()
            .contains("Stale fitting cache")
    );
    read.state = None;
    read.mesh_signature = None;
    assert_ne!(uv, read.compile().unwrap().garment.mesh.uv);
    let a = read.compile().unwrap();
    let b = read.compile().unwrap();
    assert_eq!(a.garment.mesh.uv, b.garment.mesh.uv);
}

#[test]
fn prepared_mesh_runs_cpu_vbd_with_fixed_topology_and_expected_free_fall() {
    let template = prepare(&fixture());
    let garment = Arc::new(template.compile().unwrap());
    let uv = garment.mesh.uv.clone();
    let triangles: Vec<_> = garment.mesh.triangles.iter().map(|t| t.vertices).collect();
    let mut state = garment.initial_state.clone();
    let initial = state.x.clone();
    let mut solver = VbdSolver::new(garment.clone(), None, &state).unwrap();
    let settings = VbdSettings {
        dt: 0.002,
        damping: 0.,
        iterations: 3,
        ..Default::default()
    };
    for n in 1..=5 {
        solver.step(&mut state, &settings, None).unwrap();
        for (x, rest) in state.x.iter().zip(&initial) {
            let expected =
                *rest + settings.gravity * settings.dt.powi(2) * (n * (n + 1) / 2) as f64;
            assert!(x.distance(expected) < 1e-9);
        }
        for v in &state.v {
            assert!(v.distance(settings.gravity * settings.dt * n as f64) < 1e-9);
        }
        assert_eq!(uv, garment.mesh.uv);
        assert_eq!(
            triangles,
            garment
                .mesh
                .triangles
                .iter()
                .map(|t| t.vertices)
                .collect::<Vec<_>>()
        );
    }
    assert!(state.v.iter().all(|v| v.distance(DVec3::ZERO) > 0.));
}

#[test]
fn gathered_simulation_example_has_real_eased_sewing_and_retains_pins() {
    let project = preparation::simulation_demo(std::path::PathBuf::from("unused.obj")).unwrap();
    let garment = Arc::new(project.template.compile().unwrap());
    assert_eq!(garment.seams.len(), 1);
    assert!(project.template.seams[0].allow_ease);
    let pins: Vec<_> = garment
        .mesh
        .pins
        .iter()
        .enumerate()
        .filter_map(|(i, p)| p.map(|p| (i, p)))
        .collect();
    assert_eq!(pins.len(), 2);
    let mut state = garment.initial_state.clone();
    let mut solver = VbdSolver::new(garment.clone(), None, &state).unwrap();
    let triangles = garment.mesh.triangles.len();
    for _ in 0..3 {
        solver
            .step(&mut state, project.simulation.as_ref().unwrap(), None)
            .unwrap();
    }
    assert!(state.time > 0.);
    assert_eq!(triangles, garment.mesh.triangles.len());
    for (i, position) in pins {
        assert!(state.x[i].distance(position) < 1e-12);
    }
}

#[test]
fn preparation_adapter_preserves_even_generated_looking_names_and_curves() {
    let mut t = fixture();
    let poly = t.panels[0].polygon.as_mut().unwrap();
    poly.edges = ["prepared_1", "prepared_0", "prepared_2", "prepared_3"]
        .map(str::to_owned)
        .to_vec();
    poly.curves.insert(
        "prepared_1".into(),
        [DVec2::new(0.05, 0.01), DVec2::new(0.12, 0.01)],
    );
    let before = t.compile().unwrap();
    let after = prepare(&t).compile().unwrap();
    for name in &t.panels[0].polygon.as_ref().unwrap().edges {
        let a = &before.panels[0].boundaries[name];
        let b = &after.panels[0].boundaries[name];
        assert!(before.mesh.uv[a[0]].distance(after.mesh.uv[b[0]]) < 1e-12);
        assert!(
            before.mesh.uv[*a.last().unwrap()].distance(after.mesh.uv[*b.last().unwrap()]) < 1e-12
        );
    }
    assert!(
        (before.mesh.triangles.iter().map(|t| t.area).sum::<f64>()
            - after.mesh.triangles.iter().map(|t| t.area).sum::<f64>())
        .abs()
            < 1e-12
    );
}

#[test]
fn preparation_is_one_reversible_document_edit() {
    let t = fixture();
    let prepared = prepare(&t);
    let mut history = clothcraft::authoring::History::default();
    history.record(t.clone());
    let undo = history.restore(&prepared, false).unwrap();
    assert_eq!(
        serde_json::to_value(&undo.template).unwrap(),
        serde_json::to_value(&t).unwrap()
    );
    let redo = history.restore(&undo.template, true).unwrap();
    assert_eq!(
        serde_json::to_value(&redo.template).unwrap(),
        serde_json::to_value(&prepared).unwrap()
    );
    assert_eq!(redo.garment.mesh.uv, prepared.compile().unwrap().mesh.uv);
}
