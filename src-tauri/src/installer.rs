//! Installing Fooocus from scratch.
//!
//! The supported Windows route is the prepackaged standalone archive from
//! GitHub releases: it carries its own embedded Python and every dependency,
//! so there is no git, no pip, and no system Python involved. We download it,
//! extract it, and hand the folder to `install::resolve_install`.
//!
//! The asset is resolved from the GitHub API rather than hardcoded. Upstream
//! has already moved it once — older guides point at the `release` tag, which
//! now only serves a 2023 build, while the current package sits on a version
//! tag. Resolving at runtime means a newer standalone is picked up on its own.

use std::path::Path;
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter};
use tokio::io::AsyncWriteExt;

use crate::error::{AppError, Result};

pub const EVENT_INSTALL: &str = "install://progress";

/// Keep helper processes (PowerShell, pip) from flashing a console window.
#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// Only ever download from here.
const GITHUB_RELEASES_API: &str = "https://api.github.com/repos/lllyasviel/Fooocus/releases";
const ALLOWED_HOSTS: &[&str] = &[
    "github.com",
    "objects.githubusercontent.com",
    "release-assets.githubusercontent.com",
];

/// Verified fallback, used when the GitHub API is unreachable or rate limited
/// (unauthenticated callers get 60 requests an hour).
const FALLBACK: (&str, &str, u64) = (
    "2.5.0",
    "https://github.com/lllyasviel/Fooocus/releases/download/v2.5.0/Fooocus_win64_2-5-0.7z",
    1_999_790_243,
);

/// Rough multiple of the archive size needed for the download plus the
/// extracted tree. Deliberately generous — running out of disk at 95% of a
/// two-gigabyte download is a miserable way to find out.
const SPACE_FACTOR: u64 = 3;

/// Subfolder the archive is staged in, so extraction can tell the download
/// apart from the files it is unpacking.
const STAGING: &str = ".fooocus-download";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReleasePackage {
    pub version: String,
    pub filename: String,
    pub url: String,
    pub size: u64,
    /// Bytes we want free before starting.
    pub required_space: u64,
    /// True when this came from the offline fallback rather than the API.
    pub fallback: bool,
}

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Phase {
    Downloading,
    Extracting,
    Configuring,
    Finalizing,
    Complete,
    Failed,
    Cancelled,
}

/// Which graphics stack Fooocus should be set up for.
///
/// The stock package ships CUDA builds of torch, so NVIDIA needs nothing.
/// Everything else needs its torch replaced, which is the step people get
/// stuck on — and for Intel Arc there are no upstream instructions at all.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum GpuVendor {
    Nvidia,
    Amd,
    IntelArc,
    Cpu,
}

impl GpuVendor {
    /// Extra flags Fooocus needs at launch for this stack.
    pub fn launch_flags(self) -> &'static [&'static str] {
        match self {
            // Documented in the Fooocus readme for Windows AMD GPUs.
            GpuVendor::Amd => &["--directml"],
            GpuVendor::Cpu => &["--always-cpu"],
            // Arc is driven by the flags already inside run_arc.bat, and
            // NVIDIA needs nothing beyond the defaults.
            GpuVendor::Nvidia | GpuVendor::IntelArc => &[],
        }
    }

    /// The launch profile that suits this stack, matched by file name.
    pub fn preferred_bat(self) -> &'static str {
        match self {
            GpuVendor::IntelArc => "run_arc.bat",
            _ => "run.bat",
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GpuInfo {
    /// Best guess, which the user can override.
    pub vendor: GpuVendor,
    /// The adapter the guess came from, for display.
    pub name: String,
    /// Everything Windows reported, so a multi-GPU laptop is visible.
    pub adapters: Vec<String>,
    /// Set when the chosen adapter is integrated graphics, so the UI can
    /// explain why CPU was picked rather than appearing to ignore a GPU.
    pub note: Option<String>,
}

/// Ask Windows what graphics adapters are present.
pub fn detect_gpu() -> GpuInfo {
    let adapters = query_adapters();

    // Prefer a discrete card when several are present: a laptop with Intel
    // integrated graphics plus an NVIDIA card should be treated as NVIDIA.
    let vendor = adapters
        .iter()
        .find_map(|name| classify(name).filter(|v| *v != GpuVendor::Cpu))
        .unwrap_or(GpuVendor::Cpu);

    let name = adapters
        .iter()
        .find(|name| classify(name) == Some(vendor))
        .or_else(|| adapters.first())
        .cloned()
        .unwrap_or_else(|| "No graphics adapter detected".to_string());

    let note = (vendor == GpuVendor::Cpu && adapters.iter().any(|a| is_integrated(a))).then(|| {
        concat!(
            "That is integrated graphics, which shares system memory rather than ",
            "having its own. Fooocus needs far more than it can provide, so running ",
            "on the processor is the realistic choice here — slower, but it will ",
            "actually finish."
        )
        .to_string()
    });

    GpuInfo {
        vendor,
        name,
        adapters,
        note,
    }
}

fn query_adapters() -> Vec<String> {
    let mut command = Command::new("powershell");
    command.args([
        "-NoProfile",
        "-NonInteractive",
        "-Command",
        "Get-CimInstance Win32_VideoController | Select-Object -ExpandProperty Name",
    ]);

    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(CREATE_NO_WINDOW);
    }

    let Ok(output) = command.output() else {
        return Vec::new();
    };

    String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(str::to_string)
        .collect()
}

/// Map an adapter name onto a stack, or `None` when nothing here can run
/// Fooocus usefully and the honest answer is the processor.
///
/// Integrated graphics are the case that matters. An APU reports itself as
/// "AMD Radeon(TM) Graphics", which naively reads as an AMD card — but it
/// shares system memory and has a fraction of the throughput, so setting it up
/// for DirectML produces something technically working and practically
/// unusable. CPU is slow, and says so, which is the better answer.
fn classify(name: &str) -> Option<GpuVendor> {
    let lower = name.to_lowercase();

    if lower.contains("nvidia") || lower.contains("geforce") || lower.contains("quadro") {
        return Some(GpuVendor::Nvidia);
    }
    // Intel Arc is discrete; every other Intel adapter is integrated.
    if lower.contains("arc(tm)") || lower.contains("arc ") || lower.contains("intel(r) arc") {
        return Some(GpuVendor::IntelArc);
    }
    if is_discrete_amd(&lower) {
        return Some(GpuVendor::Amd);
    }
    None
}

/// True only for AMD cards with their own memory.
///
/// Discrete Radeons carry an RX, Pro, FirePro or Instinct marker. Integrated
/// ones are named for the APU generation instead — "Radeon(TM) Graphics",
/// "Vega 8", "Radeon 780M" — so the absence of a discrete marker is the signal.
fn is_discrete_amd(lower: &str) -> bool {
    if !(lower.contains("radeon") || lower.contains("amd") || lower.contains("firepro")) {
        return false;
    }

    ["radeon rx", " rx ", "radeon pro", "firepro", "instinct", "radeon vii"]
        .iter()
        .any(|marker| lower.contains(marker))
}

/// Whether this adapter is integrated graphics, for explaining the choice.
fn is_integrated(name: &str) -> bool {
    let lower = name.to_lowercase();
    let intel_integrated = lower.contains("intel")
        && !(lower.contains("arc(tm)") || lower.contains("arc ") || lower.contains("intel(r) arc"));
    let amd_integrated =
        (lower.contains("radeon") || lower.contains("amd")) && !is_discrete_amd(&lower);

    intel_integrated || amd_integrated
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstallProgress {
    pub phase: Phase,
    /// 0 to 1 for the current phase. Meaningless while extracting, where the
    /// UI shows the byte counter instead.
    pub progress: f32,
    pub bytes: u64,
    pub total: Option<u64>,
    pub speed: u64,
    pub message: String,
    pub error: Option<String>,
    /// Set on `Complete` with the folder to adopt as the installation.
    pub install_root: Option<String>,
}

#[derive(Default)]
pub struct InstallerState {
    pub cancel: AtomicBool,
    running: AtomicBool,
}

impl InstallerState {
    pub fn request_cancel(&self) {
        self.cancel.store(true, Ordering::SeqCst);
    }
}

// --------------------------------------------------------------- release lookup

#[derive(Deserialize)]
struct GhRelease {
    tag_name: String,
    #[serde(default)]
    draft: bool,
    #[serde(default)]
    assets: Vec<GhAsset>,
}

#[derive(Deserialize)]
struct GhAsset {
    name: String,
    size: u64,
    browser_download_url: String,
}

/// Find the newest release carrying a Windows standalone archive.
pub async fn find_package() -> ReleasePackage {
    match query_github().await {
        Ok(Some(package)) => package,
        // A missing or throttled API is expected, not exceptional.
        _ => ReleasePackage {
            version: FALLBACK.0.to_string(),
            filename: "Fooocus_win64_2-5-0.7z".to_string(),
            url: FALLBACK.1.to_string(),
            size: FALLBACK.2,
            required_space: FALLBACK.2 * SPACE_FACTOR,
            fallback: true,
        },
    }
}

async fn query_github() -> Result<Option<ReleasePackage>> {
    let client = reqwest::Client::builder()
        // GitHub rejects requests without one.
        .user_agent("FooocusFront")
        .connect_timeout(Duration::from_secs(15))
        .build()?;

    let releases: Vec<GhRelease> = client
        .get(format!("{GITHUB_RELEASES_API}?per_page=30"))
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;

    // The API returns newest first, so the first match wins.
    for release in releases.iter().filter(|r| !r.draft) {
        let asset = release.assets.iter().find(|asset| {
            let name = asset.name.to_ascii_lowercase();
            name.starts_with("fooocus_win64_") && name.ends_with(".7z")
        });

        if let Some(asset) = asset {
            if !host_allowed(&asset.browser_download_url) {
                continue;
            }
            return Ok(Some(ReleasePackage {
                version: release.tag_name.trim_start_matches('v').to_string(),
                filename: asset.name.clone(),
                url: asset.browser_download_url.clone(),
                size: asset.size,
                required_space: asset.size * SPACE_FACTOR,
                fallback: false,
            }));
        }
    }

    Ok(None)
}

fn host_allowed(url: &str) -> bool {
    let Some(rest) = url.strip_prefix("https://") else {
        return false;
    };
    let host = rest.split('/').next().unwrap_or_default();
    ALLOWED_HOSTS.contains(&host)
}

/// Free bytes on the volume holding `path`, or `None` if it cannot be read.
pub fn free_space(path: &Path) -> Option<u64> {
    let disks = sysinfo::Disks::new_with_refreshed_list();

    // Pick the disk with the longest matching mount point, so `D:\` does not
    // shadow a more specific mount.
    disks
        .list()
        .iter()
        .filter(|disk| path.starts_with(disk.mount_point()))
        .max_by_key(|disk| disk.mount_point().as_os_str().len())
        .map(sysinfo::Disk::available_space)
}

// -------------------------------------------------------------------- install

/// Download and extract Fooocus into `dest`.
///
/// Returns immediately; progress arrives on `install://progress`.
pub fn install(
    app: &AppHandle,
    state: &Arc<InstallerState>,
    package: ReleasePackage,
    dest: String,
    vendor: GpuVendor,
) -> Result<()> {
    if state.running.swap(true, Ordering::SeqCst) {
        return Err(AppError::msg("an installation is already running"));
    }
    state.cancel.store(false, Ordering::SeqCst);

    let app = app.clone();
    let state = state.clone();

    tauri::async_runtime::spawn(async move {
        let result = run(&app, &state, &package, Path::new(&dest), vendor).await;

        let payload = match result {
            Ok(Some(root)) => InstallProgress {
                phase: Phase::Complete,
                progress: 1.0,
                bytes: 0,
                total: None,
                speed: 0,
                message: format!("Fooocus {} is ready", package.version),
                error: None,
                install_root: Some(root),
            },
            Ok(None) => InstallProgress {
                phase: Phase::Cancelled,
                progress: 0.0,
                bytes: 0,
                total: None,
                speed: 0,
                message: "Installation cancelled".into(),
                error: None,
                install_root: None,
            },
            Err(error) => InstallProgress {
                phase: Phase::Failed,
                progress: 0.0,
                bytes: 0,
                total: None,
                speed: 0,
                message: "Installation failed".into(),
                error: Some(error.to_string()),
                install_root: None,
            },
        };

        let _ = app.emit(EVENT_INSTALL, payload);
        state.running.store(false, Ordering::SeqCst);
    });

    Ok(())
}

/// `Ok(None)` means the user cancelled.
async fn run(
    app: &AppHandle,
    state: &Arc<InstallerState>,
    package: &ReleasePackage,
    dest: &Path,
    vendor: GpuVendor,
) -> Result<Option<String>> {
    if !host_allowed(&package.url) {
        return Err(AppError::msg(format!(
            "refusing to download from an unexpected host: {}",
            package.url
        )));
    }

    tokio::fs::create_dir_all(dest).await?;

    if let Some(free) = free_space(dest) {
        if free < package.required_space {
            return Err(AppError::msg(format!(
                "not enough free space: about {:.1} GB is needed and {:.1} GB is available",
                package.required_space as f64 / 1e9,
                free as f64 / 1e9
            )));
        }
    }

    let staging = dest.join(STAGING);
    tokio::fs::create_dir_all(&staging).await?;
    let archive = staging.join(&package.filename);

    if !download(app, state, package, &archive).await? {
        return Ok(None);
    }
    if state.cancel.load(Ordering::SeqCst) {
        return Ok(None);
    }

    extract(app, state, &archive, dest).await?;
    if state.cancel.load(Ordering::SeqCst) {
        return Ok(None);
    }

    emit(
        app,
        Phase::Finalizing,
        0.0,
        0,
        None,
        0,
        "Cleaning up and checking the installation",
    );

    // The archive is two gigabytes we no longer need.
    let _ = tokio::fs::remove_dir_all(&staging).await;

    let root = crate::install::resolve_install(dest)?;

    // Swap in the right torch build for this machine. Blocking work, so keep
    // it off the async runtime's worker threads.
    let configure = {
        let app = app.clone();
        let state = state.clone();
        let root = root.clone();
        tauri::async_runtime::spawn_blocking(move || configure_gpu(&app, &state, &root, vendor))
    };
    configure
        .await
        .map_err(|e| AppError::msg(e.to_string()))??;

    if state.cancel.load(Ordering::SeqCst) {
        return Ok(None);
    }

    Ok(Some(root.display().to_string()))
}

/// Stream the archive to disk, resuming any partial file already present.
/// Returns `false` if cancelled.
async fn download(
    app: &AppHandle,
    state: &Arc<InstallerState>,
    package: &ReleasePackage,
    target: &Path,
) -> Result<bool> {
    // A previous run may have finished the download but failed later.
    if let Ok(meta) = tokio::fs::metadata(target).await {
        if meta.len() == package.size {
            return Ok(true);
        }
    }

    let part = target.with_extension("7z.part");
    let mut offset = tokio::fs::metadata(&part).await.map(|m| m.len()).unwrap_or(0);

    let client = reqwest::Client::builder()
        .user_agent("FooocusFront")
        .connect_timeout(Duration::from_secs(30))
        .build()?;

    let mut request = client.get(&package.url);
    if offset > 0 {
        request = request.header(reqwest::header::RANGE, format!("bytes={offset}-"));
    }

    let response = request.send().await?.error_for_status()?;
    let resumed = response.status() == reqwest::StatusCode::PARTIAL_CONTENT;
    if offset > 0 && !resumed {
        offset = 0;
    }

    let total = response.content_length().map(|len| len + offset);

    let mut file = tokio::fs::OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(!resumed && offset == 0)
        .append(resumed)
        .open(&part)
        .await?;

    let mut downloaded = offset;
    let mut stream = response.bytes_stream();
    let mut last_emit = Instant::now();
    let mut window_start = Instant::now();
    let mut window_bytes = 0u64;

    while let Some(chunk) = stream.next().await {
        if state.cancel.load(Ordering::SeqCst) {
            file.flush().await?;
            // Keep the partial file: cancelling should not throw away a
            // gigabyte the user may want to resume from.
            return Ok(false);
        }

        let chunk = chunk?;
        file.write_all(&chunk).await?;
        downloaded += chunk.len() as u64;
        window_bytes += chunk.len() as u64;

        if last_emit.elapsed() >= Duration::from_millis(200) {
            let secs = window_start.elapsed().as_secs_f64().max(0.001);
            emit(
                app,
                Phase::Downloading,
                total.map_or(0.0, |t| downloaded as f32 / t as f32),
                downloaded,
                total,
                (window_bytes as f64 / secs) as u64,
                "Downloading Fooocus",
            );
            last_emit = Instant::now();
            window_start = Instant::now();
            window_bytes = 0;
        }
    }

    file.flush().await?;
    drop(file);
    tokio::fs::rename(&part, target).await?;

    Ok(true)
}

/// Unpack the archive.
///
/// Extraction is synchronous and CPU-bound, so it runs on a blocking thread
/// while a watcher reports how many bytes have landed. The archive does not
/// give us a cheap uncompressed total, so the UI shows a live byte count
/// against an indeterminate bar rather than inventing a percentage.
async fn extract(
    app: &AppHandle,
    state: &Arc<InstallerState>,
    archive: &Path,
    dest: &Path,
) -> Result<()> {
    emit(
        app,
        Phase::Extracting,
        0.0,
        0,
        None,
        0,
        "Extracting Fooocus",
    );

    let watcher = {
        let app = app.clone();
        let dest = dest.to_path_buf();
        let state = state.clone();
        let done = Arc::new(AtomicBool::new(false));
        let flag = done.clone();

        std::thread::spawn(move || {
            while !flag.load(Ordering::SeqCst) && !state.cancel.load(Ordering::SeqCst) {
                std::thread::sleep(Duration::from_millis(500));
                let written = extracted_bytes(&dest);
                emit(
                    &app,
                    Phase::Extracting,
                    0.0,
                    written,
                    None,
                    0,
                    "Extracting Fooocus",
                );
            }
        });

        done
    };

    let archive = archive.to_path_buf();
    let dest = dest.to_path_buf();
    let result = tauri::async_runtime::spawn_blocking(move || {
        sevenz_rust2::decompress_file(&archive, &dest)
    })
    .await;

    watcher.store(true, Ordering::SeqCst);

    match result {
        Ok(Ok(())) => Ok(()),
        Ok(Err(error)) => Err(AppError::msg(format!("could not extract archive: {error}"))),
        Err(error) => Err(AppError::msg(format!("extraction task failed: {error}"))),
    }
}

/// Total size of everything extracted so far, ignoring the staged archive.
fn extracted_bytes(dest: &Path) -> u64 {
    walkdir::WalkDir::new(dest)
        .into_iter()
        .filter_entry(|entry| entry.file_name() != std::ffi::OsStr::new(STAGING))
        .filter_map(std::result::Result::ok)
        .filter(|entry| entry.file_type().is_file())
        .filter_map(|entry| entry.metadata().ok())
        .map(|meta| meta.len())
        .sum()
}

/// A package whose installed version differs from what Fooocus pins.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PackageDrift {
    pub name: String,
    pub expected: String,
    /// None when the package is missing entirely.
    pub installed: Option<String>,
}

/// Compare what is installed against `requirements_versions.txt`.
///
/// Fooocus pins exact versions and patches gradio's internals, so a package
/// drifting from its pin is a real problem rather than a style preference —
/// upgrading gradio in particular stops Fooocus starting at all.
///
/// Torch is deliberately not in that file: Fooocus installs it separately, per
/// graphics stack. That is why restoring these pins is safe on an Arc or AMD
/// machine — it cannot undo the graphics setup.
pub fn check_packages(root: &Path, fooocus_dir: &Path) -> Result<Vec<PackageDrift>> {
    let requirements = fooocus_dir.join("requirements_versions.txt");
    let raw = std::fs::read_to_string(&requirements)?;

    let pinned: Vec<(String, String)> = raw
        .lines()
        .filter_map(|line| line.trim().split_once("=="))
        .map(|(name, version)| (normalise_package(name), version.trim().to_string()))
        .collect();

    let installed = installed_packages(root)?;

    Ok(pinned
        .into_iter()
        .filter_map(|(name, expected)| {
            let actual = installed.get(&name).cloned();
            // Only report a difference, not a match.
            (actual.as_deref() != Some(expected.as_str())).then_some(PackageDrift {
                name,
                expected,
                installed: actual,
            })
        })
        .collect())
}

/// pip and PyPI treat `-` and `_` as the same character, and are case
/// insensitive; comparing raw names would report false differences.
fn normalise_package(name: &str) -> String {
    name.trim().to_lowercase().replace('_', "-")
}

fn installed_packages(root: &Path) -> Result<std::collections::HashMap<String, String>> {
    let mut command = Command::new(root.join("python_embeded/python.exe"));
    command.args(["-m", "pip", "list", "--format=json", "--disable-pip-version-check"]);

    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(CREATE_NO_WINDOW);
    }

    let output = command.output()?;
    let parsed: Vec<serde_json::Value> =
        serde_json::from_slice(&output.stdout).unwrap_or_default();

    Ok(parsed
        .into_iter()
        .filter_map(|entry| {
            Some((
                normalise_package(entry.get("name")?.as_str()?),
                entry.get("version")?.as_str()?.to_string(),
            ))
        })
        .collect())
}

/// Reinstall exactly what Fooocus pins, undoing any drift.
pub fn repair_packages(
    app: &AppHandle,
    state: &Arc<InstallerState>,
    root: &Path,
    fooocus_dir: &Path,
) -> Result<()> {
    let python = root.join("python_embeded/python.exe");
    let requirements = fooocus_dir.join("requirements_versions.txt");

    let args = vec![
        "-m".to_string(),
        "pip".to_string(),
        "install".to_string(),
        "-r".to_string(),
        requirements.display().to_string(),
    ];

    run_pip(
        app,
        state,
        &python,
        root,
        &args,
        "Restoring the versions Fooocus expects",
    )?;

    emit(app, Phase::Configuring, 1.0, 0, None, 0, "Packages restored");
    Ok(())
}

/// Packages the stock CUDA build ships that must go before another stack can
/// be installed. Taken verbatim from the Fooocus readme's AMD instructions.
const TORCH_PACKAGES: &[&str] = &[
    "torch",
    "torchvision",
    "torchaudio",
    "torchtext",
    "functorch",
    "xformers",
];

/// Intel's wheel index for the XPU build of torch. Not documented by Fooocus —
/// these are the versions verified working on an Arc A770.
const INTEL_XPU_INDEX: &str = "https://pytorch-extension.intel.com/release-whl/stable/xpu/us/";
const INTEL_PACKAGES: &[&str] = &[
    "torch==2.1.0a0",
    "torchvision==0.16.0a0",
    "torchaudio==2.1.0a0",
    "intel-extension-for-pytorch==2.1.10+xpu",
];

/// Packages that must be present for a stack to count as already installed.
///
/// Deliberately the markers rather than the full pinned list: an Arc install
/// is identified by IPEX sitting alongside torch, which the stock CUDA build
/// never has.
fn stack_markers(vendor: GpuVendor) -> &'static [&'static str] {
    match vendor {
        GpuVendor::IntelArc => &["torch", "intel-extension-for-pytorch"],
        GpuVendor::Amd => &["torch", "torch-directml"],
        GpuVendor::Nvidia | GpuVendor::Cpu => &[],
    }
}

/// Whether this install already has the stack for `vendor`.
///
/// Deliberately asks pip what is installed rather than trying to import torch.
/// An import probe was tried and rejected: torch's Intel build needs the DLL
/// search path the launch profile sets up, so `python -c "import torch"` fails
/// on a perfectly healthy Arc install. A probe that reports a working install
/// as broken would trigger exactly the destructive reinstall this guard
/// exists to prevent.
///
/// A stack that is present but damaged is handled by "Restore pinned
/// versions", which reinstalls unconditionally.
fn already_configured(root: &Path, vendor: GpuVendor) -> bool {
    let markers = stack_markers(vendor);
    if markers.is_empty() {
        return true;
    }

    let Ok(installed) = installed_packages(root) else {
        // Unable to ask means unable to promise; fall through and configure.
        return false;
    };

    markers
        .iter()
        .all(|name| installed.contains_key(&normalise_package(name)))
}

/// Install the right torch stack for `vendor` into the embedded interpreter.
///
/// NVIDIA is a no-op: the stock package already ships CUDA builds. Everything
/// else replaces torch, which is the step that stops people — and the reason
/// Arc users currently have to work it out for themselves.
pub fn configure_gpu(
    app: &AppHandle,
    state: &Arc<InstallerState>,
    root: &Path,
    vendor: GpuVendor,
) -> Result<()> {
    let python = root.join("python_embeded/python.exe");
    if !python.is_file() {
        return Err(AppError::msg(format!(
            "no embedded Python at {}",
            python.display()
        )));
    }

    if matches!(vendor, GpuVendor::Nvidia | GpuVendor::Cpu) {
        emit(
            app,
            Phase::Configuring,
            1.0,
            0,
            None,
            0,
            "No extra packages needed for this graphics card",
        );
        return Ok(());
    }

    // An install that already has a working stack for this card is left
    // completely alone. This path removes torch before fetching its
    // replacement, so running it against a healthy install trades a working
    // Fooocus for a several-hundred-megabyte download and a window in which a
    // failure leaves the user with no torch at all. Nobody choosing the card
    // they already have is asking for that.
    //
    // Reinstalling is still reachable deliberately: a missing or broken stack
    // fails the check below, "Restore pinned versions" bypasses this entirely,
    // and a fresh install has nothing to detect.
    if already_configured(root, vendor) {
        emit(
            app,
            Phase::Configuring,
            1.0,
            0,
            None,
            0,
            "Already set up for this graphics card — nothing to download",
        );
        return Ok(());
    }

    // Step 1: remove the CUDA builds.
    let mut uninstall: Vec<String> = vec!["-m".into(), "pip".into(), "uninstall".into()];
    uninstall.extend(TORCH_PACKAGES.iter().map(|p| (*p).to_string()));
    uninstall.push("-y".into());
    run_pip(app, state, &python, root, &uninstall, "Removing the default graphics packages")?;

    // Step 2: install the replacement.
    let mut install: Vec<String> = vec!["-m".into(), "pip".into(), "install".into()];
    let label = match vendor {
        GpuVendor::Amd => {
            install.push("torch-directml".into());
            "Installing DirectML support for AMD"
        }
        GpuVendor::IntelArc => {
            install.extend(INTEL_PACKAGES.iter().map(|p| (*p).to_string()));
            install.push("--extra-index-url".into());
            install.push(INTEL_XPU_INDEX.into());
            "Installing Intel Arc support (this is a large download)"
        }
        GpuVendor::Nvidia | GpuVendor::Cpu => unreachable!("handled above"),
    };

    run_pip(app, state, &python, root, &install, label)?;

    emit(app, Phase::Configuring, 1.0, 0, None, 0, "Graphics setup complete");
    Ok(())
}

/// Run one pip command, forwarding its output so the user can see progress
/// rather than staring at a spinner through a multi-gigabyte download.
fn run_pip(
    app: &AppHandle,
    state: &Arc<InstallerState>,
    python: &Path,
    root: &Path,
    args: &[String],
    label: &str,
) -> Result<()> {
    emit(app, Phase::Configuring, 0.0, 0, None, 0, label);

    let mut command = Command::new(python);
    command
        .args(args)
        .current_dir(root)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .stdin(std::process::Stdio::null());

    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(CREATE_NO_WINDOW);
    }

    let mut child = command.spawn()?;
    let stdout = child.stdout.take().expect("stdout piped");

    // pip writes its progress bars with carriage returns, so read by line and
    // report the most recent one rather than accumulating thousands.
    let reader = std::io::BufReader::new(stdout);
    for line in std::io::BufRead::lines(reader).map_while(std::result::Result::ok) {
        if state.cancel.load(Ordering::SeqCst) {
            let _ = child.kill();
            return Err(AppError::msg("cancelled"));
        }
        let text = line.trim();
        if !text.is_empty() {
            emit(app, Phase::Configuring, 0.0, 0, None, 0, text);
        }
    }

    let status = child.wait()?;
    if !status.success() {
        // pip puts the useful part of a failure on stderr.
        let mut stderr = String::new();
        if let Some(mut pipe) = child.stderr.take() {
            use std::io::Read;
            let _ = pipe.read_to_string(&mut stderr);
        }
        let detail = stderr.lines().rev().take(4).collect::<Vec<_>>().join(" ");
        return Err(AppError::msg(format!("{label} failed. {detail}")));
    }

    Ok(())
}

fn emit(
    app: &AppHandle,
    phase: Phase,
    progress: f32,
    bytes: u64,
    total: Option<u64>,
    speed: u64,
    message: &str,
) {
    let _ = app.emit(
        EVENT_INSTALL,
        InstallProgress {
            phase,
            progress,
            bytes,
            total,
            speed,
            message: message.to_string(),
            error: None,
            install_root: None,
        },
    );
}
