//! Event routing and focus/resize handling.

use super::board::types::Drag2d;
use super::state::CssRunnerState;
use cssimpler::renderer::EngineEvent;

pub(super) mod buttons;
pub(super) mod keyboard;
pub(super) mod motion;
pub(super) mod scroll;

pub fn handle_runner_event(state: &mut CssRunnerState, event: &EngineEvent) -> bool {
    match event {
        EngineEvent::FocusChanged(false) => {
            state.last_2d_click = None;
            if matches!(state.drag_2d, Some(Drag2d::Marquee { .. })) {
                state.pointer_down = false;
                state.drag_2d = None;
                return true;
            }
            false
        }
        EngineEvent::ViewportChanged(vp) => {
            let new_size = (vp.width.max(1) as f32, vp.height.max(1) as f32);
            if state.window_size != new_size {
                state.window_size = new_size;
                return true;
            }
            false
        }
        EngineEvent::Key(_) => keyboard::handle_keyboard(state, event),
        EngineEvent::PointerButton { .. } => buttons::handle_buttons(state, event),
        EngineEvent::PointerMoved { .. } => motion::handle_motion(state, event),
        EngineEvent::Scroll { .. } => scroll::handle_scroll(state, event),
        _ => false,
    }
}
