//! Studio shadow sliders for the editor settings panel.

use super::state::CssRunnerState;
use cssimpler::core::{LengthPercentageCalc, Node, Style};
use cssimpler::renderer::gpu::usd::UsdShadowSettings;
use cssimpler::renderer::{ButtonState, EngineEvent, KeyIdentity, PointerButton};
use cssimpler::ui;

const LABEL_WIDTH: f32 = 82.0;
const MAX_WIDTH: f32 = 260.0;

#[derive(Default)]
pub(super) struct ShadowInput {
    x: f32,
    primary_pressed: bool,
    drag: Option<ShadowDrag>,
    focus: Option<ShadowControl>,
}

#[derive(Clone, Copy)]
struct ShadowDrag {
    control: ShadowControl,
    left: f32,
    width: f32,
}

impl ShadowDrag {
    fn apply(self, state: &CssRunnerState, x: f32) -> bool {
        let percent = ((x - self.left) / self.width * 100.0).clamp(0.0, 100.0);
        self.control.set(state, percent.round() as u8)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum ShadowControl {
    Softness,
    Strength,
    Coverage,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::atelier) struct ShadowLevels {
    softness: u8,
    strength: u8,
    coverage: u8,
}

impl Default for ShadowLevels {
    fn default() -> Self {
        Self {
            softness: 40,
            strength: 78,
            coverage: 0,
        }
    }
}

impl ShadowLevels {
    fn value(self, control: ShadowControl) -> u8 {
        match control {
            ShadowControl::Softness => self.softness,
            ShadowControl::Strength => self.strength,
            ShadowControl::Coverage => self.coverage,
        }
    }

    pub(in crate::atelier) fn apply(self, mut settings: UsdShadowSettings) -> UsdShadowSettings {
        let softness = f32::from(self.softness) / 100.0;
        settings.light_radius = 0.4 * softness;
        settings.filter_radius = if self.softness == 0 {
            0
        } else {
            (24.0 + 60.0 * softness).round().min(64.0) as u32
        };
        settings.strength = f32::from(self.strength) / 100.0;
        settings.coverage = f32::from(self.coverage) / 100.0;
        settings
    }
}

impl ShadowControl {
    fn set(self, state: &CssRunnerState, value: u8) -> bool {
        let mut levels = state.viewport_shadow_levels.get();
        match self {
            Self::Softness => levels.softness = value,
            Self::Strength => levels.strength = value,
            Self::Coverage => levels.coverage = value,
        }
        let changed = state.viewport_shadow_levels.replace(levels) != levels;
        if changed {
            state.request_viewport_redraw();
        }
        changed
    }
}

pub(super) fn build(state: &CssRunnerState) -> Node {
    let supported = state.viewport_shadow_supported.get();
    let panel_width = state.window_size.0;
    let group_width = (panel_width * 0.90 - 34.0).clamp(LABEL_WIDTH + 8.0, MAX_WIDTH);
    let left = (panel_width - group_width) * 0.5 + LABEL_WIDTH;
    let width = group_width - LABEL_WIDTH;
    let focus = state.shadow_input.lock().unwrap().focus;
    let row = |control: ShadowControl, id: &str, label: &str| {
        let value = state.viewport_shadow_levels.get().value(control);
        let filled = width * f32::from(value) / 100.0;
        let mut track_style = Style::default();
        track_style.layout.calc.width = Some(LengthPercentageCalc::new(0.0, width));
        let mut fill_style = Style::default();
        fill_style.layout.calc.width = Some(LengthPercentageCalc::new(0.0, filled));
        let mut thumb_style = Style::default();
        thumb_style.layout.calc.inset_left = Some(LengthPercentageCalc::new(0.0, filled - 4.0));
        let value_text = match (control, value) {
            (ShadowControl::Coverage, 0) => "Close fit".to_owned(),
            (ShadowControl::Coverage, 100) => "Full cove".to_owned(),
            (ShadowControl::Coverage, _) => format!("{value}% toward full cove"),
            _ => format!("{value}%"),
        };
        let title = if supported {
            format!("Shadow {label} · {value_text}")
        } else {
            "Shadow mapping is unavailable on this device".to_owned()
        };
        let class = if focus == Some(control) {
            "settings-shadow-track focused"
        } else {
            "settings-shadow-track"
        };
        let pointer = state.shadow_input.clone();
        let update = state.update_handle.clone();
        let track: Node = Node::element("div")
            .with_id(id)
            .with_class(class)
            .with_style(track_style)
            .with_attribute("title", title)
            .with_attribute("role", "slider")
            .with_attribute("aria-label", label)
            .with_attribute("aria-valuemin", "0")
            .with_attribute("aria-valuemax", "100")
            .with_attribute("aria-valuenow", value.to_string())
            .with_attribute("aria-valuetext", value_text)
            .on_mousedown_shared(move || {
                let mut input = pointer.lock().unwrap();
                if supported && input.primary_pressed {
                    input.focus = Some(control);
                    input.drag = Some(ShadowDrag {
                        control,
                        left,
                        width,
                    });
                    drop(input);
                    update.request_update();
                }
            })
            .with_children([
                Node::element("div")
                    .with_class("settings-shadow-rail")
                    .into(),
                Node::element("div")
                    .with_class("settings-shadow-fill")
                    .with_style(fill_style)
                    .into(),
                Node::element("div")
                    .with_class("settings-shadow-thumb")
                    .with_style(thumb_style)
                    .into(),
            ])
            .into();
        ui! {
            <div class="settings-shadow-row">
                <span class="settings-shadow-label">{label}</span>
                {track}
            </div>
        }
    };
    let class = if supported {
        "settings-shadow-controls"
    } else {
        "settings-shadow-controls unavailable"
    };
    let mut group_style = Style::default();
    group_style.layout.calc.width = Some(LengthPercentageCalc::new(0.0, group_width));
    Node::element("div")
        .with_class(class)
        .with_attribute("aria-label", "Shadows")
        .with_style(group_style)
        .with_children([
            row(
                ShadowControl::Softness,
                "shadow-softness-slider",
                "Softness",
            ),
            row(
                ShadowControl::Strength,
                "shadow-strength-slider",
                "Strength",
            ),
            row(
                ShadowControl::Coverage,
                "shadow-coverage-slider",
                "Coverage",
            ),
        ])
        .into()
}

pub(super) fn tick(state: &CssRunnerState) -> bool {
    if !state.settings_open {
        *state.shadow_input.lock().unwrap() = ShadowInput::default();
        return false;
    }
    let (drag, x) = {
        let input = state.shadow_input.lock().unwrap();
        (input.drag, input.x)
    };
    drag.is_some_and(|drag| drag.apply(state, x))
}

pub(super) fn handle_event(state: &mut CssRunnerState, event: &EngineEvent) -> bool {
    if !state.settings_open {
        return false;
    }
    match event {
        EngineEvent::PointerMoved { position, .. } => {
            let mut input = state.shadow_input.lock().unwrap();
            input.x = position.x;
            if input.drag.is_some() {
                state.last_pointer_pos = (position.x, position.y);
                return true;
            }
        }
        EngineEvent::PointerButton {
            button: PointerButton::Primary,
            state: ButtonState::Pressed,
            ..
        } => {
            let mut input = state.shadow_input.lock().unwrap();
            input.focus = None;
            input.primary_pressed = true;
            input.x = state.last_pointer_pos.0;
        }
        EngineEvent::PointerButton {
            button: PointerButton::Primary,
            state: ButtonState::Released,
            ..
        } => {
            let (drag, x) = {
                let mut input = state.shadow_input.lock().unwrap();
                input.primary_pressed = false;
                (input.drag.take(), input.x)
            };
            if let Some(drag) = drag {
                drag.apply(state, x);
                return true;
            }
        }
        EngineEvent::PointerLeft | EngineEvent::FocusChanged(false) | EngineEvent::Suspended => {
            *state.shadow_input.lock().unwrap() = ShadowInput::default();
        }
        EngineEvent::Key(event) if event.state == ButtonState::Pressed => {
            let focus = state.shadow_input.lock().unwrap().focus;
            if let (Some(control), KeyIdentity::Named(key)) = (focus, &event.logical_key) {
                let current = state.viewport_shadow_levels.get().value(control);
                let value = match key.as_str() {
                    "ArrowLeft" | "ArrowDown" => current.saturating_sub(1),
                    "ArrowRight" | "ArrowUp" => current.saturating_add(1).min(100),
                    "Home" => 0,
                    "End" => 100,
                    _ => return false,
                };
                control.set(state, value);
                return true;
            }
        }
        _ => {}
    }
    false
}
