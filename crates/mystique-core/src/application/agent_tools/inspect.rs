//! Read-level inspection tools (#144): `sample_rows`, `explain_query`,
//! `infer_fields`. None of them writes. Each checks `read` access on the
//! connection and database first, so a hidden connection stays hidden.

use mystique_connector_api::{
    runtime_id, sql_explain_executes, sql_explain_subject, ExplainPlan, ExplainQueryRequest,
    FetchTablePreviewRequest, FieldInference, InferFieldsRequest, PlanField, PlanNode, QueryMode,
    SqlDialect, WriteRisk,
};
use serde::Serialize;
use serde_json::Value as JsonValue;

use crate::domain::ConnectionProfile;
use crate::error::{AppError, AppResult};

use super::download::window_cell_json;
use super::policy::ToolCaller;
use super::types::AgentCellWindow;
use super::AgentTools;

/// Rows `sample_rows` returns at most.
pub const AGENT_SAMPLE_MAX_ROWS: u32 = 20;
/// Rows `sample_rows` returns when the call does not say.
pub const AGENT_SAMPLE_DEFAULT_ROWS: u32 = 10;
/// Plan nodes returned before the tree is cut (`planTruncated`).
pub const AGENT_PLAN_MAX_NODES: usize = 200;
/// Characters of one plan node name or field value.
const PLAN_TEXT_CHARS: usize = 200;
/// Characters of raw plan text returned when the call does not say.
pub const AGENT_RAW_DEFAULT_CHARS: u32 = 2_000;

/// A few rows of one table or collection, cells windowed.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentSample {
    pub engine: String,
    pub object: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub schema: Option<String>,
    pub columns: Vec<AgentSampleColumn>,
    pub rows: Vec<Vec<JsonValue>>,
    pub row_count: u64,
    /// More rows exist past this sample.
    pub truncated: bool,
    pub duration_ms: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentSampleColumn {
    pub name: String,
    pub data_type: String,
}

/// A plan: the normalised tree plus the engine's own text, windowed.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentExplain {
    pub engine: String,
    /// True only for a proven read run with ANALYZE.
    pub analyzed: bool,
    pub plan: PlanNode,
    /// The tree had more than [`AGENT_PLAN_MAX_NODES`] nodes.
    pub plan_truncated: bool,
    /// Whole text, or `{ text, next_offset }` / `{ text, end }`.
    pub raw: JsonValue,
    pub raw_format: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<String>,
}

/// `postgres`, `mysql`, `mongodb`… for a profile's product id.
fn runtime(profile: &ConnectionProfile) -> &str {
    runtime_id(&profile.engine).unwrap_or(profile.engine.as_str())
}

fn is_document_store(profile: &ConnectionProfile) -> bool {
    runtime(profile).eq_ignore_ascii_case("mongodb")
}

fn trimmed(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}

/// `main.boards` → (`main`, `boards`) on SQL engines when no schema is given.
/// A Mongo collection name may contain dots, so it is never split.
fn split_object(
    profile: &ConnectionProfile,
    object: &str,
    schema: Option<&str>,
) -> (String, Option<String>) {
    let object = object.trim();
    if let Some(schema) = trimmed(schema) {
        return (object.to_string(), Some(schema));
    }
    if !is_document_store(profile) {
        if let Some((schema, name)) = object.split_once('.') {
            let (schema, name) = (unquote(schema), unquote(name));
            if !schema.is_empty() && !name.is_empty() {
                return (name, Some(schema));
            }
        }
    }
    (object.to_string(), None)
}

fn unquote(part: &str) -> String {
    part.trim()
        .trim_matches(|c| c == '"' || c == '`' || c == '[' || c == ']')
        .to_string()
}

/// `text…` past `max` characters.
fn clip(text: String, max: usize) -> String {
    if text.chars().count() <= max {
        return text;
    }
    let mut out: String = text.chars().take(max).collect();
    out.push('…');
    out
}

/// Breadth-limited copy of a plan: at most `budget` nodes, texts clipped.
fn cap_plan(node: &PlanNode, budget: &mut usize, cut: &mut bool) -> PlanNode {
    *budget = budget.saturating_sub(1);
    let mut children = Vec::new();
    for child in &node.children {
        if *budget == 0 {
            *cut = true;
            break;
        }
        children.push(cap_plan(child, budget, cut));
    }
    PlanNode {
        name: clip(node.name.clone(), PLAN_TEXT_CHARS),
        rows: node.rows,
        cost: node.cost,
        fields: node
            .fields
            .iter()
            .map(|f| PlanField {
                label: clip(f.label.clone(), PLAN_TEXT_CHARS),
                value: clip(f.value.clone(), PLAN_TEXT_CHARS),
            })
            .collect(),
        children,
    }
}

pub(super) fn agent_explain(
    engine: &str,
    analyzed: bool,
    plan: ExplainPlan,
    raw_window: AgentCellWindow,
) -> AgentExplain {
    let mut budget = AGENT_PLAN_MAX_NODES;
    let mut cut = false;
    let root = cap_plan(&plan.root, &mut budget, &mut cut);
    AgentExplain {
        engine: engine.to_string(),
        analyzed,
        plan: root,
        plan_truncated: cut,
        raw: window_cell_json(&JsonValue::String(plan.raw), raw_window),
        raw_format: plan.raw_format,
        warnings: plan.warnings,
    }
}

impl AgentTools {
    /// At most [`AGENT_SAMPLE_MAX_ROWS`] rows through the table preview path
    /// (no user SQL), every cell windowed like a query result.
    // Mirrors the tool's arguments one to one (MCP and the in-app agent).
    #[allow(clippy::too_many_arguments)]
    pub async fn sample_rows(
        &self,
        connection_id: &str,
        object: &str,
        schema: Option<&str>,
        database: Option<&str>,
        limit: Option<u32>,
        window: AgentCellWindow,
        caller: ToolCaller,
    ) -> AppResult<AgentSample> {
        if object.trim().is_empty() {
            return Err(AppError::Invalid("`object` is required".into()));
        }
        let profile = self.require_read(caller, connection_id, database)?;
        let (table, schema) = split_object(&profile, object, schema);
        let limit = limit
            .unwrap_or(AGENT_SAMPLE_DEFAULT_ROWS)
            .clamp(1, AGENT_SAMPLE_MAX_ROWS);
        let sid = self.ensure_session_for(connection_id, caller).await?;
        let preview = self
            .connections
            .fetch_table_preview(FetchTablePreviewRequest {
                session_id: sid,
                table: table.clone(),
                schema: schema.clone(),
                database: trimmed(database),
                max_rows: Some(limit),
                offset: None,
                filters: Vec::new(),
                code_filter: None,
                sort_column: None,
                sort_dir: None,
                execution_id: None,
            })
            .await?;
        // A plugin that returns more than asked is cut here, not trusted.
        let rows: Vec<Vec<JsonValue>> = preview
            .rows
            .iter()
            .take(limit as usize)
            .map(|row| row.iter().map(|c| window_cell_json(c, window)).collect())
            .collect();
        Ok(AgentSample {
            engine: preview.engine,
            object: table,
            schema,
            columns: preview
                .columns
                .into_iter()
                .map(|c| AgentSampleColumn {
                    name: c.name,
                    data_type: c.data_type,
                })
                .collect(),
            row_count: rows.len() as u64,
            truncated: preview.truncated || preview.rows.len() > rows.len(),
            rows,
            duration_ms: preview.duration_ms,
        })
    }

    /// The plan for one statement, never its rows. Plain EXPLAIN plans
    /// without running, so any statement may be explained at `read`.
    /// ANALYZE runs the statement, so it is allowed only when ReadProof
    /// proves the statement only reads; a write is refused before anything
    /// reaches the database.
    // Mirrors the tool's arguments one to one (MCP and the in-app agent).
    #[allow(clippy::too_many_arguments)]
    pub async fn explain_query(
        &self,
        connection_id: &str,
        sql: &str,
        params: Vec<JsonValue>,
        database: Option<&str>,
        collection: Option<&str>,
        analyze: bool,
        raw_window: AgentCellWindow,
        caller: ToolCaller,
    ) -> AppResult<AgentExplain> {
        let text = sql.trim();
        if text.is_empty() {
            return Err(AppError::Invalid("`sql` is required".into()));
        }
        let profile = self.require_read(caller, connection_id, database)?;
        let engine = runtime(&profile).to_string();
        // SQL: plan the inner statement of `EXPLAIN …` text. Text that asks
        // for ANALYZE itself counts as asking for it.
        let (subject, analyze) = if is_document_store(&profile) {
            (text.to_string(), analyze)
        } else {
            let dialect = SqlDialect::from_engine(&engine);
            let subject = sql_explain_subject(text, dialect).unwrap_or_else(|_| text.to_string());
            (subject, analyze || sql_explain_executes(text, dialect))
        };
        if analyze && self.assess(&profile.engine, &subject).risk != WriteRisk::Read {
            return Err(AppError::AccessDenied(
                "explain_query runs ANALYZE only for statements Mystique can prove are reads, because ANALYZE executes the statement. Nothing ran. Call explain_query again without analyze: a plain explain plans the statement without running it.".into(),
            ));
        }
        let sid = self.ensure_session_for(connection_id, caller).await?;
        let plan = self
            .connections
            .explain_query(ExplainQueryRequest {
                session_id: sid,
                text: subject,
                analyze,
                // Read: an ANALYZE runs under the Read-mode gate. A plain
                // explain is planned only, whatever the statement is.
                mode: QueryMode::Read,
                timeout_ms: Some(30_000),
                confirm_object_name: None,
                collection: trimmed(collection),
                database: trimmed(database),
                execution_id: None,
                params,
            })
            .await?;
        Ok(agent_explain(&engine, analyze, plan, raw_window))
    }

    /// Field paths and types from a bounded sample of a MongoDB or
    /// DocumentDB collection. The plugin runs one fixed `$sample` stage.
    pub async fn infer_fields(
        &self,
        connection_id: &str,
        collection: &str,
        database: Option<&str>,
        sample: Option<u32>,
        caller: ToolCaller,
    ) -> AppResult<FieldInference> {
        let collection = collection.trim();
        if collection.is_empty() {
            return Err(AppError::Invalid("`collection` is required".into()));
        }
        let profile = self.require_read(caller, connection_id, database)?;
        if !is_document_store(&profile) {
            return Err(AppError::Invalid(format!(
                "infer_fields works on MongoDB and DocumentDB collections, and \"{}\" is a {} connection. Use describe_object for a table's columns, or sample_rows to see a few rows.",
                profile.name, profile.engine
            )));
        }
        let sid = self.ensure_session_for(connection_id, caller).await?;
        Ok(self
            .connections
            .infer_fields(InferFieldsRequest {
                session_id: sid,
                collection: collection.to_string(),
                database: trimmed(database),
                sample,
            })
            .await?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn node(name: &str, children: Vec<PlanNode>) -> PlanNode {
        PlanNode {
            children,
            ..PlanNode::leaf(name)
        }
    }

    #[test]
    fn a_huge_plan_is_cut_at_the_node_cap() {
        let wide = node(
            "root",
            (0..500)
                .map(|i| node(&format!("n{i}"), Vec::new()))
                .collect(),
        );
        let plan = ExplainPlan {
            raw: "x".repeat(10_000),
            raw_format: "text".into(),
            root: wide,
            warnings: Vec::new(),
        };
        let out = agent_explain(
            "postgres",
            false,
            plan,
            AgentCellWindow::from_options(None, Some(u64::from(AGENT_RAW_DEFAULT_CHARS))),
        );
        assert!(out.plan_truncated);
        assert_eq!(out.plan.children.len(), AGENT_PLAN_MAX_NODES - 1);
        assert_eq!(
            out.raw["text"].as_str().map(|s| s.chars().count()),
            Some(AGENT_RAW_DEFAULT_CHARS as usize)
        );
        assert_eq!(
            out.raw["next_offset"],
            serde_json::json!(AGENT_RAW_DEFAULT_CHARS)
        );
    }

    #[test]
    fn long_plan_text_is_clipped() {
        let mut root = PlanNode::leaf("x".repeat(1_000));
        root.fields.push(PlanField {
            label: "Filter".into(),
            value: "y".repeat(1_000),
        });
        let mut budget = AGENT_PLAN_MAX_NODES;
        let mut cut = false;
        let out = cap_plan(&root, &mut budget, &mut cut);
        assert_eq!(out.name.chars().count(), PLAN_TEXT_CHARS + 1);
        assert_eq!(out.fields[0].value.chars().count(), PLAN_TEXT_CHARS + 1);
        assert!(!cut);
    }
}
