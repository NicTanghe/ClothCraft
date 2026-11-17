//! Native garment workspace runner.
//!
//! State/actions, editing, board math/drawing, input and UI composition live in
//! focused modules. Public re-exports preserve the existing runner API.

#![allow(dead_code)]

mod actions;
mod app;
mod board;
mod documents;
mod editing;
mod input;
mod state;
mod ui;

#[cfg(test)]
mod tests;

// The runner is compiled as a library API and as part of cloth-fit. Some API
// re-exports are used only by library consumers and integration tests.
#[allow(unused_imports)]
pub use actions::{
    ACT_RUN_2D_FIT, ACT_RUN_2D_NEXT_PIECE, ACT_RUN_2D_ZOOM_IN, ACT_RUN_2D_ZOOM_OUT, ACT_RUN_ACC_0,
    ACT_RUN_ACC_1, ACT_RUN_ACC_2, ACT_RUN_ACC_3, ACT_RUN_ACC_4, ACT_RUN_ACC_5,
    ACT_RUN_ACCORDION_TOGGLE, ACT_RUN_CAM_RESET, ACT_RUN_CLEAN_VIEW, ACT_RUN_CORNER_MODE_TOGGLE,
    ACT_RUN_DRAWER_TOGGLE, ACT_RUN_EXPORT, ACT_RUN_PALETTE_TOGGLE, ACT_RUN_PIECE_0,
    ACT_RUN_PIECE_1, ACT_RUN_PIECE_2, ACT_RUN_PIECE_3, ACT_RUN_PIECE_NEXT, ACT_RUN_PIN_MODE_TOGGLE,
    ACT_RUN_PLAY_PAUSE, ACT_RUN_RESET, ACT_RUN_SAVE, ACT_RUN_SOLVER_AL, ACT_RUN_SOLVER_CPU,
    ACT_RUN_SOLVER_GPU, ACT_RUN_SOLVER_MENU, ACT_RUN_STEP, ACT_RUN_TAB_ENV, ACT_RUN_TAB_FORM,
    ACT_RUN_TAB_MATERIAL, ACT_RUN_TAB_PATTERN, ACT_RUN_TAB_SEWING, ACT_RUN_THEME_TOGGLE,
    ACT_RUN_TOOL_AVATAR_DIAG, ACT_RUN_TOOL_CORNER, ACT_RUN_TOOL_CORNER_SELECT2D,
    ACT_RUN_TOOL_CORNER2D, ACT_RUN_TOOL_EDIT2D, ACT_RUN_TOOL_FREE2D, ACT_RUN_TOOL_GIZMO,
    ACT_RUN_TOOL_GRAB, ACT_RUN_TOOL_INTERNAL2D, ACT_RUN_TOOL_MEASURE, ACT_RUN_TOOL_MEASURE2D,
    ACT_RUN_TOOL_MEASURE3D, ACT_RUN_TOOL_PEN2D, ACT_RUN_TOOL_PIN, ACT_RUN_TOOL_RECT2D,
    ACT_RUN_TOOL_SELECT, ACT_RUN_TOOL_SEW, ACT_RUN_TOOL_SEW2D, ACT_RUN_TOOL_SEW3D,
    ACT_RUN_TOOL_SPLIT2D, ACT_RUN_TOOL_TRIM2D, ACT_RUN_VIEW_2D, ACT_RUN_VIEW_3D,
    ACT_RUN_VIEW_SPLIT, RunnerUiOp, update_runner,
};
#[allow(unused_imports)]
pub use app::{create_runner_app, run};
#[allow(unused_imports)]
pub use board::geometry::{
    cad_2d_bar_layout, compute_2d_view_box_m, compute_2d_viewport_rects, screen_to_board_m,
    screen_to_board_unclamped_m, viewport_2d_span_x, viewport_3d_rect,
};
#[allow(unused_imports)]
pub use board::native as native_board;
#[allow(unused_imports)]
pub use board::rendering::render_runner_2d_paths;
#[allow(unused_imports)]
pub use board::types::{Drag2d, Hover2dTarget, Rect2d, ToolDraft2d};
#[allow(unused_imports)]
pub use editing::{commit_authoring_command, undo_redo_2d};
#[allow(unused_imports)]
pub use input::handle_runner_event;
#[allow(unused_imports)]
pub use state::{CssRunnerState, ObjectCategory};
#[allow(unused_imports)]
pub use ui::shell::build_runner_ui;
#[allow(unused_imports)]
pub use ui::viewports::build_runner_2d_viewport;
