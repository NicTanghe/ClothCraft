//! Project saving and artifact export.

use super::state::CssRunnerState;
use clothcraft::garment::export;
use std::fs;

impl CssRunnerState {
    pub fn save_project(&mut self) {
        let _ = fs::create_dir_all(&self.files.output);
        let path = self.files.output.join("fitting-project.json");
        let mut snapshot = self.project.clone();
        snapshot.state = Some(self.scene.state.clone());
        snapshot.simulation = Some(self.settings.clone());
        match snapshot.save(&path) {
            Ok(_) => {
                self.project = snapshot;
                self.status_message = format!("Saved {}", path.display());
            }
            Err(e) => self.status_message = format!("Save error: {e}"),
        }
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
