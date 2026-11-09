//! Table export and import as background transfers.
//!
//! Export streams rows from the plugin into a staged file (see
//! `mystique_connector_api::ExportWriter`); import parses the file as it
//! writes. Both report rows done through a progress callback, stop on
//! cancel, and run one at a time per connection. While any runs, the app
//! asks before quitting (`running_transfers`).

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use mystique_connector_api::{
    ConnectorError, ConnectorResult, ExportObjectRequest, ExportObjectResult, ExportWriter,
    ImportObjectRequest, ImportObjectResult, PreviewImportRequest, PreviewImportResult,
    QueryControl, RowProgress, SqlDialect, WriteAction, WriteRisk,
};

use super::guard::WriteCheck;
use super::run::cancelled_error;
use super::ConnectionManager;

/// Running transfers, for the quit guard.
#[derive(Default)]
pub(crate) struct TransferRegistry {
    running: Mutex<HashMap<u64, Arc<QueryControl>>>,
    next: AtomicU64,
}

impl TransferRegistry {
    fn lock(&self) -> std::sync::MutexGuard<'_, HashMap<u64, Arc<QueryControl>>> {
        self.running.lock().unwrap_or_else(|err| err.into_inner())
    }

    fn begin(&self, control: Arc<QueryControl>) -> TransferGuard<'_> {
        let id = self.next.fetch_add(1, Ordering::SeqCst);
        self.lock().insert(id, control);
        TransferGuard { registry: self, id }
    }
}

struct TransferGuard<'a> {
    registry: &'a TransferRegistry,
    id: u64,
}

impl Drop for TransferGuard<'_> {
    fn drop(&mut self) {
        self.registry.lock().remove(&self.id);
    }
}

impl ConnectionManager {
    /// Exports and imports still running (or queued on the backend).
    pub fn running_transfers(&self) -> usize {
        self.transfers.lock().len()
    }

    /// Cancel every running transfer. Staged export files are removed;
    /// import batches already written stay.
    pub fn cancel_transfers(&self) {
        let controls: Vec<Arc<QueryControl>> = self.transfers.lock().values().cloned().collect();
        for control in controls {
            control.cancel();
        }
    }

    /// Wait for this connection's previous transfer. Cancel still stops a
    /// queued one.
    async fn transfer_turn(
        &self,
        session_id: &str,
        control: &Arc<QueryControl>,
    ) -> ConnectorResult<tokio::sync::OwnedMutexGuard<()>> {
        let gate = self.gate_for(&format!("transfer:{session_id}")).await;
        tokio::select! {
            held = gate.lock_owned() => Ok(held),
            _ = control.token.cancelled() => Err(cancelled_error()),
        }
    }

    /// Stream a whole table or collection to a file. With `tab_id`, a query
    /// tab's open transaction is read through, so its uncommitted rows are
    /// exported as the tab sees them.
    pub async fn export_object(
        &self,
        req: ExportObjectRequest,
        progress: Option<RowProgress>,
    ) -> ConnectorResult<ExportObjectResult> {
        let run = self.begin_run(&req.session_id, req.execution_id.clone());
        let control = Arc::clone(&run.control);
        let _transfer = self.transfers.begin(Arc::clone(&control));
        let _turn = self.transfer_turn(&req.session_id, &control).await?;
        if req.name.trim().is_empty() {
            return Err(ConnectorError::InvalidConfig(
                "Object name is required.".into(),
            ));
        }
        if req.path.trim().is_empty() {
            return Err(ConnectorError::InvalidConfig(
                "Export path is required.".into(),
            ));
        }
        let session = self.session(&req.session_id).await?;
        let allowed = session.capabilities().export_formats;
        let format = req.format.as_str();
        if !allowed.iter().any(|f| f == format) {
            return Err(ConnectorError::NotSupported(format!(
                "this engine does not export {format} files"
            )));
        }
        let session_id = req.session_id.clone();
        let tab_id = req.tab_id.clone();
        let dialect = SqlDialect::from_engine(session.engine_id());
        let sql_style = (dialect != SqlDialect::Generic).then_some(dialect);
        let mut input = req.into_input();
        input.control = Arc::clone(&control);
        input.progress = progress;
        let source = input.source();
        let mut writer = ExportWriter::start(&input, sql_style)?;
        let pin = self.pinned_for_read(&session_id, tab_id.as_deref()).await;
        tracing::info!(
            session_id = %session_id,
            through_transaction = pin.is_some(),
            format,
            "export started"
        );
        let read = {
            let sink = writer.sink();
            let work = async {
                match &pin {
                    Some(pin) => pin.stream_rows(&source, &control, sink).await,
                    None => session.stream_rows(&source, &control, sink).await,
                }
            };
            self.drive_until_cancel(&control, work).await
        };
        let result = writer.finish(read).await;
        match &result {
            Ok(done) => {
                tracing::info!(session_id = %session_id, rows = done.row_count, "export finished")
            }
            Err(err) => {
                tracing::info!(session_id = %session_id, error = err.kind(), "export failed")
            }
        }
        result
    }

    pub async fn preview_import(
        &self,
        req: PreviewImportRequest,
    ) -> ConnectorResult<PreviewImportResult> {
        if req.name.trim().is_empty() {
            return Err(ConnectorError::InvalidConfig(
                "Object name is required.".into(),
            ));
        }
        if req.path.trim().is_empty() {
            return Err(ConnectorError::InvalidConfig(
                "Import path is required.".into(),
            ));
        }
        let session = self.session(&req.session_id).await?;
        let allowed = session.capabilities().export_formats;
        let format = req.format.as_str();
        if !allowed.iter().any(|f| f == format) {
            return Err(ConnectorError::NotSupported(format!(
                "this engine does not import {format} files"
            )));
        }
        session.preview_import(&req.into_input()).await
    }

    /// Import the whole file, parsed and written batch by batch.
    pub async fn import_object(
        &self,
        req: ImportObjectRequest,
        progress: Option<RowProgress>,
    ) -> ConnectorResult<ImportObjectResult> {
        let run = self.begin_run(&req.session_id, req.execution_id.clone());
        let control = Arc::clone(&run.control);
        let _transfer = self.transfers.begin(Arc::clone(&control));
        if req.name.trim().is_empty() {
            return Err(ConnectorError::InvalidConfig(
                "Object name is required.".into(),
            ));
        }
        if req.path.trim().is_empty() {
            return Err(ConnectorError::InvalidConfig(
                "Import path is required.".into(),
            ));
        }
        let session = self.session(&req.session_id).await?;
        let allowed = session.capabilities().export_formats;
        let format = req.format.as_str();
        if !allowed.iter().any(|f| f == format) {
            return Err(ConnectorError::NotSupported(format!(
                "this engine does not import {format} files"
            )));
        }
        // Upsert is still a write. The guard reads the live session's read-only bit.
        self.enforce_write_guard(WriteCheck {
            preview: format!("Import into {}", req.name.trim()),
            database: req.database.as_deref(),
            schema: req.schema.as_deref(),
            ..WriteCheck::new(&req.session_id, WriteAction::Import, WriteRisk::Write)
        })
        .await?;
        let _turn = self.transfer_turn(&req.session_id, &control).await?;
        let session_id = req.session_id.clone();
        let mut input = req.into_input();
        input.control = Arc::clone(&control);
        input.progress = progress;
        // Cancel stops between batches (the importer checks the control);
        // a batch in flight finishes so it is either written or not.
        let result = session.import_object(&input).await;
        // Row counts in the explorer and kept cell values are stale now.
        self.meta.invalidate(&session_id);
        self.large_values.forget_session(&session_id);
        result
    }
}
