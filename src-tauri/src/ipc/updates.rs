//! App updates: check, download, install.
//!
//! The plugin's own JS commands build a client with no connect timeout, so
//! one unreachable address of a GitHub CDN host costs the OS TCP timeout
//! (60–75 s) before the next is tried: a check with a 30 s limit fails, and a
//! download sits at 0 bytes for a minute. These go through the same Rust API
//! with a connect timeout (split evenly across a host's addresses) and a read
//! timeout for stalls, and log the whole error chain.

use std::sync::Mutex;
use std::time::Duration;

use serde::Serialize;
use tauri::ipc::Channel;
use tauri::{AppHandle, State};
use tauri_plugin_updater::{Update, UpdaterExt};

use super::IpcError;

/// For all of a host's addresses together: GitHub's CDN has four, so 5 s each.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(20);
/// Longest wait for the next bytes once connected. No cap on a slow download.
const READ_TIMEOUT: Duration = Duration::from_secs(30);

/// The update the last check found, and its bytes once downloaded.
#[derive(Default)]
pub struct PendingUpdate(Mutex<Option<Pending>>);

struct Pending {
    update: Update,
    bytes: Option<Vec<u8>>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateMetadata {
    version: String,
    current_version: String,
    notes: Option<String>,
    date: Option<String>,
}

/// Same shape as the plugin's, so the frontend reads either.
#[derive(Clone, Serialize)]
#[serde(tag = "event", content = "data")]
pub enum DownloadEvent {
    #[serde(rename_all = "camelCase")]
    Started {
        content_length: Option<u64>,
    },
    #[serde(rename_all = "camelCase")]
    Progress {
        chunk_length: usize,
    },
    Finished,
}

fn failed(during: &str, err: tauri_plugin_updater::Error) -> IpcError {
    // `{:?}` keeps the source chain the one-line message drops.
    tracing::warn!(error = ?err, "update {during} failed");
    IpcError::internal(err.to_string())
}

fn slot(state: &PendingUpdate) -> std::sync::MutexGuard<'_, Option<Pending>> {
    state
        .0
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// `None` when this is the newest version.
#[tauri::command]
pub async fn check_for_update(
    app: AppHandle,
    state: State<'_, PendingUpdate>,
) -> Result<Option<UpdateMetadata>, IpcError> {
    let updater = app
        .updater_builder()
        .configure_client(|client| {
            client
                .connect_timeout(CONNECT_TIMEOUT)
                .read_timeout(READ_TIMEOUT)
        })
        .build()
        .map_err(|e| failed("check", e))?;
    let found = updater.check().await.map_err(|e| failed("check", e))?;
    let metadata = found.as_ref().map(|u| UpdateMetadata {
        version: u.version.clone(),
        current_version: u.current_version.clone(),
        notes: u.body.clone().filter(|b| !b.trim().is_empty()),
        date: u.date.map(|d| d.to_string()),
    });
    *slot(&state) = found.map(|update| Pending {
        update,
        bytes: None,
    });
    Ok(metadata)
}

/// Download the update the last check found. Progress streams per chunk.
#[tauri::command]
pub async fn download_update(
    state: State<'_, PendingUpdate>,
    on_event: Channel<DownloadEvent>,
) -> Result<(), IpcError> {
    let update = slot(&state)
        .as_ref()
        .map(|p| p.update.clone())
        .ok_or_else(|| IpcError::internal("No update to download. Check for updates first."))?;
    let mut started = false;
    let bytes = update
        .download(
            |chunk_length, content_length| {
                if !started {
                    started = true;
                    let _ = on_event.send(DownloadEvent::Started { content_length });
                }
                let _ = on_event.send(DownloadEvent::Progress { chunk_length });
            },
            || {
                let _ = on_event.send(DownloadEvent::Finished);
            },
        )
        .await
        .map_err(|e| failed("download", e))?;
    if let Some(pending) = slot(&state).as_mut() {
        pending.bytes = Some(bytes);
    }
    Ok(())
}

/// Replace the app with the downloaded update. Takes effect on restart.
#[tauri::command]
pub async fn install_update(state: State<'_, PendingUpdate>) -> Result<(), IpcError> {
    let pending = slot(&state).take();
    let Some(Pending {
        update,
        bytes: Some(bytes),
    }) = pending
    else {
        return Err(IpcError::internal(
            "No downloaded update to install. Check for updates first.",
        ));
    };
    update.install(&bytes).map_err(|e| failed("install", e))
}
