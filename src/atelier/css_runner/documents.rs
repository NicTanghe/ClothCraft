//! Editable USD project files and artifact export.

use super::app::fitting_viewport_usda;
use super::mesh_preparation::MeshDraft;
use super::state::CssRunnerState;
use clothcraft::authoring::History;
use clothcraft::document::is_usd_path;
use clothcraft::fitting::FittingProject;
use clothcraft::garment::export;
use clothcraft::usd_scene::{self, ShadingMode};
use std::path::Path;
use std::sync::atomic::Ordering;

const DEFAULT_PROJECT_FILE: &str = "fitting-project.usda";

impl CssRunnerState {
    pub(super) fn save_target(&self) -> Option<&Path> {
        self.files
            .source
            .as_deref()
            .filter(|path| is_usd_path(path))
    }

    pub fn save_project(&mut self) {
        match self.save_target().map(Path::to_path_buf) {
            Some(path) => self.save_project_to_path(&path),
            None => self.save_project_as(),
        }
    }

    pub(super) fn save_project_as(&mut self) {
        let mut dialog = rfd::FileDialog::new()
            .set_title("Save Project As")
            .add_filter("USD project", &["usda", "usd"])
            .set_file_name(DEFAULT_PROJECT_FILE);
        if let Some(source) = self.files.source.as_deref() {
            if let Some(parent) = source.parent().filter(|p| !p.as_os_str().is_empty()) {
                dialog = dialog.set_directory(parent);
            }
            if let Some(stem) = source.file_stem().and_then(|stem| stem.to_str()) {
                dialog = dialog.set_file_name(format!("{stem}.usda"));
            }
        }
        let Some(mut path) = dialog.save_file() else {
            self.status_message = "Save cancelled".into();
            return;
        };
        if path.extension().is_none() {
            path.set_extension("usda");
            // The picker may have confirmed the extensionless name only.
            if path.exists()
                && rfd::MessageDialog::new()
                    .set_title("Replace project?")
                    .set_description(format!("Replace {}?", path.display()))
                    .set_buttons(rfd::MessageButtons::YesNo)
                    .show()
                    != rfd::MessageDialogResult::Yes
            {
                self.status_message = "Save cancelled".into();
                return;
            }
        }
        self.save_project_to_path(&path);
    }

    /// Save the displayed drape and committed edits through the portable USD
    /// writer. A failed save keeps the current path and dirty state.
    pub(super) fn save_project_to_path(&mut self, path: &Path) {
        if !is_usd_path(path) {
            self.status_message = "Choose a .usda or .usd project filename".into();
            return;
        }
        let mut snapshot = self.project.clone();
        snapshot.state = Some(self.scene.state.clone());
        snapshot.simulation = Some(self.settings.clone());
        match snapshot.save(path) {
            Ok(()) => {
                self.project = snapshot;
                self.files.source = Some(path.to_owned());
                self.files.modified = false;
                self.status_message = format!("Saved {}", path.display());
            }
            Err(e) => self.status_message = format!("Save error for {}: {e}", path.display()),
        }
    }

    pub(super) fn open_project(&mut self) {
        let Some(path) = rfd::FileDialog::new()
            .set_title("Open USD Project")
            .add_filter("USD project", &["usda", "usd"])
            .pick_file()
        else {
            self.status_message = "Open cancelled".into();
            return;
        };
        if (self.files.modified
            || self.draft_2d.is_some()
            || self.seam_assembly.is_some()
            || self.preview_template.is_some()
            || self.placement_draft.is_some())
            && rfd::MessageDialog::new()
                .set_title("Discard unsaved changes?")
                .set_description("Opening another project will discard unsaved changes.")
                .set_buttons(rfd::MessageButtons::YesNo)
                .show()
                != rfd::MessageDialogResult::Yes
        {
            self.status_message = "Open cancelled".into();
            return;
        }
        self.status_message = match self.open_project_from_path(&path) {
            Ok(()) => format!("Opened {}", path.display()),
            Err(e) => format!("Open error for {}: {e}", path.display()),
        };
    }

    /// Validate the replacement before changing the current document or its
    /// shared viewport. Existing renderer handles keep the same USD stage.
    pub(super) fn open_project_from_path(&mut self, path: &Path) -> Result<(), String> {
        if !is_usd_path(path) {
            return Err("Choose a .usda or .usd ClothCraft project".into());
        }
        let project = FittingProject::load(path)?;
        let scene = project.compile()?;
        let avatar = scene.collider.surface.as_ref();
        let body_outline = super::body_outline::build(avatar, &project.template);
        let camera = usd_scene::default_scene_camera(&scene.state.x, Some(avatar));
        if let Some(viewport) = &self.usd_scene {
            let usda = fitting_viewport_usda(&scene, avatar, self.studio_backdrop_theme())?;
            usd_scene::replace_viewport_stage_usda(viewport.stage(), &usda)?;
        }

        self.settings = project.simulation.clone().unwrap_or_default();
        self.seam_assembly = None;
        self.seam_collision = None;
        self.seam_to_join = None;
        self.selected_seam = None;
        self.project = project;
        self.initial_scene = scene.clone();
        self.scene = scene;
        self.generation = self.generation.wrapping_add(1);
        self.worker_pending = false;
        self.playing = false;
        self.step_count = 0;
        self.step_ms = 0.0;
        self.last_metrics = None;
        self.history = History::default();
        self.files.source = Some(path.to_owned());
        self.files.modified = false;
        self.staged_avatar = None;
        self.avatar_display = None;
        self.body_outline = body_outline;
        self.camera = camera;
        self.selected_piece_index = 0;
        self.clear_2d_subselection();
        self.mirrored_pieces.clear();
        self.mesh_draft = MeshDraft::from_panel(self.project.template.panels.first());
        self.placement_draft = None;
        *self.number_drag.lock().unwrap() = Default::default();
        *self.numeric_text.lock().unwrap() = None;
        self.numeric_input_active.store(false, Ordering::Relaxed);
        self.preview_template = None;
        self.draft_2d = None;
        self.drag_2d = None;
        self.hovered_2d = None;
        self.last_2d_click = None;
        self.pointer_down = false;
        self.dragging_3d = false;
        self.frame_drag.reset();
        self.frame_all_2d();
        if self.shading == ShadingMode::Flat {
            self.sync_usd_shading();
        }
        self.sync_usd_camera();
        self.request_viewport_redraw();
        self.send_control_state(false);
        Ok(())
    }

    pub fn export_pattern_svg(&mut self) {
        let Some(path) = rfd::FileDialog::new()
            .set_title("Save True-Scale Pattern SVG")
            .add_filter("SVG", &["svg"])
            .set_file_name("patterns-print.svg")
            .save_file()
        else {
            self.status_message = "SVG export cancelled".into();
            return;
        };
        match export::save_print_pattern_svg(&self.project.template, &path) {
            Ok(()) => self.status_message = format!("Exported {}", path.display()),
            Err(e) => self.status_message = format!("SVG export error: {e}"),
        }
    }

    pub fn export_obj(&mut self) {
        let Some(path) = rfd::FileDialog::new()
            .set_title("Save Garment OBJ")
            .add_filter("Wavefront OBJ", &["obj"])
            .set_file_name("cloth.obj")
            .save_file()
        else {
            self.status_message = "OBJ export cancelled".into();
            return;
        };
        let obj = export::obj(&self.scene.garment, &self.scene.state);
        match clothcraft::document::atomic_write(&path, obj.as_bytes()) {
            Ok(()) => self.status_message = format!("Exported {}", path.display()),
            Err(e) => self.status_message = format!("OBJ export error: {e}"),
        }
    }
}
