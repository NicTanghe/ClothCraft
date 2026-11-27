//! Inspector tabs, category routing and property sections.

use self::categories::{
    build_cat_environment, build_cat_form_3d, build_cat_material, build_cat_pattern_2d,
    build_cat_sewing,
};
use self::properties::build_runner_property_accordions;
use super::super::actions::{RunnerAction, queue_action};
use super::super::state::{CssRunnerState, ObjectCategory};
use cssimpler::core::Node;
use cssimpler::ui;

pub(in super::super) mod categories;
pub(in super::super) mod properties;

pub(in super::super) fn build_runner_right_dock(state: &CssRunnerState) -> Node {
    if state.clean_view {
        return ui! { <aside class="right-dock collapsed"></aside> };
    }

    let (tp, ts, tf, tm, te) = match state.active_category {
        ObjectCategory::Pattern2D => (
            "dock-tab active",
            "dock-tab",
            "dock-tab",
            "dock-tab",
            "dock-tab",
        ),
        ObjectCategory::Sewing => (
            "dock-tab",
            "dock-tab active",
            "dock-tab",
            "dock-tab",
            "dock-tab",
        ),
        ObjectCategory::Form3D => (
            "dock-tab",
            "dock-tab",
            "dock-tab active",
            "dock-tab",
            "dock-tab",
        ),
        ObjectCategory::Material => (
            "dock-tab",
            "dock-tab",
            "dock-tab",
            "dock-tab active",
            "dock-tab",
        ),
        ObjectCategory::Environment => (
            "dock-tab",
            "dock-tab",
            "dock-tab",
            "dock-tab",
            "dock-tab active",
        ),
    };

    let content = match state.active_category {
        ObjectCategory::Pattern2D => build_cat_pattern_2d(state),
        ObjectCategory::Sewing => build_cat_sewing(state),
        ObjectCategory::Form3D => build_cat_form_3d(state),
        ObjectCategory::Material => build_cat_material(state),
        ObjectCategory::Environment => build_cat_environment(state),
    };

    ui! {
        <aside id="right-dock" class="sidebar right-object-browser-panel">
            <div id="object-browser">
                <div class="dock-section-title">"Object Browser"</div>
                <div class="dock-tabs">
                    <button class={tp} onclick_shared={queue_action(&state.actions, RunnerAction::TabPattern)} title="2D Pattern">"2D"</button>
                    <button id="sewing-tab" class={ts} onclick_shared={queue_action(&state.actions, RunnerAction::TabSewing)} title="Sewing modes and existing seams">"Sew"</button>
                    <button class={tf} onclick_shared={queue_action(&state.actions, RunnerAction::TabForm)} title="3D Form">"3D"</button>
                    <button class={tm} onclick_shared={queue_action(&state.actions, RunnerAction::TabMaterial)} title="Material">"Mat."</button>
                    <button class={te} onclick_shared={queue_action(&state.actions, RunnerAction::TabEnv)} title="Environment">"Env."</button>
                </div>
                <div class="object-browser-content-viewport">
                    {content}
                </div>
            </div>
            <div id="property-editor">
                <div class="dock-section-title">"Property Editor"</div>
                {build_runner_property_accordions(state)}
            </div>
        </aside>
    }
}
