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

    pub fn export_artifacts(&mut self) {
        let _ = fs::create_dir_all(&self.files.output);
        match export::write_artifacts(
            &self.project.template,
            &self.scene.garment,
            &self.scene.state,
            &self.files.output,
        ) {
            Ok(_) => {
                self.status_message =
                    format!("Exported artifacts to {}", self.files.output.display())
            }
            Err(e) => self.status_message = format!("Export error: {e}"),
        }
    }
}
