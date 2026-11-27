//! Transactional authoring commands, history and geometry previews.

use super::board::corners::{
    clamp_fillet_radius_m, connected_batch_fillet_max_radius_m, resolve_corner_edge,
};
use super::board::geometry::fit_2d_board;
use super::board::selection::{
    batch_corners_for_points, effective_selected_points, sanitize_2d_subselection,
};
use super::board::types::ToolDraft2d;
use super::state::CssRunnerState;
use clothcraft::authoring::corner_tool::{
    CIRCLE_TENSION, DartPointOptions, is_sewn_dart_point,
    preview_adjust_handle_with_piece_mirroring, preview_dart_point_with_piece_mirroring,
    preview_move_corner, preview_move_groups_with_piece_mirroring, preview_move_piece_corner,
    slide_corner_position_with_mirroring, validate_fillet_batch,
};
use clothcraft::authoring::{self, Command};
use clothcraft::fitting::FittingScene;
use clothcraft::garment::GarmentTemplate;
use clothcraft::garment::meshing::MeshingSettings;
use clothcraft::garment::seam::{BoundaryCurve, BoundaryRef, SeamSpec, Side};
use glam::DVec2;
use std::sync::Arc;

pub(super) fn default_fabric_id(state: &CssRunnerState) -> String {
    state
        .project
        .template
        .panels
        .get(state.selected_piece_index)
        .map(|p| p.fabric.clone())
        .or_else(|| state.project.template.fabrics.first().map(|f| f.id.clone()))
        .unwrap_or_else(|| "cotton".to_string())
}

pub(super) fn install_authoring_edit(
    state: &mut CssRunnerState,
    edit: authoring::Edit,
    record_old: Option<GarmentTemplate>,
    msg: String,
) {
    install_authoring_edit_with_pose(state, edit, record_old, msg, None);
}

pub(super) fn install_authoring_edit_with_pose(
    state: &mut CssRunnerState,
    edit: authoring::Edit,
    record_old: Option<GarmentTemplate>,
    mut msg: String,
    pose: Option<clothcraft::model::State>,
) {
    super::seam_assembly::cancel_motion(state);
    super::seam_assembly::clear_collision(state);
    if let Some(old) = record_old {
        state.history.record(old);
    }
    let next_garment = Arc::new(edit.garment.clone());
    let next_state = if let Some(pose) = pose {
        pose
    } else if edit.reset_simulation {
        edit.state_after(&state.scene.garment, &state.scene.state)
    } else {
        state.scene.state.clone()
    };

    if let Some(ref sel_id) = edit.selected
        && let Some(idx) = edit.template.panels.iter().position(|p| &p.id == sel_id)
    {
        state.selected_piece_index = idx;
    }
    if let Some(np) = edit.new_point
        && let Some(p) = edit.template.panels.get(state.selected_piece_index)
    {
        state.set_single_2d_point(p.id.clone(), np);
    }

    // Keep the existing framing behavior when pieces enter or leave the
    // board. Editing points on existing pieces keeps the current camera.
    let previous_panels = &state.project.template.panels;
    if previous_panels.len() != edit.template.panels.len()
        || previous_panels
            .iter()
            .zip(&edit.template.panels)
            .any(|(old, new)| old.id != new.id)
    {
        state.board_2d_fit = fit_2d_board(&edit.template);
    }
    state.project.template = edit.template;
    state.seam_mode_error = None;
    if state.selected_seam.as_ref().is_some_and(|id| {
        !state
            .project
            .template
            .seams
            .iter()
            .any(|seam| &seam.id == id)
    }) {
        state.selected_seam = None;
    }
    if state.seam_to_join.as_ref().is_some_and(|id| {
        !state
            .project
            .template
            .seams
            .iter()
            .any(|seam| &seam.id == id)
    }) {
        state.seam_to_join = None;
    }
    state.files.modified = true;
    state
        .mirrored_pieces
        .retain(|id| state.project.template.panels.iter().any(|p| p.id == *id));
    if edit.reset_simulation {
        state.project.state = None;
        state.project.mesh_signature = None;
        state.generation = state.generation.wrapping_add(1);
        state.worker_pending = false;
        state.scene = FittingScene {
            garment: Arc::clone(&next_garment),
            collider: Arc::clone(&state.scene.collider),
            state: next_state.clone(),
            quasistatic: state.scene.quasistatic,
            next_dt: state.scene.next_dt,
        };
        state.initial_scene = FittingScene {
            garment: next_garment,
            collider: Arc::clone(&state.scene.collider),
            state: state.scene.garment.initial_state.clone(),
            quasistatic: state.scene.quasistatic,
            next_dt: state.scene.next_dt,
        };
        if let Some(ref usd) = state.usd_scene
            && let Err(error) = clothcraft::usd_scene::update_panel_viewport_mesh(
                usd,
                &state.scene.garment,
                &state.scene.state,
                state.shading,
            )
        {
            msg = format!("{msg} · Viewport update failed: {error}");
        }
        state.sync_usd_wireframe();
        state.send_control_state(false);
    }

    sanitize_2d_subselection(state);
    state.status_message = msg;
}

pub fn commit_authoring_command(
    state: &mut CssRunnerState,
    cmd: Command,
    msg: impl Into<String>,
) -> bool {
    state.preview_template = None;
    let before = state.project.template.clone();
    let replaces_pieces = matches!(
        &cmd,
        Command::Polygon { replace: true, .. } | Command::Primitive { replace: true, .. }
    );
    match authoring::apply_with_piece_mirroring(&before, cmd, &state.mirrored_pieces) {
        Ok(edit) => {
            if replaces_pieces {
                state.mirrored_pieces.clear();
            }
            install_authoring_edit(state, edit, Some(before), msg.into());
            true
        }
        Err(err) => {
            state.status_message = format!("Edit rejected: {err}");
            false
        }
    }
}

pub fn undo_redo_2d(state: &mut CssRunnerState, redo: bool) -> bool {
    if !redo && super::seam_assembly::cancel(state) {
        return true;
    }
    state.preview_template = None;
    match state.history.restore(&state.project.template, redo) {
        Ok(edit) => {
            let label = if redo {
                "Redid 2D edit"
            } else {
                "Undid 2D edit"
            };
            install_authoring_edit(state, edit, None, label.into());
            true
        }
        Err(err) => {
            state.status_message = err;
            false
        }
    }
}

pub(super) fn round_corners_batch(
    state: &mut CssRunnerState,
    corners: &[(String, String)],
    max_radius_m: f64,
) -> bool {
    let Some((panel_id, edge_name)) = corners.first() else {
        return false;
    };
    let Some(eff_r) = clamp_fillet_radius_m(state.corner_radius_mm, max_radius_m) else {
        state.status_message = "Incident boundaries too short to round".into();
        return false;
    };
    let tension = if state.corner_mode_radius {
        CIRCLE_TENSION
    } else {
        state.corner_tension
    };
    let total_corners = validate_fillet_batch(&state.project.template, corners, eff_r, tension)
        .map_or(corners.len(), |v| v.corners.len());

    if corners.len() == 1 {
        let linked_extra = total_corners.saturating_sub(1);
        let msg = if linked_extra > 0 {
            format!(
                "Rounded corner on {edge_name} (+{linked_extra} linked) (R={:.1} mm, T={tension:.2})",
                eff_r * 1000.0
            )
        } else {
            format!(
                "Rounded corner on {edge_name} (R={:.1} mm, T={tension:.2})",
                eff_r * 1000.0
            )
        };
        let ok = commit_authoring_command(
            state,
            Command::RoundCornerShape {
                panel: panel_id.clone(),
                edge: edge_name.clone(),
                radius_m: eff_r,
                tension,
                rule: state.corner_rule,
            },
            msg,
        );
        if ok {
            state.clear_2d_subselection();
            // Select the newly created fillet curve edge for immediate Bézier inspection
            if let Some(p) = state
                .project
                .template
                .panels
                .iter()
                .find(|p| &p.id == panel_id)
                && let Some(ref poly) = p.polygon
                && let Some(idx) = poly.edges.iter().position(|e| e == edge_name)
            {
                let fillet_idx = (idx + poly.edges.len() - 1) % poly.edges.len();
                state.selected_2d_edge = Some((panel_id.clone(), poly.edges[fillet_idx].clone()));
            }
        }
        return ok;
    }

    let ok = commit_authoring_command(
        state,
        Command::RoundCornersShape {
            corners: corners.to_vec(),
            radius_m: eff_r,
            tension,
            rule: state.corner_rule,
        },
        format!(
            "Rounded {} selected corners ({total_corners} total w/ linked) (R={:.1} mm, T={tension:.2})",
            corners.len(),
            eff_r * 1000.0
        ),
    );
    if ok {
        state.clear_2d_subselection();
    }
    ok
}

pub(super) fn round_corner_by_name(
    state: &mut CssRunnerState,
    panel_id: &str,
    edge_name: &str,
    max_radius_m: f64,
) -> bool {
    round_corners_batch(
        state,
        &[(panel_id.to_owned(), edge_name.to_owned())],
        max_radius_m,
    )
}

pub(super) fn round_selected_corner(state: &mut CssRunnerState) -> bool {
    let sel_pts = effective_selected_points(state);
    let corners = if !sel_pts.is_empty() {
        batch_corners_for_points(&state.project.template, &sel_pts)
    } else if let Some((ref pid, ref ename)) = state.selected_2d_edge {
        vec![(pid.clone(), ename.clone())]
    } else {
        Vec::new()
    };

    if corners.is_empty() {
        state.status_message = "Select one or more corner points first to round".into();
        return false;
    }
    match connected_batch_fillet_max_radius_m(&state.project.template, &corners) {
        Ok(val) if val.max_radius_m > 1e-4 => {
            round_corners_batch(state, &corners, val.max_radius_m)
        }
        Ok(_) => {
            state.status_message = "Incident boundaries too short to round".into();
            false
        }
        Err(err) => {
            state.status_message = format!("Cannot round corner: {err}");
            false
        }
    }
}

pub(super) fn toggle_selected_curve(state: &mut CssRunnerState, curved: bool) -> bool {
    let sel_pts = effective_selected_points(state);
    let points = if !sel_pts.is_empty() {
        sel_pts
    } else if let Some((ref pid, ref ename)) = state.selected_2d_edge {
        state
            .project
            .template
            .panels
            .iter()
            .find(|p| &p.id == pid)
            .and_then(|p| p.polygon.as_ref())
            .and_then(|poly| poly.edges.iter().position(|e| e == ename))
            .map_or_else(Vec::new, |idx| vec![(pid.clone(), idx)])
    } else {
        Vec::new()
    };
    if points.is_empty() {
        state.status_message = "Select a point or boundary to toggle Bézier curves".into();
        return false;
    }
    let count = points.len();
    let msg = if count == 1 {
        if curved {
            "Converted incident boundaries to cubic Bézier".to_string()
        } else {
            "Straightened incident boundaries".to_string()
        }
    } else if curved {
        format!("Converted incident boundaries on {count} selected corners to cubic Bézier")
    } else {
        format!("Straightened incident boundaries on {count} selected corners")
    };
    commit_authoring_command(state, Command::CurvePoints { points, curved }, msg)
}

pub(super) fn point_drag_target(
    state: &CssRunnerState,
    panel_id: &str,
    point_idx: usize,
    position: DVec2,
) -> Result<(DVec2, bool), String> {
    let source = &state.project.template;
    let dart = is_sewn_dart_point(source, panel_id, point_idx);
    let position = if state.slide_on_edge
        && !dart
        && resolve_corner_edge(source, panel_id, point_idx).is_some()
    {
        slide_corner_position_with_mirroring(
            source,
            panel_id,
            point_idx,
            position,
            state.mirrored_pieces.contains(panel_id),
        )?
    } else {
        position
    };
    Ok((position, dart))
}

pub(super) fn update_preview_point(
    state: &mut CssRunnerState,
    panel_id: &str,
    point_idx: usize,
    local_pos: DVec2,
) {
    let Ok((local_pos, dart)) = point_drag_target(state, panel_id, point_idx, local_pos) else {
        return;
    };
    let mut draft = state.project.template.clone();
    let source = &state.project.template;
    let preview = if dart {
        preview_dart_point_with_piece_mirroring(
            source,
            &mut draft,
            panel_id,
            point_idx,
            local_pos,
            DartPointOptions {
                mirror: state.mirror_dart_bases,
                slide: state.slide_on_edge,
            },
            &state.mirrored_pieces,
        )
    } else if state.mirrored_pieces.contains(panel_id) {
        preview_move_piece_corner(
            source,
            &mut draft,
            panel_id,
            point_idx,
            local_pos,
            &state.mirrored_pieces,
        )
    } else {
        preview_move_corner(source, &mut draft, panel_id, point_idx, local_pos)
    };
    if preview.is_ok() {
        state.preview_template = Some(draft);
    }
}

pub(super) fn update_preview_point_groups(
    state: &mut CssRunnerState,
    points: &[(String, usize)],
    board_delta: DVec2,
) {
    let mut draft = state.project.template.clone();
    if preview_move_groups_with_piece_mirroring(
        &state.project.template,
        &mut draft,
        points,
        board_delta,
        &state.mirrored_pieces,
    )
    .is_ok()
    {
        state.preview_template = Some(draft);
    }
}

pub(super) fn update_preview_bezier_handle(
    state: &mut CssRunnerState,
    panel_id: &str,
    edge_name: &str,
    handle_index: usize,
    position: DVec2,
) {
    let mut draft = state.project.template.clone();
    if preview_adjust_handle_with_piece_mirroring(
        &state.project.template,
        &mut draft,
        panel_id,
        edge_name,
        handle_index,
        position,
        (state.corner_rule, state.mirrored_pieces.contains(panel_id)),
    )
    .is_ok()
    {
        state.preview_template = Some(draft);
    }
}

pub(super) fn finish_pen_draft(state: &mut CssRunnerState) -> bool {
    let Some(ToolDraft2d::Pen { points_board_m, .. }) = state.draft_2d.take() else {
        return false;
    };
    if points_board_m.len() < 3 {
        state.status_message = "Pen tool requires at least 3 vertices".into();
        return true;
    }
    let count = points_board_m.len();
    let fabric = default_fabric_id(state);
    commit_authoring_command(
        state,
        Command::Polygon {
            points: points_board_m,
            tangents: vec![],
            meshing: MeshingSettings::default(),
            fabric,
            replace: false,
        },
        format!("Created {count}-point polygon pattern"),
    );
    true
}

pub(super) fn finish_internal_line_draft(state: &mut CssRunnerState) -> bool {
    let Some(ToolDraft2d::InternalLine {
        panel_id,
        points_local_m,
        ..
    }) = state.draft_2d.take()
    else {
        return false;
    };
    if points_local_m.len() < 2 {
        state.status_message = "Internal Line requires at least 2 points".into();
        return true;
    }
    let count = points_local_m.len();
    commit_authoring_command(
        state,
        Command::InternalLine {
            panel: panel_id.clone(),
            points: points_local_m,
            tangents: vec![DVec2::ZERO; count],
        },
        format!("Added {count}-point internal line on {panel_id}"),
    );
    true
}

/// Measure the same compiled material boundary that the seam compiler uses.
pub(super) fn sew_boundary_length(state: &CssRunnerState, panel: &str, edge: &str) -> Option<f64> {
    let garment = &state.scene.garment;
    let range = garment.panels.iter().find(|p| p.id == panel)?;
    let vertices = range.boundaries.get(edge)?.clone();
    BoundaryCurve::new(&garment.mesh, vertices, [0.0, 1.0], false)
        .ok()
        .map(|curve| curve.length)
}

/// Same relative tolerance the seam compiler uses before it demands `allow_ease`
/// (`garment::seam`).
pub fn sew_lengths_equal(a_len_m: f64, b_len_m: f64) -> bool {
    (a_len_m - b_len_m).abs() <= 1e-8 * a_len_m.max(b_len_m)
}

fn picked_sew_boundaries(a: &(String, String), b: &(String, String)) -> [BoundaryRef; 2] {
    [(a, Side::Right), (b, Side::Left)].map(|((panel, edge), side)| BoundaryRef {
        panel: panel.clone(),
        edge: Some(edge.clone()),
        side,
        range: [0., 1.],
        reversed: false,
    })
}

pub(in super::super) struct ReflectedSewPreview {
    pub edges: [(String, String); 2],
    pub lengths: [f64; 2],
}

impl ReflectedSewPreview {
    pub fn length_difference_m(&self) -> Option<f64> {
        let [a, b] = self.lengths;
        (!sew_lengths_equal(a, b)).then_some((a - b).abs())
    }
}

pub(in super::super) fn reflected_sew_preview(
    state: &CssRunnerState,
    a: &(String, String),
    b: &(String, String),
    reverse: bool,
) -> Result<Option<ReflectedSewPreview>, String> {
    let [a, b] = picked_sew_boundaries(a, b);
    let seam = SeamSpec {
        id: String::new(),
        a,
        b,
        a_tail: vec![],
        b_tail: vec![],
        reverse_b: reverse,
        allow_ease: false,
        notches: vec![],
        stiffness: 50_000.,
    };
    let Some(mate) =
        authoring::mirrored_seam(&state.project.template, &state.mirrored_pieces, &seam)?
    else {
        return Ok(None);
    };
    let [Some(a), Some(b)] = [mate.a, mate.b].map(|r| r.edge.map(|edge| (r.panel, edge))) else {
        return Err("The reflected seam needs named boundaries".into());
    };
    let lengths = [
        sew_boundary_length(state, &a.0, &a.1),
        sew_boundary_length(state, &b.0, &b.1),
    ];
    let [Some(a_len), Some(b_len)] = lengths else {
        return Err("Could not measure the reflected sewing boundaries".into());
    };
    Ok(Some(ReflectedSewPreview {
        edges: [a, b],
        lengths: [a_len, b_len],
    }))
}

pub(super) fn sew_confirm_prompt(
    state: &CssRunnerState,
    a: &(String, String),
    b: &(String, String),
    a_len_m: f64,
    b_len_m: f64,
    reverse: bool,
) -> String {
    match reflected_sew_preview(state, a, b, reverse) {
        Ok(Some(mate)) => {
            return if !sew_lengths_equal(a_len_m, b_len_m) || mate.length_difference_m().is_some() {
                "Choose Sew with easing to confirm both seams, or Escape to cancel".into()
            } else {
                "Press Enter or Sew to confirm both seams, or Escape to cancel".into()
            };
        }
        Err(error) => return error,
        Ok(None) => {}
    }
    let lengths = format!(
        "Sew {}.{} ({:.1} mm) ↔ {}.{} ({:.1} mm)",
        a.0,
        a.1,
        a_len_m * 1e3,
        b.0,
        b.1,
        b_len_m * 1e3
    );
    if sew_lengths_equal(a_len_m, b_len_m) {
        format!("{lengths}: press Enter or Sew to confirm, Escape to cancel")
    } else {
        format!(
            "{lengths}: lengths differ by {:.1} mm; choose Sew with easing or Escape to cancel",
            (a_len_m - b_len_m).abs() * 1e3
        )
    }
}

/// Commit the pending seam. Without `ease`, unequal lengths are refused and the
/// draft is kept so the user can still choose easing or cancel.
pub fn confirm_pending_sew(state: &mut CssRunnerState, ease: bool) -> bool {
    finish_pending_sew(state, ease, false)
}

pub(super) fn select_seam(state: &mut CssRunnerState, id: &str) {
    if state
        .project
        .template
        .seams
        .iter()
        .any(|seam| seam.id == id)
    {
        state.active_category = super::state::ObjectCategory::Sewing;
        state.selected_seam = Some(id.into());
        super::seam_assembly::clear_collision(state);
        state.seam_mode_error = None;
        state.draft_2d = None;
        state.status_message = format!("Selected seam {id}");
    }
}

pub(super) fn set_seam_mode(state: &mut CssRunnerState, id: &str, join_on_play: bool) {
    if state.selected_seam.as_deref() != Some(id) {
        return;
    }
    state.seam_mode_error = None;
    super::seam_assembly::clear_collision(state);
    if !join_on_play {
        if state.seam_to_join.as_deref() == Some(id) {
            let was_playing = state.playing;
            super::seam_assembly::cancel(state);
            state.playing = was_playing;
            state.send_control_state(false);
            state.status_message = format!("{id} · Solver Pull");
        }
        return;
    }
    if state.seam_to_join.as_deref() == Some(id) {
        return;
    }
    if state.seam_to_join.is_some() || state.seam_assembly.is_some() {
        state.seam_mode_error = Some("Finish or cancel the pending join first".into());
        return;
    }
    state.seam_to_join = Some(id.into());
    state.status_message = format!("{id} · Join on Play");
    if state.playing {
        state.send_control_state(false);
    }
}

pub(super) fn align_and_sew(state: &mut CssRunnerState, ease: bool) -> bool {
    finish_pending_sew(state, ease, true)
}

fn finish_pending_sew(state: &mut CssRunnerState, ease: bool, align: bool) -> bool {
    if state.seam_assembly.is_some() || (align && state.seam_to_join.is_some()) {
        state.status_message = "Finish the pending seam or press Esc to cancel it".into();
        return false;
    }
    let Some(ToolDraft2d::SewConfirm {
        a,
        b,
        reverse,
        a_len_m,
        b_len_m,
    }) = state.draft_2d.clone()
    else {
        return false;
    };
    let mirrored = match reflected_sew_preview(state, &a, &b, reverse) {
        Ok(mirrored) => mirrored,
        Err(error) => {
            state.status_message = error;
            return false;
        }
    };
    let original_difference =
        (!sew_lengths_equal(a_len_m, b_len_m)).then_some((a_len_m - b_len_m).abs());
    let difference = original_difference.or(mirrored
        .as_ref()
        .and_then(ReflectedSewPreview::length_difference_m));
    if let Some(difference) = difference.filter(|_| !ease) {
        state.status_message = format!(
            "Lengths differ by {:.1} mm: choose Sew with easing to accept the mismatch, or Escape to cancel",
            difference * 1e3
        );
        return false;
    }
    let equal = difference.is_none();
    let message = if mirrored.is_some() {
        if equal {
            "Sewed selected and reflected boundaries".into()
        } else {
            "Sewed selected and reflected boundaries with easing".into()
        }
    } else if equal {
        format!("Sewed {}.{} ↔ {}.{}", a.0, a.1, b.0, b.1)
    } else {
        format!(
            "Sewed {}.{} ↔ {}.{} with {:.1} mm easing",
            a.0,
            a.1,
            b.0,
            b.1,
            (a_len_m - b_len_m).abs() * 1e3
        )
    };
    let [a, b] = picked_sew_boundaries(&a, &b);
    let command = Command::Sew {
        a,
        b,
        reverse,
        ease: ease && !equal,
    };
    let committed = if align {
        let before = &state.project.template;
        let result = authoring::apply_with_piece_mirroring(before, command, &state.mirrored_pieces)
            .and_then(|edit| {
                clothcraft::fitting::seam_placement::prepare(&state.scene, before, edit)
            });
        match result {
            Ok(approach) => {
                state.preview_template = None;
                super::seam_assembly::begin(state, approach);
                state.send_control_state(false);
                true
            }
            Err(error) => {
                state.status_message = format!("Alignment unavailable · {error}");
                false
            }
        }
    } else {
        commit_authoring_command(state, command, message)
    };
    if committed {
        state.draft_2d = None;
    }
    committed
}
