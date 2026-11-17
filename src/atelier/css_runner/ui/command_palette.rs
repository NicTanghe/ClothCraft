//! Command palette and keyboard shortcut hints.

use super::super::actions::{
    ACT_RUN_CAM_RESET, ACT_RUN_CLEAN_VIEW, ACT_RUN_DRAWER_TOGGLE, ACT_RUN_EXPORT,
    ACT_RUN_PALETTE_TOGGLE, ACT_RUN_PLAY_PAUSE, ACT_RUN_RESET, ACT_RUN_SAVE, ACT_RUN_STEP,
    ACT_RUN_THEME_TOGGLE, ACT_RUN_VIEW_2D, ACT_RUN_VIEW_3D, ACT_RUN_VIEW_SPLIT, queue_action,
};
use super::super::state::CssRunnerState;
use clothcraft::css_ui::Theme;
use clothcraft::css_ui::icons::{
    icon_cam_reset, icon_clean_view, icon_export, icon_moon, icon_pause, icon_play, icon_reset,
    icon_save, icon_sidebar_toggle, icon_step, icon_sun, icon_view_2d, icon_view_3d,
    icon_view_split,
};
use cssimpler::core::Node;
use cssimpler::ui;

pub(in super::super) fn build_runner_command_palette(state: &CssRunnerState) -> Node {
    if !state.command_palette_open {
        return ui! { <div id="command-palette-container"></div> };
    }

    ui! {
        <div id="command-palette-container" class="command-palette-backdrop">
            <div class="command-palette-modal">
                <div class="palette-header">
                    <span class="palette-title">"Studio V2 Command Palette"</span>
                    <button class="palette-close-btn" onclick_shared={queue_action(&state.actions, ACT_RUN_PALETTE_TOGGLE)}>"✕"</button>
                </div>
                <div class="palette-input">
                    "Type a command or shortcut..."
                </div>
                <div class="palette-items">
                    <button class="palette-item" onclick_shared={queue_action(&state.actions, ACT_RUN_PLAY_PAUSE)}>
                        <div class="item-left">
                            {if state.playing { icon_pause() } else { icon_play() }}
                            <span>"Toggle Simulation"</span>
                        </div>
                        <span class="command-shortcut">"Space"</span>
                    </button>
                    <button class="palette-item" onclick_shared={queue_action(&state.actions, ACT_RUN_STEP)}>
                        <div class="item-left">
                            {icon_step()}
                            <span>"Step Simulation"</span>
                        </div>
                        <span class="command-shortcut">"S"</span>
                    </button>
                    <button class="palette-item" onclick_shared={queue_action(&state.actions, ACT_RUN_RESET)}>
                        <div class="item-left">
                            {icon_reset()}
                            <span>"Reset Simulation"</span>
                        </div>
                        <span class="command-shortcut">"R"</span>
                    </button>
                    <button class="palette-item" onclick_shared={queue_action(&state.actions, ACT_RUN_SAVE)}>
                        <div class="item-left">
                            {icon_save()}
                            <span>"Save Fitting Project"</span>
                        </div>
                        <span class="command-shortcut">"Ctrl+S"</span>
                    </button>
                    <button class="palette-item" onclick_shared={queue_action(&state.actions, ACT_RUN_EXPORT)}>
                        <div class="item-left">
                            {icon_export()}
                            <span>"Export Artifacts (OBJ & True-Scale SVG)"</span>
                        </div>
                        <span class="command-shortcut">"Ctrl+E"</span>
                    </button>
                    <button class="palette-item" onclick_shared={queue_action(&state.actions, ACT_RUN_CLEAN_VIEW)}>
                        <div class="item-left">
                            {icon_clean_view()}
                            <span>"Toggle Clean View"</span>
                        </div>
                        <span class="command-shortcut">"F11"</span>
                    </button>
                    <button class="palette-item" onclick_shared={queue_action(&state.actions, ACT_RUN_THEME_TOGGLE)}>
                        <div class="item-left">
                            {if matches!(state.theme, Theme::Dark) { icon_sun() } else { icon_moon() }}
                            <span>"Toggle Dark / Light Theme"</span>
                        </div>
                        <span class="command-shortcut">"T"</span>
                    </button>
                    <button class="palette-item" onclick_shared={queue_action(&state.actions, ACT_RUN_VIEW_SPLIT)}>
                        <div class="item-left">
                            {icon_view_split()}
                            <span>"Split 2D / 3D Layout"</span>
                        </div>
                        <span class="command-shortcut">"1"</span>
                    </button>
                    <button class="palette-item" onclick_shared={queue_action(&state.actions, ACT_RUN_VIEW_3D)}>
                        <div class="item-left">
                            {icon_view_3d()}
                            <span>"3D Only Viewport"</span>
                        </div>
                        <span class="command-shortcut">"2"</span>
                    </button>
                    <button class="palette-item" onclick_shared={queue_action(&state.actions, ACT_RUN_VIEW_2D)}>
                        <div class="item-left">
                            {icon_view_2d()}
                            <span>"2D Only Pattern Board"</span>
                        </div>
                        <span class="command-shortcut">"3"</span>
                    </button>
                    <button class="palette-item" onclick_shared={queue_action(&state.actions, ACT_RUN_CAM_RESET)}>
                        <div class="item-left">
                            {icon_cam_reset()}
                            <span>"Reset 3D Camera"</span>
                        </div>
                        <span class="command-shortcut">"Home"</span>
                    </button>
                    <button class="palette-item" onclick_shared={queue_action(&state.actions, ACT_RUN_DRAWER_TOGGLE)}>
                        <div class="item-left">
                            {icon_sidebar_toggle()}
                            <span>"Toggle Left Asset Drawer"</span>
                        </div>
                        <span class="command-shortcut">"Ctrl+B"</span>
                    </button>
                </div>
            </div>
        </div>
    }
}
