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
mod translate;

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use tauri::{AppHandle, Emitter, Manager, State};

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

/// Which packages differ from the versions Fooocus pins.
#[tauri::command]
fn check_packages(state: State<AppState>) -> Result<Vec<installer::PackageDrift>> {
    let info = state.install()?;
    installer::check_packages(Path::new(&info.root), Path::new(&info.fooocus_dir))
}

/// Put the pinned versions back, undoing an upgrade that broke something.
#[tauri::command]
fn repair_packages(app: AppHandle, state: State<AppState>) -> Result<()> {
    let info = state.install()?;
    let installer_state = state.installer.clone();
    let root = PathBuf::from(info.root);
    let fooocus_dir = PathBuf::from(info.fooocus_dir);

    tauri::async_runtime::spawn_blocking(move || {
        let _ = installer::repair_packages(&app, &installer_state, &root, &fooocus_dir);
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

    let (vendor, translate_from) = {
        let mut settings = state.settings.lock().unwrap();
        settings.last_bat = Some(bat);
        settings.last_preset = preset.clone();
        settings::save(&app, &settings)?;
        (
            settings.gpu_vendor,
            settings.translate_from().map(str::to_string),
        )
    };

    let flags = vendor.map(installer::GpuVendor::launch_flags).unwrap_or(&[]);
    launcher::start(
        &app,
        &state.launcher,
        &info,
        &profile,
        preset.as_deref(),
        flags,
        translate_from.as_deref(),
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

/// Event carrying what a prompt was before and after translation, so the UI
/// can show the user the English that was actually sent.
pub const EVENT_TRANSLATED: &str = "prompt://translated";

/// Translate the prompt fields in a generate payload, when translation is on.
///
/// Done here rather than in each screen: Studio, Inpaint, Upscale and Image
/// Prompt all reach generation through this one command, so a screen cannot
/// be forgotten and no screen can bypass it.
///
/// A translation failure must never cost someone their generation. If the
/// model will not load, the original prompt goes through untouched and the UI
/// is told why — worse images beat no images.
async fn translate_options(
    app: &AppHandle,
    state: &State<'_, AppState>,
    mut options: serde_json::Value,
) -> serde_json::Value {
    let active = {
        let settings = state.settings.lock().unwrap();
        settings.translate_from().map(str::to_string)
    };

    let Some(active) = active else {
        return options;
    };
    if !translate::model_ready(app, &active) {
        return options;
    }

    for field in ["prompt", "negative_prompt"] {
        let Some(original) = options.get(field).and_then(|v| v.as_str()) else {
            continue;
        };
        if original.trim().is_empty() {
            continue;
        }
        let original = original.to_string();

        match bridge::post(
            &state.launcher,
            "/translate",
            serde_json::json!({ "text": original }),
        )
        .await
        {
            Ok(body) => {
                let translated = body
                    .get("text")
                    .and_then(|v| v.as_str())
                    .unwrap_or_default()
                    .to_string();

                if !translated.is_empty() && translated != original {
                    options[field] = serde_json::Value::String(translated.clone());
                    let _ = app.emit(
                        EVENT_TRANSLATED,
                        serde_json::json!({
                            "field": field,
                            "original": original,
                            "translated": translated,
                        }),
                    );
                }
            }
            Err(error) => {
                let _ = app.emit(
                    EVENT_TRANSLATED,
                    serde_json::json!({
                        "field": field,
                        "original": original,
                        "error": error.to_string(),
                    }),
                );
            }
        }
    }

    options
}

#[tauri::command]
async fn bridge_generate(
    app: AppHandle,
    state: State<'_, AppState>,
    options: serde_json::Value,
) -> Result<serde_json::Value> {
    let options = translate_options(&app, &state, options).await;
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

// ---------------------------------------------------------------- translation

#[tauri::command]
fn translation_languages() -> Vec<translate::Language> {
    translate::languages()
}

/// Whether translation is installed, and whether it is currently doing
/// anything. The active language is resolved here, against the settings, so
/// the UI never has to reimplement the "English needs no translation" rule.
#[tauri::command]
fn translation_status(app: AppHandle, state: State<AppState>) -> Result<translate::Status> {
    // The chosen language, whether or not it is currently usable — the UI needs
    // it to offer the download in the first place.
    let chosen = {
        let settings = state.settings.lock().unwrap();
        settings
            .prompt_language
            .clone()
            .filter(|code| code != "en" && translate::is_supported(code))
    };

    let mut status = translate::status(&app, chosen.as_deref())?;

    // Only claim a language is active when the model could actually service it
    // and the user has translation switched on.
    let live = status.model_ready
        && status.runtime_ready
        && state.settings.lock().unwrap().translate_from().is_some();

    if !live {
        status.active_language = None;
    }

    Ok(status)
}

/// Vendor the missing Python package, then queue the model.
///
/// The package is small and quick, and the model is useless without it, so it
/// goes first and synchronously. The model itself is several hundred megabytes
/// and rides the normal download queue, so the UI stays responsive and a
/// restart resumes rather than restarts.
#[tauri::command]
fn install_translation(app: AppHandle, state: State<AppState>) -> Result<usize> {
    let info = state.install()?;

    // Deliberately the chosen language rather than the active one: someone
    // picks a language, downloads it, and only then turns translation on.
    let code = {
        let settings = state.settings.lock().unwrap();
        settings
            .prompt_language
            .clone()
            .filter(|code| code != "en" && translate::is_supported(code))
            .ok_or_else(|| {
                error::AppError::msg("choose a prompt language before installing translation")
            })?
    };

    if !translate::runtime_ready(&app) {
        translate::install_runtime(&app, Path::new(&info.root))?;
    }

    translate::install_model(&app, &state.downloads, &code)
}

#[tauri::command]
fn remove_translation(app: AppHandle) -> Result<u64> {
    translate::remove_models(&app)
}

/// Translate a prompt into English.
///
/// Runs in the Fooocus process, where torch and transformers already live. The
/// English is handed back to the UI so the user can see what was actually
/// sent, rather than having their words silently rewritten.
#[tauri::command]
async fn translate_prompt(state: State<'_, AppState>, text: String) -> Result<String> {
    let response = bridge::post(
        &state.launcher,
        "/translate",
        serde_json::json!({ "text": text }),
    )
    .await?;

    Ok(response
        .get("text")
        .and_then(|v| v.as_str())
        .unwrap_or_default()
        .to_string())
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
    settings::save(&app, &settings)?;

    // Raising the limit should start waiting downloads straight away.
    state.downloads.set_limit(settings.concurrency());
    downloads::pump(&app, &state.downloads);
    Ok(())
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

            // The default 1440x900 does not fit every display, and a window
            // larger than the screen puts its buttons out of reach entirely.
            if let Some(window) = app.get_webview_window("main") {
                if let Ok(Some(monitor)) = window.primary_monitor() {
                    let screen = monitor.size();
                    let max_w = (screen.width as f64 * 0.92) as u32;
                    let max_h = (screen.height as f64 * 0.88) as u32;

                    if let Ok(size) = window.outer_size() {
                        if size.width > max_w || size.height > max_h {
                            let _ = window.set_size(tauri::PhysicalSize::new(
                                size.width.min(max_w).max(640),
                                size.height.min(max_h).max(480),
                            ));
                            let _ = window.center();
                        }
                    }
                }
            }
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

            // Apply the saved concurrency before restoring, so a big queue
            // does not all start at once on the way back up.
            state
                .downloads
                .set_limit(state.settings.lock().unwrap().concurrency());

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
            check_packages,
            repair_packages,
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
            translation_languages,
            translation_status,
            install_translation,
            remove_translation,
            translate_prompt,
            list_outputs,
            get_settings,
            save_settings,
            read_fooocus_config,
            write_fooocus_config,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
