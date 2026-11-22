//! Studio shadow controls in the native 3D viewport toolbar.

use super::board::{geometry::viewport_3d_rect, types::Rect2d};
use super::state::CssRunnerState;
use cssimpler::core::{LengthPercentageCalc, Node, Style};
use cssimpler::renderer::gpu::usd::UsdShadowSettings;
use cssimpler::renderer::{ButtonState, EngineEvent, KeyIdentity, PointerButton};
use cssimpler::ui;

pub(super) const HEIGHT: f32 = 28.0;
const ROW_HEIGHT: f32 = HEIGHT / 2.0;
const LABEL_WIDTH: f32 = 76.0;
const SIDE_PADDING: f32 = 6.0;
const MAX_WIDTH: f32 = 225.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum ShadowControl {
    Softness,
    Strength,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::atelier) struct ShadowLevels {
    softness: u8,
    strength: u8,
}

impl Default for ShadowLevels {
    fn default() -> Self {
        Self {
            softness: 40,
            strength: 78,
        }
    }
}

impl ShadowLevels {
    fn value(self, control: ShadowControl) -> u8 {
        match control {
            ShadowControl::Softness => self.softness,
            ShadowControl::Strength => self.strength,
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
        settings
    }
}

impl ShadowControl {
    const ALL: [Self; 2] = [Self::Softness, Self::Strength];

    fn track(self, state: &CssRunnerState) -> Rect2d {
        let viewport = viewport_3d_rect(state);
        let row = match self {
            Self::Softness => 0.0,
            Self::Strength => 1.0,
        };
        Rect2d {
            x: viewport.x + SIDE_PADDING + LABEL_WIDTH,
            y: viewport.y - HEIGHT + row * ROW_HEIGHT,
            w: (viewport.w.max(100.0) - 2.0 * SIDE_PADDING).min(MAX_WIDTH) - LABEL_WIDTH,
            h: ROW_HEIGHT,
        }
    }

    fn set(self, state: &CssRunnerState, value: u8) {
        let mut levels = state.viewport_shadow_levels.get();
        match self {
            Self::Softness => levels.softness = value,
            Self::Strength => levels.strength = value,
        }
        if state.viewport_shadow_levels.replace(levels) != levels {
            state.request_viewport_redraw();
        }
    }

    fn set_from_pointer(self, state: &CssRunnerState, x: f32) {
        let track = self.track(state);
        let percent = ((x - track.x) / track.w * 100.0).clamp(0.0, 100.0);
        self.set(state, percent.round() as u8);
    }
}

pub(super) fn build(state: &CssRunnerState) -> Node {
    let supported = state.viewport_shadow_supported.get();
    let row = |control: ShadowControl, id: &str, label: &str| {
        let value = state.viewport_shadow_levels.get().value(control);
        let width = control.track(state).w;
        let filled = width * f32::from(value) / 100.0;
        let mut track_style = Style::default();
        track_style.layout.calc.width = Some(LengthPercentageCalc::new(0.0, width));
        let mut fill_style = Style::default();
        fill_style.layout.calc.width = Some(LengthPercentageCalc::new(0.0, filled));
        let mut thumb_style = Style::default();
        thumb_style.layout.calc.inset_left = Some(LengthPercentageCalc::new(0.0, filled - 4.0));
        let title = if supported {
            format!("Shadow {label} · {value}%")
        } else {
            "Shadow mapping is unavailable on this device".to_owned()
        };
        let class = if state.shadow_focus == Some(control) {
            "viewport-shadow-track focused"
        } else {
            "viewport-shadow-track"
        };
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
            .with_children([
                Node::element("div")
                    .with_class("viewport-shadow-rail")
                    .into(),
                Node::element("div")
                    .with_class("viewport-shadow-fill")
                    .with_style(fill_style)
                    .into(),
                Node::element("div")
                    .with_class("viewport-shadow-thumb")
                    .with_style(thumb_style)
                    .into(),
            ])
            .into();
        ui! {
            <div class="viewport-shadow-row">
                <span class="viewport-shadow-label">{label}</span>
                {track}
            </div>
        }
    };
    let class = if supported {
        "viewport-shadow-controls"
    } else {
        "viewport-shadow-controls unavailable"
    };
    ui! {
        <div class={class} aria-label="Shadows">
            {row(ShadowControl::Softness, "shadow-softness-slider", "Softness")}
            {row(ShadowControl::Strength, "shadow-strength-slider", "Strength")}
        </div>
    }
}

pub(super) fn handle_event(state: &mut CssRunnerState, event: &EngineEvent) -> bool {
    let available = state.viewport_shadow_supported.get()
        && !state.settings_open
        && !state.command_palette_open
        && !state.solver_menu_open
        && state.import_dialog.is_none()
        && state.layout_mode != clothcraft::css_ui::ViewLayoutMode::TwoDOnly;
    match event {
        EngineEvent::PointerMoved { position, .. } => {
            if let Some(drag) = state.shadow_drag {
                state.last_pointer_pos = (position.x, position.y);
                drag.set_from_pointer(state, position.x);
                return true;
            }
        }
        EngineEvent::PointerButton {
            button: PointerButton::Primary,
            state: ButtonState::Pressed,
            ..
        } => {
            state.shadow_focus = None;
            if available && !state.pointer_down {
                let (x, y) = state.last_pointer_pos;
                if let Some(control) = ShadowControl::ALL
                    .into_iter()
                    .find(|control| control.track(state).contains(x, y))
                {
                    state.shadow_focus = Some(control);
                    state.shadow_drag = Some(control);
                    state.pointer_down = true;
                    control.set_from_pointer(state, x);
                    return true;
                }
            }
        }
        EngineEvent::PointerButton {
            button: PointerButton::Primary,
            state: ButtonState::Released,
            ..
        } if state.shadow_drag.take().is_some() => {
            state.pointer_down = false;
            return true;
        }
        EngineEvent::PointerLeft | EngineEvent::FocusChanged(false) | EngineEvent::Suspended => {
            if state.shadow_drag.take().is_some() {
                state.pointer_down = false;
            }
            state.shadow_focus = None;
        }
        EngineEvent::Key(event) if available && event.state == ButtonState::Pressed => {
            if let (Some(control), KeyIdentity::Named(key)) =
                (state.shadow_focus, &event.logical_key)
            {
                let current = state.viewport_shadow_levels.get().value(control);
                let value = match key.as_str() {
                    "ArrowLeft" | "ArrowDown" => current.saturating_sub(1),
                    "ArrowRight" | "ArrowUp" => current.saturating_add(1).min(100),
                    "Home" => 0,
                    "End" => 100,
                    "Escape" => {
                        state.shadow_focus = None;
                        if state.shadow_drag.take().is_some() {
                            state.pointer_down = false;
                        }
                        return true;
                    }
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
