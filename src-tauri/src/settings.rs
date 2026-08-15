//! Persisted user preferences, stored as JSON in the app config directory.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use tauri::Manager;

use crate::error::Result;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    /// Folder containing the launch bats and `python_embeded`.
    pub install_root: Option<String>,
    /// File name of the last launch profile used, e.g. `run_arc.bat`.
    pub last_bat: Option<String>,
    pub last_preset: Option<String>,
    /// Start Fooocus as soon as the app opens.
    pub auto_start: bool,
    /// Stop Fooocus when the window closes.
    pub stop_on_exit: bool,
    /// Graphics stack this install was configured for. Drives the launch flags
    /// and which profile we recommend.
    pub gpu_vendor: Option<crate::installer::GpuVendor>,
    /// Civitai API key. Required for downloads, which return 401 without one.
    /// Stored in the app's config directory, never in the project.
    pub civitai_key: Option<String>,
}

pub fn path(app: &tauri::AppHandle) -> Result<PathBuf> {
    let dir = app
        .path()
        .app_config_dir()
        .map_err(|e| crate::error::AppError::msg(e.to_string()))?;
    std::fs::create_dir_all(&dir)?;
    Ok(dir.join("settings.json"))
}

pub fn load(app: &tauri::AppHandle) -> Settings {
    let Ok(file) = path(app) else {
        return Settings::default();
    };
    std::fs::read_to_string(file)
        .ok()
        .and_then(|raw| serde_json::from_str(&raw).ok())
        .unwrap_or_else(|| Settings {
            stop_on_exit: true,
            ..Default::default()
        })
}

pub fn save(app: &tauri::AppHandle, settings: &Settings) -> Result<()> {
    let file = path(app)?;
    let raw = serde_json::to_string_pretty(settings).map_err(|source| crate::error::AppError::Json {
        file: file.display().to_string(),
        source,
    })?;
    std::fs::write(file, raw)?;
    Ok(())
}
