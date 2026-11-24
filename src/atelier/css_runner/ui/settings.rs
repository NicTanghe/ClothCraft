//! Lightweight settings panel; controls use the runner's per-instance queue.

use super::super::actions::{RunnerAction, RunnerUiOp, queue_action, queue_ui_op};
use super::super::state::CssRunnerState;
use super::number::{NumberField, number_field};
use clothcraft::css_ui::icons::{icon_chevron_down, icon_chevron_right};
use cssimpler::core::Node;
use cssimpler::ui;

pub(in super::super) fn build_runner_settings(state: &CssRunnerState) -> Option<Node> {
    if !state.settings_open {
        return None;
    }
    let toggle = |id, label, enabled, action| {
        let status = if enabled { "On" } else { "Off" };
        ui! {
            <button id={id} class="cad-2d-pill settings-body-toggle"
                aria-pressed={enabled.to_string()}
                onclick_shared={queue_action(&state.actions, action)}>
                <span>{label}</span><span>{status}</span>
            </button>
        }
    };
    let body_options = state.body_settings_open.then(|| ui! {
        <div class="settings-entry">
            {toggle("settings-body-visible", "Body", state.show_body_outline, RunnerAction::ToggleBodyOutline)}
            {toggle("settings-body-fill", "Fill", state.show_body_fill, RunnerAction::Ui(RunnerUiOp::ToggleBodyFill))}
            {toggle("settings-body-outline", "Outline", state.show_body_border, RunnerAction::Ui(RunnerUiOp::ToggleBodyBorder))}
            {toggle("settings-body-details", "Details", state.show_body_details, RunnerAction::Ui(RunnerUiOp::ToggleBodyDetails))}
        </div>
    });
    let wireframe = state.wireframe_settings();
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
                <div class="settings-entry">
                    <span class="settings-label">"3D shadows"</span>
                    {super::super::shadow_controls::build(state)}
                </div>
                <div class="settings-entry">
                    <button id="settings-body-section" class="cad-2d-pill settings-body-toggle"
                        aria-expanded={state.body_settings_open.to_string()}
                        onclick_shared={queue_ui_op(&state.actions, RunnerUiOp::ToggleBodySettings)}>
                        <span>"Body reference"</span>
                        {if state.body_settings_open { icon_chevron_down() } else { icon_chevron_right() }}
                    </button>
                    {body_options}
                </div>
                <div class="settings-entry">
                    <span class="settings-label">"3D wireframe"</span>
                    <div class="settings-controls">
                        <button id="wireframe-colour" class="cad-2d-pill" title="Cycle wireframe colour"
                            onclick_shared={queue_ui_op(&state.actions, RunnerUiOp::CycleWireframeColour)}>{wireframe.colour_name()}</button>
                        <button id="wireframe-width-dec" class="cad-2d-pill" title="Thinner wireframe"
                            onclick_shared={queue_ui_op(&state.actions, RunnerUiOp::AdjustWireframeWidth(-1.0))}>"−"</button>
                        <span id="wireframe-width-value" class="settings-value">{format!("{:.0} px", wireframe.width_px)}</span>
                        <button id="wireframe-width-inc" class="cad-2d-pill" title="Thicker wireframe"
                            onclick_shared={queue_ui_op(&state.actions, RunnerUiOp::AdjustWireframeWidth(1.0))}>"+"</button>
                        <button id="wireframe-reset" class="cad-2d-pill" title="Restore the default wireframe colour and width"
                            onclick_shared={queue_ui_op(&state.actions, RunnerUiOp::ResetWireframeStyle)}>"Reset"</button>
                    </div>
                    <span class="settings-hint">"Cloth edges only; view-only"</span>
                </div>
            </div>
        </div>
    })
}
