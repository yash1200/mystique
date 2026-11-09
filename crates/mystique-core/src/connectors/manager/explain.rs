//! Explain one statement. Analyze of anything that is not a proven read
//! still cannot delete: plugins run it inside a read-only transaction.

use std::sync::Arc;

use mystique_connector_api::{
    clamp_timeout_ms, explain_rejection, gate_sql_batch, sql_analyze_guard_risk,
    sql_destructive_object_name_for, sql_explain_subject, sql_is_proven_read, ConnectorError,
    ConnectorResult, ExplainPlan, ExplainQueryRequest, QueryMode, SqlDialect, ValidatedQuery,
    WriteAction, WriteRisk, EXPLAIN_NOT_EXECUTED,
};

use super::guard::WriteCheck;
use super::ConnectionManager;

impl ConnectionManager {
    pub async fn explain_query(
        &self,
        mut req: ExplainQueryRequest,
    ) -> ConnectorResult<ExplainPlan> {
        let run = self.begin_run(&req.session_id, req.execution_id.take());
        self.explain_registered(req, &run.control).await
    }

    async fn explain_registered(
        &self,
        req: ExplainQueryRequest,
        control: &Arc<mystique_connector_api::QueryControl>,
    ) -> ConnectorResult<ExplainPlan> {
        let (session, live_read_only) = self.session_and_read_only(&req.session_id).await?;
        if !session.capabilities().supports_explain_plan {
            return Err(ConnectorError::NotSupported(
                "This engine has no explain plan.".into(),
            ));
        }
        let engine = session.engine_id().to_string();
        let read_only = live_read_only;
        let mode = if read_only { QueryMode::Read } else { req.mode };
        let timeout_ms = clamp_timeout_ms(req.timeout_ms);
        self.touch_session(&req.session_id);

        let prepared = if engine.eq_ignore_ascii_case("mongodb") {
            self.prepare_mongo_explain(&session, &req, mode).await?
        } else {
            prepare_sql_explain(&req, &engine, mode)?
        };

        self.enforce_write_guard(WriteCheck {
            preview: req.text.clone(),
            statements: vec![req.text.clone()],
            object_name: prepared.destructive_name.as_deref(),
            confirm_object_name: req.confirm_object_name.as_deref(),
            profile_read_only: read_only,
            ..WriteCheck::new(&req.session_id, WriteAction::Query, prepared.guard)
        })
        .await?;

        let validated = ValidatedQuery {
            text: prepared.subject,
            mode: QueryMode::Read,
            max_rows: 1,
            timeout_ms,
            collection: req.collection,
            database: req.database,
            control: Arc::clone(control),
            params: Vec::new(),
        };
        let result = self
            .drive_registered(
                control,
                timeout_ms,
                session.explain_query(&validated, req.analyze),
            )
            .await;
        finish_explain(result, req.analyze, prepared.proven_read)
    }

    async fn prepare_mongo_explain(
        &self,
        session: &Arc<dyn mystique_connector_api::ConnectorSession>,
        req: &ExplainQueryRequest,
        mode: QueryMode,
    ) -> ConnectorResult<PreparedExplain> {
        let engine = session.engine_id().to_string();
        let policy = self.registry.policy(&engine).ok_or_else(|| {
            ConnectorError::Internal(format!("no query policy registered for engine `{engine}`"))
        })?;
        policy.validate(&req.text, mode)?;
        let risk = policy.classify(&req.text);
        let proven_read = risk == WriteRisk::Read;
        let guard = if req.analyze && !proven_read {
            risk
        } else {
            WriteRisk::Read
        };
        Ok(PreparedExplain {
            subject: req.text.clone(),
            proven_read,
            guard,
            destructive_name: None,
        })
    }
}

struct PreparedExplain {
    subject: String,
    proven_read: bool,
    guard: WriteRisk,
    destructive_name: Option<String>,
}

fn prepare_sql_explain(
    req: &ExplainQueryRequest,
    engine: &str,
    mode: QueryMode,
) -> ConnectorResult<PreparedExplain> {
    let dialect = SqlDialect::from_engine(engine);
    let plan = gate_sql_batch(&req.text, dialect, mode)?;
    if plan.statements.len() != 1 {
        return Err(ConnectorError::Policy(
            "Explain one statement. Nothing ran.".into(),
        ));
    }
    let Some(stmt) = plan.statements.first() else {
        return Err(ConnectorError::Policy("Query is empty.".into()));
    };
    let subject = sql_explain_subject(&stmt.sql, dialect).map_err(ConnectorError::Query)?;
    if subject.is_empty() {
        return Err(ConnectorError::Policy("Query is empty.".into()));
    }
    let proven_read = sql_is_proven_read(&subject, dialect);
    let guard = if req.analyze && !proven_read {
        sql_analyze_guard_risk(&subject, dialect)
    } else {
        WriteRisk::Read
    };
    let destructive_name = if guard == WriteRisk::Destructive {
        sql_destructive_object_name_for(&subject, dialect)
    } else {
        None
    };
    Ok(PreparedExplain {
        subject,
        proven_read,
        guard,
        destructive_name,
    })
}

fn finish_explain(
    result: ConnectorResult<ExplainPlan>,
    analyze: bool,
    proven_read: bool,
) -> ConnectorResult<ExplainPlan> {
    if !(analyze && !proven_read) {
        return result;
    }
    match result {
        Ok(mut plan) => {
            plan.warnings.push(EXPLAIN_NOT_EXECUTED.to_string());
            Ok(plan)
        }
        Err(err) => explain_rejection(err),
    }
}
