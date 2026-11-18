//! Lightweight settings panel; controls use the runner's per-instance queue.

use super::super::actions::{RunnerUiOp, queue_ui_op};
use super::super::state::CssRunnerState;
use super::number::{NumberField, number_field};
use cssimpler::core::Node;
use cssimpler::ui;

pub(in super::super) fn build_runner_settings(state: &CssRunnerState) -> Option<Node> {
    if !state.settings_open {
        return None;
    }
    Some(ui! {
        <div id="settings-layer" class="settings-layer">
            <div id="settings-backdrop" class="settings-backdrop"
                onclick_shared={queue_ui_op(&state.actions, RunnerUiOp::CloseSettings)} />
            <div id="settings-panel" class="settings-panel" onclick_shared={|| {}}>
                <div class="settings-header">
                    <span id="settings-title" class="settings-title">"Settings"</span>
                    <button id="settings-close" class="cad-2d-pill"
                        onclick_shared={queue_ui_op(&state.actions, RunnerUiOp::CloseSettings)}
                        title="Close settings (Escape)">"Close"</button>
                </div>
                <div class="settings-entry">
                    <span class="settings-label">"New measurement precision"</span>
                    <button id="measurement-precision" class="cad-2d-pill" onclick_shared={queue_ui_op(&state.actions,RunnerUiOp::CycleMeasurementBits)}>{format!("{} bits",state.project.template.measurement_precision())}</button>
                </div>
                <div class="settings-entry">
                    <span class="settings-label">"2D zoom speed"</span>
                    <div class="settings-controls">
                        <button id="zoom-speed-dec" class="cad-2d-pill" title="Decrease 2D zoom speed"
                            onclick_shared={queue_ui_op(&state.actions, RunnerUiOp::Adjust2dZoomSpeed(-0.25))}>"−"</button>
                        {number_field(state, "zoom-speed-value", NumberField::ZoomSpeed)}
                        <button id="zoom-speed-inc" class="cad-2d-pill" title="Increase 2D zoom speed"
                            onclick_shared={queue_ui_op(&state.actions, RunnerUiOp::Adjust2dZoomSpeed(0.25))}>"+"</button>
                        <button id="zoom-speed-reset" class="cad-2d-pill" title="Restore default 2D zoom speed"
                            onclick_shared={queue_ui_op(&state.actions, RunnerUiOp::Reset2dZoomSpeed)}>"Reset"</button>
                    </div>
                    <span class="settings-hint">"Mouse wheel and trackpad"</span>
                </div>
            </div>
        </div>
    })
}
