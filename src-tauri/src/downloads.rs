//! Resumable download manager.
//!
//! Files land next to their final destination as `<name>.part` and are only
//! renamed into place once complete, so an interrupted download can never be
//! mistaken for an installed model. Resuming re-opens the partial file and
//! asks the server for the remaining bytes with a `Range` header.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use futures_util::StreamExt;
use serde::Serialize;
use tauri::{AppHandle, Emitter};
use tokio::io::AsyncWriteExt;

use crate::error::{AppError, Result};

pub const EVENT_DOWNLOAD: &str = "download://progress";

/// How often at most we push a progress event per job. Fast enough to feel
/// live, slow enough not to flood the webview on a gigabit connection.
const EMIT_INTERVAL: Duration = Duration::from_millis(200);

/// Control signal shared with a running download task.
const RUN: u8 = 0;
const PAUSE: u8 = 1;
const CANCEL: u8 = 2;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum JobState {
    Queued,
    Downloading,
    Paused,
    Completed,
    Failed,
    Cancelled,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Job {
    pub id: String,
    pub name: String,
    pub filename: String,
    pub category: String,
    pub url: String,
    pub target: String,
    pub state: JobState,
    pub downloaded: u64,
    /// Total size in bytes once the server has told us. `None` while unknown.
    pub total: Option<u64>,
    /// Bytes per second over the last sampling window.
    pub speed: u64,
    pub error: Option<String>,
}

struct Entry {
    job: Job,
    control: Arc<AtomicU8>,
}

#[derive(Default)]
pub struct DownloadManager {
    entries: Mutex<HashMap<String, Entry>>,
}

impl DownloadManager {
    pub fn jobs(&self) -> Vec<Job> {
        let mut jobs: Vec<Job> = self
            .entries
            .lock()
            .unwrap()
            .values()
            .map(|e| e.job.clone())
            .collect();
        jobs.sort_by(|a, b| a.name.cmp(&b.name));
        jobs
    }

    fn update(&self, app: &AppHandle, id: &str, f: impl FnOnce(&mut Job)) {
        let mut guard = self.entries.lock().unwrap();
        let Some(entry) = guard.get_mut(id) else { return };
        f(&mut entry.job);
        let snapshot = entry.job.clone();
        drop(guard);
        let _ = app.emit(EVENT_DOWNLOAD, snapshot);
    }
}

/// Queue a download. Re-queuing an id that is paused or failed resumes it;
/// re-queuing one that is already active is a no-op.
pub fn enqueue(
    app: &AppHandle,
    manager: &Arc<DownloadManager>,
    id: String,
    name: String,
    filename: String,
    category: String,
    url: String,
    target: String,
) -> Result<()> {
    if !crate::catalog::is_huggingface(&url) {
        return Err(AppError::msg(format!(
            "refusing to download from an unexpected host: {url}"
        )));
    }

    {
        let mut guard = manager.entries.lock().unwrap();
        if let Some(existing) = guard.get(&id) {
            if matches!(existing.job.state, JobState::Downloading | JobState::Queued) {
                return Ok(());
            }
        }

        // Any partial bytes already on disk count towards this job immediately.
        let downloaded = std::fs::metadata(part_path(&target))
            .map(|m| m.len())
            .unwrap_or(0);

        guard.insert(
            id.clone(),
            Entry {
                job: Job {
                    id: id.clone(),
                    name,
                    filename,
                    category,
                    url: url.clone(),
                    target: target.clone(),
                    state: JobState::Queued,
                    downloaded,
                    total: None,
                    speed: 0,
                    error: None,
                },
                control: Arc::new(AtomicU8::new(RUN)),
            },
        );
    }

    let control = manager.entries.lock().unwrap()[&id].control.clone();
    control.store(RUN, Ordering::SeqCst);

    let app = app.clone();
    let manager = manager.clone();
    tauri::async_runtime::spawn(async move {
        let result = run_download(&app, &manager, &id, &url, &target, &control).await;
        match result {
            Ok(Outcome::Completed) => manager.update(&app, &id, |j| {
                j.state = JobState::Completed;
                j.speed = 0;
                if let Some(total) = j.total {
                    j.downloaded = total;
                }
            }),
            Ok(Outcome::Paused) => manager.update(&app, &id, |j| {
                j.state = JobState::Paused;
                j.speed = 0;
            }),
            Ok(Outcome::Cancelled) => manager.update(&app, &id, |j| {
                j.state = JobState::Cancelled;
                j.speed = 0;
                j.downloaded = 0;
            }),
            Err(err) => manager.update(&app, &id, |j| {
                j.state = JobState::Failed;
                j.speed = 0;
                j.error = Some(err.to_string());
            }),
        }
    });

    Ok(())
}

pub fn pause(manager: &Arc<DownloadManager>, id: &str) {
    if let Some(entry) = manager.entries.lock().unwrap().get(id) {
        entry.control.store(PAUSE, Ordering::SeqCst);
    }
}

/// Cancel a job and discard its partial file.
pub fn cancel(manager: &Arc<DownloadManager>, id: &str) {
    let guard = manager.entries.lock().unwrap();
    let Some(entry) = guard.get(id) else { return };
    entry.control.store(CANCEL, Ordering::SeqCst);

    // If it never started there is no task to notice the flag; clean up here.
    if matches!(entry.job.state, JobState::Queued | JobState::Paused) {
        let _ = std::fs::remove_file(part_path(&entry.job.target));
    }
}

pub fn clear_finished(manager: &Arc<DownloadManager>) {
    manager.entries.lock().unwrap().retain(|_, e| {
        !matches!(
            e.job.state,
            JobState::Completed | JobState::Cancelled | JobState::Failed
        )
    });
}

enum Outcome {
    Completed,
    Paused,
    Cancelled,
}

async fn run_download(
    app: &AppHandle,
    manager: &Arc<DownloadManager>,
    id: &str,
    url: &str,
    target: &str,
    control: &Arc<AtomicU8>,
) -> Result<Outcome> {
    let target = PathBuf::from(target);
    let part = part_path(&target);

    if let Some(parent) = target.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }

    let mut offset = tokio::fs::metadata(&part).await.map(|m| m.len()).unwrap_or(0);

    let client = reqwest::Client::builder()
        // Large models over a slow link should not trip a total-request timeout.
        .connect_timeout(Duration::from_secs(30))
        .build()?;

    let mut request = client.get(url);
    if offset > 0 {
        request = request.header(reqwest::header::RANGE, format!("bytes={offset}-"));
    }

    let response = request.send().await?.error_for_status()?;

    // A server that ignored our Range header restarts the file from scratch.
    let resumed = response.status() == reqwest::StatusCode::PARTIAL_CONTENT;
    if offset > 0 && !resumed {
        offset = 0;
    }

    let total = response.content_length().map(|len| len + offset);
    manager.update(app, id, |j| {
        j.state = JobState::Downloading;
        j.downloaded = offset;
        j.total = total;
        j.error = None;
    });

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
        match control.load(Ordering::SeqCst) {
            PAUSE => {
                file.flush().await?;
                return Ok(Outcome::Paused);
            }
            CANCEL => {
                drop(file);
                let _ = tokio::fs::remove_file(&part).await;
                return Ok(Outcome::Cancelled);
            }
            _ => {}
        }

        let chunk = chunk?;
        file.write_all(&chunk).await?;
        downloaded += chunk.len() as u64;
        window_bytes += chunk.len() as u64;

        if last_emit.elapsed() >= EMIT_INTERVAL {
            let secs = window_start.elapsed().as_secs_f64().max(0.001);
            let speed = (window_bytes as f64 / secs) as u64;
            manager.update(app, id, |j| {
                j.downloaded = downloaded;
                j.speed = speed;
            });
            last_emit = Instant::now();
            window_start = Instant::now();
            window_bytes = 0;
        }
    }

    file.flush().await?;
    drop(file);

    // Only now is the file real as far as the model manager is concerned.
    tokio::fs::rename(&part, &target).await?;
    Ok(Outcome::Completed)
}

fn part_path(target: impl AsRef<Path>) -> PathBuf {
    let target = target.as_ref();
    let mut name = target.file_name().unwrap_or_default().to_os_string();
    name.push(".part");
    target.with_file_name(name)
}

/// Ask the server how big a file is without downloading it.
pub async fn probe_size(url: &str) -> Result<Option<u64>> {
    let client = reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(15))
        .build()?;
    let response = client.head(url).send().await?.error_for_status()?;
    Ok(response.content_length())
}
