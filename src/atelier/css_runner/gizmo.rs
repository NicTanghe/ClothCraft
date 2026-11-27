//! Input and transactional placement previews for the GPU viewport handles.

use super::state::CssRunnerState;
use crate::atelier::placement_gizmo::{self, Handle, Mode, Overlay, Projection};
use clothcraft::authoring::{Command, rotate_about};
use clothcraft::garment::template::{PanelRange, Placement};
use clothcraft::picking::{ray_plane, visible_cloth_face};
use clothcraft::usd_scene::viewport_panel_path;
use cssimpler::renderer::gpu::GpuViewportInfo;
use cssimpler::renderer::{ButtonState, EngineEvent, KeyIdentity, PointerButton};
use glam::{DMat3, DQuat, DVec2, DVec3, EulerRot, Vec2, Vec3};

#[derive(Default)]
pub(super) struct Gizmo {
    pub enabled: bool,
    mode: Mode,
    hover: bool,
    toggle_pressed: bool,
    axis_hover: Option<Vec3>,
    select_start: Option<Vec2>,
    drag: Option<Drag>,
    pivot: Option<Pivot>,
}

struct Pivot {
    panel_id: String,
    generation: u64,
    step_count: u64,
    material: DVec3,
    world: DVec3,
}

struct Drag {
    panel_id: String,
    material_pivot: DVec3,
    world_pivot: DVec3,
    original: Placement,
    pending: Option<Placement>,
    mode: Mode,
    handle: Handle,
    start: Vec2,
    rotation: Option<Rotation>,
    preview_dirty: bool,
    preview_published: bool,
}

struct Rotation {
    radial: DVec3,
    angle: f64,
}

impl Drag {
    fn update(&mut self, projection: Projection, pointer: Vec2, snap: bool) {
        let tangent_delta = f64::from(
            (pointer - self.start).dot(self.handle.tangent) * self.handle.units_per_pixel,
        );
        let axis = self.handle.axis.as_dvec3();
        let mut amount = match (self.mode, &mut self.rotation) {
            (Mode::Rotate, Some(rotation)) => {
                if let Some(radial) = radial(projection, pointer, self.world_pivot, axis) {
                    let previous = rotation.radial;
                    rotation.angle += axis.dot(previous.cross(radial)).atan2(previous.dot(radial));
                    rotation.radial = radial;
                }
                rotation.angle
            }
            _ => tangent_delta,
        };
        if snap {
            let step = match self.mode {
                Mode::Move => 0.01,
                Mode::Rotate => 15_f64.to_radians(),
            };
            amount = (amount / step).round() * step;
        }
        self.pending = (amount.abs() > 1e-6).then(|| {
            let mut start = self.original.clone();
            // Drape can shift the displayed center away from the original
            // arrangement. Start the flat preview at the visible handle.
            start.translation += self.world_pivot - start.transform(self.material_pivot);
            match self.mode {
                Mode::Move => {
                    start.translation += axis * amount;
                    start
                }
                Mode::Rotate => rotate_about(&start, self.material_pivot, axis, amount),
            }
        });
        self.preview_dirty = true;
    }
}

fn radial(projection: Projection, pointer: Vec2, pivot: DVec3, axis: DVec3) -> Option<DVec3> {
    let center = projection.point(pivot.as_vec3())?;
    if pointer.distance(center) < 8.0 {
        return None;
    }
    let hit = ray_plane(
        projection.eye.as_dvec3(),
        projection.ray(pointer).as_dvec3(),
        pivot,
        axis,
    )?;
    (hit - pivot).try_normalize()
}

fn visible(state: &CssRunnerState) -> bool {
    !state.clean_view && state.layout_mode != clothcraft::css_ui::ViewLayoutMode::TwoDOnly
}

fn blocked(state: &CssRunnerState) -> bool {
    state.settings_open
        || state.command_palette_open
        || state.solver_menu_open
        || state.import_dialog.is_some()
}

fn placement_active(state: &CssRunnerState) -> bool {
    state.seam_assembly.is_none() && state.gizmo.enabled && state.active_3d_tool == "Gizmo"
}

fn selected_range(state: &CssRunnerState) -> Option<&PanelRange> {
    let panel = state
        .project
        .template
        .panels
        .get(state.selected_piece_index)?;
    state
        .scene
        .garment
        .panels
        .iter()
        .find(|range| range.id == panel.id)
}

fn held(state: &CssRunnerState, range: &PanelRange) -> bool {
    state.scene.garment.authored_holds[range.vertices.clone()]
        .iter()
        .any(|held| *held)
}

fn sync_pivot(state: &mut CssRunnerState) {
    if !placement_active(state) || !visible(state) {
        state.gizmo.pivot = None;
        return;
    }
    let Some(range) = selected_range(state) else {
        state.gizmo.pivot = None;
        return;
    };
    if state.gizmo.pivot.as_ref().is_some_and(|pivot| {
        pivot.panel_id == range.id
            && pivot.generation == state.generation
            && pivot.step_count == state.step_count
    }) {
        return;
    }
    if held(state, range) {
        state.gizmo.pivot = None;
        return;
    }
    let count = range.vertices.len() as f64;
    let uv = state.scene.garment.mesh.uv[range.vertices.clone()]
        .iter()
        .sum::<glam::DVec2>()
        / count;
    let world = state.scene.state.x[range.vertices.clone()]
        .iter()
        .sum::<DVec3>()
        / count;
    state.gizmo.pivot = Some(Pivot {
        panel_id: range.id.clone(),
        generation: state.generation,
        step_count: state.step_count,
        material: DVec3::new(uv.x, -uv.y, 0.0),
        world,
    });
}

fn world_pivot(state: &CssRunnerState) -> Option<Vec3> {
    if let Some(drag) = &state.gizmo.drag {
        return Some(
            drag.pending
                .as_ref()
                .map_or(drag.world_pivot, |placement| {
                    placement.transform(drag.material_pivot)
                })
                .as_vec3(),
        );
    }
    state
        .gizmo
        .pivot
        .as_ref()
        .map(|pivot| pivot.world.as_vec3())
}

struct View {
    origin: Vec2,
    size: Vec2,
    projection: Projection,
}

impl View {
    fn new(state: &CssRunnerState) -> Option<Self> {
        let (origin, size, aspect) = if let Some(info) = state.gizmo_viewport.get() {
            let [width, height] = info.physical_size;
            (
                Vec2::new(info.bounds.x, info.bounds.y),
                Vec2::new(info.bounds.width, info.bounds.height),
                width as f32 / height as f32,
            )
        } else {
            let rect = super::board::geometry::viewport_3d_rect(state);
            (
                Vec2::new(rect.x, rect.y),
                Vec2::new(rect.w, rect.h),
                rect.w / rect.h,
            )
        };
        (size.min_element() > 0.0 && aspect.is_finite() && aspect > 0.0).then(|| Self {
            origin,
            size,
            projection: Projection::new(&state.camera, size, aspect),
        })
    }

    fn contains(&self, pointer: Vec2) -> bool {
        pointer.cmpge(Vec2::ZERO).all() && pointer.cmplt(self.size).all()
    }
}

fn publish_points(
    state: &CssRunnerState,
    panel_id: &str,
    preview: Option<&Placement>,
) -> Result<(), String> {
    use cssimpler::renderer::gpu::openusd::gf;
    let Some(usd) = &state.usd_scene else {
        return Ok(());
    };
    let Some(panel) = state
        .scene
        .garment
        .panels
        .iter()
        .find(|panel| panel.id == panel_id)
    else {
        return Ok(());
    };
    let to_point = |p: DVec3| gf::vec3f(p.x as f32, p.y as f32, p.z as f32);
    let positions = match preview {
        Some(placement) => {
            let radians = placement.rotation_degrees * (std::f64::consts::PI / 180.0);
            let rotation = DQuat::from_euler(EulerRot::XYZ, radians.x, radians.y, radians.z);
            state.scene.garment.mesh.uv[panel.vertices.clone()]
                .iter()
                .map(|uv| to_point(rotation * DVec3::new(uv.x, -uv.y, 0.0) + placement.translation))
                .collect()
        }
        None => state.scene.state.x[panel.vertices.clone()]
            .iter()
            .copied()
            .map(to_point)
            .collect(),
    };
    usd.set_deformed_mesh_points(&viewport_panel_path(panel_id), positions)
        .map_err(|error| error.to_string())?;
    state.request_viewport_redraw();
    Ok(())
}

pub(super) fn cancel(state: &mut CssRunnerState) -> bool {
    state.gizmo.select_start = None;
    let Some(drag) = state.gizmo.drag.take() else {
        return false;
    };
    state.status_message = match restore_preview(state, &drag) {
        Ok(()) => "Cancelled 3D placement".into(),
        Err(error) => format!("Could not restore panel preview: {error}"),
    };
    true
}

fn restore_preview(state: &CssRunnerState, drag: &Drag) -> Result<(), String> {
    if drag.preview_published {
        publish_points(state, &drag.panel_id, None)?;
    }
    Ok(())
}

pub(super) fn sync(state: &mut CssRunnerState) {
    if !visible(state) || blocked(state) || !placement_active(state) {
        cancel(state);
    }
    if let Some(mut drag) = state.gizmo.drag.take() {
        if drag.preview_dirty {
            if drag.pending.is_some() || drag.preview_published {
                match publish_points(state, &drag.panel_id, drag.pending.as_ref()) {
                    Ok(()) => drag.preview_published = drag.pending.is_some(),
                    Err(error) => {
                        state.status_message = format!("3D placement preview failed: {error}")
                    }
                }
            }
            drag.preview_dirty = false;
        }
        state.gizmo.drag = Some(drag);
    }
    sync_pivot(state);
    let overlay = Overlay {
        visible: visible(state),
        mode: state.gizmo.mode,
        pivot: world_pivot(state),
        axis_highlight: state
            .gizmo
            .drag
            .as_ref()
            .map(|drag| drag.handle.axis)
            .or(state
                .gizmo
                .axis_hover
                .filter(|_| !blocked(state) && !state.pointer_down)),
        toggle_hovered: state.gizmo.hover && !blocked(state) && state.cube_pointer_present,
    };
    if state.gizmo_overlay.replace(overlay) != overlay {
        state.request_viewport_redraw();
    }
}

fn begin(state: &mut CssRunnerState, view: &View, pointer: Vec2, handle: Handle) {
    let Some(pivot) = &state.gizmo.pivot else {
        return;
    };
    let mode = state.gizmo.mode;
    let axis = handle.axis.as_dvec3();
    let rotation =
        if mode == Mode::Rotate && view.projection.ray(pointer).dot(handle.axis).abs() > 0.35 {
            radial(view.projection, pointer, pivot.world, axis)
                .map(|radial| Rotation { radial, angle: 0.0 })
        } else {
            None
        };
    state.gizmo.drag = Some(Drag {
        panel_id: pivot.panel_id.clone(),
        material_pivot: pivot.material,
        world_pivot: pivot.world,
        original: state.project.template.panels[state.selected_piece_index]
            .placement
            .clone(),
        pending: None,
        mode,
        handle,
        start: pointer,
        rotation,
        preview_dirty: false,
        preview_published: false,
    });
    pause_simulation(state);
    state.status_message = format!("{} panel · Shift to snap · Esc to cancel", mode.label());
}

fn pause_simulation(state: &mut CssRunnerState) {
    if state.playing || state.worker_pending {
        state.playing = false;
        state.worker_pending = false;
        state.generation = state.generation.wrapping_add(1);
        state.send_control_state(false);
    }
}

pub(super) fn spawn_rectangle(state: &mut CssRunnerState) {
    let size = DVec2::splat(0.25);
    let (board_center, board_span) = super::board::geometry::fit_2d_board(&state.project.template);
    let origin = board_center + DVec2::new(board_span * 0.5, -size.y * 0.5);
    let toward_camera = state.camera.direction().as_dvec3();
    let right = state
        .camera
        .up()
        .as_dvec3()
        .cross(toward_camera)
        .normalize();
    let up = toward_camera.cross(right);
    let center = state.camera.target.lerp(state.camera.eye(), 0.4).as_dvec3();
    let rotation = DQuat::from_mat3(&DMat3::from_cols(right, up, toward_camera));
    let (x, y, z) = rotation.to_euler(EulerRot::XYZ);
    let placement = Placement {
        translation: center - right * (size.x * 0.5) + up * (size.y * 0.5),
        rotation_degrees: DVec3::new(x.to_degrees(), y.to_degrees(), z.to_degrees()),
    };
    let fabric = super::editing::default_fabric_id(state);
    pause_simulation(state);
    if super::editing::commit_authoring_command(
        state,
        Command::SpawnRectangle {
            size,
            origin,
            fabric,
            placement,
        },
        "Added a 250 × 250 mm piece · drag a handle to place it",
    ) {
        state.gizmo.enabled = true;
        state.active_3d_tool = "Gizmo".into();
    }
}

fn finish(state: &mut CssRunnerState) {
    let Some(drag) = state.gizmo.drag.take() else {
        return;
    };
    if let Some(placement) = &drag.pending {
        let label = match drag.mode {
            Mode::Move => "Moved panel",
            Mode::Rotate => "Rotated panel",
        };
        if super::editing::commit_authoring_command(
            state,
            Command::Place {
                panel: drag.panel_id.clone(),
                placement: placement.clone(),
            },
            label,
        ) {
            return;
        }
    } else {
        state.status_message = "3D placement ready".into();
    }
    if let Err(error) = restore_preview(state, &drag) {
        state.status_message = format!("Could not restore panel preview: {error}");
    }
}

pub(super) fn handle_event(state: &mut CssRunnerState, event: &EngineEvent) -> bool {
    match event {
        EngineEvent::FocusChanged(false) | EngineEvent::Suspended | EngineEvent::PointerLeft => {
            state.gizmo.hover = false;
            state.gizmo.axis_hover = None;
            state.gizmo.toggle_pressed = false;
            state.input_dirty |= cancel(state);
            state.update_handle.request_update();
            return false;
        }
        EngineEvent::Key(key)
            if key.state == ButtonState::Pressed
                && matches!(&key.logical_key, KeyIdentity::Named(name) if name == "Escape") =>
        {
            return cancel(state);
        }
        EngineEvent::Key(_) if state.gizmo.drag.is_some() => return true,
        _ => {}
    }
    if blocked(state) || !visible(state) {
        return false;
    }
    let Some(view) = View::new(state) else {
        return false;
    };
    match event {
        EngineEvent::PointerMoved {
            position,
            modifiers,
        } => {
            let global = Vec2::new(position.x, position.y);
            let pointer = global - view.origin;
            if let Some(drag) = &mut state.gizmo.drag {
                drag.update(view.projection, pointer, modifiers.shift);
                state.last_pointer_pos = (global.x, global.y);
                return true;
            }
            if state
                .gizmo
                .select_start
                .is_some_and(|start| pointer.distance(start) > 3.0)
            {
                state.gizmo.select_start = None;
            }
            let hover = placement_gizmo::toggle_contains(view.size, pointer) && !state.pointer_down;
            let axis = if !hover && !state.pointer_down && view.contains(pointer) {
                world_pivot(state)
                    .and_then(|pivot| {
                        placement_gizmo::pick(view.projection, state.gizmo.mode, pivot, pointer)
                    })
                    .map(|handle| handle.axis)
            } else {
                None
            };
            let changed = state.gizmo.hover != hover || state.gizmo.axis_hover != axis;
            state.gizmo.hover = hover;
            state.gizmo.axis_hover = axis;
            if changed {
                state.input_dirty = true;
                state.update_handle.request_update();
            }
            false
        }
        EngineEvent::PointerButton {
            button: PointerButton::Primary,
            state: button,
            modifiers,
        } => {
            let (x, y) = state.last_pointer_pos;
            let pointer = Vec2::new(x, y) - view.origin;
            if *button == ButtonState::Released {
                if std::mem::take(&mut state.gizmo.toggle_pressed) {
                    return true;
                }
                if let Some(drag) = &mut state.gizmo.drag {
                    drag.update(view.projection, pointer, modifiers.shift);
                    finish(state);
                    return true;
                }
                if state.gizmo.select_start.take().is_some() && view.contains(pointer) {
                    return select_panel(state, view.projection, pointer);
                }
                return false;
            }
            if state.pointer_down || !view.contains(pointer) {
                return false;
            }
            if placement_gizmo::toggle_contains(view.size, pointer) {
                state.gizmo.toggle_pressed = true;
                state.gizmo.mode = state.gizmo.mode.toggled();
                state.gizmo.axis_hover = None;
                state.status_message = format!("3D placement mode · {}", state.gizmo.mode.label());
                return true;
            }
            if !placement_active(state)
                || super::super::view_cube::pick(&state.camera, view.size, pointer).is_some()
            {
                return false;
            }
            sync_pivot(state);
            if let Some(handle) = world_pivot(state).and_then(|pivot| {
                placement_gizmo::pick(view.projection, state.gizmo.mode, pivot, pointer)
            }) {
                begin(state, &view, pointer, handle);
                return true;
            }
            if !modifiers.alt && !modifiers.shift && !modifiers.control && !modifiers.super_key {
                state.gizmo.select_start = Some(pointer);
            }
            false
        }
        EngineEvent::PointerButton { .. } | EngineEvent::Scroll { .. } => {
            state.gizmo.drag.is_some()
        }
        _ => false,
    }
}

fn select_panel(state: &mut CssRunnerState, projection: Projection, pointer: Vec2) -> bool {
    let Some(face) = visible_cloth_face(
        &state.scene,
        projection.eye.as_dvec3(),
        projection.ray(pointer).as_dvec3(),
    ) else {
        return false;
    };
    let Some(range) = state
        .scene
        .garment
        .panels
        .iter()
        .find(|range| range.triangles.contains(&face))
    else {
        return false;
    };
    let Some(index) = state
        .project
        .template
        .panels
        .iter()
        .position(|panel| panel.id == range.id)
    else {
        return false;
    };
    let locked = held(state, range);
    let name = &state.project.template.panels[index].name;
    state.status_message = if locked {
        format!("Selected {name} · release authored pins to move or rotate")
    } else {
        format!(
            "Selected {name} · drag a handle to {}",
            state.gizmo.mode.label().to_lowercase()
        )
    };
    if state.selected_piece_index != index {
        state.selected_piece_index = index;
        state.clear_2d_subselection();
    }
    state.pointer_down = false;
    state.dragging_3d = false;
    true
}

pub(super) type ViewportInfo = std::rc::Rc<std::cell::Cell<Option<GpuViewportInfo>>>;
