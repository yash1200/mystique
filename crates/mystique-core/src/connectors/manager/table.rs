use mystique_connector_api::{
    ApplyTableEditsInput, ApplyTableEditsRequest, ApplyTableEditsResult, ConnectorError,
    ConnectorResult, FetchTablePreviewRequest, FetchTablePreviewStatsRequest, TableAdminOp,
    TableAdminRequest, TableAdminResult, TablePreviewResult, TablePreviewStats, WriteRisk,
    DEFAULT_MAX_ROWS, HARD_MAX_ROWS,
};

use super::ConnectionManager;

impl ConnectionManager {
    /// Persist grid edits via the engine plugin (dialect + execution).
    pub async fn apply_table_edits(
        &self,
        req: ApplyTableEditsRequest,
    ) -> ConnectorResult<ApplyTableEditsResult> {
        if req.updates.is_empty() && req.inserts.is_empty() && req.deletes.is_empty() {
            return Ok(ApplyTableEditsResult::empty());
        }
        let preview = format!(
            "Save {} update(s), {} insert(s), {} delete(s) on {}",
            req.updates.len(),
            req.inserts.len(),
            req.deletes.len(),
            req.table
        );
        self.enforce_write_guard(
            &req.session_id,
            WriteRisk::Write,
            &preview,
            Some(req.table.as_str()),
            req.confirm_object_name.as_deref(),
            req.profile_read_only,
        )
        .await?;
        let session = self.session(&req.session_id).await?;
        let input = ApplyTableEditsInput::from(req);
        session.apply_table_edits(&input).await
    }

    pub async fn fetch_table_preview(
        &self,
        req: FetchTablePreviewRequest,
    ) -> ConnectorResult<TablePreviewResult> {
        let session = self.session(&req.session_id).await?;
        let input = req.into_input(DEFAULT_MAX_ROWS, HARD_MAX_ROWS);
        session.fetch_table_preview(&input).await
    }

    pub async fn fetch_table_preview_stats(
        &self,
        req: FetchTablePreviewStatsRequest,
    ) -> ConnectorResult<TablePreviewStats> {
        let session = self.session(&req.session_id).await?;
        session.fetch_table_preview_stats(&req.into_input()).await
    }
    /// Plugin-owned table/collection admin (CREATE text + mutating ops).
    pub async fn apply_table_admin(
        &self,
        req: TableAdminRequest,
    ) -> ConnectorResult<TableAdminResult> {
        let risk = match req.op {
            TableAdminOp::CreateStatement => WriteRisk::Read,
            TableAdminOp::Drop | TableAdminOp::Truncate => WriteRisk::Destructive,
            _ => WriteRisk::Write,
        };
        if risk != WriteRisk::Read {
            let preview = format!("{:?} {}", req.op, req.name);
            self.enforce_write_guard(
                &req.session_id,
                risk,
                &preview,
                Some(req.name.as_str()),
                req.confirm_object_name.as_deref(),
                req.profile_read_only,
            )
            .await?;
        } else if req.profile_read_only && req.op != TableAdminOp::CreateStatement {
            return Err(ConnectorError::Policy(
                "This connection is read-only. Disable read-only to change objects.".into(),
            ));
        }
        let needs_name = matches!(req.op, TableAdminOp::Rename | TableAdminOp::Duplicate);
        if needs_name
            && req
                .new_name
                .as_deref()
                .map(str::trim)
                .unwrap_or("")
                .is_empty()
        {
            return Err(ConnectorError::InvalidConfig(
                "newName is required for rename and duplicate.".into(),
            ));
        }
        if req.name.trim().is_empty() {
            return Err(ConnectorError::InvalidConfig(
                "Object name is required.".into(),
            ));
        }
        if matches!(
            req.op,
            TableAdminOp::CreateGroup | TableAdminOp::CreateObject
        ) && !req
            .name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_')
        {
            return Err(ConnectorError::InvalidConfig(
                "Name may only contain letters, numbers, and underscore.".into(),
            ));
        }
        let session = self.session(&req.session_id).await?;
        let input = req.into_input();
        session.apply_table_admin(&input).await
    }
}
