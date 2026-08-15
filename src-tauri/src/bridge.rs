//! Client for the in-process Python bridge.
//!
//! The bridge script is embedded in this binary and written to the app's own
//! data directory at launch, so the user's Fooocus folder is never touched.
//! Communication is plain JSON over loopback, guarded by a per-run token.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use tauri::{AppHandle, Emitter, Manager};

use crate::error::{AppError, Result};
use crate::launcher::{BridgeEndpoint, LauncherState};

pub const EVENT_BRIDGE: &str = "bridge://event";

const SCRIPT: &str = include_str!("../resources/fooocus_bridge.py");

/// Write the bridge script out and return its path.
///
/// Rewritten every launch so an app update always ships its own version rather
/// than reusing a stale copy from a previous install.
pub fn ensure_script(app: &AppHandle) -> Result<PathBuf> {
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|e| AppError::msg(e.to_string()))?
        .join("bridge");
    std::fs::create_dir_all(&dir)?;

    let path = dir.join("fooocus_bridge.py");
    std::fs::write(&path, SCRIPT)?;
    Ok(path)
}

fn endpoint(state: &Arc<LauncherState>) -> Result<BridgeEndpoint> {
    state
        .bridge()
        .ok_or_else(|| AppError::msg("Fooocus is not running"))
}

async fn client() -> Result<reqwest::Client> {
    Ok(reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(5))
        .timeout(Duration::from_secs(120))
        .build()?)
}

/// GET a bridge endpoint, returning the parsed JSON body.
pub async fn get(state: &Arc<LauncherState>, path: &str) -> Result<serde_json::Value> {
    let bridge = endpoint(state)?;
    let response = client()
        .await?
        .get(format!("http://127.0.0.1:{}{path}", bridge.port))
        .header("X-Bridge-Token", &bridge.token)
        .send()
        .await?;

    parse(response).await
}

/// POST a JSON body to a bridge endpoint.
pub async fn post(
    state: &Arc<LauncherState>,
    path: &str,
    body: serde_json::Value,
) -> Result<serde_json::Value> {
    let bridge = endpoint(state)?;
    let response = client()
        .await?
        .post(format!("http://127.0.0.1:{}{path}", bridge.port))
        .header("X-Bridge-Token", &bridge.token)
        .json(&body)
        .send()
        .await?;

    parse(response).await
}

/// Surface the bridge's own error text rather than a bare status code — it
/// carries the Python exception, which is what actually explains a failure.
async fn parse(response: reqwest::Response) -> Result<serde_json::Value> {
    let status = response.status();
    let body: serde_json::Value = response.json().await.unwrap_or(serde_json::Value::Null);

    if status.is_success() {
        return Ok(body);
    }

    let message = body
        .get("error")
        .and_then(|v| v.as_str())
        .unwrap_or("bridge request failed");
    Err(AppError::msg(message.to_string()))
}

/// Poll the bridge for events and forward them to the UI.
///
/// Polling rather than a socket keeps the Python side to the standard library,
/// which matters because it runs inside an interpreter we do not control and
/// must not add dependencies to. Events are append-only and fetched by index,
/// so nothing is lost between polls.
pub fn spawn_event_pump(app: AppHandle, state: Arc<LauncherState>) {
    tauri::async_runtime::spawn(async move {
        let mut since = 0u64;

        loop {
            tokio::time::sleep(Duration::from_millis(250)).await;

            // Stop pumping when Fooocus goes away; a fresh run starts over.
            if state.bridge().is_none() {
                since = 0;
                continue;
            }

            let Ok(body) = get(&state, &format!("/events?since={since}")).await else {
                continue;
            };
            let Some(events) = body.get("events").and_then(|v| v.as_array()) else {
                continue;
            };

            for event in events {
                if let Some(index) = event.get("index").and_then(serde_json::Value::as_u64) {
                    since = since.max(index + 1);
                }
                let _ = app.emit(EVENT_BRIDGE, event.clone());
            }
        }
    });
}
