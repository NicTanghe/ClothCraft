//! Playback of a checked rigid seam approach before starting cloth forces.

use super::{editing::install_authoring_edit_with_pose, state::CssRunnerState};
use clothcraft::fitting::seam_placement::{APPROACH_FRAMES, Approach};
use std::time::{Duration, Instant};

const FRAME_PERIOD: Duration = Duration::from_nanos(16_666_667);

pub(super) struct SeamAssembly {
    approach: Approach,
    generation: u64,
    frame: u32,
    elapsed: Duration,
    deadline: Instant,
    pub step_requested: bool,
}

impl SeamAssembly {
    pub fn deadline(&self) -> Instant {
        self.deadline
    }
}

pub(super) struct CollisionPreview {
    pose: clothcraft::model::State,
    pub shown: bool,
}

pub(super) fn hide_collision(state: &mut CssRunnerState) -> bool {
    let Some(preview) = state
        .seam_collision
        .as_mut()
        .filter(|preview| preview.shown)
    else {
        return false;
    };
    preview.shown = false;
    state.sync_usd_simulation_pose();
    state.request_viewport_redraw();
    true
}

pub(super) fn clear_collision(state: &mut CssRunnerState) {
    hide_collision(state);
    state.seam_collision = None;
}

pub(super) fn toggle_collision(state: &mut CssRunnerState) {
    if hide_collision(state) {
        return;
    }
    if let (Some(preview), Some(usd)) = (&mut state.seam_collision, &state.usd_scene) {
        match clothcraft::usd_scene::update_panel_viewport_pose(
            usd,
            &state.scene.garment,
            &preview.pose,
        ) {
            Ok(()) => {
                preview.shown = true;
                state.request_viewport_redraw();
            }
            Err(error) => {
                state.status_message = format!("Could not preview the blocked pose · {error}")
            }
        }
    }
}

pub(super) fn begin(state: &mut CssRunnerState, approach: Approach) {
    state.generation = state.generation.wrapping_add(1);
    state.worker_pending = false;
    state.seam_assembly = Some(SeamAssembly {
        approach,
        generation: state.generation,
        frame: 0,
        elapsed: Duration::ZERO,
        deadline: Instant::now(),
        step_requested: false,
    });
    state.status_message = if state.playing {
        "Joining seam"
    } else {
        "Press Play or Step to join the seam · Esc cancels"
    }
    .into();
}

pub(super) fn begin_existing(state: &mut CssRunnerState, id: &str) -> bool {
    match clothcraft::fitting::seam_placement::prepare_existing(
        &state.scene,
        &state.project.template,
        id,
    ) {
        Ok(approach) => {
            state.seam_mode_error = None;
            begin(state, approach);
            true
        }
        Err(error) => {
            state.generation = state.generation.wrapping_add(1);
            state.worker_pending = false;
            state.playing = false;
            state.selected_seam = Some(id.into());
            state.active_category = super::state::ObjectCategory::Sewing;
            state.status_message = format!("Join on Play unavailable · {}", error.message);
            state.seam_mode_error = Some(error.message);
            state.seam_collision = error
                .blocked_pose
                .map(|pose| CollisionPreview { pose, shown: false });
            false
        }
    }
}

pub(super) fn cancel(state: &mut CssRunnerState) -> bool {
    let queued = state.seam_to_join.take().is_some();
    let cancelled = cancel_motion(state) || queued;
    if cancelled {
        clear_collision(state);
        state.seam_mode_error = None;
        state.status_message = "Seam joining cancelled".into();
    }
    cancelled
}

pub(super) fn cancel_motion(state: &mut CssRunnerState) -> bool {
    if state.seam_assembly.take().is_none() {
        return false;
    }
    state.playing = false;
    state.sync_usd_simulation_pose();
    state.request_viewport_redraw();
    true
}

/// The scene holds the last accepted simulation state during the approach.
/// Only the display moves until the full seam can be committed.
pub(super) fn tick(state: &mut CssRunnerState, delta: Duration) -> bool {
    let Some(assembly) = state.seam_assembly.as_mut() else {
        return false;
    };
    if assembly.generation != state.generation {
        // A different document, mesh or solver installed its own snapshot.
        return cancel(state);
    }
    let step = std::mem::take(&mut assembly.step_requested);
    if !state.playing && !step {
        return false;
    }
    assembly.deadline = Instant::now() + FRAME_PERIOD;
    assembly.elapsed += if step {
        FRAME_PERIOD
    } else {
        delta.min(FRAME_PERIOD * 3)
    };
    let frames = (assembly.elapsed.as_nanos() / FRAME_PERIOD.as_nanos()) as u32;
    if frames <= assembly.frame {
        return false;
    }
    if assembly.frame == APPROACH_FRAMES {
        let assembly = state.seam_assembly.take().expect("assembly at handoff");
        state.seam_to_join = None;
        let old = state.project.template.clone();
        state.draft_2d = None;
        state.selected_seam = Some(assembly.approach.seam_id);
        state.seam_mode_error = None;
        install_authoring_edit_with_pose(
            state,
            assembly.approach.edit,
            Some(old),
            "Joined the full seam · Cloth simulation ready".into(),
            Some(assembly.approach.joined_state),
        );
        return true;
    }
    // Publish the meeting pose for a frame before handing it to the solver.
    assembly.frame = frames.min(APPROACH_FRAMES);
    let panel = state
        .scene
        .garment
        .panels
        .iter()
        .find(|p| p.id == assembly.approach.panel_id)
        .expect("assembly still belongs to this generation");
    if let Some(usd) = &state.usd_scene {
        let transform = assembly.approach.transform(assembly.frame);
        let points = state.scene.state.x[panel.vertices.clone()]
            .iter()
            .copied()
            .map(transform);
        if let Err(error) =
            clothcraft::usd_scene::update_viewport_panel_pose(usd, &panel.id, points)
        {
            cancel(state);
            state.status_message = format!("Seam display update failed · {error}");
            return true;
        }
        state.request_viewport_redraw();
    }
    let status_changed = state.status_message != "Joining seam";
    if status_changed {
        state.status_message = "Joining seam".into();
    }
    status_changed
}
