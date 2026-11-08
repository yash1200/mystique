//! Mongo `explain` command. It does not apply writes.

use mongodb::bson::{doc, Bson, Document};
use mystique_connector_api::{
    ConnectorError, ConnectorResult, ExplainPlan, PlanField, PlanNode, ValidatedQuery,
};
use serde_json::Value;

use super::encode::{bson_to_json, resolve_db_name};
use super::query::drive;
use super::MongoSession;
use crate::shell::{parse_executable, Executable, FindSpec, MongoOp, ShellWrite};

/// Explain the same parsed call the executor would run. Writes are sent as
/// the `delete` / `update` / `findAndModify` command inside `explain`, which
/// plans them without applying them.
pub(super) async fn explain_query(
    session: &MongoSession,
    query: &ValidatedQuery,
    analyze: bool,
) -> ConnectorResult<ExplainPlan> {
    session.ensure_open()?;
    let call = match parse_executable(&query.text)? {
        Executable::Call(call) => call,
        Executable::Listing(listing) => {
            return Err(ConnectorError::Query(format!(
                "`{}` has no query plan. Explain a find, an aggregate, or a write.",
                listing.command()
            )))
        }
    };
    let coll = collection_name(call.target.collection.as_deref(), query)?;
    let db_name = match call.target.database {
        Some(db) => db,
        None => resolve_db_name(session, query)?,
    };
    let (spec, write) = match call.op {
        MongoOp::Aggregate(pipeline) => (
            doc! {
                "aggregate": coll,
                "pipeline": pipeline,
                "cursor": {},
            },
            false,
        ),
        MongoOp::Find(find) => (find_spec(coll, find), false),
        MongoOp::Write(write) => (write_spec(&coll, write)?, true),
        MongoOp::Count(_) | MongoOp::Distinct { .. } => {
            return Err(ConnectorError::Query(
                "Explain a find, an aggregate, or a write. Nothing ran.".into(),
            ))
        }
    };
    run_explain(session, &db_name, spec, analyze, query, write).await
}

fn find_spec(coll: String, find: FindSpec) -> Document {
    let mut spec = doc! { "find": coll, "filter": find.filter };
    if let Some(projection) = find.projection {
        spec.insert("projection", projection);
    }
    if let Some(sort) = find.sort {
        spec.insert("sort", sort);
    }
    if let Some(limit) = find.limit {
        spec.insert("limit", limit);
    }
    if let Some(skip) = find.skip {
        spec.insert("skip", skip as i64);
    }
    spec
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

fn write_spec(coll: &str, write: ShellWrite) -> ConnectorResult<Document> {
    let update = |q: Document, u: Document, multi: bool| {
        doc! {
            "update": coll,
            "updates": [{ "q": q, "u": u, "multi": multi }],
        }
    };
    match write {
        ShellWrite::DeleteOne(filter) => Ok(doc! {
            "delete": coll,
            "deletes": [{ "q": filter, "limit": 1 }],
        }),
        ShellWrite::DeleteMany(filter) => Ok(doc! {
            "delete": coll,
            "deletes": [{ "q": filter, "limit": 0 }],
        }),
        ShellWrite::UpdateOne { filter, update: u } => Ok(update(filter, u, false)),
        ShellWrite::UpdateMany { filter, update: u } => Ok(update(filter, u, true)),
        ShellWrite::ReplaceOne {
            filter,
            replacement,
        } => Ok(update(filter, replacement, false)),
        ShellWrite::FindOneAndUpdate { filter, update: u } => Ok(doc! {
            "findAndModify": coll,
            "query": filter,
            "update": u,
        }),
        ShellWrite::InsertOne(_) | ShellWrite::InsertMany(_) | ShellWrite::Bulk(_) => {
            Err(ConnectorError::NotSupported(
                "Mongo explain covers find, aggregate, update, replace and delete. The write was not run."
                    .into(),
            ))
        }
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
