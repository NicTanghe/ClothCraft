//! ClothCraft drag handles; cssimpler owns native text editing.
use super::super::{
    actions::{RunnerUiOp, apply_runner_ui_op},
    state::CssRunnerState,
    ui::number::NumberField,
};
use super::cursor_wrap::CursorWrap;
use cssimpler::renderer::{ButtonState, EngineEvent, KeyIdentity, PointerButton};

#[derive(Default)]
pub(in super::super) struct NumberDragState {
    pub pointer_y: f32,
    pub shift: bool,
    pub control: bool,
    pub active: Option<NumberDrag>,
}
pub(in super::super) struct NumberDrag {
    pub field: NumberField,
    pub last_y: f32,
    pub initial_value: f64,
    pub initial_mode_radius: bool,
    pub initial_preview_active: bool,
    pub value: f64,
    pub remainder: f64,
    pub cursor: Option<CursorWrap>,
}

fn move_value(
    drag: &mut NumberDrag,
    dy: f64,
    shift: bool,
    control: bool,
) -> Option<(NumberField, f64)> {
    drag.remainder += dy;
    let steps = (drag.remainder / 4.0).trunc();
    if steps == 0.0 {
        return None;
    }
    drag.remainder -= steps * 4.0;
    let (min, max, step, _) = drag.field.spec();
    let multiplier = if shift { 10.0 } else { 1.0 } * if control { 0.1 } else { 1.0 };
    drag.value = (drag.value + steps * step * multiplier).clamp(min, max);
    // Keep precision from fine drags when modifiers are released. Never round
    // to the normal step, which would discard Ctrl's smaller increments.
    Some((drag.field, drag.value))
}

pub(in super::super) fn handle_number_event(
    state: &mut CssRunnerState,
    event: &EngineEvent,
) -> bool {
    use std::sync::atomic::Ordering;
    let mut drag = state.number_drag.lock().unwrap();
    let mut edit = None;
    let mut cancelled_preview = None;
    let mut consumed = false;
    match event {
        EngineEvent::PointerMoved {
            position,
            modifiers,
        } => {
            drag.pointer_y = position.y;
            drag.shift = modifiers.shift;
            drag.control = modifiers.control;
            if let Some(active) = &mut drag.active {
                if active.cursor.is_none() {
                    let dy = (active.last_y - position.y) as f64;
                    edit = move_value(active, dy, modifiers.shift, modifiers.control);
                }
                active.last_y = position.y;
                state.last_pointer_pos = (position.x, position.y);
                consumed = true;
            }
        }
        EngineEvent::ModifiersChanged(modifiers) => {
            drag.shift = modifiers.shift;
            drag.control = modifiers.control;
        }
        EngineEvent::PointerButton {
            button: PointerButton::Primary,
            state: button,
            modifiers,
        } => {
            drag.shift = modifiers.shift;
            drag.control = modifiers.control;
            if *button == ButtonState::Pressed {
                state.numeric_input_active.store(false, Ordering::Relaxed);
            }
            if *button == ButtonState::Released
                && let Some(mut active) = drag.active.take()
            {
                // Include physical motion since the last rendered frame.
                if let Some(cursor) = &mut active.cursor
                    && let Some(dy) = cursor.sample()
                {
                    edit = move_value(&mut active, dy, modifiers.shift, modifiers.control);
                }
                consumed = true;
                state.pointer_down = false;
            }
        }

        EngineEvent::Key(key)
            if key.state == ButtonState::Pressed
                && key.logical_key == KeyIdentity::Named("Escape".into()) =>
        {
            if let Some(active) = drag.active.take() {
                state.pointer_down = false;
                cancelled_preview =
                    Some((active.initial_mode_radius, active.initial_preview_active));
                edit = Some((active.field, active.initial_value));
                consumed = true;
            }
            if state.numeric_input_active.swap(false, Ordering::Relaxed) {
                state.settings_open = false;
                consumed = false;
            }
        }
        EngineEvent::Key(_) | EngineEvent::Scroll { .. } if drag.active.is_some() => {
            consumed = true
        }
        EngineEvent::FocusChanged(false) | EngineEvent::Suspended => {
            if drag.active.take().is_some() {
                state.pointer_down = false;
            }
            state.numeric_input_active.store(false, Ordering::Relaxed);
        }
        _ => {}
    }
    drop(drag);
    if let Some((field, value)) = edit {
        apply_runner_ui_op(state, RunnerUiOp::SetNumber(field, value));
    }
    if let Some((mode, preview)) = cancelled_preview {
        state.corner_mode_radius = mode;
        state.corner_preview_active = preview;
    }
    consumed
}

pub(in super::super) fn tick_number_drag(state: &mut CssRunnerState) -> bool {
    let mut drag = state.number_drag.lock().unwrap();
    let (shift, control) = (drag.shift, drag.control);
    let edit = drag.active.as_mut().and_then(|active| {
        let dy = active.cursor.as_mut()?.sample()?;
        move_value(active, dy, shift, control)
    });
    drop(drag);
    if let Some((field, value)) = edit {
        apply_runner_ui_op(state, RunnerUiOp::SetNumber(field, value));
        true
    } else {
        false
    }
}
