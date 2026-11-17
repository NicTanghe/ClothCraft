//! Native board display coordinates, preserved curves, layering and CPU painting.
#![cfg(feature = "desktop")]

use clothcraft::atelier::DocumentFiles;
use clothcraft::atelier::css_runner::{
    CssRunnerState, Hover2dTarget, build_runner_2d_viewport, compute_2d_view_box_m,
    render_runner_2d_paths,
};
use clothcraft::authoring::{self, Command, CornerRule, corner_tool::CIRCLE_TENSION};
use clothcraft::css_ui::{Theme, load_studio_stylesheet};
use clothcraft::fitting::FittingProject;
use clothcraft::garment::presets::GarmentKind;
use cssimpler::core::{Color, ElementNode, Node, PathCommand::*, VectorPoint};
use std::sync::{Arc, Mutex, mpsc};

fn state(dir: &std::path::Path, rounded: bool) -> CssRunnerState {
    let avatar = dir.join("avatar.obj");
    std::fs::write(&avatar, "v 10 0 -10\nv 11 0 -10\nv 10 1 -10\nf 1 2 3\n").unwrap();
    let mut project = FittingProject::basic(GarmentKind::Tshirt, 0, avatar);
    if rounded {
        project.template = authoring::apply(
            &project.template,
            Command::RoundCornerShape {
                panel: "front".into(),
                edge: "neck-bottom".into(),
                radius_m: 0.008,
                tension: CIRCLE_TENSION,
                rule: CornerRule::Smooth,
            },
        )
        .unwrap()
        .template;
    }
    let scene = project.compile().unwrap();
    let (tx, _rx) = mpsc::channel();
    CssRunnerState::new(
        project,
        scene,
        DocumentFiles {
            output: dir.join("out"),
            source: None,
            modified: false,
        },
        tx,
        Arc::new(Mutex::new(None)),
    )
}

fn element<'a>(node: &'a Node, id: &str) -> Option<&'a ElementNode> {
    if let Node::Element(el) = node {
        if el.id.as_deref() == Some(id) {
            return Some(el);
        }
        return el.children.iter().find_map(|child| element(child, id));
    }
    None
}

#[test]
fn native_board_has_aligned_leaf_layers_and_preserves_authored_cubic_controls() {
    let dir = tempfile::tempdir().unwrap();
    let mut state = state(dir.path(), true);
    state.board_2d_zoom = 1.7;
    state.board_2d_pan = (0.123_456_789, -0.087_654_321);
    let template_before = serde_json::to_string(&state.project.template).unwrap();
    let (paths, vb) = render_runner_2d_paths(&state);
    let (x, y, span) = compute_2d_view_box_m(&state);
    assert_eq!(
        (vb.min_x, vb.min_y, vb.width, vb.height),
        (
            (x * 1000.0) as f32,
            (y * 1000.0) as f32,
            (span * 1000.0) as f32,
            (span * 1000.0) as f32
        )
    );
    let front = paths
        .iter()
        .find(|p| p.class == "pattern-piece active")
        .unwrap();
    let poly = state.project.template.panels[0].polygon.as_ref().unwrap();
    // Display controls keep the authored precision until the final mm/f32 conversion.
    let panel = &state.project.template.panels[0];
    let controls = poly.curves.values().next().unwrap();
    let expected = |p: glam::DVec2| {
        let p = (authoring::layout(&state.project.template, 0)
            + authoring::rotate_2d(p, panel.layout_rotation_degrees))
            * 1000.0;
        VectorPoint::new(p.x as f32, p.y as f32)
    };
    assert!(front.path.commands().iter().any(|c| matches!(c,
        CubicTo {control1, control2, ..} if *control1 == expected(controls[0]) && *control2 == expected(controls[1]))));
    assert!(paths.iter().filter(|p| p.class == "bezier-handle").count() >= 2);
    assert!(paths.iter().filter(|p| p.class == "cad-vertex").count() >= 10);
    let node = build_runner_2d_viewport(&state);
    let background = element(&node, "board-2d-background").unwrap();
    let drawing = element(&node, "board-2d-drawing").unwrap();
    for el in [background, drawing] {
        assert_eq!(el.tag, "vector");
        assert!(el.children.is_empty());
        let scene = el
            .vector_scene
            .as_ref()
            .expect("native scene, without SVG attributes");
        assert_eq!(scene.view_box, vb);
        assert!(el.attribute("viewBox").is_none());
    }
    assert!(background.style.visual.cache_layer);
    assert!(!drawing.style.visual.cache_layer);
    assert_eq!(
        serde_json::to_string(&state.project.template).unwrap(),
        template_before
    );
}

#[test]
fn native_board_cpu_paints_both_themes_and_updates_after_pan_and_zoom() {
    let dir = tempfile::tempdir().unwrap();
    let mut state = state(dir.path(), true);
    let stylesheet = load_studio_stylesheet().unwrap();
    let mut images = Vec::new();
    for (theme, zoom, pan) in [
        (Theme::Dark, 1.0, (0.0, 0.0)),
        (Theme::Light, 1.0, (0.0, 0.0)),
        (Theme::Light, 1.4, (0.1, -0.05)),
    ] {
        state.theme = theme;
        state.board_2d_zoom = zoom;
        state.board_2d_pan = pan;
        let node = build_runner_2d_viewport(&state);
        // Render the board alone: this test measures the actual native leaf
        // paint route independently of the surrounding shell's layout issues.
        let board = Node::from(element(&node, "board-2d").unwrap().clone());
        let tree = cssimpler::style::build_render_tree_in_viewport(&board, &stylesheet, 640, 640);
        let mut rgba = vec![0; 640 * 640 * 4];
        cssimpler::renderer::render_to_rgba8(&[tree], &mut rgba, 640, 640, Color::BLACK);
        // Both the grid and colored pattern strokes must reach the framebuffer.
        assert!(
            rgba.as_chunks::<4>()
                .0
                .iter()
                .any(|p| p[2] > p[0].saturating_add(30))
        );
        assert!(
            rgba.as_chunks::<4>()
                .0
                .iter()
                .any(|p| p[0] > p[2].saturating_add(30))
        );
        images.push(rgba);
    }
    assert_ne!(images[0], images[1]);
    assert_ne!(images[1], images[2]);
    let cache_before = cssimpler::renderer::layer_cache_stats();
    assert!(
        cache_before.bytes > 0,
        "the small grid should fit the default layer budget"
    );
    state.hovered_2d = Some(Hover2dTarget::Panel(1));
    let node = build_runner_2d_viewport(&state);
    let board = Node::from(element(&node, "board-2d").unwrap().clone());
    let tree = cssimpler::style::build_render_tree_in_viewport(&board, &stylesheet, 640, 640);
    let mut rgba = vec![0; 640 * 640 * 4];
    cssimpler::renderer::render_to_rgba8(&[tree], &mut rgba, 640, 640, Color::BLACK);
    assert_ne!(rgba, images[2], "hover should update the uncached drawing");
    assert_eq!(
        cssimpler::renderer::layer_cache_stats().builds,
        cache_before.builds,
        "changing hover must reuse the unchanged grid raster"
    );
}

#[test]
fn native_board_layers_match_hit_testing_across_layouts_and_window_sizes() {
    use clothcraft::atelier::css_runner::{build_runner_ui, compute_2d_viewport_rects};
    use clothcraft::css_ui::ViewLayoutMode;
    use cssimpler::core::{LayoutBox, RenderNode};
    fn bounds(node: &RenderNode, id: &str) -> Option<LayoutBox> {
        if node.id() == Some(id) {
            return Some(node.layout);
        }
        node.children.iter().find_map(|n| bounds(n, id))
    }
    let dir = tempfile::tempdir().unwrap();
    let mut state = state(dir.path(), false);
    let stylesheet = load_studio_stylesheet().unwrap();
    for (w, h) in [(1600, 980), (1280, 800), (1024, 768)] {
        for mode in [ViewLayoutMode::Split, ViewLayoutMode::TwoDOnly] {
            state.window_size = (w as f32, h as f32);
            state.layout_mode = mode;
            let tree = cssimpler::style::build_render_tree_in_viewport(
                &build_runner_ui(&state),
                &stylesheet,
                w,
                h,
            );
            let expected = compute_2d_viewport_rects(&state).1;
            for id in ["board-2d", "board-2d-background", "board-2d-drawing"] {
                let b = bounds(&tree, id).unwrap();
                assert!((b.width - b.height).abs() < 1.5, "{id}: {b:?}");
                assert!(
                    (b.x - expected.x).abs() < 3.0
                        && (b.y - expected.y).abs() < 3.0
                        && (b.width - expected.w).abs() < 3.0,
                    "{id} at {w}x{h} ({mode:?}): {b:?} vs {expected:?}"
                );
            }
        }
    }
}
