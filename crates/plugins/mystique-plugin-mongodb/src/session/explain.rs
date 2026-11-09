//! Mongo `explain` command. It does not apply writes.

use mongodb::bson::{doc, Bson, Document};
use mystique_connector_api::{
    ConnectorError, ConnectorResult, ExplainPlan, PlanField, PlanNode, ValidatedQuery,
};
use serde_json::Value;

use super::encode::{bson_to_json, resolve_db_name};
use super::query::drive;
use super::write::{write_batches, CommandFields};
use super::MongoSession;
use crate::shell::{
    parse_executable, AggregateSpec, Executable, FindAndModify, FindSpec, Modify, MongoOp,
    ReadOptions, ShellWrite, WriteMethod,
};

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
        MongoOp::Aggregate(aggregate) => (aggregate_spec(coll, aggregate), false),
        MongoOp::Find(find) => (find_spec(coll, find), false),
        MongoOp::Write(write) => (write_spec(&coll, write)?, true),
        MongoOp::Count(_) | MongoOp::EstimatedCount(_) | MongoOp::Distinct { .. } => {
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
    if let Some(size) = find.batch_size {
        spec.insert("batchSize", i64::from(size));
    }
    if let Some(allow) = find.allow_disk_use {
        spec.insert("allowDiskUse", allow);
    }
    read_fields(&mut spec, find.read);
    spec
}

fn aggregate_spec(coll: String, aggregate: AggregateSpec) -> Document {
    let mut spec = doc! {
        "aggregate": coll,
        "pipeline": aggregate.pipeline,
        "cursor": {},
    };
    if let Some(allow) = aggregate.allow_disk_use {
        spec.insert("allowDiskUse", allow);
    }
    if let Some(vars) = aggregate.let_vars {
        spec.insert("let", vars);
    }
    read_fields(&mut spec, aggregate.read);
    spec
}

/// `hint`, `collation` and `comment` as command fields. `maxTimeMS` goes on
/// the `explain` command itself.
fn read_fields(spec: &mut Document, read: ReadOptions) {
    if let Some(hint) = read.hint {
        spec.insert("hint", hint);
    }
    if let Some(collation) = read.collation {
        spec.insert("collation", collation);
    }
    if let Some(comment) = read.comment {
        spec.insert("comment", comment);
    }
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

/// The `update` / `delete` / `findAndModify` command the write would send,
/// without `writeConcern` or `maxTimeMS` (explain carries its own).
fn write_spec(coll: &str, write: ShellWrite) -> ConnectorResult<Document> {
    let not_explained = || {
        ConnectorError::NotSupported(
            "Mongo explain covers find, aggregate, update, replace and delete. The write was not run."
                .into(),
        )
    };
    match write {
        ShellWrite::Models {
            method: WriteMethod::InsertOne | WriteMethod::InsertMany | WriteMethod::BulkWrite,
            ..
        } => Err(not_explained()),
        ShellWrite::Models {
            models, options, ..
        } => {
            let mut batches = write_batches(coll, models, &options, &CommandFields::default());
            match (batches.pop(), batches.is_empty()) {
                (Some(batch), true) => {
                    let mut command = batch.command;
                    command.remove("ordered");
                    Ok(command)
                }
                _ => Err(not_explained()),
            }
        }
        ShellWrite::FindAndModify(spec) => Ok(find_and_modify_spec(coll, spec)),
    }
}

fn find_and_modify_spec(coll: &str, spec: FindAndModify) -> Document {
    let mut command = doc! { "findAndModify": coll, "query": spec.filter };
    match spec.modify {
        Modify::Update(update) | Modify::Replace(update) => {
            command.insert("update", update);
        }
        Modify::Delete => {
            command.insert("remove", true);
        }
    }
    if let Some(sort) = spec.sort {
        command.insert("sort", sort);
    }
    if let Some(fields) = spec.projection {
        command.insert("fields", fields);
    }
    if let Some(upsert) = spec.upsert {
        command.insert("upsert", upsert);
    }
    if let Some(after) = spec.return_after {
        command.insert("new", after);
    }
    if let Some(filters) = spec.array_filters {
        command.insert("arrayFilters", filters);
    }
    if let Some(collation) = spec.collation {
        command.insert("collation", collation);
    }
    if let Some(hint) = spec.hint {
        command.insert("hint", hint);
    }
    if let Some(vars) = spec.options.let_vars {
        command.insert("let", vars);
    }
    if let Some(bypass) = spec.options.bypass_document_validation {
        command.insert("bypassDocumentValidation", bypass);
    }
    if let Some(comment) = spec.options.comment {
        command.insert("comment", comment);
    }
    command
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

    fn write_of(text: &str) -> ShellWrite {
        match parse_executable(text) {
            Ok(Executable::Call(call)) => match call.op {
                MongoOp::Write(write) => write,
                other => panic!("{other:?}"),
            },
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn a_write_explains_as_the_command_it_sends() {
        let spec = write_spec(
            "t",
            write_of(
                r#"db.t.updateOne({a: 1}, {$set: {b: 1}}, {upsert: true, hint: "a_1", maxTimeMS: 5})"#,
            ),
        );
        assert_eq!(
            spec.ok(),
            Some(doc! {
                "update": "t",
                "updates": [{ "q": { "a": 1 }, "u": { "$set": { "b": 1 } }, "multi": false, "upsert": true, "hint": "a_1" }],
            }),
            "explain carries its own maxTimeMS"
        );
        let spec = write_spec(
            "t",
            write_of(
                r#"db.t.findOneAndUpdate({a: 1}, {$inc: {n: 1}}, {upsert: true, returnDocument: "after", sort: {n: 1}})"#,
            ),
        );
        assert_eq!(
            spec.ok(),
            Some(doc! {
                "findAndModify": "t",
                "query": { "a": 1 },
                "update": { "$inc": { "n": 1 } },
                "sort": { "n": 1 },
                "upsert": true,
                "new": true,
            })
        );
        assert!(write_spec("t", write_of("db.t.insertOne({a: 1})")).is_err());
        let delete = write_spec("t", write_of("db.t.findOneAndDelete({a: 1})")).ok();
        assert_eq!(delete.and_then(|d| d.get_bool("remove").ok()), Some(true));
    }

    #[test]
    fn find_explain_keeps_hint_and_collation() {
        let MongoOp::Find(find) =
            (match parse_executable(r#"db.t.find({a: 1}).hint("a_1").collation({locale: "en"})"#) {
                Ok(Executable::Call(call)) => call.op,
                other => panic!("{other:?}"),
            })
        else {
            panic!("find");
        };
        let spec = find_spec("t".into(), find);
        assert_eq!(spec.get_str("hint").ok(), Some("a_1"));
        assert_eq!(
            spec.get_document("collation").ok(),
            Some(&doc! { "locale": "en" })
        );
    }

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
