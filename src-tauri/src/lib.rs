mod bridge;
mod catalog;
mod civitai;
mod downloads;
mod error;
mod gallery;
mod install;
mod installer;
mod launcher;
mod secrets;
mod settings;

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use tauri::{AppHandle, Manager, State};

use error::{AppError, Result};
use install::InstallInfo;

pub struct AppState {
    install: Mutex<Option<InstallInfo>>,
    settings: Mutex<settings::Settings>,
    launcher: Arc<launcher::LauncherState>,
    downloads: Arc<downloads::DownloadManager>,
    installer: Arc<installer::InstallerState>,
}

impl AppState {
    /// The currently selected installation, or an error the UI can show.
    fn install(&self) -> Result<InstallInfo> {
        self.install
            .lock()
            .unwrap()
            .clone()
            .ok_or_else(|| AppError::msg("no Fooocus installation selected yet"))
    }
}

// ---------------------------------------------------------------- installation

/// Adopt a folder as the active installation and remember it.
///
/// Also widens the asset protocol scope to the outputs folder so the gallery
/// can display images that live outside the app's own directories.
#[tauri::command]
fn set_install_root(app: AppHandle, state: State<AppState>, path: String) -> Result<InstallInfo> {
    let root = install::resolve_install(Path::new(&path))?;
    let info = install::inspect(&root)?;

    app.asset_protocol_scope()
        .allow_directory(&info.outputs_dir, true)
        .map_err(|e| AppError::msg(e.to_string()))?;

    *state.install.lock().unwrap() = Some(info.clone());

    let mut settings = state.settings.lock().unwrap();
    settings.install_root = Some(info.root.clone());
    settings::save(&app, &settings)?;

    Ok(info)
}

/// Return the active installation, discovering one on first run.
#[tauri::command]
fn get_install(app: AppHandle, state: State<AppState>) -> Result<Option<InstallInfo>> {
    if let Some(info) = state.install.lock().unwrap().clone() {
        return Ok(Some(info));
    }

    let saved = state.settings.lock().unwrap().install_root.clone();
    let candidate = saved.map(PathBuf::from).or_else(autodetect);

    let Some(candidate) = candidate else {
        return Ok(None);
    };
    match set_install_root(app, state, candidate.display().to_string()) {
        Ok(info) => Ok(Some(info)),
        // A stale saved path should not block the UI from starting.
        Err(_) => Ok(None),
    }
}

/// Probe a short list of conventional locations rather than scanning the disk.
fn autodetect() -> Option<PathBuf> {
    let mut candidates: Vec<PathBuf> = Vec::new();

    for drive in ['C', 'D', 'E', 'F'] {
        candidates.push(PathBuf::from(format!("{drive}:\\AI\\Fooocus")));
        candidates.push(PathBuf::from(format!("{drive}:\\Fooocus")));
        candidates.push(PathBuf::from(format!("{drive}:\\AI")));
    }
    if let Some(home) = std::env::var_os("USERPROFILE").map(PathBuf::from) {
        for sub in ["Documents", "Downloads", "Desktop"] {
            candidates.push(home.join(sub));
            candidates.push(home.join(sub).join("Fooocus"));
        }
    }

    candidates
        .into_iter()
        .filter(|p| p.is_dir())
        .find_map(|p| install::resolve_install(&p).ok())
}

#[tauri::command]
fn scan_models(state: State<AppState>) -> Result<Vec<install::ModelCategory>> {
    Ok(install::scan_models(&state.install()?))
}

// ------------------------------------------------------- installing from scratch

/// Look up the standalone package we would install.
#[tauri::command]
async fn find_fooocus_package() -> installer::ReleasePackage {
    installer::find_package().await
}

/// Free space on the volume holding `path`, for the install location picker.
/// Falls back to the nearest existing ancestor so a folder that does not exist
/// yet still reports the right volume.
#[tauri::command]
fn check_free_space(path: String) -> Option<u64> {
    let mut candidate = PathBuf::from(path);
    loop {
        if candidate.exists() {
            return installer::free_space(&candidate);
        }
        if !candidate.pop() {
            return None;
        }
    }
}

/// A sensible default install location: the largest fixed drive, else the
/// user's home. Fooocus plus its models runs well past 10 GB, so the roomiest
/// volume is nearly always the right suggestion.
#[tauri::command]
fn suggest_install_location() -> String {
    let best = ["D:\\", "E:\\", "F:\\", "C:\\"]
        .into_iter()
        .map(PathBuf::from)
        .filter(|drive| drive.is_dir())
        .max_by_key(|drive| installer::free_space(drive).unwrap_or(0));

    match best {
        Some(drive) => drive.join("AI").join("Fooocus").display().to_string(),
        None => std::env::var("USERPROFILE")
            .map(|home| format!("{home}\\Fooocus"))
            .unwrap_or_else(|_| "C:\\Fooocus".to_string()),
    }
}

/// What graphics adapters this machine has, and our best guess at the stack.
#[tauri::command]
fn detect_gpu() -> installer::GpuInfo {
    installer::detect_gpu()
}

#[tauri::command]
fn install_fooocus(
    app: AppHandle,
    state: State<AppState>,
    package: installer::ReleasePackage,
    dest: String,
    vendor: installer::GpuVendor,
) -> Result<()> {
    {
        let mut settings = state.settings.lock().unwrap();
        settings.gpu_vendor = Some(vendor);
        // Preselect the launch profile that suits this card, so the first
        // start after setup is already correct.
        settings.last_bat = Some(vendor.preferred_bat().to_string());
        settings::save(&app, &settings)?;
    }
    installer::install(&app, &state.installer, package, dest, vendor)
}

/// Re-run the graphics configuration on an existing install, for when the
/// first attempt failed or the card changed.
#[tauri::command]
fn configure_gpu(
    app: AppHandle,
    state: State<AppState>,
    vendor: installer::GpuVendor,
) -> Result<()> {
    let info = state.install()?;
    {
        let mut settings = state.settings.lock().unwrap();
        settings.gpu_vendor = Some(vendor);
        settings::save(&app, &settings)?;
    }

    let installer_state = state.installer.clone();
    let root = PathBuf::from(info.root);
    tauri::async_runtime::spawn_blocking(move || {
        let _ = installer::configure_gpu(&app, &installer_state, &root, vendor);
    });
    Ok(())
}

#[tauri::command]
fn cancel_install(state: State<AppState>) {
    state.installer.request_cancel();
}

// -------------------------------------------------------------------- catalog

#[tauri::command]
fn get_catalog(state: State<AppState>) -> Result<Vec<catalog::CatalogEntry>> {
    Ok(catalog::build(&state.install()?))
}

#[tauri::command]
async fn probe_size(url: String) -> Result<Option<u64>> {
    downloads::probe_size(&url).await
}

// ------------------------------------------------------------------- launcher

#[tauri::command]
fn start_fooocus(
    app: AppHandle,
    state: State<AppState>,
    bat: String,
    preset: Option<String>,
) -> Result<launcher::StatusPayload> {
    let info = state.install()?;
    let profile = info
        .bats
        .iter()
        .find(|b| b.name == bat)
        .ok_or_else(|| AppError::msg(format!("launch profile {bat} not found")))?
        .clone();

    let vendor = {
        let mut settings = state.settings.lock().unwrap();
        settings.last_bat = Some(bat);
        settings.last_preset = preset.clone();
        settings::save(&app, &settings)?;
        settings.gpu_vendor
    };

    let flags = vendor.map(installer::GpuVendor::launch_flags).unwrap_or(&[]);
    launcher::start(
        &app,
        &state.launcher,
        &info,
        &profile,
        preset.as_deref(),
        flags,
    )
}

#[tauri::command]
fn stop_fooocus(app: AppHandle, state: State<AppState>) -> Result<()> {
    launcher::stop(&app, &state.launcher)
}

#[tauri::command]
fn get_status(state: State<AppState>) -> launcher::StatusPayload {
    state.launcher.status()
}

/// Read one of the install's presets.
///
/// Presets are plain JSON describing a base model, styles, performance and so
/// on. Applying them per-job avoids restarting Fooocus just to switch preset,
/// which is what passing `--preset` at launch would require.
#[tauri::command]
fn read_preset(state: State<AppState>, name: String) -> Result<serde_json::Value> {
    let info = state.install()?;

    // Guard against a crafted name escaping the presets folder.
    if name.contains(['/', '\\', '.']) {
        return Err(AppError::msg("invalid preset name"));
    }

    let path = Path::new(&info.fooocus_dir)
        .join("presets")
        .join(format!("{name}.json"));
    let raw = std::fs::read_to_string(&path)?;

    serde_json::from_str(&raw).map_err(|source| AppError::Json {
        file: path.display().to_string(),
        source,
    })
}

// --------------------------------------------------------------------- bridge

/// Controls and defaults for the native Studio, read from the live install.
#[tauri::command]
async fn bridge_options(state: State<'_, AppState>) -> Result<serde_json::Value> {
    bridge::get(&state.launcher, "/options").await
}

/// True once Fooocus has finished importing and can accept jobs.
#[tauri::command]
async fn bridge_ready(state: State<'_, AppState>) -> Result<bool> {
    match bridge::get(&state.launcher, "/health").await {
        Ok(body) => Ok(body
            .get("ready")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false)),
        Err(_) => Ok(false),
    }
}

#[tauri::command]
async fn bridge_generate(
    state: State<'_, AppState>,
    options: serde_json::Value,
) -> Result<serde_json::Value> {
    bridge::post(&state.launcher, "/generate", options).await
}

#[tauri::command]
async fn bridge_stop(state: State<'_, AppState>, skip: bool) -> Result<()> {
    let path = if skip { "/skip" } else { "/stop" };
    bridge::post(&state.launcher, path, serde_json::json!({})).await?;
    Ok(())
}

// -------------------------------------------------------------------- civitai

#[tauri::command]
async fn civitai_search(
    state: State<'_, AppState>,
    params: civitai::SearchParams,
) -> Result<civitai::SearchResults> {
    let key = civitai_key(&state);
    let mut results = civitai::search(params, key.as_deref()).await?;

    // Flag anything already on disk, so a model you own does not sit there
    // offering to download itself again.
    if let Ok(info) = state.install() {
        civitai::mark_installed(&mut results, &info);
    }

    Ok(results)
}

/// Whether a key is stored. The key itself is never sent to the frontend.
/// The stored key, preferring the OS credential store.
fn civitai_key(state: &AppState) -> Option<String> {
    secrets::civitai_key()
        .or_else(|| state.settings.lock().unwrap().civitai_key.clone())
        .filter(|k| !k.trim().is_empty())
}

/// Whether the OS credential store is usable, so the UI can say where the key
/// is being kept.
#[tauri::command]
fn secure_storage_available() -> bool {
    secrets::available()
}

#[tauri::command]
fn civitai_has_key(state: State<AppState>) -> bool {
    civitai_key(&state).is_some()
}

/// Validate a key against the API before storing it, so a typo is caught here
/// rather than surfacing as a failed download later.
#[tauri::command]
async fn civitai_set_key(
    app: AppHandle,
    state: State<'_, AppState>,
    key: String,
) -> Result<bool> {
    let trimmed = key.trim().to_string();

    if trimmed.is_empty() {
        secrets::set_civitai_key("");
        let mut settings = state.settings.lock().unwrap();
        settings.civitai_key = None;
        settings::save(&app, &settings)?;
        return Ok(true);
    }

    if !civitai::verify_key(&trimmed).await? {
        return Ok(false);
    }

    // Prefer the credential store; only fall back to the settings file if the
    // platform has no usable one.
    let stored = secrets::set_civitai_key(&trimmed);

    let mut settings = state.settings.lock().unwrap();
    settings.civitai_key = if stored { None } else { Some(trimmed) };
    settings::save(&app, &settings)?;
    Ok(true)
}

#[tauri::command]
fn civitai_hidden_tags(state: State<AppState>) -> Vec<String> {
    state.settings.lock().unwrap().civitai_hidden_tags.clone()
}

#[tauri::command]
fn civitai_set_hidden_tags(
    app: AppHandle,
    state: State<AppState>,
    tags: Vec<String>,
) -> Result<()> {
    let mut settings = state.settings.lock().unwrap();
    settings.civitai_hidden_tags = tags;
    settings::save(&app, &settings)
}

/// Queue a Civitai download into the folder its type belongs in.
#[tauri::command]
fn civitai_download(
    app: AppHandle,
    state: State<AppState>,
    version_id: u64,
    name: String,
    filename: String,
    category: String,
    url: String,
) -> Result<()> {
    let info = state.install()?;

    let dir = info
        .model_paths
        .get(&category)
        .and_then(|paths| paths.first())
        .cloned()
        .ok_or_else(|| AppError::msg(format!("no folder configured for {category}")))?;

    let key = civitai_key(&state);
    if key.is_none() {
        return Err(AppError::msg(
            "A Civitai API key is needed to download. Add one in Settings.",
        ));
    }

    downloads::enqueue(
        &app,
        &state.downloads,
        format!("civitai-{version_id}"),
        name,
        filename.clone(),
        category,
        url,
        Path::new(&dir).join(filename).display().to_string(),
        key,
    )
}

// ------------------------------------------------------------------ downloads

#[tauri::command]
fn start_download(app: AppHandle, state: State<AppState>, id: String) -> Result<()> {
    let info = state.install()?;
    let entry = catalog::build(&info)
        .into_iter()
        .find(|e| e.id == id)
        .ok_or_else(|| AppError::msg(format!("catalog entry {id} not found")))?;

    downloads::enqueue(
        &app,
        &state.downloads,
        entry.id,
        entry.name,
        entry.filename,
        entry.category,
        entry.url,
        entry.target_path,
        None,
    )
}

/// Resume or retry any queued download, catalog or Civitai alike.
#[tauri::command]
fn resume_download(app: AppHandle, state: State<AppState>, id: String) -> Result<()> {
    let key = civitai_key(&state);
    downloads::resume(&app, &state.downloads, &id, key)
}

#[tauri::command]
fn pause_download(app: AppHandle, state: State<AppState>, id: String) {
    downloads::pause(&app, &state.downloads, &id);
}

#[tauri::command]
fn cancel_download(app: AppHandle, state: State<AppState>, id: String) {
    downloads::cancel(&app, &state.downloads, &id);
}

#[tauri::command]
fn clear_finished_downloads(app: AppHandle, state: State<AppState>) {
    downloads::clear_finished(&app, &state.downloads);
}

#[tauri::command]
fn get_downloads(state: State<AppState>) -> Vec<downloads::Job> {
    state.downloads.jobs()
}

// -------------------------------------------------------------------- gallery

#[tauri::command]
fn list_outputs(state: State<AppState>, limit: Option<usize>) -> Result<Vec<gallery::GalleryImage>> {
    let info = state.install()?;
    Ok(gallery::list(&info.outputs_dir, limit.unwrap_or(300)))
}

// ------------------------------------------------------------------- settings

#[tauri::command]
fn get_settings(state: State<AppState>) -> settings::Settings {
    state.settings.lock().unwrap().clone()
}

#[tauri::command]
fn save_settings(
    app: AppHandle,
    state: State<AppState>,
    settings: settings::Settings,
) -> Result<()> {
    *state.settings.lock().unwrap() = settings.clone();
    settings::save(&app, &settings)
}

/// Raw `config.txt` contents, for the advanced editor.
#[tauri::command]
fn read_fooocus_config(state: State<AppState>) -> Result<String> {
    let info = state.install()?;
    Ok(std::fs::read_to_string(
        Path::new(&info.fooocus_dir).join("config.txt"),
    )?)
}

/// Write `config.txt` back, refusing anything that is not valid JSON so a typo
/// cannot leave Fooocus unable to start.
#[tauri::command]
fn write_fooocus_config(state: State<AppState>, contents: String) -> Result<()> {
    let info = state.install()?;
    serde_json::from_str::<serde_json::Value>(&contents).map_err(|source| AppError::Json {
        file: "config.txt".into(),
        source,
    })?;
    std::fs::write(Path::new(&info.fooocus_dir).join("config.txt"), contents)?;
    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let handle = app.handle().clone();
            app.manage(AppState {
                install: Mutex::new(None),
                settings: Mutex::new(settings::load(&handle)),
                launcher: Arc::new(launcher::LauncherState::default()),
                downloads: Arc::new(downloads::DownloadManager::default()),
                installer: Arc::new(installer::InstallerState::default()),
            });

            // Forward generation events from the bridge for as long as the app
            // runs; it idles harmlessly while Fooocus is stopped.
            let state = app.state::<AppState>();
            let launcher = state.launcher.clone();

            // Move a plaintext key out of settings.json and into the OS
            // credential store, so an upgrade tidies itself up.
            {
                let mut settings = state.settings.lock().unwrap();
                if let Some(plain) = settings.civitai_key.clone() {
                    if !plain.trim().is_empty() && secrets::set_civitai_key(&plain) {
                        settings.civitai_key = None;
                        let _ = settings::save(&handle, &settings);
                    }
                }
            }

            // Put back any downloads that were still going when we last closed.
            let key = secrets::civitai_key();
            downloads::restore(&handle, &state.downloads.clone(), key);

            bridge::spawn_event_pump(handle, launcher);
            Ok(())
        })
        .on_window_event(|window, event| {
            // Never leave an orphaned Fooocus process behind holding VRAM.
            if let tauri::WindowEvent::Destroyed = event {
                let state = window.state::<AppState>();
                if state.settings.lock().unwrap().stop_on_exit {
                    let _ = launcher::stop(&window.app_handle().clone(), &state.launcher);
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            set_install_root,
            get_install,
            find_fooocus_package,
            check_free_space,
            suggest_install_location,
            install_fooocus,
            detect_gpu,
            configure_gpu,
            cancel_install,
            scan_models,
            get_catalog,
            probe_size,
            start_fooocus,
            stop_fooocus,
            get_status,
            read_preset,
            bridge_options,
            bridge_ready,
            bridge_generate,
            bridge_stop,
            civitai_search,
            civitai_hidden_tags,
            civitai_set_hidden_tags,
            civitai_has_key,
            secure_storage_available,
            civitai_set_key,
            civitai_download,
            start_download,
            resume_download,
            pause_download,
            cancel_download,
            clear_finished_downloads,
            get_downloads,
            list_outputs,
            get_settings,
            save_settings,
            read_fooocus_config,
            write_fooocus_config,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
