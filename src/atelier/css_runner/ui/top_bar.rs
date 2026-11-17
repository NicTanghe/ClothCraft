//! Transport, solver, view and document controls.

use super::super::actions::{
    ACT_RUN_CLEAN_VIEW, ACT_RUN_EXPORT, ACT_RUN_PALETTE_TOGGLE, ACT_RUN_PLAY_PAUSE, ACT_RUN_RESET,
    ACT_RUN_SAVE, ACT_RUN_SOLVER_AL, ACT_RUN_SOLVER_CPU, ACT_RUN_SOLVER_GPU, ACT_RUN_SOLVER_MENU,
    ACT_RUN_STEP, ACT_RUN_THEME_TOGGLE, ACT_RUN_VIEW_2D, ACT_RUN_VIEW_3D, ACT_RUN_VIEW_SPLIT,
    RunnerUiOp, queue_action, queue_ui_op,
};
use super::super::state::CssRunnerState;
use clothcraft::css_ui::icons::{
    icon_clean_view, icon_day_night_switch, icon_palette, icon_pause, icon_play, icon_reset,
    icon_step, icon_view_2d, icon_view_3d, icon_view_split,
};
use clothcraft::css_ui::{Theme, ViewLayoutMode};
use cssimpler::core::Node;
use cssimpler::ui;

pub(in super::super) fn build_runner_top_bar(state: &CssRunnerState) -> Node {
    let play_label = if state.playing { "Pause" } else { "Play" };
    let play_class = if state.playing {
        "transport-btn playing"
    } else {
        "transport-btn"
    };

    let (split_class, threed_class, twod_class) = match state.layout_mode {
        ViewLayoutMode::Split => ("layout-btn active", "layout-btn", "layout-btn"),
        ViewLayoutMode::ThreeDOnly => ("layout-btn", "layout-btn active", "layout-btn"),
        ViewLayoutMode::TwoDOnly => ("layout-btn", "layout-btn", "layout-btn active"),
    };

    let clean_class = if state.clean_view {
        "clean-view-toggle active"
    } else {
        "clean-view-toggle"
    };

    let (backend_class, backend_label) = if state.gpu_sim {
        ("backend-badge gpu", state.contact_method.label())
    } else {
        ("backend-badge cpu", "CPU (OGC)")
    };

    let top_bar_class = if state.window_size.0 < 1400.0 {
        "top-bar compact"
    } else {
        "top-bar"
    };
    let theme_label = match state.theme {
        Theme::Dark => "Theme: Dark",
        Theme::Light => "Theme: Light",
    };

    ui! {
        <header id="top-bar" class={top_bar_class}>
            <div class="top-bar-left">
                <span class="app-title">"ClothCraft"</span>
                <nav class="top-bar-menus">
                    <button class="menu-item" onclick_shared={queue_action(&state.actions, ACT_RUN_SAVE)} title="Save Project (Ctrl+S)">"Save"</button>
                    <button id="runner-export-btn" class="menu-item" onclick_shared={queue_action(&state.actions, ACT_RUN_EXPORT)} title="Export Artifacts">"Export"</button>
                    <button id="settings-toggle" class="menu-item" onclick_shared={queue_ui_op(&state.pending_ui_ops, RunnerUiOp::ToggleSettings)} title="Settings">"Settings"</button>
                </nav>
            </div>
            <div class="transport-controls">
                <button id="sim-play-btn" class={play_class} onclick_shared={queue_action(&state.actions, ACT_RUN_PLAY_PAUSE)} title="Start/Stop simulation (Space)">
                    {if state.playing { icon_pause() } else { icon_play() }}
                    <span class="btn-label">{play_label}</span>
                </button>
                <button id="sim-step-btn" class="transport-btn" onclick_shared={queue_action(&state.actions, ACT_RUN_STEP)} title="Advance single solver step">
                    {icon_step()}
                    <span class="btn-label">"Step"</span>
                </button>
                <button id="sim-reset-btn" class="transport-btn" onclick_shared={queue_action(&state.actions, ACT_RUN_RESET)} title="Reset simulation state">
                    {icon_reset()}
                    <span class="btn-label">"Reset"</span>
                </button>
                <div class="solver-picker">
                    <button id="sim-backend-indicator" class={backend_class} onclick_shared={queue_action(&state.actions, ACT_RUN_SOLVER_MENU)} title="Choose active solver">{format!("{backend_label} ▾")}</button>
                    {if state.solver_menu_open { ui! {
                        <div id="solver-dropdown" class="solver-dropdown">
                            <button id="solver-wgsl-ipc" class="menu-item" onclick_shared={queue_action(&state.actions, ACT_RUN_SOLVER_GPU)}>"WGSL AVBD / IPC"</button>
                            <button id="solver-barrier-al" class="menu-item" onclick_shared={queue_action(&state.actions, ACT_RUN_SOLVER_AL)}>"WGSL AVBD / barrier AL (experimental)"</button>
                            <button id="solver-cpu" class="menu-item" onclick_shared={queue_action(&state.actions, ACT_RUN_SOLVER_CPU)}>"CPU VBD / OGC"</button>
                        </div>
                    }} else { ui! { <span /> } }}
                </div>
            </div>
            <div class="top-bar-right">
                <div class="layout-switcher">
                    <button class={split_class} onclick_shared={queue_action(&state.actions, ACT_RUN_VIEW_SPLIT)} title="Split 3D + 2D View">
                        {icon_view_split()}
                        <span class="btn-label">"Split"</span>
                    </button>
                    <button class={threed_class} onclick_shared={queue_action(&state.actions, ACT_RUN_VIEW_3D)} title="Full 3D Garment View">
                        {icon_view_3d()}
                        <span class="btn-label">"3D"</span>
                    </button>
                    <button class={twod_class} onclick_shared={queue_action(&state.actions, ACT_RUN_VIEW_2D)} title="Full 2D Pattern Board">
                        {icon_view_2d()}
                        <span class="btn-label">"2D"</span>
                    </button>
                </div>
                <button id="clean-view-toggle" class={clean_class} onclick_shared={queue_action(&state.actions, ACT_RUN_CLEAN_VIEW)} title="Minimalist Presentation View">
                    {icon_clean_view()}
                    <span class="btn-label">"Clean View"</span>
                </button>
                <button id="command-palette-trigger" onclick_shared={queue_action(&state.actions, ACT_RUN_PALETTE_TOGGLE)} title="Open Command Palette (Ctrl+K)">
                    {icon_palette()}
                    <span class="btn-label">"Commands"</span>
                </button>
                <button id="theme-toggle" onclick_shared={queue_action(&state.actions, ACT_RUN_THEME_TOGGLE)} title={theme_label}>
                    {icon_day_night_switch(matches!(state.theme, Theme::Dark))}
                </button>
            </div>
        </header>
    }
}
