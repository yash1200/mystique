//! Mongo `explain` command. It does not apply writes.

use mongodb::bson::{doc, Bson, Document};
use mystique_connector_api::{
    ConnectorError, ConnectorResult, ExplainPlan, PlanField, PlanNode, ValidatedQuery,
};
use serde_json::Value;

use super::encode::{bson_to_json, resolve_db_name};
use super::query::drive;
use super::MongoSession;
use crate::shell_filter::json_to_document;
use crate::shell_query::{parse_aggregate, parse_read_query};

pub(super) async fn explain_query(
    session: &MongoSession,
    query: &ValidatedQuery,
    analyze: bool,
) -> ConnectorResult<ExplainPlan> {
    session.ensure_open()?;
    let text = query.text.trim();
    if let Some(aggregate) = parse_aggregate(text)? {
        let coll = collection_name(aggregate.collection.as_deref(), query)?;
        let db_name = resolve_db_name(session, query)?;
        let spec = doc! {
            "aggregate": coll,
            "pipeline": aggregate.pipeline,
            "cursor": {},
        };
        return run_explain(session, &db_name, spec, analyze, query, false).await;
    }
    if text.starts_with('{') {
        if let Ok(cmd) = serde_json::from_str::<serde_json::Value>(text) {
            if let Some(op) = cmd.get("op").and_then(Value::as_str) {
                let coll = collection_name(None, query)?;
                let db_name = resolve_db_name(session, query)?;
                let spec = write_spec(op, &coll, &cmd)?;
                return run_explain(session, &db_name, spec, analyze, query, true).await;
            }
        }
    }
    if mystique_connector_api::mongo_is_write_command(text) {
        return Err(ConnectorError::Query(
            "Explain a find, an aggregate, or a JSON write { op, filter }. The write was not run."
                .into(),
        ));
    }
    let parsed = parse_read_query(text)?;
    let coll = collection_name(parsed.collection.as_deref(), query)?;
    let db_name = resolve_db_name(session, query)?;
    let mut spec = doc! { "find": coll, "filter": parsed.filter };
    if let Some(projection) = parsed.projection {
        spec.insert("projection", projection);
    }
    if let Some(sort) = parsed.sort {
        spec.insert("sort", sort);
    }
    if let Some(limit) = parsed.limit {
        spec.insert("limit", limit);
    }
    if let Some(skip) = parsed.skip {
        spec.insert("skip", skip as i64);
    }
    run_explain(session, &db_name, spec, analyze, query, false).await
}

fn collection_name(parsed: Option<&str>, query: &ValidatedQuery) -> ConnectorResult<String> {
    parsed
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .map(str::to_string)
        .or_else(|| {
            query
                .collection
                .as_deref()
                .map(str::trim)
                .filter(|name| !name.is_empty())
                .map(str::to_string)
        })
        .ok_or_else(|| {
            ConnectorError::InvalidConfig(
                "Mongo explain needs a collection (db.collection.find(...) or the Collection field)."
                    .into(),
            )
        })
}

fn write_spec(op: &str, coll: &str, cmd: &Value) -> ConnectorResult<Document> {
    let op = op.to_ascii_lowercase();
    let filter = match cmd.get("filter") {
        Some(value) => json_to_document(value)?,
        None => Document::new(),
    };
    match op.as_str() {
        "deleteone" => Ok(doc! {
            "delete": coll,
            "deletes": [{ "q": filter, "limit": 1 }],
        }),
        "deletemany" | "remove" | "delete" => Ok(doc! {
            "delete": coll,
            "deletes": [{ "q": filter, "limit": 0 }],
        }),
        "updateone" | "updatemany" | "update" | "replaceone" => {
            let update = match cmd.get("update") {
                Some(value) => json_to_document(value)?,
                None => {
                    return Err(ConnectorError::Query(
                        "update explain requires `update`.".into(),
                    ))
                }
            };
            let limit = if op == "updatemany" { 0 } else { 1 };
            Ok(doc! {
                "update": coll,
                "updates": [{ "q": filter, "u": update, "limit": limit }],
            })
        }
        other => Err(ConnectorError::NotSupported(format!(
            "Mongo explain does not cover `{other}`. The write was not run."
        ))),
    }
}

async fn run_explain(
    session: &MongoSession,
    db_name: &str,
    spec: Document,
    analyze: bool,
    query: &ValidatedQuery,
    write: bool,
) -> ConnectorResult<ExplainPlan> {
    let verbosity = if analyze {
        "executionStats"
    } else {
        "queryPlanner"
    };
    let mut command = doc! {
        "explain": spec,
        "verbosity": verbosity,
    };
    // A run with no timeout sends no maxTimeMS.
    if query.timeout_ms != mystique_connector_api::NO_TIMEOUT {
        let max_time = i64::try_from(query.timeout_ms).unwrap_or(i64::MAX);
        command.insert("maxTimeMS", max_time);
    }
    let document = drive(
        &query.control,
        session.client.database(db_name).run_command(command),
    )
    .await?;
    let json = bson_to_json(Bson::Document(document));
    let raw = serde_json::to_string_pretty(&json).unwrap_or_else(|_| json.to_string());
    let mut warnings = Vec::new();
    if write {
        warnings.push("Mongo explain does not apply the write.".into());
    }
    Ok(ExplainPlan {
        raw,
        raw_format: "json".into(),
        root: plan_from_mongo(&json),
        warnings,
    })
}

pub(super) fn plan_from_mongo(value: &Value) -> PlanNode {
    let stage = value
        .pointer("/executionStats/executionStages")
        .or_else(|| value.pointer("/queryPlanner/winningPlan"));
    match stage {
        Some(stage) => mongo_stage(stage),
        None => PlanNode::leaf("explain"),
    }
}

fn mongo_stage(value: &Value) -> PlanNode {
    let name = value
        .get("stage")
        .and_then(Value::as_str)
        .unwrap_or("stage");
    let mut children = Vec::new();
    if let Some(input) = value.get("inputStage") {
        children.push(mongo_stage(input));
    }
    if let Some(inputs) = value.get("inputStages").and_then(Value::as_array) {
        for input in inputs {
            children.push(mongo_stage(input));
        }
    }
    let mut fields = Vec::new();
    if let Some(index) = value.get("indexName").and_then(Value::as_str) {
        fields.push(PlanField {
            label: "index".into(),
            value: index.to_string(),
        });
    }
    if let Some(time) = json_f64(value.get("executionTimeMillis")) {
        fields.push(PlanField {
            label: "time ms".into(),
            value: format_num(time),
        });
    }
    PlanNode {
        name: name.to_string(),
        rows: json_f64(value.get("nReturned")).or_else(|| json_f64(value.get("docsExamined"))),
        cost: None,
        fields,
        children,
    }
}

fn json_f64(value: Option<&Value>) -> Option<f64> {
    let value = value?;
    value
        .as_f64()
        .or_else(|| value.as_i64().map(|n| n as f64))
        .or_else(|| value.as_u64().map(|n| n as f64))
}

fn format_num(value: f64) -> String {
    if value.fract() == 0.0 {
        format!("{value:.0}")
    } else {
        format!("{value:.2}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_fetch_over_an_index_scan_is_a_tree() {
        let value = serde_json::json!({
            "queryPlanner": {
                "winningPlan": {
                    "stage": "FETCH",
                    "inputStage": {
                        "stage": "IXSCAN",
                        "indexName": "a_1",
                        "nReturned": 1
                    }
                }
            }
        });
        let root = plan_from_mongo(&value);
        assert_eq!(root.name, "FETCH");
        assert!(root.cost.is_none());
        let child = match root.children.first() {
            Some(child) => child,
            None => panic!("missing index scan"),
        };
        assert_eq!(child.name, "IXSCAN");
        assert_eq!(child.rows, Some(1.0));
        assert!(child.fields.iter().any(|field| field.value == "a_1"));
    }

    #[test]
    fn execution_stats_win_over_the_planner_when_both_exist() {
        let value = serde_json::json!({
            "queryPlanner": { "winningPlan": { "stage": "COLLSCAN" } },
            "executionStats": {
                "executionStages": {
                    "stage": "COLLSCAN",
                    "nReturned": 3,
                    "executionTimeMillis": 2
                }
            }
        });
        let root = plan_from_mongo(&value);
        assert_eq!(root.rows, Some(3.0));
        assert!(root.fields.iter().any(|field| field.label == "time ms"));
    }
}
