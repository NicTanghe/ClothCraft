//! Shell layout, instance-local handlers and rendering regressions.

use super::super::super::{DocumentFiles, worker};
use super::super::actions::{RunnerAction, update_runner};
use super::super::app::create_runner_app;
use super::super::state::{CssRunnerState, ObjectCategory};
use clothcraft::css_ui::{Theme, ViewLayoutMode, load_studio_stylesheet};
use clothcraft::fitting::FittingProject;
use clothcraft::garment::presets::GarmentKind;
use clothcraft::gpu_avbd::ContactMethod;
use clothcraft::vbd::VbdMetrics;
use cssimpler::renderer::FrameInfo;
use std::time::Duration;

#[test]
fn test_css_runner_headless_frame_and_rendering() {
    let _guard = worker::WORKER_TEST_LOCK
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let stylesheet = load_studio_stylesheet().expect("load studio stylesheet");
    let dir = tempfile::tempdir().expect("tempdir");
    let avatar = dir.path().join("avatar.obj");
    std::fs::write(&avatar, "v 0 0 -10\nv 1 0 -10\nv 0 1 -10\nf 1 2 3\n").expect("write avatar");
    let project = FittingProject::basic(GarmentKind::Tshirt, 1, avatar);
    let scene = project.compile().expect("compile scene");
    let files = DocumentFiles {
        output: dir.path().join("test_runner_out"),
        source: None,
        modified: false,
    };

    let (tx, latest) = worker::start_cpu().expect("start cpu worker");
    let state = CssRunnerState::new(project, scene, files, tx, latest);
    let mut app = create_runner_app(state, &stylesheet);
    app.set_viewport(cssimpler::renderer::ViewportSize::new(1600, 980));

    // Headless frame advance
    let scene = app.frame(FrameInfo {
        delta: Duration::from_millis(16),
        frame_index: 0,
    });
    assert!(!scene.is_empty(), "runner scene should not be empty");

    let collider_points = app
        .state()
        .usd_scene
        .as_ref()
        .unwrap()
        .stage()
        .attribute("/Collider.points")
        .unwrap()
        .get::<Vec<cssimpler::renderer::gpu::openusd::gf::Vec3f>>()
        .unwrap()
        .unwrap();
    assert!(
        !collider_points.is_empty(),
        "collision geometry must be visible in USD"
    );
    // A click belongs to the instance that built the scene, even if another
    // instance advances first. An unrelated menu update must not reset the
    // switch's transition clock.
    let (other_tx, other_latest) = worker::start_cpu().expect("start second CPU worker");
    let other_files = DocumentFiles {
        output: dir.path().join("other"),
        source: None,
        modified: false,
    };
    let other = CssRunnerState::new(
        app.state().project.clone(),
        app.state().scene.clone(),
        other_files,
        other_tx,
        other_latest,
    );
    let mut other_app = create_runner_app(other, &stylesheet);
    other_app.set_viewport(cssimpler::renderer::ViewportSize::new(1600, 980));
    other_app.frame(FrameInfo {
        delta: Duration::ZERO,
        frame_index: 0,
    });
    let button = app.find_element_bounds_by_id("#theme-toggle").unwrap();
    assert!(cssimpler::renderer::dispatch_click(
        app.scene(),
        button.x + button.width / 2.0,
        button.y + button.height / 2.0
    ));
    other_app.frame(FrameInfo {
        delta: Duration::from_millis(16),
        frame_index: 1,
    });
    assert_eq!(other_app.state().theme, Theme::Dark);
    app.frame(FrameInfo {
        delta: Duration::from_millis(16),
        frame_index: 1,
    });
    assert_eq!(app.state().theme, Theme::Light);
    assert!(app.latest_stats().transition_active);
    let elapsed = app.latest_stats().transition_elapsed_us;
    let button = app
        .find_element_bounds_by_id("#sim-backend-indicator")
        .unwrap();
    assert!(cssimpler::renderer::dispatch_click(
        app.scene(),
        button.x + button.width / 2.0,
        button.y + button.height / 2.0
    ));
    app.frame(FrameInfo {
        delta: Duration::from_millis(16),
        frame_index: 2,
    });
    assert!(app.state().solver_menu_open);
    assert!(app.latest_stats().transition_active);
    assert!(app.latest_stats().transition_elapsed_us > elapsed);
    let button = app
        .find_element_bounds_by_id("#sim-backend-indicator")
        .unwrap();
    assert!(cssimpler::renderer::dispatch_click(
        app.scene(),
        button.x + button.width / 2.0,
        button.y + button.height / 2.0
    ));
    app.frame(FrameInfo {
        delta: Duration::from_millis(16),
        frame_index: 3,
    });
    assert!(!app.state().solver_menu_open);

    // Repeated worker results must not prevent either theme endpoint.
    let dark_x = other_app
        .find_element_bounds_by_id("#theme-switch-thumb")
        .unwrap()
        .x;
    app.state_mut().playing = true;
    for i in 0..40 {
        let state = app.state();
        let done = worker::Done {
            generation: state.generation,
            had_grab: false,
            result: Ok((
                state.scene.state.clone(),
                VbdMetrics {
                    step_ms: i as f64 * 0.1,
                    ..VbdMetrics::default()
                },
            )),
        };
        *state.worker_latest.lock().unwrap() = Some(done);
        app.frame(FrameInfo {
            delta: Duration::from_millis(16),
            frame_index: 100 + i,
        });
    }
    let light_x = app
        .find_element_bounds_by_id("#theme-switch-thumb")
        .unwrap()
        .x;
    assert!(
        (dark_x - light_x - 24.0).abs() < 0.1,
        "dark {dark_x}, light {light_x}"
    );
    let button = app.find_element_bounds_by_id("#theme-toggle").unwrap();
    assert!(cssimpler::renderer::dispatch_click(
        app.scene(),
        button.x + button.width / 2.0,
        button.y + button.height / 2.0
    ));
    for i in 0..40 {
        let state = app.state();
        let done = worker::Done {
            generation: state.generation,
            had_grab: false,
            result: Ok((
                state.scene.state.clone(),
                VbdMetrics {
                    step_ms: i as f64 * 0.1,
                    ..VbdMetrics::default()
                },
            )),
        };
        *state.worker_latest.lock().unwrap() = Some(done);
        app.frame(FrameInfo {
            delta: Duration::from_millis(16),
            frame_index: 200 + i,
        });
    }
    app.state_mut().playing = false;
    app.state_mut().step_count = 0;
    let final_x = app
        .find_element_bounds_by_id("#theme-switch-thumb")
        .unwrap()
        .x;
    assert!(
        (dark_x - final_x).abs() < 0.1,
        "dark {dark_x}, final {final_x}"
    );

    // Structural controls must update even while simulation is playing.
    app.state_mut().playing = true;
    let button = app
        .find_element_bounds_by_id("#sim-backend-indicator")
        .unwrap();
    assert!(cssimpler::renderer::dispatch_click(
        app.scene(),
        button.x + button.width / 2.0,
        button.y + button.height / 2.0
    ));
    app.frame(FrameInfo {
        delta: Duration::from_millis(16),
        frame_index: 4,
    });
    assert!(app.state().solver_menu_open);
    assert!(app.find_element_bounds_by_id("#solver-dropdown").is_some());
    app.state_mut().playing = false;
    let button = app
        .find_element_bounds_by_id("#sim-backend-indicator")
        .unwrap();
    assert!(cssimpler::renderer::dispatch_click(
        app.scene(),
        button.x + button.width / 2.0,
        button.y + button.height / 2.0
    ));
    app.frame(FrameInfo {
        delta: Duration::from_millis(16),
        frame_index: 5,
    });
    assert!(!app.state().solver_menu_open);

    // Verify layout containers
    assert!(app.find_element_bounds_by_id("#viewport-3d").is_some());
    let usd_bounds = app
        .find_element_bounds_by_id("#runner-viewport-3d-scene")
        .expect("USD viewport exists");
    assert!(
        usd_bounds.width > 100.0 && usd_bounds.height > 100.0,
        "USD viewport must have drawable area: {usd_bounds:?}"
    );
    assert!(app.find_element_bounds_by_id("#viewport-2d").is_some());
    assert!(app.find_element_bounds_by_id("#right-dock").is_some());

    // The menu is rendered in the live app; changing backend transfers the
    // displayed checkpoint without stepping or initializing a GPU.
    let checkpoint = app.state().scene.state.clone();
    let generation = app.state().generation;
    let button = app
        .find_element_bounds_by_id("#sim-backend-indicator")
        .expect("solver button");
    assert!(cssimpler::renderer::dispatch_click(
        app.scene(),
        button.x + button.width / 2.0,
        button.y + button.height / 2.0
    ));
    app.frame(FrameInfo {
        delta: Duration::from_millis(16),
        frame_index: 10,
    });
    assert!(app.state().solver_menu_open);
    let menu = app
        .find_element_bounds_by_id("#solver-dropdown")
        .expect("open solver menu");
    assert!(menu.width > 100.0 && menu.height > 20.0);
    assert!(app.find_element_bounds_by_id("#solver-wgsl-ipc").is_some());
    let button = app
        .find_element_bounds_by_id("#solver-wgsl-ipc")
        .expect("solver button");
    assert!(cssimpler::renderer::dispatch_click(
        app.scene(),
        button.x + button.width / 2.0,
        button.y + button.height / 2.0
    ));
    app.frame(FrameInfo {
        delta: Duration::from_millis(16),
        frame_index: 11,
    });
    assert!(app.state().gpu_sim);
    assert!(!app.state().playing && !app.state().solver_menu_open);
    assert_eq!(app.state().generation, generation + 1);
    assert_eq!(app.state().scene.state.x, checkpoint.x);
    assert_eq!(app.state().scene.state.v, checkpoint.v);
    assert_eq!(app.state().scene.state.time, checkpoint.time);
    let button = app
        .find_element_bounds_by_id("#sim-backend-indicator")
        .unwrap();
    assert!(cssimpler::renderer::dispatch_click(
        app.scene(),
        button.x + button.width / 2.0,
        button.y + button.height / 2.0
    ));
    app.frame(FrameInfo {
        delta: Duration::from_millis(16),
        frame_index: 12,
    });
    let button = app.find_element_bounds_by_id("#solver-barrier-al").unwrap();
    assert!(cssimpler::renderer::dispatch_click(
        app.scene(),
        button.x + button.width / 2.0,
        button.y + button.height / 2.0
    ));
    app.frame(FrameInfo {
        delta: Duration::from_millis(16),
        frame_index: 13,
    });
    assert_eq!(app.state().contact_method, ContactMethod::BarrierAl);
    assert_eq!(app.state().generation, generation + 2);
    assert_eq!(app.state().scene.state.x, checkpoint.x);
    let button = app
        .find_element_bounds_by_id("#sim-backend-indicator")
        .unwrap();
    assert!(cssimpler::renderer::dispatch_click(
        app.scene(),
        button.x + button.width / 2.0,
        button.y + button.height / 2.0
    ));
    app.frame(FrameInfo {
        delta: Duration::from_millis(16),
        frame_index: 12,
    });
    let button = app.find_element_bounds_by_id("#solver-cpu").unwrap();
    assert!(cssimpler::renderer::dispatch_click(
        app.scene(),
        button.x + button.width / 2.0,
        button.y + button.height / 2.0
    ));
    app.frame(FrameInfo {
        delta: Duration::from_millis(16),
        frame_index: 12,
    });
    assert!(!app.state().gpu_sim);
    assert_eq!(app.state().generation, generation + 3);
    assert_eq!(app.state().scene.state.x, checkpoint.x);

    // Headless RGBA8 rendering
    let width = 640;
    let height = 480;
    let mut rgba = vec![0_u8; width * height * 4];
    app.render_rgba8(
        FrameInfo {
            delta: Duration::from_millis(16),
            frame_index: 1,
        },
        &mut rgba,
        width,
        height,
        1.0,
        cssimpler::core::Color::rgba(0, 0, 0, 0),
    );
    assert!(
        rgba.iter().any(|&b| b > 0),
        "rendered buffer should have painted pixels"
    );

    // Deliver an accepted worker snapshot without running GPU work.
    let read_points = |state: &CssRunnerState| {
        let path = clothcraft::usd_scene::viewport_panel_path(&state.scene.garment.panels[0].id);
        state
            .usd_scene
            .as_ref()
            .unwrap()
            .stage()
            .attribute(format!("{path}.points"))
            .unwrap()
            .get::<Vec<cssimpler::renderer::gpu::openusd::gf::Vec3f>>()
            .unwrap()
            .unwrap()
    };
    let initial_points = read_points(app.state());
    let mut moved = app.state().scene.state.clone();
    moved.x[0].y += 0.01;
    *app.state().worker_latest.lock().unwrap() = Some(worker::Done {
        generation: app.state().generation,
        had_grab: false,
        result: Ok((moved, VbdMetrics::default())),
    });
    update_runner(
        app.state_mut(),
        FrameInfo {
            delta: Duration::from_millis(16),
            frame_index: 2,
        },
    );
    assert_ne!(read_points(app.state()), initial_points);
    // Save the visible accepted checkpoint, not the launch-time project cache.
    app.state_mut().save_project();
    let saved =
        FittingProject::load(&app.state().files.output.join("fitting-project.usda")).unwrap();
    let checkpoint = saved.state.unwrap();
    assert_eq!(checkpoint.x, app.state().scene.state.x);
    assert_eq!(checkpoint.v, app.state().scene.state.v);
    assert_eq!(checkpoint.time, app.state().scene.state.time);
    assert_eq!(saved.simulation.unwrap().dt, app.state().settings.dt);

    app.set_viewport(cssimpler::renderer::ViewportSize::new(1600, 980));
    app.frame(FrameInfo {
        delta: Duration::from_millis(16),
        frame_index: 3,
    });
    let bounds = app
        .find_element_bounds_by_id("#runner-viewport-3d-scene")
        .unwrap();
    assert!(
        bounds.width > 100.0
            && bounds.height > 100.0
            && bounds.x >= 0.0
            && bounds.x + bounds.width <= 1600.0
            && bounds.y + bounds.height <= 980.0,
        "live USD host must remain inside the window: {bounds:?}"
    );

    app.state_mut().reset_simulation();
    assert_eq!(read_points(app.state()), initial_points);

    // Native USD hosts are hidden if later text paint overlaps their area.
    // Check expanded diagnostics as well as Ready, across window sizes.
    for (width, height) in [(1600, 980), (1280, 800), (1024, 768), (800, 600)] {
        app.set_viewport(cssimpler::renderer::ViewportSize::new(width, height));
        for message in ["Ready", "Step 124: min dist 0.124 mm, res 0.0e0, 65.2 ms"] {
            app.state_mut().status_message = message.into();
            let roots = app.frame(FrameInfo {
                delta: Duration::from_millis(16),
                frame_index: 4,
            });
            let viewport = app
                .find_element_bounds_by_id("#runner-viewport-3d-scene")
                .unwrap();
            let paint = cssimpler::core::ExtractedScene::from_render_roots(&roots);
            for item in paint
                .items
                .iter()
                .filter(|item| item.kind == cssimpler::core::ExtractedPaintKind::TextRun)
            {
                let text = item.layout;
                let overlaps = text.x < viewport.x + viewport.width
                    && viewport.x < text.x + text.width
                    && text.y < viewport.y + viewport.height
                    && viewport.y < text.y + text.height;
                assert!(
                    !overlaps,
                    "text must not hide USD at {width}x{height}: {:?} {:?} over {viewport:?}",
                    item.text, text
                );
            }
        }
    }

    // Test Play / Step
    app.state_mut().playing = true;
    app.state_mut().trigger_worker_step(true);
    assert!(app.state().worker_pending);

    // Test Reset
    app.state_mut().reset_simulation();
    assert!(!app.state().playing);
    assert_eq!(app.state().step_count, 0);

    // Test Category Tab Switching
    assert_eq!(app.state().active_category, ObjectCategory::Pattern2D);
    app.state_mut().active_category = ObjectCategory::Sewing;
    assert_eq!(app.state().active_category, ObjectCategory::Sewing);

    // Test Tool Switching
    app.state_mut().active_2d_tool = "Corner".to_string();
    assert_eq!(app.state().active_2d_tool, "Corner");
}

#[test]
fn test_all_runner_buttons_click_and_layout_bounds() {
    let _guard = worker::WORKER_TEST_LOCK
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let stylesheet = load_studio_stylesheet().expect("load studio stylesheet");
    let dir = tempfile::tempdir().expect("tempdir");
    let avatar = dir.path().join("avatar.obj");
    std::fs::write(&avatar, "v 0 0 -10\nv 1 0 -10\nv 0 1 -10\nf 1 2 3\n").expect("write avatar");
    let project = FittingProject::basic(GarmentKind::Tshirt, 0, avatar);
    let scene = project.compile().expect("compile scene");
    let files = DocumentFiles {
        output: dir.path().join("out_buttons"),
        source: None,
        modified: false,
    };
    let (tx, latest) = worker::start_cpu().expect("start cpu worker");
    let state = CssRunnerState::new(project, scene, files, tx, latest);
    let mut app = create_runner_app(state, &stylesheet);

    fn check_container_buttons(
        roots: &[cssimpler::renderer::RenderNode],
        container_id: &str,
        win_w: f32,
        win_h: f32,
    ) {
        fn find_node<'a>(
            nodes: &'a [cssimpler::renderer::RenderNode],
            id: &str,
        ) -> Option<&'a cssimpler::renderer::RenderNode> {
            for n in nodes {
                if n.id() == Some(id) {
                    return Some(n);
                }
                if let Some(found) = find_node(&n.children, id) {
                    return Some(found);
                }
            }
            None
        }
        fn collect_clickable(
            node: &cssimpler::renderer::RenderNode,
            out: &mut Vec<cssimpler::core::LayoutBox>,
        ) {
            if node.handlers.click().is_some() {
                out.push(node.layout);
            }
            for child in &node.children {
                collect_clickable(child, out);
            }
        }
        let root = find_node(roots, container_id)
            .unwrap_or_else(|| panic!("missing container #{container_id}"));
        let sew_button = match container_id {
            "rail-2d" => Some("tool-sew-2d"),
            "rail-3d" => Some("tool-sew-3d"),
            _ => None,
        };
        if let Some(id) = sew_button {
            let button = find_node(&root.children, id).expect("missing Sew button");
            assert!(
                button.children.iter().any(|child| {
                    matches!(&child.kind, cssimpler::core::RenderKind::Svg(scene)
                        if scene.paths.len() == 2 && scene.bounds.is_some())
                }),
                "expected visible edge and stitch paths inside #{id}"
            );
        }
        let mut btns = Vec::new();
        collect_clickable(root, &mut btns);
        assert!(
            !btns.is_empty(),
            "expected clickable controls inside #{container_id}"
        );
        for (i, b) in btns.iter().enumerate() {
            assert!(
                b.width >= 10.0
                    && b.height >= 10.0
                    && b.x >= -1.0
                    && b.y >= -1.0
                    && b.x + b.width <= win_w + 1.0
                    && b.y + b.height <= win_h + 1.0,
                "button #{i} in #{container_id} out of bounds at {win_w}x{win_h}: {b:?}"
            );
        }
    }

    // 1. Audit layout bounds across 1600x980 and 1280x800
    for (w, h) in [(1600_usize, 980_usize), (1280_usize, 800_usize)] {
        app.state_mut().window_size = (w as f32, h as f32);
        app.set_viewport(cssimpler::renderer::ViewportSize::new(w, h));
        let roots = app.frame(FrameInfo {
            delta: Duration::from_millis(16),
            frame_index: w as u64,
        });
        for cid in [
            "top-bar",
            "rail-3d",
            "rail-2d",
            "left-browser",
            "right-dock",
        ] {
            check_container_buttons(&roots, cid, w as f32, h as f32);
        }
    }

    // 2. Click buttons across #rail-3d, #rail-2d, #right-dock, and #command-palette-trigger
    app.state_mut().window_size = (1600.0, 980.0);
    app.set_viewport(cssimpler::renderer::ViewportSize::new(1600, 980));
    let mut frame = 100_u64;
    app.frame(FrameInfo {
        delta: Duration::from_millis(16),
        frame_index: frame,
    });

    // Verify clicking all 2D tools via actions
    for (act, expected) in [
        (RunnerAction::ToolRect2d, "Rect"),
        (RunnerAction::ToolPen2d, "Pen"),
        (RunnerAction::ToolFree2d, "Freehand"),
        (RunnerAction::ToolSew2d, "Sew"),
        (RunnerAction::ToolSplit2d, "Split"),
        (RunnerAction::ToolInternal2d, "Internal Line"),
        (RunnerAction::ToolMeasure2d, "Measure"),
        (RunnerAction::ToolTrim2d, "Trim"),
        (RunnerAction::ToolCornerSelect2d, "Corner Select"),
        (RunnerAction::ToolCorner2d, "Corner"),
        (RunnerAction::ToolEdit2d, "Edit"),
    ] {
        frame += 1;
        app.state_mut().actions.push(act);
        app.frame(FrameInfo {
            delta: Duration::from_millis(16),
            frame_index: frame,
        });
        assert_eq!(app.state().active_2d_tool, expected);
    }

    // Verify clicking all 3D tools via actions
    for (act, expected) in [
        (RunnerAction::ToolGrab, "Grab"),
        (RunnerAction::ToolSew3d, "3D Sew"),
        (RunnerAction::ToolPin, "Pin"),
        (RunnerAction::ToolMeasure3d, "Measure"),
        (RunnerAction::ToolAvatarDiag, "Avatar Diagnostics"),
        (RunnerAction::ToolGizmo, "Gizmo"),
    ] {
        frame += 1;
        app.state_mut().actions.push(act);
        app.frame(FrameInfo {
            delta: Duration::from_millis(16),
            frame_index: frame,
        });
        assert_eq!(app.state().active_3d_tool, expected);
    }

    // Verify clicking #command-palette-trigger opens palette and closing works
    let pal_btn = app
        .find_element_bounds_by_id("#command-palette-trigger")
        .unwrap();
    assert!(cssimpler::renderer::dispatch_click(
        app.scene(),
        pal_btn.x + pal_btn.width * 0.5,
        pal_btn.y + pal_btn.height * 0.5
    ));
    frame += 1;
    app.frame(FrameInfo {
        delta: Duration::from_millis(16),
        frame_index: frame,
    });
    assert!(app.state().command_palette_open);
    let pal_box = app
        .find_element_bounds_by_id("#command-palette-container")
        .unwrap();
    assert!(pal_box.width > 100.0 && pal_box.height > 100.0);

    // Toggle palette back off
    frame += 1;
    app.state_mut().actions.push(RunnerAction::PaletteToggle);
    app.frame(FrameInfo {
        delta: Duration::from_millis(16),
        frame_index: frame,
    });
    assert!(!app.state().command_palette_open);

    // Clean view + layout modes + drawer toggle roundtrip
    for (act, expected_mode) in [
        (RunnerAction::View3d, ViewLayoutMode::ThreeDOnly),
        (RunnerAction::View2d, ViewLayoutMode::TwoDOnly),
        (RunnerAction::ViewSplit, ViewLayoutMode::Split),
    ] {
        frame += 1;
        app.state_mut().actions.push(act);
        app.frame(FrameInfo {
            delta: Duration::from_millis(16),
            frame_index: frame,
        });
        assert_eq!(app.state().layout_mode, expected_mode);
    }
}

#[test]
fn viewport_shading_toggle_rewrites_usd_normals() {
    use cssimpler::renderer::gpu::openusd::gf;
    use cssimpler::renderer::gpu::openusd_schemas::geom::AttributeMetadata;

    let _guard = worker::WORKER_TEST_LOCK
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let stylesheet = load_studio_stylesheet().expect("load studio stylesheet");
    let dir = tempfile::tempdir().expect("tempdir");
    let avatar = dir.path().join("avatar.obj");
    std::fs::write(&avatar, "v 0 0 -10\nv 1 0 -10\nv 0 1 -10\nf 1 2 3\n").expect("write avatar");
    let project = FittingProject::basic(GarmentKind::Tshirt, 1, avatar);
    let scene = project.compile().expect("compile scene");
    let files = DocumentFiles {
        output: dir.path().join("shading_out"),
        source: None,
        modified: false,
    };
    let (tx, latest) = worker::start_cpu().expect("start cpu worker");
    let state = CssRunnerState::new(project, scene, files, tx, latest);
    let mut app = create_runner_app(state, &stylesheet);
    app.set_viewport(cssimpler::renderer::ViewportSize::new(1600, 980));
    app.frame(FrameInfo {
        delta: Duration::ZERO,
        frame_index: 0,
    });

    let panel_path =
        clothcraft::usd_scene::viewport_panel_path(&app.state().scene.garment.panels[0].id);
    let read = |state: &CssRunnerState| {
        let attribute = state
            .usd_scene
            .as_ref()
            .unwrap()
            .stage()
            .attribute(format!("{panel_path}.normals"))
            .unwrap();
        let interpolation = attribute.interpolation().unwrap().unwrap();
        let values = attribute.get::<Vec<gf::Vec3f>>().unwrap().unwrap();
        (interpolation, values)
    };

    assert_eq!(
        app.state().shading,
        clothcraft::usd_scene::ShadingMode::Smooth
    );
    let (interpolation, smooth) = read(app.state());
    assert_eq!(interpolation.as_str(), "vertex");
    assert_eq!(
        smooth.len(),
        app.state().scene.garment.panels[0].vertices.len()
    );

    let shading_button = app.find_element_bounds_by_id("#shading-toggle").unwrap();
    assert!(cssimpler::renderer::dispatch_click(
        app.scene(),
        shading_button.x + shading_button.width * 0.5,
        shading_button.y + shading_button.height * 0.5
    ));
    app.frame(FrameInfo {
        delta: Duration::from_millis(16),
        frame_index: 1,
    });
    assert_eq!(
        app.state().shading,
        clothcraft::usd_scene::ShadingMode::Flat
    );
    let (interpolation, flat) = read(app.state());
    assert_eq!(interpolation.as_str(), "faceVarying");
    assert_eq!(
        flat.len(),
        app.state().scene.garment.panels[0].triangles.len() * 3
    );
    assert!(
        flat.as_chunks::<3>()
            .0
            .iter()
            .all(|corner| corner[0] == corner[1] && corner[1] == corner[2])
    );

    assert!(cssimpler::renderer::dispatch_click(
        app.scene(),
        shading_button.x + shading_button.width * 0.5,
        shading_button.y + shading_button.height * 0.5
    ));
    app.frame(FrameInfo {
        delta: Duration::from_millis(16),
        frame_index: 2,
    });
    assert_eq!(
        app.state().shading,
        clothcraft::usd_scene::ShadingMode::Smooth
    );
    let (interpolation, roundtrip) = read(app.state());
    assert_eq!(interpolation.as_str(), "vertex");
    assert_eq!(roundtrip, smooth);

    // A moved vertex must refresh the authored normals, not leave launch-time values.
    let panel = &app.state().scene.garment.panels[0];
    let vertex = app.state().scene.garment.mesh.triangles[panel.triangles.start].vertices[0];
    let direction = smooth[0];
    app.state_mut().scene.state.x[vertex] += glam::DVec3::new(
        f64::from(direction.x),
        f64::from(direction.y),
        f64::from(direction.z),
    ) * 0.05;
    app.state_mut().sync_usd_points();
    let (interpolation, moved) = read(app.state());
    assert_eq!(interpolation.as_str(), "vertex");
    assert_ne!(moved, smooth);
}

#[test]
fn viewport_wireframe_toggle_strokes_only_garment_panels() {
    let _guard = worker::WORKER_TEST_LOCK
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let stylesheet = load_studio_stylesheet().expect("load studio stylesheet");
    let dir = tempfile::tempdir().expect("tempdir");
    let avatar = dir.path().join("avatar.obj");
    std::fs::write(&avatar, "v 0 0 -10\nv 1 0 -10\nv 0 1 -10\nf 1 2 3\n").expect("write avatar");
    let project = FittingProject::basic(GarmentKind::Tshirt, 1, avatar);
    let scene = project.compile().expect("compile scene");
    let output = dir.path().join("wireframe_out");
    let files = DocumentFiles {
        output: output.clone(),
        source: None,
        modified: false,
    };
    let (tx, latest) = worker::start_cpu().expect("start cpu worker");
    let state = CssRunnerState::new(project, scene, files, tx, latest);
    let toggle = state.viewport_wireframe.clone();
    let mut app = create_runner_app(state, &stylesheet);
    app.set_viewport(cssimpler::renderer::ViewportSize::new(1600, 980));
    app.frame(FrameInfo {
        delta: Duration::ZERO,
        frame_index: 0,
    });

    let shared = || *toggle.lock().unwrap_or_else(|e| e.into_inner());

    // The display stage carries the garment panels and the collider, and no
    // longer a ribbon wireframe mesh: the stroke lives in the GPU renderer.
    let scene = app
        .state()
        .usd_scene
        .as_ref()
        .expect("viewport scene")
        .clone();
    let stage = scene.stage();
    assert!(stage.root_layer().is_anonymous());
    assert!(!output.join("runner_preview.usda").exists());
    for path in ["/World/Garment", "/Collider", "/InfinityCove"] {
        assert!(
            stage.prim(path).unwrap().is_valid().unwrap(),
            "missing {path}"
        );
    }

    assert!(!app.state().wireframe);
    assert!(!shared().enabled);
    assert_eq!(shared().colour, 0);
    assert_eq!(shared().width_px, 2.0);
    let ui = super::super::build_runner_ui(app.state());
    assert_eq!(
        find_title(&ui, "wireframe-toggle"),
        Some("Show cloth wireframe")
    );
    assert_eq!(
        find_title(&ui, "shading-toggle"),
        Some("Smooth shading — click for flat shading")
    );

    let button = app.find_element_bounds_by_id("#wireframe-toggle").unwrap();
    assert!(cssimpler::renderer::dispatch_click(
        app.scene(),
        button.x + button.width * 0.5,
        button.y + button.height * 0.5
    ));
    app.frame(FrameInfo {
        delta: Duration::from_millis(16),
        frame_index: 1,
    });
    assert!(app.state().wireframe);
    assert!(shared().enabled);
    let ui = super::super::build_runner_ui(app.state());
    assert_eq!(
        find_title(&ui, "wireframe-toggle"),
        Some("Hide cloth wireframe")
    );

    // Toggling the stroke must not re-author overlay geometry into the stage.
    assert!(!stage.prim("/World/ClothWire").unwrap().is_valid().unwrap());

    assert!(cssimpler::renderer::dispatch_click(
        app.scene(),
        button.x + button.width * 0.5,
        button.y + button.height * 0.5
    ));
    app.frame(FrameInfo {
        delta: Duration::from_millis(16),
        frame_index: 2,
    });
    assert!(!app.state().wireframe);
    assert!(!shared().enabled);
}

fn find_title<'a>(node: &'a cssimpler::core::Node, id: &str) -> Option<&'a str> {
    match node {
        cssimpler::core::Node::Element(element) => {
            if element.id.as_deref() == Some(id) {
                return element.attribute("title");
            }
            for child in &element.children {
                if let Some(title) = find_title(child, id) {
                    return Some(title);
                }
            }
            None
        }
        _ => None,
    }
}

#[test]
fn view_cube_clicks_use_native_viewport_coordinates() {
    use super::super::input::handle_runner_event;
    use super::support::{pointer_btn_ev, pointer_move_ev, selection_test_state};
    use clothcraft::render::PresetView;
    use glam::Vec3;

    let _guard = worker::WORKER_TEST_LOCK
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    let dir = tempfile::tempdir().unwrap();
    let stylesheet = load_studio_stylesheet().unwrap();
    let mut app = create_runner_app(selection_test_state(dir.path()), &stylesheet);
    for (width, height) in [(1600, 980), (1024, 768)] {
        let size = cssimpler::renderer::ViewportSize::new(width, height);
        app.set_viewport(size);
        handle_runner_event(
            app.state_mut(),
            &cssimpler::renderer::EngineEvent::ViewportChanged(
                cssimpler::renderer::ViewportEvent {
                    width,
                    height,
                    scale_factor: 1.0,
                },
            ),
        );
        app.frame(FrameInfo {
            delta: Duration::from_millis(16),
            frame_index: 0,
        });
        let host = app
            .find_element_bounds_by_id("#runner-viewport-3d-scene")
            .unwrap();
        let centre_x = host.x + host.width - 62.75;
        let centre_y = host.y + 62.75;
        let state = app.state_mut();
        let target = state.camera.target;
        let distance = state.camera.distance;
        for (x, y, direction) in [
            (centre_x, centre_y, Vec3::Z),
            (centre_x + 25.0, centre_y, (Vec3::X + Vec3::Z).normalize()),
            (centre_x + 25.0, centre_y - 25.0, Vec3::ONE.normalize()),
        ] {
            state.camera.apply_preset(PresetView::Front);
            handle_runner_event(state, &pointer_move_ev(x, y));
            state.sync_view_cube_hover();
            assert!(
                state
                    .viewport_cube_hover
                    .get()
                    .unwrap()
                    .abs_diff_eq(direction, 1e-5)
            );
            assert!(handle_runner_event(state, &pointer_btn_ev(true)));
            handle_runner_event(state, &pointer_btn_ev(false));
            assert!(state.camera.direction().abs_diff_eq(direction, 1e-5));
            assert!(!state.dragging_3d);
            assert_eq!(state.camera.target, target);
            assert_eq!(state.camera.distance, distance);
        }
        handle_runner_event(state, &cssimpler::renderer::EngineEvent::PointerLeft);
        state.sync_view_cube_hover();
        assert_eq!(state.viewport_cube_hover.get(), None);
    }
}

#[test]
fn shadow_sliders_live_in_settings_and_update_independently() {
    use super::super::input::handle_runner_event;
    use super::support::{pointer_btn_ev, pointer_move_ev, selection_test_state};
    use clothcraft::css_ui::ViewLayoutMode;
    use cssimpler::core::RenderNode;
    use cssimpler::renderer::{
        EngineEvent, SceneProvider, ViewportEvent, ViewportSize, gpu::usd::UsdShadowSettings,
    };

    fn node<'a>(roots: &'a [RenderNode], id: &str) -> Option<&'a RenderNode> {
        roots.iter().find_map(|root| {
            if root.id() == Some(id) {
                Some(root)
            } else {
                node(&root.children, id)
            }
        })
    }

    let _guard = worker::WORKER_TEST_LOCK
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    let dir = tempfile::tempdir().unwrap();
    let stylesheet = load_studio_stylesheet().unwrap();
    let state = selection_test_state(dir.path());
    let mut app = create_runner_app(state, &stylesheet);
    let (wake, updates) = std::sync::mpsc::channel();
    app.install_update_wake(std::sync::Arc::new(move || {
        let _ = wake.send(());
    }));
    let frame = FrameInfo {
        delta: Duration::from_millis(16),
        frame_index: 0,
    };
    app.set_viewport(ViewportSize::new(1600, 980));
    app.frame(frame);
    assert!(
        app.find_element_bounds_by_id("#shadow-softness-slider")
            .is_none()
    );
    let default_levels = app.state().viewport_shadow_levels.get();
    let defaults = default_levels.apply(UsdShadowSettings::default());
    assert_eq!(defaults.coverage, 0.0);
    assert_eq!(defaults.resolution, 2048);
    assert!((defaults.light_radius - 0.16).abs() < 1e-6);
    assert!((default_levels.filter_radius_m() - 0.016).abs() < 1e-6);

    for (width, height, mode, clean_view) in [
        (1600, 980, ViewLayoutMode::Split, false),
        (1024, 768, ViewLayoutMode::ThreeDOnly, false),
        (800, 600, ViewLayoutMode::Split, false),
        (800, 600, ViewLayoutMode::ThreeDOnly, true),
    ] {
        app.set_viewport(ViewportSize::new(width, height));
        handle_runner_event(
            app.state_mut(),
            &EngineEvent::ViewportChanged(ViewportEvent {
                width,
                height,
                scale_factor: 1.0,
            }),
        );
        let state = app.state_mut();
        state.layout_mode = mode;
        state.clean_view = clean_view;
        state.settings_open = false;
        app.invalidate(cssimpler::app::Invalidation::Structure);
        app.frame(frame);
        let viewport = app
            .find_element_bounds_by_id("#runner-viewport-3d-scene")
            .unwrap();
        assert!(viewport.width > 0.0);
        app.state_mut().settings_open = true;
        app.invalidate(cssimpler::app::Invalidation::Structure);
        app.frame(frame);
        let panel = app.find_element_bounds_by_id("#settings-panel").unwrap();
        assert_eq!(
            app.find_element_bounds_by_id("#runner-viewport-3d-scene"),
            Some(viewport)
        );
        let backdrop = app.find_element_bounds_by_id("#settings-backdrop").unwrap();
        assert!(
            backdrop.x <= viewport.x && backdrop.x + backdrop.width >= viewport.x + viewport.width
        );
        for id in [
            "shadow-softness-slider",
            "shadow-strength-slider",
            "shadow-coverage-slider",
            "shadow-resolution-slider",
        ] {
            let track = node(app.scene(), id).unwrap().layout;
            assert!(track.x >= panel.x && track.x + track.width <= panel.x + panel.width);
            assert!(track.y >= panel.y && track.y + track.height <= panel.y + panel.height);
            let y = track.y + track.height / 2.0;
            let fraction = if id == "shadow-resolution-slider" {
                2.0 / 3.0
            } else {
                0.5
            };
            handle_runner_event(
                app.state_mut(),
                &pointer_move_ev(track.x + track.width * fraction, y),
            );
            handle_runner_event(app.state_mut(), &pointer_btn_ev(true));
            while updates.try_recv().is_ok() {}
            node(app.scene(), id).unwrap().handlers.mousedown().unwrap()();
            assert!(
                updates.try_recv().is_ok(),
                "slider press should wake the paused editor"
            );
            app.frame(frame);
            let levels = app.state().viewport_shadow_levels.get();
            let settings = levels.apply(defaults);
            let drag_x = match id {
                "shadow-strength-slider" => {
                    assert!(
                        (settings.strength - 0.50).abs() <= 0.01,
                        "{track:?} {settings:?}"
                    );
                    Some(track.x - 20.0)
                }
                "shadow-coverage-slider" => {
                    assert!(
                        (settings.coverage - 0.16).abs() <= 0.0033,
                        "{track:?} {settings:?}"
                    );
                    Some(track.x + track.width + 20.0)
                }
                "shadow-resolution-slider" => {
                    assert_eq!(settings.resolution, 2048);
                    Some(track.x + track.width + 20.0)
                }
                _ => {
                    assert!(
                        (settings.light_radius - 0.20).abs() < 0.005,
                        "{track:?} {settings:?}"
                    );
                    assert!((levels.filter_radius_m() - 0.044).abs() <= 0.005);
                    Some(track.x + track.width + 20.0)
                }
            };
            if let Some(x) = drag_x {
                handle_runner_event(app.state_mut(), &pointer_move_ev(x, y));
                assert!(
                    updates.try_recv().is_ok(),
                    "slider drag should wake the paused editor"
                );
                app.frame(frame);
            }
            assert!(handle_runner_event(app.state_mut(), &pointer_btn_ev(false)));
        }
        let levels = app.state().viewport_shadow_levels.get();
        let settings = levels.apply(defaults);
        assert_eq!(settings.strength, 0.0);
        assert_eq!(settings.coverage, 0.32);
        assert_eq!(settings.resolution, 4096);
        assert_eq!(levels.filter_radius_m(), 1.0);
        assert_eq!(settings.light_radius, 0.5);
        assert!(!app.state().dragging_3d && !app.state().pointer_down);
    }
}
