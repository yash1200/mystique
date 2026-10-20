//! `EXPLAIN (FORMAT JSON)` and `EXPLAIN (ANALYZE, FORMAT JSON)`.
//!
//! Both run inside `BEGIN READ ONLY`, so analyze of a write fails closed.

use mystique_connector_api::{
    ConnectorError, ConnectorResult, ExplainPlan, PlanField, PlanNode, ValidatedQuery,
};
use serde_json::Value;
use tokio_postgres::SimpleQueryMessage;

use super::query::end_read_only_txn;
use super::query_failed;
use super::PostgresSession;

pub(super) async fn explain_query(
    session: &PostgresSession,
    query: &ValidatedQuery,
    analyze: bool,
) -> ConnectorResult<ExplainPlan> {
    session.ensure_open()?;
    let client = session.client().await?;
    let _guard = session.arm_cancel(&client, &query.control);
    let timeout = query.timeout_ms.max(1);
    client
        .batch_execute("BEGIN READ ONLY")
        .await
        .map_err(query_failed)?;
    if let Err(err) = client
        .batch_execute(&format!("SET LOCAL statement_timeout = {timeout}"))
        .await
    {
        return end_read_only_txn(client, Err(query_failed(err))).await;
    }
    let sql = postgres_explain_sql(&query.text, analyze);
    let fetched = fetch_text(&client, &sql).await;
    let raw = end_read_only_txn(client, fetched).await?;
    parse_postgres_explain(&raw)
}

pub(super) fn postgres_explain_sql(subject: &str, analyze: bool) -> String {
    let subject = subject.trim().trim_end_matches(';').trim();
    if analyze {
        format!("EXPLAIN (ANALYZE, FORMAT JSON) {subject}")
    } else {
        format!("EXPLAIN (FORMAT JSON) {subject}")
    }
}

async fn fetch_text(client: &deadpool_postgres::Object, sql: &str) -> ConnectorResult<String> {
    let messages = client.simple_query(sql).await.map_err(query_failed)?;
    let mut lines = Vec::new();
    for message in messages {
        let SimpleQueryMessage::Row(row) = message else {
            continue;
        };
        let mut cells = Vec::new();
        // `get` panics past the last column. `None` is SQL NULL, not the end.
        // EXPLAIN (FORMAT JSON) is one column, so the old loop died on column 1.
        for index in 0..row.len() {
            match row.try_get(index) {
                Ok(Some(cell)) => cells.push(cell.to_string()),
                Ok(None) => cells.push(String::new()),
                Err(err) => return Err(query_failed(err)),
            }
        }
        if !cells.is_empty() {
            lines.push(cells.join("\t"));
        }
    }
    if lines.is_empty() {
        return Err(ConnectorError::Query("Explain returned no plan.".into()));
    }
    Ok(lines.join("\n"))
}

pub(super) fn parse_postgres_explain(raw: &str) -> ConnectorResult<ExplainPlan> {
    let value: Value = serde_json::from_str(raw.trim())
        .map_err(|err| ConnectorError::Query(format!("Postgres explain was not JSON: {err}")))?;
    let nodes = postgres_roots(&value);
    let root = single_root("Query", nodes);
    Ok(ExplainPlan {
        raw: raw.to_string(),
        raw_format: "json".into(),
        root,
        warnings: Vec::new(),
    })
}

fn postgres_roots(value: &Value) -> Vec<PlanNode> {
    let list = value
        .as_array()
        .or_else(|| value.get("QUERY PLAN").and_then(Value::as_array));
    let Some(list) = list else {
        return Vec::new();
    };
    list.iter()
        .filter_map(|item| item.get("Plan").map(postgres_node))
        .collect()
}

fn postgres_node(value: &Value) -> PlanNode {
    let kind = value
        .get("Node Type")
        .and_then(Value::as_str)
        .unwrap_or("Plan");
    let relation = value.get("Relation Name").and_then(Value::as_str);
    let name = match relation {
        Some(relation) => format!("{kind} on {relation}"),
        None => kind.to_string(),
    };
    let mut fields = Vec::new();
    if let Some(index) = value.get("Index Name").and_then(Value::as_str) {
        fields.push(PlanField {
            label: "index".into(),
            value: index.to_string(),
        });
    }
    if let Some(filter) = value.get("Filter").and_then(Value::as_str) {
        fields.push(PlanField {
            label: "filter".into(),
            value: clip(filter),
        });
    }
    if let Some(time) = json_f64(value.get("Actual Total Time")) {
        fields.push(PlanField {
            label: "actual time ms".into(),
            value: format_num(time),
        });
    }
    let children = value
        .get("Plans")
        .and_then(Value::as_array)
        .map(|plans| plans.iter().map(postgres_node).collect())
        .unwrap_or_default();
    PlanNode {
        name,
        rows: json_f64(value.get("Actual Rows")).or_else(|| json_f64(value.get("Plan Rows"))),
        cost: json_f64(value.get("Total Cost")),
        fields,
        children,
    }
}

fn single_root(name: &str, mut nodes: Vec<PlanNode>) -> PlanNode {
    if nodes.len() == 1 {
        return nodes.pop().unwrap_or_else(|| PlanNode::leaf(name));
    }
    if nodes.is_empty() {
        return PlanNode::leaf(name);
    }
    PlanNode {
        name: name.into(),
        rows: None,
        cost: None,
        fields: Vec::new(),
        children: nodes,
    }
}

fn json_f64(value: Option<&Value>) -> Option<f64> {
    let value = value?;
    match value {
        Value::Number(number) => number.as_f64(),
        Value::String(text) => text.parse().ok(),
        _ => None,
    }
}

fn format_num(value: f64) -> String {
    if value.fract() == 0.0 {
        format!("{value:.0}")
    } else {
        format!("{value:.2}")
    }
}

fn clip(text: &str) -> String {
    let mut out = String::new();
    for (index, ch) in text.chars().enumerate() {
        if index == 160 {
            out.push('…');
            break;
        }
        out.push(ch);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_seq_scan_keeps_cost_and_its_child() {
        let raw = r#"[{
            "Plan": {
                "Node Type": "Limit",
                "Startup Cost": 0.00,
                "Total Cost": 0.03,
                "Plan Rows": 1,
                "Plans": [{
                    "Node Type": "Seq Scan",
                    "Relation Name": "t",
                    "Alias": "t",
                    "Startup Cost": 0.00,
                    "Total Cost": 1.10,
                    "Plan Rows": 4,
                    "Filter": "(a = 1)"
                }]
            }
        }]"#;
        let plan = match parse_postgres_explain(raw) {
            Ok(plan) => plan,
            Err(err) => panic!("{err}"),
        };
        assert_eq!(plan.root.name, "Limit");
        assert_eq!(plan.root.cost, Some(0.03));
        assert_eq!(plan.root.rows, Some(1.0));
        let Some(child) = plan.root.children.first() else {
            panic!("missing child");
        };
        assert_eq!(child.name, "Seq Scan on t");
        assert_eq!(child.cost, Some(1.10));
        assert!(child
            .fields
            .iter()
            .any(|field| field.label == "filter" && field.value.contains("a = 1")));
        assert_eq!(plan.raw_format, "json");
        assert!(plan.raw.contains("Seq Scan"));
    }

    #[test]
    fn analyze_keeps_the_measured_rows_and_time() {
        let raw = r#"[{
            "Plan": {
                "Node Type": "Seq Scan",
                "Relation Name": "authors",
                "Schema": "content",
                "Alias": "authors",
                "Plan Rows": 1,
                "Actual Rows": 12,
                "Actual Total Time": 0.42,
                "Total Cost": 1.50
            }
        }]"#;
        let plan = match parse_postgres_explain(raw) {
            Ok(plan) => plan,
            Err(err) => panic!("{err}"),
        };
        assert_eq!(plan.root.name, "Seq Scan on authors");
        assert_eq!(plan.root.rows, Some(12.0));
        assert!(plan
            .root
            .fields
            .iter()
            .any(|field| { field.label == "actual time ms" && field.value == "0.42" }));
    }

    #[test]
    fn analyze_sql_names_analyze_and_plain_sql_does_not() {
        let plain = postgres_explain_sql("SELECT 1", false);
        let analyze = postgres_explain_sql("SELECT 1;", true);
        assert_eq!(plain, "EXPLAIN (FORMAT JSON) SELECT 1");
        assert_eq!(analyze, "EXPLAIN (ANALYZE, FORMAT JSON) SELECT 1");
    }
}
