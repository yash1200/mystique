use std::sync::Arc;

use mystique_connector_api::{
    plan_schema_diff_with, plan_table_alter, ApplyDdlRequest, ApplyDdlResult,
    ApplySchemaDiffRequest, ApplyTableEditsInput, ApplyTableEditsRequest, ApplyTableEditsResult,
    ConnectorError, ConnectorResult, DdlTarget, DescribeTableDefinitionRequest,
    DistinctColumnValuesRequest, FetchTablePreviewRequest, ListTableDefinitionsRequest,
    PlanDdlRequest, PlanSchemaDiffRequest, SchemaMigration, TableAdminOp, TableAdminRequest,
    TableAdminResult, TableDefinition, TablePreviewResult, WriteAction, WriteRisk,
    DEFAULT_MAX_ROWS, FETCH_TIMEOUT_MS, HARD_MAX_ROWS,
};

use super::depth::{reject_beyond_depth, DepthFeature};
use super::guard::WriteCheck;
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
        let session = self.session(&req.session_id).await?;
        let session_id = req.session_id.clone();
        let check = WriteCheck {
            preview: format!(
                "Save {} update(s), {} insert(s), {} delete(s) on {}",
                req.updates.len(),
                req.inserts.len(),
                req.deletes.len(),
                req.table
            ),
            confirm_object_name: req.confirm_object_name.as_deref(),
            database: req.database.as_deref(),
            schema: req.schema.as_deref(),
            ..WriteCheck::new(&session_id, WriteAction::GridSave, WriteRisk::Write)
        };
        let prompt = if req.dry_run {
            None
        } else {
            self.write_prompt(check).await?
        };
        let input = ApplyTableEditsInput::from(req);
        if let Some(mut prompt) = prompt {
            // The dialog shows the SQL this save would run.
            let plan = ApplyTableEditsInput {
                dry_run: true,
                ..input
            };
            if let Ok(planned) = session.apply_table_edits(&plan).await {
                prompt.statements = planned.statements;
            }
            return Err(ConnectorError::WriteGuard(Box::new(prompt)));
        }
        let started = std::time::Instant::now();
        let saved = session.apply_table_edits(&input).await;
        if !input.dry_run {
            let sql = match &saved {
                Ok(result) => result.statements.clone(),
                Err(_) => vec![format!("-- grid save on {}", input.table)],
            };
            self.log_action(super::history::HistoryAction {
                session_id: &session_id,
                kind: crate::domain::QueryHistoryKind::GridSave,
                sql,
                database: input.database.as_deref(),
                started,
                outcome: saved.as_ref().map(|result| {
                    Some(
                        u64::from(result.updated)
                            + u64::from(result.inserted)
                            + u64::from(result.deleted),
                    )
                }),
            })
            .await;
        }
        let mut result = saved?;
        // Values kept for the cell modal predate this save.
        self.large_values.forget_session(&session_id);
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
        let run = self.begin_run(&req.session_id, req.execution_id.clone());
        let control = &run.control;
        let session = self.session(&req.session_id).await?;
        let mut input = req.into_input(DEFAULT_MAX_ROWS, HARD_MAX_ROWS);
        input.control = Arc::clone(control);
        self.drive_registered(
            control,
            FETCH_TIMEOUT_MS,
            session.fetch_table_preview(&input),
        )
        .await
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
            self.enforce_write_guard(WriteCheck {
                preview: admin_preview(req.op, &req.name, req.new_name.as_deref()),
                object_name: Some(req.name.as_str()),
                confirm_object_name: req.confirm_object_name.as_deref(),
                database: req.database.as_deref(),
                schema: req.schema.as_deref(),
                ..WriteCheck::new(&req.session_id, WriteAction::ObjectAdmin, risk)
            })
            .await?;
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
        let session_id = req.session_id.clone();
        let input = req.into_input();
        let result = meta.session.apply_table_admin(&input).await;
        self.meta.invalidate(&session_id);
        result
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
        self.enforce_write_guard(WriteCheck {
            preview: format!("Structure change {}", req.draft.name),
            statements: plan.statements.clone(),
            object_name: Some(req.draft.name.as_str()),
            confirm_object_name: req.confirm_object_name.as_deref(),
            database: req.draft.database.as_deref(),
            schema: req.draft.schema.as_deref(),
            ..WriteCheck::new(
                &req.session_id,
                WriteAction::Structure,
                mystique_connector_api::ddl_risk(&req.draft),
            )
        })
        .await?;
        let started = std::time::Instant::now();
        let result = meta.session.apply_ddl(&plan, &req.draft).await;
        self.log_ddl(&req.session_id, &plan.statements, started, &result)
            .await;
        result
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
        Ok(plan_schema_diff_with(
            dialect_of(&meta),
            &req.source,
            &req.target,
            &req.renames,
            &req.selection,
        ))
    }

    pub async fn apply_schema_diff(
        &self,
        req: ApplySchemaDiffRequest,
    ) -> ConnectorResult<ApplyDdlResult> {
        let session_id = req.session_id.clone();
        let started = std::time::Instant::now();
        let mut planned = Vec::new();
        let result = self.apply_schema_diff_inner(req, &mut planned).await;
        if !planned.is_empty() {
            self.log_ddl(&session_id, &planned, started, &result).await;
        }
        result
    }

    /// A structure change or schema diff in history: what ran, or the plan
    /// when it failed part way.
    async fn log_ddl(
        &self,
        session_id: &str,
        planned: &[String],
        started: std::time::Instant,
        result: &ConnectorResult<ApplyDdlResult>,
    ) {
        // Applied or failed part way, the catalog may have changed.
        self.meta.invalidate(session_id);
        let sql = match result {
            Ok(done) if !done.applied.is_empty() => done.applied.clone(),
            Ok(_) => return,
            Err(_) => planned.to_vec(),
        };
        self.log_action(super::history::HistoryAction {
            session_id,
            kind: crate::domain::QueryHistoryKind::Ddl,
            sql,
            database: None,
            started,
            outcome: result.as_ref().map(|_| None),
        })
        .await;
    }

    /// `planned` gets the statements once the guard has passed.
    async fn apply_schema_diff_inner(
        &self,
        req: ApplySchemaDiffRequest,
        planned: &mut Vec<String>,
    ) -> ConnectorResult<ApplyDdlResult> {
        let meta = self.session_meta(&req.session_id).await?;
        if !meta.caps.supports_structure_edit {
            return Err(ConnectorError::NotSupported(
                "This engine cannot edit structure.".into(),
            ));
        }
        let migration = plan_schema_diff_with(
            dialect_of(&meta),
            &req.source,
            &req.target,
            &req.renames,
            &req.selection,
        );
        if !migration.warnings.is_empty() {
            return Err(ConnectorError::Policy(migration.warnings.join(" ")));
        }
        let changes: Vec<_> = migration
            .changes
            .into_iter()
            .filter(|change| {
                change.selected && change.action != "unchanged" && change.plan.applyable()
            })
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
        let risk = if destructive {
            WriteRisk::Destructive
        } else {
            WriteRisk::Write
        };
        let statements: Vec<String> = changes
            .iter()
            .flat_map(|change| change.plan.statements.iter().cloned())
            .collect();
        let targets = self
            .text_targets(&req.session_id, &statements.join(";\n"), None)
            .await;
        self.enforce_write_guard(WriteCheck {
            preview: "Schema diff".into(),
            statements,
            targets: Some(targets),
            object_name: Some(object),
            confirm_object_name: req.confirm_object_name.as_deref(),
            ..WriteCheck::new(&req.session_id, WriteAction::Structure, risk)
        })
        .await?;
        *planned = changes
            .iter()
            .flat_map(|change| change.plan.statements.iter().cloned())
            .collect();
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

fn dialect_of(meta: &super::SessionMeta) -> DdlTarget {
    let engine = if meta.session.engine_id() == "mongodb" {
        "mongodb"
    } else {
        meta.product_id.as_str()
    };
    DdlTarget::from_engine(engine, &meta.caps.sql_dialect)
}

/// What the guard dialog says for a menu action.
fn admin_preview(op: TableAdminOp, name: &str, new_name: Option<&str>) -> String {
    let new_name = new_name.map(str::trim).unwrap_or("");
    match op {
        TableAdminOp::Drop => format!("Drop {name}"),
        TableAdminOp::Truncate => format!("Delete every row in {name}"),
        TableAdminOp::Rename => format!("Rename {name} to {new_name}"),
        TableAdminOp::Duplicate => format!("Duplicate {name} as {new_name}"),
        TableAdminOp::CreateGroup | TableAdminOp::CreateObject => format!("Create {name}"),
        TableAdminOp::CreateStatement => format!("Show the CREATE statement for {name}"),
    }
}
