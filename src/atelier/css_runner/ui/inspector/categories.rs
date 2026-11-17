//! Pattern, sewing, form, material and environment categories.

use super::super::super::actions::{RunnerUiOp, queue_ui_op};
use super::super::super::board::selection::compute_connected_2d_highlight;
use super::super::super::state::CssRunnerState;
use cssimpler::core::Node;
use cssimpler::ui;

pub(in super::super::super) fn build_cat_pattern_2d(state: &CssRunnerState) -> Node {
    let panels = &state.project.template.panels;
    let connected = compute_connected_2d_highlight(state);
    let selected = panels.get(state.selected_piece_index);
    let name = selected.map_or("None", |p| p.id.as_str());
    let fabric = selected.map_or("Default", |p| p.fabric.as_str());
    let grain = selected.map_or(0.0, |p| p.grain_degrees);
    let count = panels.len();

    let mut piece_rows = Vec::new();
    for (idx, p) in panels.iter().enumerate() {
        let is_sel = idx == state.selected_piece_index;
        let is_conn = !is_sel && connected.connected_panels.contains(&p.id);
        let item_class = if is_sel {
            "tree-item active pointer-item"
        } else if is_conn {
            "tree-item connected pointer-item"
        } else {
            "tree-item pointer-item"
        };
        let label = format!("{}: {}", p.id, p.name);
        let grain_str = if is_conn {
            format!("{:.0}° · linked", p.grain_degrees)
        } else {
            format!("{:.0}°", p.grain_degrees)
        };
        piece_rows.push(ui! {
            <div class={item_class} onclick_shared={queue_ui_op(&state.pending_ui_ops, RunnerUiOp::SelectPiece(idx))}>
                <span>{label}</span>
                <span>{grain_str}</span>
            </div>
        });
    }

    ui! {
        <div class="tab-panel">
            <h3 class="panel-heading">"Pattern Pieces"</h3>
            <div class="prop-row">
                <span class="prop-label">"Total Pieces"</span>
                <span class="prop-value">{format!("{count}")}</span>
            </div>
            <div class="prop-row">
                <span class="prop-label">"Selected ID"</span>
                <span class="prop-value">{name}</span>
            </div>
            <div class="prop-row">
                <span class="prop-label">"Fabric Preset"</span>
                <span class="prop-value">{fabric}</span>
            </div>
            <div class="prop-row">
                <span class="prop-label">"Grain Angle"</span>
                <span class="prop-value">{format!("{grain:.1}°")}</span>
            </div>
            <div class="browser-tree browser-tree-spaced">
                {piece_rows}
            </div>
        </div>
    }
}

pub(in super::super::super) fn build_cat_sewing(state: &CssRunnerState) -> Node {
    let seams = &state.project.template.seams;
    let seams_count = seams.len();

    let mut seam_rows = Vec::new();
    for (idx, s) in seams.iter().enumerate() {
        let label = format!("{}: {} ↔ {}", s.id, s.a.panel, s.b.panel);
        let rev_badge = if s.reverse_b { " (rev)" } else { "" };
        seam_rows.push(ui! {
            <div class="tree-item">
                <span>{label}</span>
                <span class="badge-accent">{format!("#{idx}{rev_badge}")}</span>
            </div>
        });
    }

    ui! {
        <div class="tab-panel">
            <h3 class="panel-heading">"Sewing Seams"</h3>
            <div class="prop-row">
                <span class="prop-label">"Total Seams"</span>
                <span class="prop-value">{format!("{seams_count}")}</span>
            </div>
            <div class="prop-row">
                <span class="prop-label">"Active Tool"</span>
                <span class="prop-value">{state.active_2d_tool.clone()}</span>
            </div>
            <div class="browser-tree browser-tree-spaced">
                {seam_rows}
            </div>
        </div>
    }
}

pub(in super::super::super) fn build_cat_form_3d(state: &CssRunnerState) -> Node {
    let verts = state.scene.garment.mesh.uv.len();
    let tris = state.scene.garment.mesh.triangles.len();
    let pins_count = state
        .project
        .template
        .panels
        .iter()
        .map(|p| p.pins.len() + p.pin_points.len())
        .sum::<usize>();

    ui! {
        <div class="tab-panel">
            <h3 class="panel-heading">"3D Form & Positioning"</h3>
            <div class="prop-row">
                <span class="prop-label">"Garment Vertices"</span>
                <span class="prop-value">{format!("{verts}")}</span>
            </div>
            <div class="prop-row">
                <span class="prop-label">"Garment Triangles"</span>
                <span class="prop-value">{format!("{tris}")}</span>
            </div>
            <div class="prop-row">
                <span class="prop-label">"Pinned Points"</span>
                <span class="prop-value">{format!("{pins_count}")}</span>
            </div>
            <div class="prop-row">
                <span class="prop-label">"Active 3D Tool"</span>
                <span class="prop-value">{state.active_3d_tool.clone()}</span>
            </div>
        </div>
    }
}

pub(in super::super::super) fn build_cat_material(state: &CssRunnerState) -> Node {
    let mat_count = state.scene.garment.materials.len();
    let fabrics_count = state.project.template.fabrics.len();

    let mut fabric_rows = Vec::new();
    for f in &state.project.template.fabrics {
        let dens = format!("{:.0} g/m²", f.properties.density * 1000.0);
        fabric_rows.push(ui! {
            <div class="tree-item">
                <span>{f.name.clone()}</span>
                <span class="text-muted-sm">{dens}</span>
            </div>
        });
    }

    ui! {
        <div class="tab-panel">
            <h3 class="panel-heading">"Fabric & Appearance"</h3>
            <div class="prop-row">
                <span class="prop-label">"Defined Fabrics"</span>
                <span class="prop-value">{format!("{fabrics_count}")}</span>
            </div>
            <div class="prop-row">
                <span class="prop-label">"Materials Bound"</span>
                <span class="prop-value">{format!("{mat_count}")}</span>
            </div>
            <div class="prop-row">
                <span class="prop-label">"Albedo Maps"</span>
                <span class="prop-value">"Portable Linear-Light"</span>
            </div>
            <div class="browser-tree browser-tree-spaced">
                {fabric_rows}
            </div>
        </div>
    }
}

pub(in super::super::super) fn build_cat_environment(state: &CssRunnerState) -> Node {
    let avatar_tris = state.scene.collider.surface.triangles.len();
    let clearance_mm = format!("{:.1} mm", state.project.contact_clearance * 1000.0);

    ui! {
        <div class="tab-panel">
            <h3 class="panel-heading">"Avatar & Environment"</h3>
            <div class="prop-row">
                <span class="prop-label">"Avatar Triangles"</span>
                <span class="prop-value">{format!("{avatar_tris}")}</span>
            </div>
            <div class="prop-row">
                <span class="prop-label">"Contact Clearance"</span>
                <span class="prop-value">{clearance_mm}</span>
            </div>
            <div class="prop-row">
                <span class="prop-label">"Profiles Store"</span>
                <span class="prop-value">"workspace-profiles.json"</span>
            </div>
            <div class="prop-row">
                <span class="prop-label">"Status"</span>
                <span class="prop-value">"Synced"</span>
            </div>
        </div>
    }
}
