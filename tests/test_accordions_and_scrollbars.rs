//! Tests for independent accordion expand/collapse and thin scrollbar styling.
#![cfg(feature = "desktop")]

use clothcraft::atelier::css_runner::RunnerAction;

#[test]
fn typed_action_queue_preserves_order_and_duplicates() {
    use cssimpler::ActionQueue;

    let queue = ActionQueue::new();
    for action in [
        RunnerAction::Step,
        RunnerAction::Accordion(0),
        RunnerAction::Step,
    ] {
        queue.push(action);
    }

    let mut batch = Vec::new();
    queue.drain_into(&mut batch);
    assert_eq!(
        batch,
        [
            RunnerAction::Step,
            RunnerAction::Accordion(0),
            RunnerAction::Step,
        ],
        "typed actions retain FIFO order and repeated commands"
    );
}

#[test]
fn studio_v2_css_defines_thin_scrollbars_and_collapsed_accordions() {
    let css = include_str!("../src/atelier/css_ui/studio_v2.css");

    // Must define thin scrollbar rules
    assert!(
        css.contains("scrollbar-width: thin;"),
        "studio_v2.css must set scrollbar-width: thin"
    );
    assert!(
        css.contains("scrollbar-color:"),
        "studio_v2.css must define subtle scrollbar-color"
    );

    // Must define collapsed accordion rules
    assert!(
        css.contains(".accordion.collapsed"),
        "studio_v2.css must define .accordion.collapsed rule"
    );
    assert!(
        css.contains(".accordion.collapsed .accordion-content") || css.contains("display: none"),
        "studio_v2.css must hide collapsed accordion content"
    );

    // Must provide container padding for right gutter breathing room
    assert!(
        css.contains(".accordions-container"),
        "studio_v2.css must define .accordions-container"
    );
    assert!(
        css.contains(".drawer-content"),
        "studio_v2.css must define .drawer-content"
    );

    // Must define day-night pill switch styles
    assert!(
        css.contains(".day-night-switch"),
        "studio_v2.css must define .day-night-switch"
    );
    assert!(
        css.contains(".day-night-switch.dark"),
        "studio_v2.css must define dark switch state"
    );
    assert!(
        css.contains(".day-night-switch.light"),
        "studio_v2.css must define light switch state"
    );
    assert!(
        css.contains(".switch-thumb"),
        "studio_v2.css must define .switch-thumb"
    );
    // The same shapes remain mounted while their positions and colors animate.
    for class in [
        ".crater1", ".crater2", ".crater3", ".shape1", ".shape2", ".shape3", ".shape4", ".shape5",
    ] {
        assert!(css.contains(class), "studio_v2.css must define {class}");
    }

    // Must NOT contain AM / PM text labels
    assert!(
        !css.contains("'AM'"),
        "studio_v2.css must not contain 'AM' text label"
    );
    assert!(
        !css.contains("'PM'"),
        "studio_v2.css must not contain 'PM' text label"
    );

    // Must define command palette item icon styling
    assert!(
        css.contains(".palette-item .item-left"),
        "studio_v2.css must define .palette-item .item-left"
    );
    assert!(
        css.contains(".palette-item .icon"),
        "studio_v2.css must define .palette-item .icon"
    );
}

#[test]
fn day_night_switch_keeps_thumb_craters_and_sky_shapes_in_both_modes() {
    use clothcraft::atelier::css_ui::icons::icon_day_night_switch;
    use cssimpler::core::Node;

    let dark_node = icon_day_night_switch(true);
    let light_node = icon_day_night_switch(false);

    let check_switch = |node: &Node, expected_mode: &str| {
        let Node::Element(elem) = node else {
            panic!("switch root must be an ElementNode");
        };
        assert!(
            elem.classes.iter().any(|c| c == expected_mode),
            "switch must have {expected_mode} class"
        );
        assert!(
            elem.classes.iter().any(|c| c == "day-night-switch"),
            "switch must have day-night-switch class"
        );

        // Handler knob
        let handler_node = elem
            .children
            .iter()
            .find(|child| {
                if let Node::Element(e) = child {
                    e.classes.iter().any(|c| c == "switch-thumb")
                } else {
                    false
                }
            })
            .expect("switch must contain handler knob");

        let Node::Element(handler) = handler_node else {
            panic!("handler must be Element");
        };
        assert_eq!(handler.children.len(), 3, "handler must contain 3 craters");
        for i in 1..=3 {
            let crater_class = format!("crater{i}");
            assert!(
                handler.children.iter().any(|child| {
                    if let Node::Element(e) = child {
                        e.classes.iter().any(|c| c == &crater_class)
                    } else {
                        false
                    }
                }),
                "handler must contain {crater_class}"
            );
        }

        // Five persistent shapes animate between clouds and stars.
        for i in 1..=5 {
            let star_class = format!("shape{i}");
            assert!(
                elem.children.iter().any(|child| {
                    if let Node::Element(e) = child {
                        e.classes.iter().any(|c| c == &star_class)
                    } else {
                        false
                    }
                }),
                "switch must contain {star_class}"
            );
        }
    };

    check_switch(&dark_node, "dark");
    check_switch(&light_node, "light");
}

#[test]
fn piece_selection_actions_are_distinct() {
    assert_ne!(RunnerAction::Piece(0), RunnerAction::Piece(1));
    assert_ne!(RunnerAction::Piece(1), RunnerAction::Piece(2));
    assert_ne!(RunnerAction::Piece(2), RunnerAction::Piece(3));
}

#[test]
fn solver_selection_actions_are_distinct_from_editor_actions() {
    let solvers = [
        RunnerAction::SolverMenu,
        RunnerAction::SolverGpu,
        RunnerAction::SolverCpu,
        RunnerAction::SolverAl,
    ];
    let editor = [
        RunnerAction::Accordion(0),
        RunnerAction::Accordion(5),
        RunnerAction::AccordionToggle,
        RunnerAction::Piece(0),
        RunnerAction::Piece(3),
    ];
    for solver in &solvers {
        for action in &editor {
            assert_ne!(
                solver, action,
                "solver selection must not alias accordion or piece actions"
            );
        }
    }
}

#[test]
fn board_2d_actions_are_distinct() {
    assert_ne!(RunnerAction::Zoom2dIn, RunnerAction::Zoom2dOut);
    assert_ne!(RunnerAction::Zoom2dOut, RunnerAction::Zoom2dFit);
    assert_ne!(RunnerAction::Zoom2dFit, RunnerAction::NextPiece2d);
    assert_ne!(RunnerAction::Zoom2dIn, RunnerAction::NextPiece2d);
}

#[test]
fn studio_v2_css_defines_viewport_theme_colors_and_transitions() {
    let css = include_str!("../src/atelier/css_ui/studio_v2.css");

    // Theme color variables for 3D and 2D viewports
    assert!(
        css.contains("--viewport-bg-3d: #0b0d13;"),
        "Dark theme must define dark 3D viewport background"
    );
    assert!(
        css.contains("--viewport-bg-3d: #e8ecf2;"),
        "Light theme must define light 3D viewport background"
    );
    assert!(
        css.contains("--viewport-bg-2d: #0d0f17;"),
        "Dark theme must define dark 2D viewport background"
    );
    assert!(
        css.contains("--viewport-bg-2d: #f8fafc;"),
        "Light theme must define light 2D viewport background"
    );

    // Viewport elements must use the theme variables
    assert!(
        css.contains("background-color: var(--viewport-bg-3d);")
            || css.contains("background-color: var(--viewport-bg-2d);"),
        "Viewports must reference theme background variables"
    );

    // Smooth transition rules across shell and surfaces
    assert!(
        css.contains("transition: background-color 0.45s")
            || css.contains("background-color 0.45s cubic-bezier(0.4, 0, 0.2, 1)"),
        "studio_v2.css must provide smooth color transitions for theme switches"
    );

    // 2D viewport header and interactive controls
    assert!(
        css.contains(".viewport-tools"),
        "studio_v2.css must define .viewport-tools"
    );
    assert!(
        css.contains(".viewport-tool-btn"),
        "studio_v2.css must define .viewport-tool-btn"
    );
}
