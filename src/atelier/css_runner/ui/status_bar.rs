//! Simulation and document status.

use super::super::state::CssRunnerState;
use cssimpler::core::Node;
use cssimpler::ui;

pub(in super::super) fn build_runner_status_bar(state: &CssRunnerState) -> Node {
    let step_info = format!(
        "Step: {} | Worker: {:.1} ms | Status: {} | Tool: {} / {}",
        state.step_count,
        state.step_ms,
        state.status_message,
        state.active_3d_tool,
        state.active_2d_tool
    );
    let mesh_info = format!(
        "Cloth: {} verts, {} tris | Avatar: {} tris",
        state.scene.state.x.len(),
        state.scene.garment.mesh.triangles.len(),
        state.scene.collider.surface.triangles.len()
    );

    ui! {
        <footer class="status-bar">
            <div class="status-left">
                <span class="status-indicator">"●"</span>
                <span class="status-message">{step_info}</span>
            </div>
            <div class="status-right">
                <span class="status-mesh-info">{mesh_info}</span>
            </div>
        </footer>
    }
}
