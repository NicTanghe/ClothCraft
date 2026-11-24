//! Persisted application preferences.
//!
//! The Settings panel and workspace chrome hold values that have no home in a
//! garment document: theme, view layout, 2D zoom speed, body-reference layers,
//! wireframe style and the default measurement precision. They are stored as a
//! small JSON file in the platform's per-user configuration directory, so a
//! restart keeps them.
//!
//! Project-owned values (geometry, fabric, seams and simulation settings) stay
//! in the garment document. Loading preferences never changes rest geometry,
//! meshes, history or the physical solver.

use super::super::viewport_wireframe::{
    COLOURS, DEFAULT_COLOUR, DEFAULT_WIDTH_PX, MAX_WIDTH_PX, MIN_WIDTH_PX,
};
use super::state::{DEFAULT_2D_ZOOM_SPEED, MAX_2D_ZOOM_SPEED, MIN_2D_ZOOM_SPEED};
use clothcraft::css_ui::{Theme, ViewLayoutMode};
use serde::{Deserialize, Serialize};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

/// Name of the preference file inside the per-user configuration directory.
pub const SETTINGS_FILE: &str = "settings.ron";

/// Directory override used by portable builds and manual testing.
pub const CONFIG_DIR_ENV: &str = "CLOTHCRAFT_CONFIG_DIR";

/// Current on-disk schema version. Files with another version fall back to
/// defaults instead of applying fields whose meaning may have changed.
pub const VERSION: u32 = 1;

/// Visible theme choice. Stored separately from [`Theme`] so the file does not
/// depend on the UI enum's field order.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ThemePreference {
    #[default]
    Dark,
    Light,
}

impl From<Theme> for ThemePreference {
    fn from(theme: Theme) -> Self {
        match theme {
            Theme::Dark => Self::Dark,
            Theme::Light => Self::Light,
        }
    }
}

impl From<ThemePreference> for Theme {
    fn from(theme: ThemePreference) -> Self {
        match theme {
            ThemePreference::Dark => Self::Dark,
            ThemePreference::Light => Self::Light,
        }
    }
}

/// Workspace arrangement choice.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LayoutPreference {
    #[default]
    Split,
    ThreeDOnly,
    TwoDOnly,
}

impl From<ViewLayoutMode> for LayoutPreference {
    fn from(layout: ViewLayoutMode) -> Self {
        match layout {
            ViewLayoutMode::Split => Self::Split,
            ViewLayoutMode::ThreeDOnly => Self::ThreeDOnly,
            ViewLayoutMode::TwoDOnly => Self::TwoDOnly,
        }
    }
}

impl From<LayoutPreference> for ViewLayoutMode {
    fn from(layout: LayoutPreference) -> Self {
        match layout {
            LayoutPreference::Split => Self::Split,
            LayoutPreference::ThreeDOnly => Self::ThreeDOnly,
            LayoutPreference::TwoDOnly => Self::TwoDOnly,
        }
    }
}

/// Application preferences as stored on disk.
///
/// `#[serde(default)]` keeps older files readable when a field is added; any
/// missing field starts from the built-in default. Values are clamped by
/// [`AppPreferences::sanitized`] so a hand-edited file cannot break the editor.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AppPreferences {
    pub version: u32,
    pub theme: ThemePreference,
    pub layout: LayoutPreference,
    /// Multiplier for wheel and trackpad zoom in the 2D board.
    pub zoom_speed: f64,
    pub body_visible: bool,
    pub body_fill: bool,
    pub body_outline: bool,
    pub body_details: bool,
    pub wireframe_enabled: bool,
    /// Index into [`COLOURS`].
    pub wireframe_colour: usize,
    pub wireframe_width_px: f32,
    /// Default precision for new measurements, in bits.
    pub measurement_bits: u8,
}

impl Default for AppPreferences {
    fn default() -> Self {
        Self {
            version: VERSION,
            theme: ThemePreference::default(),
            layout: LayoutPreference::default(),
            zoom_speed: DEFAULT_2D_ZOOM_SPEED,
            body_visible: true,
            body_fill: true,
            body_outline: true,
            body_details: true,
            wireframe_enabled: false,
            wireframe_colour: DEFAULT_COLOUR,
            wireframe_width_px: DEFAULT_WIDTH_PX,
            measurement_bits: 8,
        }
    }
}

impl AppPreferences {
    /// Normalize a value read from disk into the ranges the editor supports.
    pub fn sanitized(self) -> Self {
        if self.version != VERSION {
            return Self::default();
        }
        Self {
            version: VERSION,
            zoom_speed: if self.zoom_speed.is_finite() {
                self.zoom_speed.clamp(MIN_2D_ZOOM_SPEED, MAX_2D_ZOOM_SPEED)
            } else {
                DEFAULT_2D_ZOOM_SPEED
            },
            wireframe_colour: self.wireframe_colour.min(COLOURS.len() - 1),
            wireframe_width_px: if self.wireframe_width_px.is_finite() {
                self.wireframe_width_px.clamp(MIN_WIDTH_PX, MAX_WIDTH_PX)
            } else {
                DEFAULT_WIDTH_PX
            },
            measurement_bits: if matches!(self.measurement_bits, 8 | 10 | 16) {
                self.measurement_bits
            } else {
                8
            },
            ..self
        }
    }
}

/// A preference file at a known path.
#[derive(Clone, Debug)]
pub struct PreferenceStore {
    path: PathBuf,
}

impl PreferenceStore {
    /// Store at an explicit file path. Used by tests and portable setups.
    pub fn at(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    /// Store at the standard per-user location for this platform.
    pub fn user() -> Result<Self, String> {
        settings_path()
            .map(Self::at)
            .ok_or_else(|| format!("no per-user configuration directory; set {CONFIG_DIR_ENV}"))
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Read the preferences. A missing, unreadable or invalid file falls back to
    /// defaults; the editor keeps working and the next save repairs the file.
    pub fn load(&self) -> AppPreferences {
        let text = match fs::read_to_string(&self.path) {
            Ok(text) => text,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return AppPreferences::default();
            }
            Err(error) => {
                tracing::warn!(
                    path = %self.path.display(),
                    %error,
                    "could not read application preferences; using defaults"
                );
                return AppPreferences::default();
            }
        };
        match ron::from_str::<AppPreferences>(&text) {
            Ok(preferences) => preferences.sanitized(),
            Err(error) => {
                tracing::warn!(
                    path = %self.path.display(),
                    %error,
                    "could not parse application preferences; using defaults"
                );
                AppPreferences::default()
            }
        }
    }

    /// Write the preferences atomically, creating the directory if needed.
    pub fn save(&self, preferences: &AppPreferences) -> Result<(), String> {
        let preferences = preferences.clone().sanitized();
        let parent = self
            .path
            .parent()
            .ok_or_else(|| format!("{} has no parent directory", self.path.display()))?;
        fs::create_dir_all(parent)
            .map_err(|error| format!("create {}: {error}", parent.display()))?;
        let mut file = tempfile::NamedTempFile::new_in(parent)
            .map_err(|error| format!("create a temporary settings file: {error}"))?;
        let text = ron::ser::to_string_pretty(&preferences, ron::ser::PrettyConfig::default())
            .map_err(|error| format!("encode application preferences: {error}"))?;
        file.write_all(text.as_bytes())
            .and_then(|()| file.write_all(b"\n"))
            .map_err(|error| format!("write application preferences: {error}"))?;
        file.persist(&self.path)
            .map_err(|error| format!("replace {}: {}", self.path.display(), error.error))?;
        Ok(())
    }
}

/// Standard preference path for the current platform.
pub fn settings_path() -> Option<PathBuf> {
    if let Some(directory) = non_empty_env(CONFIG_DIR_ENV) {
        return Some(PathBuf::from(directory).join(SETTINGS_FILE));
    }
    platform_settings_path()
}

fn non_empty_env(name: &str) -> Option<std::ffi::OsString> {
    std::env::var_os(name).filter(|value| !value.is_empty())
}

#[cfg(target_os = "windows")]
fn platform_settings_path() -> Option<PathBuf> {
    // Roaming AppData: the preference file follows a Windows roaming profile.
    non_empty_env("APPDATA").map(|base| PathBuf::from(base).join("ClothCraft").join(SETTINGS_FILE))
}

#[cfg(target_os = "macos")]
fn platform_settings_path() -> Option<PathBuf> {
    non_empty_env("HOME").map(|home| {
        PathBuf::from(home)
            .join("Library")
            .join("Application Support")
            .join("ClothCraft")
            .join(SETTINGS_FILE)
    })
}

#[cfg(all(unix, not(target_os = "macos")))]
fn platform_settings_path() -> Option<PathBuf> {
    let base = non_empty_env("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
        .or_else(|| non_empty_env("HOME").map(|home| PathBuf::from(home).join(".config")))?;
    Some(base.join("clothcraft").join(SETTINGS_FILE))
}

#[cfg(not(any(unix, target_os = "windows")))]
fn platform_settings_path() -> Option<PathBuf> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stored(preferences: &AppPreferences) -> String {
        ron::ser::to_string_pretty(preferences, ron::ser::PrettyConfig::default()).unwrap()
    }

    #[test]
    fn defaults_match_the_editor_ranges() {
        let defaults = AppPreferences::default();
        assert_eq!(defaults.version, VERSION);
        assert_eq!(defaults.zoom_speed, DEFAULT_2D_ZOOM_SPEED);
        assert_eq!(defaults.wireframe_width_px, DEFAULT_WIDTH_PX);
        assert_eq!(defaults.wireframe_colour, DEFAULT_COLOUR);
        assert_eq!(defaults.measurement_bits, 8);
        assert!(defaults.body_visible && defaults.body_fill);
    }

    #[test]
    fn missing_file_loads_defaults() {
        let directory = tempfile::tempdir().unwrap();
        let store = PreferenceStore::at(directory.path().join(SETTINGS_FILE));
        assert_eq!(store.load(), AppPreferences::default());
    }

    #[test]
    fn save_creates_directories_and_loads_back() {
        let directory = tempfile::tempdir().unwrap();
        let store = PreferenceStore::at(
            directory
                .path()
                .join("nested/ClothCraft")
                .join(SETTINGS_FILE),
        );
        let preferences = AppPreferences {
            theme: ThemePreference::Light,
            layout: LayoutPreference::TwoDOnly,
            zoom_speed: 2.5,
            body_fill: false,
            wireframe_enabled: true,
            wireframe_colour: 3,
            wireframe_width_px: 5.0,
            measurement_bits: 16,
            ..AppPreferences::default()
        };
        store.save(&preferences).unwrap();
        assert!(store.path().exists());
        assert_eq!(store.load(), preferences);
    }

    #[test]
    fn invalid_ron_falls_back_to_defaults() {
        let directory = tempfile::tempdir().unwrap();
        let store = PreferenceStore::at(directory.path().join(SETTINGS_FILE));
        fs::write(store.path(), b"not ron").unwrap();
        assert_eq!(store.load(), AppPreferences::default());
    }

    #[test]
    fn out_of_range_values_are_sanitized_on_load() {
        let directory = tempfile::tempdir().unwrap();
        let store = PreferenceStore::at(directory.path().join(SETTINGS_FILE));
        let preferences = AppPreferences {
            zoom_speed: 99.0,
            wireframe_colour: 99,
            wireframe_width_px: -8.0,
            measurement_bits: 12,
            ..AppPreferences::default()
        };
        fs::write(store.path(), stored(&preferences)).unwrap();
        let loaded = store.load();
        assert_eq!(loaded.zoom_speed, MAX_2D_ZOOM_SPEED);
        assert_eq!(loaded.wireframe_colour, COLOURS.len() - 1);
        assert_eq!(loaded.wireframe_width_px, MIN_WIDTH_PX);
        assert_eq!(loaded.measurement_bits, 8);
    }

    #[test]
    fn another_schema_version_falls_back_to_defaults() {
        let directory = tempfile::tempdir().unwrap();
        let store = PreferenceStore::at(directory.path().join(SETTINGS_FILE));
        let preferences = AppPreferences {
            version: VERSION + 1,
            zoom_speed: 3.0,
            ..AppPreferences::default()
        };
        fs::write(store.path(), stored(&preferences)).unwrap();
        assert_eq!(store.load(), AppPreferences::default());
    }

    #[test]
    fn missing_fields_keep_defaults() {
        let directory = tempfile::tempdir().unwrap();
        let store = PreferenceStore::at(directory.path().join(SETTINGS_FILE));
        // Older files may not contain every field yet.
        fs::write(store.path(), format!("(version: {VERSION})")).unwrap();
        let loaded = store.load();
        assert_eq!(loaded, AppPreferences::default());
    }

    #[test]
    fn stored_ron_is_readable() {
        let preferences = AppPreferences {
            theme: ThemePreference::Light,
            ..AppPreferences::default()
        };
        let text = stored(&preferences);
        assert!(text.contains("theme:"), "{text}");
        assert!(text.contains("light"), "{text}");
        assert!(text.contains("version: 1"), "{text}");
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn windows_path_is_under_appdata() {
        if std::env::var_os(CONFIG_DIR_ENV).is_some() {
            return;
        }
        let Some(path) = settings_path() else {
            return;
        };
        assert!(
            path.ends_with(Path::new("ClothCraft").join(SETTINGS_FILE)),
            "{}",
            path.display()
        );
        assert!(path.starts_with(std::env::var_os("APPDATA").unwrap()));
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn macos_path_is_in_application_support() {
        if std::env::var_os(CONFIG_DIR_ENV).is_some() {
            return;
        }
        let Some(path) = settings_path() else {
            return;
        };
        assert!(
            path.ends_with(Path::new("ClothCraft").join(SETTINGS_FILE)),
            "{}",
            path.display()
        );
        assert!(
            path.to_string_lossy().contains("Application Support"),
            "{}",
            path.display()
        );
    }

    #[cfg(all(unix, not(target_os = "macos")))]
    #[test]
    fn linux_path_is_under_the_xdg_config_directory() {
        if std::env::var_os(CONFIG_DIR_ENV).is_some() {
            return;
        }
        let Some(path) = settings_path() else {
            return;
        };
        assert!(
            path.ends_with(Path::new("clothcraft").join(SETTINGS_FILE)),
            "{}",
            path.display()
        );
        if let Some(xdg) = non_empty_env("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .filter(|path| path.is_absolute())
        {
            assert!(path.starts_with(&xdg), "{}", path.display());
        } else if let Some(home) = non_empty_env("HOME") {
            assert!(
                path.starts_with(PathBuf::from(home).join(".config")),
                "{}",
                path.display()
            );
        }
    }
}
