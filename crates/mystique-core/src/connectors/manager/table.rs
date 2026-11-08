use std::sync::Arc;

use mystique_connector_api::{
    plan_schema_diff, plan_table_alter, ApplyDdlRequest, ApplyDdlResult, ApplySchemaDiffRequest,
    ApplyTableEditsInput, ApplyTableEditsRequest, ApplyTableEditsResult, ConnectorError,
    ConnectorResult, DdlDialect, DescribeTableDefinitionRequest, DistinctColumnValuesRequest,
    FetchLargeValueRequest, FetchLargeValueResult, FetchTablePreviewRequest,
    FetchTablePreviewStatsRequest, ListTableDefinitionsRequest, PlanDdlRequest,
    PlanSchemaDiffRequest, SchemaMigration, TableAdminOp, TableAdminRequest, TableAdminResult,
    TableDefinition, TablePreviewResult, TablePreviewStats, WriteRisk, DEFAULT_MAX_ROWS,
    HARD_MAX_ROWS, MAX_TIMEOUT_MS,
};

use super::depth::{reject_beyond_depth, DepthFeature};
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
        if !req.dry_run {
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
        }
        let session = self.session(&req.session_id).await?;
        let session_id = req.session_id.clone();
        let input = ApplyTableEditsInput::from(req);
        let mut result = session.apply_table_edits(&input).await?;
        self.remember_save(
            &session_id,
            input.database.as_deref(),
            input.schema.as_deref(),
            &input.table,
            &mut result,
        )
        .await;
        Ok(result)
    }

    pub async fn distinct_column_values(
        &self,
        req: DistinctColumnValuesRequest,
    ) -> ConnectorResult<Vec<serde_json::Value>> {
        let session = self.session(&req.session_id).await?;
        let input = mystique_connector_api::DistinctColumnValuesInput::from(req);
        session.distinct_column_values(&input).await
    }

    pub async fn fetch_table_preview(
        &self,
        req: FetchTablePreviewRequest,
    ) -> ConnectorResult<TablePreviewResult> {
        let session = self.session(&req.session_id).await?;
        let session_id = req.session_id.clone();
        let execution_id = req.execution_id.clone();
        let mut input = req.into_input(DEFAULT_MAX_ROWS, HARD_MAX_ROWS);
        let (id, control) = self.begin_run(&session_id, execution_id).await;
        input.control = Arc::clone(&control);
        let result = self
            .drive_registered(
                &control,
                MAX_TIMEOUT_MS,
                session.fetch_table_preview(&input),
            )
            .await;
        self.finish_run(&id).await;
        result
    }

    pub async fn fetch_table_preview_stats(
        &self,
        req: FetchTablePreviewStatsRequest,
    ) -> ConnectorResult<TablePreviewStats> {
        let session = self.session(&req.session_id).await?;
        session.fetch_table_preview_stats(&req.into_input()).await
    }

    pub async fn fetch_large_value(
        &self,
        req: FetchLargeValueRequest,
    ) -> ConnectorResult<FetchLargeValueResult> {
        let session = self.session(&req.session_id).await?;
        let session_id = req.session_id.clone();
        let execution_id = req.execution_id.clone();
        let mut input = req.into_input();
        let (id, control) = self.begin_run(&session_id, execution_id).await;
        input.control = Arc::clone(&control);
        let result = self
            .drive_registered(&control, MAX_TIMEOUT_MS, session.fetch_large_value(&input))
            .await;
        self.finish_run(&id).await;
        result
    }
    /// Plugin-owned table/collection admin (CREATE text + mutating ops).
    pub async fn apply_table_admin(
        &self,
        req: TableAdminRequest,
    ) -> ConnectorResult<TableAdminResult> {
        let meta = self.session_meta(&req.session_id).await?;
        reject_beyond_depth(&meta.caps, DepthFeature::TableAdmin(req.op))?;
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
        let input = req.into_input();
        meta.session.apply_table_admin(&input).await
    }

    pub async fn describe_table_definition(
        &self,
        req: DescribeTableDefinitionRequest,
    ) -> ConnectorResult<TableDefinition> {
        let session = self.session(&req.session_id).await?;
        session
            .describe_table_definition(
                req.database.as_deref(),
                req.schema.as_deref(),
                &req.table,
                req.kind.as_deref(),
            )
            .await
    }

    pub async fn list_table_definitions(
        &self,
        req: ListTableDefinitionsRequest,
    ) -> ConnectorResult<Vec<TableDefinition>> {
        let session = self.session(&req.session_id).await?;
        session
            .list_table_definitions(req.database.as_deref(), req.schema.as_deref())
            .await
    }

    pub async fn plan_ddl(
        &self,
        req: PlanDdlRequest,
    ) -> ConnectorResult<mystique_connector_api::DdlPlan> {
        let meta = self.session_meta(&req.session_id).await?;
        if !meta.caps.supports_structure_edit {
            return Err(ConnectorError::NotSupported(
                "This engine cannot edit structure.".into(),
            ));
        }
        Ok(plan_table_alter(dialect_of(&meta), &req.draft))
    }

    pub async fn apply_ddl(&self, req: ApplyDdlRequest) -> ConnectorResult<ApplyDdlResult> {
        let meta = self.session_meta(&req.session_id).await?;
        if !meta.caps.supports_structure_edit {
            return Err(ConnectorError::NotSupported(
                "This engine cannot edit structure.".into(),
            ));
        }
        let plan = plan_table_alter(dialect_of(&meta), &req.draft);
        if !plan.warnings.is_empty() {
            return Err(ConnectorError::Policy(plan.warnings.join(" ")));
        }
        if plan.statements.is_empty() {
            return Ok(ApplyDdlResult {
                applied: Vec::new(),
                message: "Nothing to change.".into(),
            });
        }
        self.enforce_write_guard(
            &req.session_id,
            mystique_connector_api::ddl_risk(&req.draft),
            &format!("Structure change {}", req.draft.name),
            Some(req.draft.name.as_str()),
            req.confirm_object_name.as_deref(),
            req.profile_read_only,
        )
        .await?;
        meta.session.apply_ddl(&plan, &req.draft).await
    }

    pub async fn plan_schema_diff(
        &self,
        req: PlanSchemaDiffRequest,
    ) -> ConnectorResult<SchemaMigration> {
        let meta = self.session_meta(&req.session_id).await?;
        if !meta.caps.supports_structure_edit {
            return Err(ConnectorError::NotSupported(
                "This engine cannot edit structure.".into(),
            ));
        }
        Ok(plan_schema_diff(
            dialect_of(&meta),
            &req.source,
            &req.target,
            &req.renames,
        ))
    }

    pub async fn apply_schema_diff(
        &self,
        req: ApplySchemaDiffRequest,
    ) -> ConnectorResult<ApplyDdlResult> {
        let meta = self.session_meta(&req.session_id).await?;
        if !meta.caps.supports_structure_edit {
            return Err(ConnectorError::NotSupported(
                "This engine cannot edit structure.".into(),
            ));
        }
        let migration = plan_schema_diff(dialect_of(&meta), &req.source, &req.target, &req.renames);
        if !migration.warnings.is_empty() {
            return Err(ConnectorError::Policy(migration.warnings.join(" ")));
        }
        let changes: Vec<_> = migration
            .changes
            .into_iter()
            .filter(|change| change.action != "unchanged" && change.plan.applyable())
            .collect();
        if changes.is_empty() {
            return Ok(ApplyDdlResult {
                applied: Vec::new(),
                message: "Nothing to change.".into(),
            });
        }
        let destructive = changes.iter().any(|change| {
            change.action == "drop"
                || change.draft.drop_table
                || change.draft.columns.iter().any(|column| column.drop)
        });
        let object = changes
            .iter()
            .find(|change| {
                change.action == "drop"
                    || change.draft.drop_table
                    || change.draft.columns.iter().any(|column| column.drop)
            })
            .map(|change| change.name.as_str())
            .unwrap_or("schema");
        self.enforce_write_guard(
            &req.session_id,
            if destructive {
                WriteRisk::Destructive
            } else {
                WriteRisk::Write
            },
            "Schema diff",
            Some(object),
            req.confirm_object_name.as_deref(),
            req.profile_read_only,
        )
        .await?;
        let mut applied = Vec::new();
        for change in &changes {
            let result = match meta.session.apply_ddl(&change.plan, &change.draft).await {
                Ok(result) => result,
                Err(err) if applied.is_empty() => return Err(err),
                Err(err) => {
                    return Err(ConnectorError::Query(format!(
                        "Applied {} statement(s) from earlier changes, then failed. {err}",
                        applied.len()
                    )));
                }
            };
            applied.extend(result.applied);
        }
        let count = applied.len();
        Ok(ApplyDdlResult {
            applied,
            message: format!("Applied {count} statement(s)."),
        })
    }
}

fn dialect_of(meta: &super::SessionMeta) -> DdlDialect {
    let engine = if meta.session.engine_id() == "mongodb" {
        "mongodb"
    } else {
        meta.product_id.as_str()
    };
    DdlDialect::from_engine(engine, &meta.caps.sql_dialect)
}
