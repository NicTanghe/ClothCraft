use clothcraft::{
    authoring::{self, Command},
    garment::{
        GarmentTemplate,
        meshing::MeshingSettings,
        seam::{BoundaryRef, Side},
        shapes::Primitive,
    },
};
use glam::DVec2;

fn rectangle(replace: bool, origin: DVec2) -> Command {
    Command::Primitive {
        kind: Primitive::Rectangle,
        size: DVec2::new(0.4, 0.5),
        sweep_degrees: 180.,
        origin,
        meshing: MeshingSettings {
            target_edge_m: 0.09,
            ..Default::default()
        },
        fabric: "woven-demo".into(),
        replace,
    }
}

fn edge_ref(panel: &str) -> BoundaryRef {
    BoundaryRef {
        panel: panel.into(),
        edge: Some("edge_1".into()),
        side: Side::Top,
        range: [0., 1.],
        reversed: false,
    }
}

fn sewn_pair() -> (GarmentTemplate, String, String) {
    let t = authoring::apply(
        &GarmentTemplate::two_panel_demo(),
        rectangle(true, DVec2::ZERO),
    )
    .unwrap()
    .template;
    let t = authoring::apply(&t, rectangle(false, DVec2::new(0.6, 0.)))
        .unwrap()
        .template;
    let (a, b) = (t.panels[0].id.clone(), t.panels[1].id.clone());
    let t = authoring::apply(
        &t,
        Command::Sew {
            a: edge_ref(&a),
            b: edge_ref(&b),
            reverse: false,
            ease: false,
        },
    )
    .unwrap()
    .template;
    (t, a, b)
}

fn split(t: &GarmentTemplate, panel: &str, at_m: f64) -> Result<GarmentTemplate, String> {
    authoring::apply(
        t,
        Command::SplitBoundary {
            panel: panel.into(),
            edge: "edge_1".into(),
            at_m,
        },
    )
    .map(|edit| edit.template)
}

fn points(t: &GarmentTemplate, panel: &str) -> usize {
    t.panels
        .iter()
        .find(|p| p.id == panel)
        .and_then(|p| p.polygon.as_ref())
        .unwrap()
        .points
        .len()
}

#[test]
fn splitting_a_sewn_edge_turns_one_seam_into_two() {
    let (sewn, a, b) = sewn_pair();
    let before = sewn.seams.len();
    let (a_points, b_points) = (points(&sewn, &a), points(&sewn, &b));
    let edited = split(&sewn, &a, 0.1).unwrap();
    assert_eq!(edited.seams.len(), before + 1);
    assert_eq!(points(&edited, &a), a_points + 1);
    assert_eq!(points(&edited, &b), b_points + 1);
    // Both halves are named boundaries to the second endpoint of the original.
    for seam in &edited.seams {
        for reference in seam.references() {
            assert!((0.0..=1.0).contains(&reference.range[0]));
            assert!((0.0..=1.0).contains(&reference.range[1]));
            assert!(reference.range[1] > reference.range[0]);
        }
    }
    edited.compile().unwrap();
}

#[test]
fn splitting_a_reversed_seam_keeps_valid_ranges() {
    let t = authoring::apply(
        &GarmentTemplate::two_panel_demo(),
        rectangle(true, DVec2::ZERO),
    )
    .unwrap()
    .template;
    let t = authoring::apply(&t, rectangle(false, DVec2::new(0.6, 0.)))
        .unwrap()
        .template;
    let (a, b) = (t.panels[0].id.clone(), t.panels[1].id.clone());
    let sewn = authoring::apply(
        &t,
        Command::Sew {
            a: edge_ref(&a),
            b: edge_ref(&b),
            reverse: true,
            ease: false,
        },
    )
    .unwrap()
    .template;
    let edited = split(&sewn, &a, 0.23).unwrap();
    assert_eq!(edited.seams.len(), sewn.seams.len() + 1);
    for seam in &edited.seams {
        assert!(seam.reverse_b);
        for reference in seam.references() {
            assert!(reference.range[0] >= 0.0 && reference.range[1] <= 1.0);
            assert!(reference.range[1] > reference.range[0]);
        }
    }
    edited.compile().unwrap();
}

#[test]
fn splitting_an_unsewn_edge_keeps_seam_ranges() {
    let (sewn, a, b) = sewn_pair();
    let before = sewn.seams[0].clone();
    let other = sewn
        .panels
        .iter()
        .find(|p| p.id == a)
        .and_then(|p| p.polygon.as_ref())
        .unwrap()
        .edges
        .iter()
        .find(|edge| *edge != "edge_1")
        .unwrap()
        .clone();
    // Split a different boundary on the sewn panel: the seam must be untouched.
    let edited = authoring::apply(
        &sewn,
        Command::SplitBoundary {
            panel: a.clone(),
            edge: other,
            at_m: 0.05,
        },
    )
    .unwrap()
    .template;
    assert_eq!(edited.seams.len(), sewn.seams.len());
    assert_eq!(edited.seams[0].a, before.a);
    assert_eq!(edited.seams[0].b, before.b);
    assert_eq!(points(&edited, &b), points(&sewn, &b));
}

#[test]
fn splitting_on_a_linked_panel_is_still_refused() {
    let (template, panel, _) = sewn_pair();
    let linked = authoring::apply(
        &template,
        Command::LinkedMirror {
            panel: panel.clone(),
            axis: clothcraft::garment::shapes::MirrorAxis::X,
        },
    )
    .unwrap()
    .template;
    let before = serde_json::to_value(&linked).unwrap();
    let error = split(&linked, &panel, 0.1).unwrap_err();
    assert!(error.contains("Unlink the mirrored pair"), "{error}");
    assert_eq!(serde_json::to_value(&linked).unwrap(), before);
}

#[test]
fn presets_split_at_their_midpoints_without_compiling_failures() {
    use clothcraft::garment::presets::{self, GarmentKind};
    for kind in [GarmentKind::Tshirt, GarmentKind::Dress] {
        let template = presets::basic(kind, 0);
        for panel in ["front", "back"] {
            let Some(polygon) = template
                .panels
                .iter()
                .find(|p| p.id == panel)
                .and_then(|p| p.polygon.clone())
            else {
                continue;
            };
            for edge in polygon.edges.clone() {
                let length = authoring::edge_length(&polygon, &edge).unwrap();
                let result = authoring::apply(
                    &template,
                    Command::SplitBoundary {
                        panel: panel.into(),
                        edge,
                        at_m: length * 0.5,
                    },
                );
                if let Ok(edit) = result {
                    edit.template.compile().unwrap_or_else(|error| {
                        panic!("{kind:?} {panel} compiles after split: {error}")
                    });
                }
            }
        }
    }
}
