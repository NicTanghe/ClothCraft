//! Playback of a checked rigid seam approach before starting cloth forces.

use super::{editing::install_authoring_edit, state::CssRunnerState};
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

pub(super) fn queue(state: &mut CssRunnerState, approach: Approach) {
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
    state.send_control_state(false);
}

pub(super) fn cancel(state: &mut CssRunnerState) -> bool {
    if state.seam_assembly.take().is_none() {
        return false;
    }
    state.playing = false;
    state.sync_usd_simulation_pose();
    state.request_viewport_redraw();
    state.status_message = "Seam joining cancelled".into();
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
        let old = state.project.template.clone();
        state.draft_2d = None;
        install_authoring_edit(
            state,
            assembly.approach.edit,
            Some(old),
            "Joined the full seam · Cloth simulation ready".into(),
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
