use mystique_connector_api::{
    ConnectorError, ConnectorResult, ExportObjectRequest, ExportObjectResult, ImportObjectRequest,
    ImportObjectResult, PreviewImportRequest, PreviewImportResult,
};

use super::ConnectionManager;

impl ConnectionManager {
    /// Plugin-owned full-table file export (100-row chunks, no row cap).
    pub async fn export_object(
        &self,
        req: ExportObjectRequest,
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
        session.export_object(&req.into_input()).await
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
        if req.profile_read_only {
            return Err(ConnectorError::Policy(
                "This connection is read-only. Disable read-only to import data.".into(),
            ));
        }
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
        session.import_object(&req.into_input()).await
    }
}
