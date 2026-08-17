//! Starting and supervising the Fooocus process.
//!
//! We deliberately do not execute the `.bat` files. Every stock Fooocus bat
//! ends in `pause`, which behind a hidden window blocks forever on a keypress
//! that can never arrive. Instead we read the bat, take the python arguments
//! out of it (see `install::parse_bat`) and run the embedded interpreter
//! ourselves. That keeps the user's choice of launch profile meaningful while
//! giving us a process we can pipe, supervise, and kill cleanly.

use std::io::{BufRead, BufReader, Read};
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde::Serialize;
use tauri::{AppHandle, Emitter};

use crate::error::{AppError, Result};
use crate::install::{BatFile, InstallInfo};

/// Windows process creation flag that suppresses the console window.
/// Without this the embedded interpreter flashes up a black terminal.
#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

pub const EVENT_LOG: &str = "fooocus://log";
pub const EVENT_STATUS: &str = "fooocus://status";

/// Startup milestones, in the order Fooocus prints them, with the progress we
/// report when each is seen. Fooocus gives no machine-readable startup signal,
/// so this is a curated read of its console output.
/// Every needle must be distinctive enough that it cannot appear in an
/// unrelated warning. Python's import machinery is noisy — torchvision alone
/// emits a warning mentioning both "torch" and "using", which is why bare
/// substrings like those are not safe to match on.
const MILESTONES: &[(&str, f32, &str)] = &[
    ("[system argv]", 0.05, "Checking installation"),
    ("installing requirements", 0.12, "Installing requirements"),
    // Fooocus fetches missing models itself before the UI comes up. On a fresh
    // install that is roughly 7GB, and without this the bar would sit on an
    // earlier stage for many minutes, which reads as a hang. The needle is the
    // start of torch's download line, which is specific enough not to collide.
    ("downloading: \"http", 0.15, "Downloading models (first run)"),
    ("total vram", 0.25, "Detecting hardware"),
    ("cross attention", 0.30, "Configuring attention backend"),
    ("split attention", 0.30, "Configuring attention backend"),
    ("refiner unloaded", 0.35, "Preparing pipeline"),
    ("running on local url", 0.45, "Web server started"),
    ("base model loaded", 0.60, "Loading base model"),
    ("vae loaded", 0.75, "Loading VAE"),
    ("loaded lora", 0.82, "Loading LoRAs"),
    ("fooocus expansion engine", 0.90, "Loading prompt expansion"),
    ("started worker", 0.95, "Starting worker"),
    ("app started successful", 1.0, "Ready"),
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum RunState {
    Stopped,
    Starting,
    Ready,
    Stopping,
    Crashed,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StatusPayload {
    pub state: RunState,
    pub port: Option<u16>,
    pub url: Option<String>,
    pub progress: f32,
    pub stage: String,
    /// What is happening inside the current stage — which package is being
    /// fetched and how far along it is. Installing requirements on a fresh
    /// machine downloads gigabytes, and a stage label alone reads as a hang.
    pub detail: Option<String>,
    /// Populated on `Crashed` with the process exit code, if any.
    pub exit_code: Option<i32>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LogPayload {
    pub line: String,
    /// `stdout` or `stderr`.
    pub stream: &'static str,
    /// True when the line overwrote the previous one (a `\r` progress bar),
    /// so the UI can replace rather than append.
    pub transient: bool,
}

#[derive(Default)]
pub struct LauncherState {
    inner: Mutex<Option<Running>>,
    /// Last status we broadcast, so a newly-mounted UI can catch up.
    status: Mutex<Option<StatusPayload>>,
    bridge: Mutex<Option<BridgeEndpoint>>,
    last_detail: Mutex<Option<Instant>>,
}

struct Running {
    child: Child,
    port: u16,
}

/// Connection details for the in-process bridge, valid while Fooocus runs.
#[derive(Debug, Clone)]
pub struct BridgeEndpoint {
    pub port: u16,
    pub token: String,
}

impl LauncherState {
    pub fn status(&self) -> StatusPayload {
        self.status
            .lock()
            .unwrap()
            .clone()
            .unwrap_or_else(|| StatusPayload {
                state: RunState::Stopped,
                port: None,
                url: None,
                progress: 0.0,
                stage: "Not running".into(),
                detail: None,
                exit_code: None,
            })
    }

    fn set_status(&self, app: &AppHandle, status: StatusPayload) {
        *self.status.lock().unwrap() = Some(status.clone());
        let _ = app.emit(EVENT_STATUS, status);
    }

    pub fn is_running(&self) -> bool {
        self.inner.lock().unwrap().is_some()
    }

    pub fn bridge(&self) -> Option<BridgeEndpoint> {
        self.bridge.lock().unwrap().clone()
    }
}

/// A loopback-only shared secret, so nothing else on the machine can drive
/// generation through the bridge port.
fn bridge_token() -> String {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or_default();
    format!("{:x}{:x}", nanos, std::process::id())
}

/// Launch Fooocus using the arguments from `bat`.
///
/// `--disable-in-browser` stops Fooocus opening the system browser (the whole
/// point is that our window is the UI) and `--port` pins it somewhere we know,
/// so the embedded view can find it. Both are appended only when the chosen
/// bat has not already set them.
pub fn start(
    app: &AppHandle,
    state: &Arc<LauncherState>,
    info: &InstallInfo,
    bat: &BatFile,
    preset: Option<&str>,
    extra_flags: &[&str],
    // Language to translate prompts from, decided by `Settings::translate_from`
    // so the launcher does not have to restate the "English needs nothing" rule.
    translate_from: Option<&str>,
) -> Result<StatusPayload> {
    if state.is_running() {
        return Err(AppError::AlreadyRunning);
    }

    let port = free_port()?;
    let mut args = bat.args.clone();

    // Flags this machine's graphics stack needs, e.g. --directml on AMD.
    for flag in extra_flags {
        if !args.iter().any(|a| a == flag) {
            args.push((*flag).to_string());
        }
    }

    if !args.iter().any(|a| a == "--disable-in-browser") {
        args.push("--disable-in-browser".into());
    }
    if !args.iter().any(|a| a == "--port") {
        args.push("--port".into());
        args.push(port.to_string());
    }
    // An explicit preset choice in our UI overrides whatever the bat carried.
    if let Some(p) = preset {
        if let Some(i) = args.iter().position(|a| a == "--preset") {
            args.truncate(i);
        }
        args.push("--preset".into());
        args.push(p.to_string());
    }

    // Boot Fooocus through the bridge rather than calling its entry script
    // directly. The bridge starts a loopback server, then hands over to the
    // very same script, so Gradio still comes up untouched.
    let bridge_port = free_port()?;
    let token = bridge_token();
    let script = crate::bridge::ensure_script(app)?;

    // args[0] is the entry script the chosen bat named (`launch.py`, or
    // `entry_with_update.py` for the profiles that self-update first).
    let (entry, passthrough) = args
        .split_first()
        .ok_or_else(|| AppError::msg("launch profile has no entry script"))?;
    let entry_path = Path::new(&info.root).join(entry);

    let mut args: Vec<String> = vec![
        script.display().to_string(),
        "--bridge-port".into(),
        bridge_port.to_string(),
        "--bridge-token".into(),
        token.clone(),
        "--fooocus-launch".into(),
        entry_path.display().to_string(),
    ];

    // The vendor directory goes in whenever it exists, regardless of which
    // language is set or whether its model has been downloaded.
    //
    // It has to be on sys.path before Fooocus imports transformers, because
    // transformers decides once and for all at import whether SentencePiece is
    // available. Passing it only alongside a ready model meant someone who
    // installed a language mid-session got a tokenizer that refused to load,
    // insisting SentencePiece was missing when it was sitting on disk.
    if let Ok(vendor) = crate::translate::vendor_dir(app) {
        if crate::translate::runtime_ready(app) {
            args.push("--vendor-dir".into());
            args.push(vendor.display().to_string());
        }
    }

    // The model, only once it is actually on disk. A language installed later
    // is picked up per request instead, so this is a convenience rather than
    // the only route.
    if let Some(code) = translate_from.filter(|code| crate::translate::model_ready(app, code)) {
        if let Ok(model) = crate::translate::model_dir(app, code) {
            args.push("--translate-model".into());
            args.push(model.display().to_string());
        }
    }

    args.push("--".into());
    args.extend(passthrough.iter().cloned());

    let root = Path::new(&info.root);
    let mut command = Command::new(&info.python);
    command
        .arg("-s") // ignore user site-packages, exactly as the bats do
        // Unbuffered. Python line-buffers only when stdout is a console; on a
        // pipe it switches to an 8KB block buffer, so a quiet startup produces
        // no output at all until long after it has finished loading. Without
        // this the UI cannot tell "still starting" from "hung".
        .arg("-u")
        .args(&args)
        .current_dir(root)
        .env("PYTHONUNBUFFERED", "1")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .stdin(Stdio::null());

    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(CREATE_NO_WINDOW);
    }

    let mut child = command.spawn()?;
    let stdout = child.stdout.take().expect("stdout was piped");
    let stderr = child.stderr.take().expect("stderr was piped");

    *state.inner.lock().unwrap() = Some(Running { child, port });
    *state.bridge.lock().unwrap() = Some(BridgeEndpoint {
        port: bridge_port,
        token,
    });

    let status = StatusPayload {
        state: RunState::Starting,
        port: Some(port),
        url: None,
        progress: 0.0,
        stage: "Starting Fooocus".into(),
        detail: None,
        exit_code: None,
    };
    state.set_status(app, status.clone());

    spawn_reader(app.clone(), state.clone(), stdout, "stdout", port);
    spawn_reader(app.clone(), state.clone(), stderr, "stderr", port);
    spawn_supervisor(app.clone(), state.clone());

    Ok(status)
}

/// Stop Fooocus. Idempotent so the UI can call it on window close.
pub fn stop(app: &AppHandle, state: &Arc<LauncherState>) -> Result<()> {
    let mut guard = state.inner.lock().unwrap();
    let Some(running) = guard.as_mut() else {
        return Ok(());
    };

    state.set_status(
        app,
        StatusPayload {
            state: RunState::Stopping,
            port: Some(running.port),
            url: None,
            progress: 0.0,
            stage: "Stopping Fooocus".into(),
            detail: None,
            exit_code: None,
        },
    );

    let _ = running.child.kill();
    let _ = running.child.wait();
    *guard = None;
    drop(guard);
    *state.bridge.lock().unwrap() = None;

    state.set_status(
        app,
        StatusPayload {
            state: RunState::Stopped,
            port: None,
            url: None,
            progress: 0.0,
            stage: "Not running".into(),
            detail: None,
            exit_code: None,
        },
    );
    Ok(())
}

/// Read one of the child's pipes, forwarding every line to the UI and
/// translating recognised lines into progress updates.
///
/// Progress bars (from `torch.hub.download_url_to_file`, which Fooocus uses to
/// fetch missing models on first run) are written with carriage returns rather
/// than newlines, so we split on both and mark `\r`-terminated chunks as
/// transient — the UI replaces the previous line instead of spamming the log.
fn spawn_reader<R: Read + Send + 'static>(
    app: AppHandle,
    state: Arc<LauncherState>,
    pipe: R,
    stream: &'static str,
    port: u16,
) {
    std::thread::spawn(move || {
        let mut reader = BufReader::new(pipe);
        let mut buf: Vec<u8> = Vec::with_capacity(1024);

        loop {
            buf.clear();
            // Read up to a newline, then split the chunk on carriage returns.
            match reader.read_until(b'\n', &mut buf) {
                Ok(0) | Err(_) => break,
                Ok(_) => {}
            }

            let chunk = String::from_utf8_lossy(&buf).to_string();
            let had_newline = chunk.ends_with('\n');
            let segments: Vec<&str> = chunk
                .trim_end_matches(['\n', '\r'])
                .split('\r')
                .filter(|s| !s.trim().is_empty())
                .collect();

            for (i, segment) in segments.iter().enumerate() {
                let is_last = i == segments.len() - 1;
                let _ = app.emit(
                    EVENT_LOG,
                    LogPayload {
                        line: segment.to_string(),
                        stream,
                        transient: !(is_last && had_newline),
                    },
                );
                apply_milestone(&app, &state, segment, port);
                apply_detail(&app, &state, segment);
            }
        }
    });
}

/// Turn a line of pip or download output into a short human-readable detail.
///
/// pip reports plenty — which package it is fetching and how many megabytes
/// have arrived — but writes it as progress-bar noise that is unreadable at a
/// glance. This pulls out the parts worth showing.
fn parse_detail(line: &str) -> Option<String> {
    let trimmed = line.trim();
    let lower = trimmed.to_lowercase();

    // "  12.3/2473.9 MB 5.2 MB/s eta 0:07:53" — the in-flight download.
    if let Some(rest) = trimmed.split_whitespace().find(|part| part.contains('/')) {
        if lower.contains(" mb") || lower.contains(" kb") || lower.contains(" gb") {
            if let Some((done, total)) = rest.split_once('/') {
                if let (Ok(done), Ok(total)) = (done.parse::<f64>(), total.parse::<f64>()) {
                    if total > 0.0 {
                        let unit = if lower.contains(" gb") {
                            "GB"
                        } else if lower.contains(" kb") {
                            "KB"
                        } else {
                            "MB"
                        };
                        // pip prints "-:--:--" before it can estimate; showing
                        // that is worse than showing nothing.
                        let eta = trimmed
                            .split_once("eta ")
                            .map(|(_, e)| e.trim())
                            .filter(|e| !e.contains("--"))
                            .map(|e| format!(", {e} left"))
                            .unwrap_or_default();
                        return Some(format!(
                            "{done:.0} of {total:.0} {unit} ({:.0}%){eta}",
                            done / total * 100.0
                        ));
                    }
                }
            }
        }
    }

    // "Collecting torch==2.1.0" / "Downloading torch-2.1.0-...whl (2473.9 MB)"
    for prefix in ["collecting ", "downloading ", "installing collected packages:"] {
        if let Some(rest) = lower.strip_prefix(prefix) {
            let value = trimmed[trimmed.len() - rest.len()..].trim();
            // Wheel filenames are long and mostly noise; keep the package name.
            let short = value.split(['-', '=', ' ']).next().unwrap_or(value);
            let label = match prefix {
                "collecting " => format!("Fetching {short}"),
                "downloading " => format!("Downloading {short}"),
                _ => format!("Installing {}", value.trim_end_matches('.')),
            };
            return Some(label.chars().take(90).collect());
        }
    }

    if lower.starts_with("building wheel") || lower.starts_with("preparing metadata") {
        return Some(trimmed.chars().take(90).collect());
    }

    None
}

/// Show what the current stage is actually doing, without moving the bar.
///
/// Throttled: pip repaints its progress bar many times a second, and every
/// repaint would otherwise become an event.
fn apply_detail(app: &AppHandle, state: &Arc<LauncherState>, line: &str) {
    let Some(detail) = parse_detail(line) else {
        return;
    };

    {
        let mut last = state.last_detail.lock().unwrap();
        if last.is_some_and(|at| at.elapsed() < Duration::from_millis(250)) {
            return;
        }
        *last = Some(Instant::now());
    }

    let current = state.status();
    if current.state != RunState::Starting {
        return;
    }

    state.set_status(
        app,
        StatusPayload {
            detail: Some(detail),
            ..current
        },
    );
}

/// Advance the startup progress if this line matches a known milestone.
/// Progress only ever moves forward, so out-of-order output cannot rewind it.
fn apply_milestone(app: &AppHandle, state: &Arc<LauncherState>, line: &str, port: u16) {
    let lower = line.to_lowercase();
    let Some((_, progress, stage)) = MILESTONES
        .iter()
        .find(|(needle, _, _)| lower.contains(needle))
    else {
        return;
    };

    let current = state.status();
    if current.state != RunState::Starting && current.state != RunState::Ready {
        return;
    }
    if *progress <= current.progress && current.progress > 0.0 {
        return;
    }

    let ready = *progress >= 1.0;
    state.set_status(
        app,
        StatusPayload {
            state: if ready {
                RunState::Ready
            } else {
                RunState::Starting
            },
            port: Some(port),
            url: ready.then(|| format!("http://127.0.0.1:{port}")),
            progress: *progress,
            stage: (*stage).to_string(),
            detail: None,
            exit_code: None,
        },
    );
}

/// Watch for the process ending on its own, which means it crashed — a clean
/// stop goes through `stop()` and clears the slot before we get here.
fn spawn_supervisor(app: AppHandle, state: Arc<LauncherState>) {
    std::thread::spawn(move || loop {
        std::thread::sleep(std::time::Duration::from_millis(500));

        let mut guard = state.inner.lock().unwrap();
        let Some(running) = guard.as_mut() else {
            return; // stopped deliberately
        };

        match running.child.try_wait() {
            Ok(Some(exit)) => {
                *guard = None;
                drop(guard);
                *state.bridge.lock().unwrap() = None;
                state.set_status(
                    &app,
                    StatusPayload {
                        state: RunState::Crashed,
                        port: None,
                        url: None,
                        progress: 0.0,
                        stage: "Fooocus stopped unexpectedly".into(),
                        detail: None,
                        exit_code: exit.code(),
                    },
                );
                return;
            }
            Ok(None) => {}
            Err(_) => return,
        }
    });
}

/// Ask the OS for an unused loopback port by binding to port 0 and releasing it.
fn free_port() -> Result<u16> {
    let listener = std::net::TcpListener::bind("127.0.0.1:0")?;
    Ok(listener.local_addr()?.port())
}
