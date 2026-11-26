//! Overall workspace layout and theme.

use super::super::actions::{RunnerUiOp, queue_ui_op};
use super::super::state::CssRunnerState;
use super::asset_browser::build_runner_left_browser;
use super::command_palette::build_runner_command_palette;
use super::import_dialog::build_runner_import_dialog;
use super::inspector::build_runner_right_dock;
use super::settings::build_runner_settings;
use super::status_bar::build_runner_status_bar;
use super::tool_rails::{build_runner_2d_rail, build_runner_3d_rail};
use super::top_bar::build_runner_top_bar;
use super::viewports::build_runner_stage;
use cssimpler::core::Node;
use cssimpler::ui;

pub fn build_runner_ui(state: &CssRunnerState) -> Node {
    let theme_class = state.theme.class_name();
    let shell_class = format!("studio-shell {theme_class}");
    let file_backdrop = state.file_menu_open.then(|| {
        ui! {
            <div id="file-menu-backdrop" class="file-menu-backdrop"
                onclick_shared={queue_ui_op(&state.actions, RunnerUiOp::CloseFileMenu)} />
        }
    });

    ui! {
        <div id="app" class={shell_class}>
            {build_runner_top_bar(state)}
            <div class="studio-body">
                {build_runner_3d_rail(state)}
                {build_runner_left_browser(state)}
                {build_runner_stage(state)}
                {build_runner_2d_rail(state)}
                {build_runner_right_dock(state)}
            </div>
            {build_runner_status_bar(state)}
            {file_backdrop}
            {build_runner_command_palette(state)}
            {build_runner_settings(state)}
            {build_runner_import_dialog(state)}
            {super::super::mesh_preparation::render_value_editor(state)}
        </div>
    }
}
