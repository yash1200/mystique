use std::path::PathBuf;
use std::sync::Arc;

use mystique_core::diagnostics::{build_bundle, default_bundle_name, BundleContext};
use tauri::{AppHandle, Manager, State};

use super::IpcError;
use crate::AppState;

/// Settings > About > Export diagnostics. Writes a `.tar.gz` (see
/// `mystique_core::diagnostics::bundle` for what is in it) to `path`, or to
/// the Downloads folder when `path` is empty. Returns where it was written.
#[tauri::command]
pub async fn export_diagnostics(
    app: AppHandle,
    state: State<'_, AppState>,
    path: Option<String>,
) -> Result<String, IpcError> {
    let services = Arc::clone(&state.services);
    let log_dir = state.log_dir.clone();
    let version = app.package_info().version.to_string();
    let target = match path.map(|p| p.trim().to_string()).filter(|p| !p.is_empty()) {
        Some(p) => PathBuf::from(p),
        None => app
            .path()
            .download_dir()
            .map_err(|e| IpcError::internal(format!("downloads folder: {e}")))?
            .join(default_bundle_name()),
    };
    super::guard_blocking(move || {
        let ctx = BundleContext {
            app_version: &version,
            log_dir: log_dir.as_deref(),
        };
        let bytes = build_bundle(&services, &ctx).map_err(IpcError::from)?;
        std::fs::write(&target, &bytes).map_err(|e| {
            IpcError::new(
                mystique_core::IpcErrorKind::Storage,
                format!("could not write {}: {e}", target.display()),
            )
        })?;
        tracing::info!(bytes = bytes.len(), "diagnostics bundle exported");
        Ok(target.to_string_lossy().into_owned())
    })
    .await
}
