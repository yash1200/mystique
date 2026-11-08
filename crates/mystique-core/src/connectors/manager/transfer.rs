use std::sync::Arc;

use mystique_connector_api::{
    ConnectorError, ConnectorResult, ExportObjectRequest, ExportObjectResult, ImportObjectRequest,
    ImportObjectResult, PreviewImportRequest, PreviewImportResult, WriteAction, WriteRisk,
};

use super::guard::WriteCheck;
use super::ConnectionManager;

impl ConnectionManager {
    /// Plugin-owned full-table file export (100-row chunks, no row cap).
    pub async fn export_object(
        &self,
        req: ExportObjectRequest,
    ) -> ConnectorResult<ExportObjectResult> {
        let (id, control) = self
            .begin_run(&req.session_id, req.execution_id.clone())
            .await;
        let result = self.export_registered(req, &control).await;
        self.finish_run(&id).await;
        result
    }

    async fn export_registered(
        &self,
        req: ExportObjectRequest,
        control: &Arc<mystique_connector_api::QueryControl>,
    ) -> ConnectorResult<ExportObjectResult> {
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
        let mut input = req.into_input();
        input.control = Arc::clone(control);
        self.drive_until_cancel(control, session.export_object(&input))
            .await
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

    pub async fn import_object(
        &self,
        req: ImportObjectRequest,
    ) -> ConnectorResult<ImportObjectResult> {
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
            ..WriteCheck::new(&req.session_id, WriteAction::Import, WriteRisk::Write)
        })
        .await?;
        session.import_object(&req.into_input()).await
    }
}
