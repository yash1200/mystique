//! `EXPLAIN FORMAT=JSON` and `EXPLAIN ANALYZE`.
//!
//! Both run inside `START TRANSACTION READ ONLY`. Analyze of a write fails
//! closed on MySQL 8. Plain explain does not execute the statement.

use mysql_async::prelude::Queryable;
use mystique_connector_api::{
    ConnectorError, ConnectorResult, ExplainPlan, PlanField, PlanNode, QueryControl, ValidatedQuery,
};
use serde_json::{Map, Value};

use super::cancel::{apply_timeouts, arm_kill, clear_timeouts};
use super::query::end_read_only_txn;
use super::query_failed;
use super::MysqlSession;

pub(super) async fn explain_query(
    session: &MysqlSession,
    query: &ValidatedQuery,
    analyze: bool,
) -> ConnectorResult<ExplainPlan> {
    session.ensure_open()?;
    let mut conn = session.conn().await?;
    arm_kill(&session.pool(), conn.id(), &query.control);
    apply_timeouts(&mut conn, query.timeout_ms).await;
    if let Err(err) = conn.query_drop("START TRANSACTION READ ONLY").await {
        clear_timeouts(&mut conn).await;
        return Err(map_mysql(&query.control, err));
    }
    let sql = mysql_explain_sql(&query.text, analyze);
    let fetched = fetch_text(&mut conn, &sql, &query.control).await;
    let raw = end_read_only_txn(conn, fetched).await?;
    if analyze {
        parse_mysql_analyze(&raw)
    } else {
        parse_mysql_json(&raw)
    }
}

pub(super) fn mysql_explain_sql(subject: &str, analyze: bool) -> String {
    let subject = subject.trim().trim_end_matches(';').trim();
    if analyze {
        format!("EXPLAIN ANALYZE {subject}")
    } else {
        format!("EXPLAIN FORMAT=JSON {subject}")
    }
}

async fn fetch_text(
    conn: &mut mysql_async::Conn,
    sql: &str,
    control: &QueryControl,
) -> ConnectorResult<String> {
    let rows: Vec<mysql_async::Row> = conn
        .query(sql)
        .await
        .map_err(|err| map_mysql(control, err))?;
    let mut lines = Vec::new();
    for row in &rows {
        let mut cells = Vec::new();
        let mut index = 0;
        while let Some(cell) = cell_string(row, index) {
            cells.push(cell);
            index += 1;
            if index > 16 {
                break;
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

fn cell_string(row: &mysql_async::Row, index: usize) -> Option<String> {
    if let Some(Ok(value)) = row.get_opt::<Option<String>, usize>(index) {
        return value;
    }
    if let Some(Ok(bytes)) = row.get_opt::<Vec<u8>, usize>(index) {
        return Some(String::from_utf8_lossy(&bytes).into_owned());
    }
    None
}

fn map_mysql(control: &QueryControl, err: mysql_async::Error) -> ConnectorError {
    if control.is_cancelled() {
        ConnectorError::Query("Query cancelled.".into())
    } else {
        query_failed(err)
    }
}

pub(super) fn parse_mysql_json(raw: &str) -> ConnectorResult<ExplainPlan> {
    let value: Value = serde_json::from_str(raw.trim())
        .map_err(|err| ConnectorError::Query(format!("MySQL explain was not JSON: {err}")))?;
    let nodes = if let Some(block) = value.get("query_block") {
        walk_mysql(block, "query_block")
    } else {
        walk_mysql(&value, "plan")
    };
    Ok(ExplainPlan {
        raw: raw.to_string(),
        raw_format: "json".into(),
        root: single_root("Query", nodes),
        warnings: Vec::new(),
    })
}

pub(super) fn parse_mysql_analyze(raw: &str) -> ConnectorResult<ExplainPlan> {
    let root = mysql_analyze_tree(raw);
    Ok(ExplainPlan {
        raw: raw.to_string(),
        raw_format: "text".into(),
        root,
        warnings: Vec::new(),
    })
}

fn walk_mysql(value: &Value, key: &str) -> Vec<PlanNode> {
    match value {
        Value::Array(items) => items
            .iter()
            .flat_map(|item| walk_mysql(item, key))
            .collect(),
        Value::Object(map) => vec![mysql_object(key, map)],
        _ => Vec::new(),
    }
}

fn mysql_object(key: &str, map: &Map<String, Value>) -> PlanNode {
    let mut fields = Vec::new();
    let mut children = Vec::new();
    if let Some(message) = map.get("message").and_then(Value::as_str) {
        fields.push(PlanField {
            label: "message".into(),
            value: message.to_string(),
        });
    }
    if let Some(index) = map
        .get("key")
        .and_then(Value::as_str)
        .or_else(|| map.get("possible_keys").and_then(Value::as_str))
    {
        fields.push(PlanField {
            label: "index".into(),
            value: index.to_string(),
        });
    }
    for (child_key, child) in map {
        if matches!(
            child_key.as_str(),
            "cost_info"
                | "table_name"
                | "access_type"
                | "select_id"
                | "message"
                | "key"
                | "possible_keys"
        ) {
            continue;
        }
        if child.is_object() || child.is_array() {
            children.extend(walk_mysql(child, child_key));
        }
    }
    PlanNode {
        name: mysql_name(key, map),
        rows: first_number(
            map,
            &["rows_examined_per_scan", "rows_produced_per_join", "rows"],
        ),
        cost: mysql_cost(map),
        fields,
        children,
    }
}

fn mysql_name(key: &str, map: &Map<String, Value>) -> String {
    if let Some(table) = map.get("table_name").and_then(Value::as_str) {
        let access = map
            .get("access_type")
            .and_then(Value::as_str)
            .unwrap_or("table");
        let access = match access {
            "ALL" => "table scan",
            "index" => "index scan",
            "range" => "range",
            "ref" => "ref",
            "eq_ref" => "eq_ref",
            "const" | "system" => "const",
            other => other,
        };
        return format!("{access} {table}");
    }
    match key {
        "query_block" => "query block".into(),
        "ordering_operation" => "order".into(),
        "grouping_operation" => "group".into(),
        "duplicates_removal" => "distinct".into(),
        "nested_loop" => "nested loop".into(),
        other => other.replace('_', " "),
    }
}

fn mysql_cost(map: &Map<String, Value>) -> Option<f64> {
    let info = map.get("cost_info")?;
    json_f64(info.get("query_cost"))
        .or_else(|| json_f64(info.get("prefix_cost")))
        .or_else(|| json_f64(info.get("read_cost")))
}

fn first_number(map: &Map<String, Value>, keys: &[&str]) -> Option<f64> {
    for key in keys {
        if let Some(value) = json_f64(map.get(*key)) {
            return Some(value);
        }
    }
    None
}

struct AnalyzeItem {
    indent: usize,
    node: PlanNode,
}

fn mysql_analyze_tree(raw: &str) -> PlanNode {
    let mut stack: Vec<AnalyzeItem> = Vec::new();
    let mut roots = Vec::new();
    let mut saw_arrow = false;
    for line in raw.lines() {
        let trimmed = line.trim_end();
        if trimmed.trim().is_empty() {
            continue;
        }
        let indent = trimmed.chars().take_while(|ch| *ch == ' ').count();
        let body = trimmed.trim_start();
        let Some(body) = body.strip_prefix("->") else {
            continue;
        };
        saw_arrow = true;
        let body = body.trim_start();
        pop_analyze(&mut stack, &mut roots, indent);
        stack.push(AnalyzeItem {
            indent,
            node: mysql_analyze_node(body),
        });
    }
    pop_analyze(&mut stack, &mut roots, 0);
    if !saw_arrow {
        return PlanNode::leaf(raw.trim());
    }
    single_root("Query", roots)
}

fn pop_analyze(stack: &mut Vec<AnalyzeItem>, roots: &mut Vec<PlanNode>, indent: usize) {
    while stack.last().is_some_and(|item| item.indent >= indent) {
        let Some(done) = stack.pop() else {
            break;
        };
        if let Some(parent) = stack.last_mut() {
            parent.node.children.push(done.node);
        } else {
            roots.push(done.node);
        }
    }
}

fn mysql_analyze_node(body: &str) -> PlanNode {
    let (name, rest) = split_analyze_name(body);
    let mut fields = Vec::new();
    let mut rows = None;
    let mut cost = None;
    for part in parentheticals(rest) {
        if let Some(value) = part.strip_prefix("cost=") {
            let mut bits = value.split_whitespace();
            if let Some(cost_text) = bits.next() {
                cost = cost_text.parse().ok();
            }
            if let Some(rows_text) = bits.next().and_then(|text| text.strip_prefix("rows=")) {
                rows = rows_text.parse().ok();
            }
        } else if let Some(value) = part.strip_prefix("actual time=") {
            fields.push(PlanField {
                label: "actual time".into(),
                value: value.to_string(),
            });
            if let Some(actual_rows) = value
                .split_whitespace()
                .find_map(|bit| bit.strip_prefix("rows="))
            {
                rows = actual_rows.parse().ok().or(rows);
            }
        }
    }
    PlanNode {
        name,
        rows,
        cost,
        fields,
        children: Vec::new(),
    }
}

fn split_analyze_name(body: &str) -> (String, &str) {
    if let Some(at) = body.find(" (cost=").or_else(|| body.find(" (actual time=")) {
        let name = body.get(..at).unwrap_or(body).trim();
        let rest = body.get(at..).unwrap_or("");
        return (name.to_string(), rest);
    }
    (body.trim().to_string(), "")
}

fn parentheticals(text: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let bytes = text.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        if bytes.get(index) != Some(&b'(') {
            index += 1;
            continue;
        }
        let start = index + 1;
        let mut depth = 1;
        index += 1;
        while index < bytes.len() {
            match bytes.get(index) {
                Some(&b'(') => depth += 1,
                Some(&b')') => {
                    depth -= 1;
                    if depth == 0 {
                        if let Some(inner) = text.get(start..index) {
                            out.push(inner);
                        }
                        break;
                    }
                }
                _ => {}
            }
            index += 1;
        }
        index += 1;
    }
    out
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn json_explain_names_the_table_scan_and_its_cost() {
        let raw = r#"{
            "query_block": {
                "select_id": 1,
                "cost_info": { "query_cost": "1.20" },
                "table": {
                    "table_name": "t",
                    "access_type": "ALL",
                    "rows_examined_per_scan": 4,
                    "cost_info": { "read_cost": "1.00", "prefix_cost": "1.20" }
                }
            }
        }"#;
        let plan = match parse_mysql_json(raw) {
            Ok(plan) => plan,
            Err(err) => panic!("{err}"),
        };
        assert_eq!(plan.root.name, "query block");
        assert_eq!(plan.root.cost, Some(1.20));
        let child = match plan.root.children.first() {
            Some(child) => child,
            None => panic!("missing table"),
        };
        assert_eq!(child.name, "table scan t");
        assert_eq!(child.rows, Some(4.0));
        assert_eq!(plan.raw, raw);
    }

    #[test]
    fn analyze_tree_nests_the_child_under_the_parent() {
        let raw = "\
-> Nested loop inner join  (cost=1.50 rows=2) (actual time=0.05..0.08 rows=2 loops=1)
    -> Index lookup on a using PRIMARY (id=1)  (cost=0.35 rows=1) (actual time=0.02..0.02 rows=1 loops=1)
    -> Filter: (b.x = 1)  (cost=0.40 rows=1) (actual time=0.01..0.02 rows=1 loops=1)
        -> Table scan on b  (cost=0.35 rows=1) (actual time=0.01..0.01 rows=1 loops=1)
";
        let plan = match parse_mysql_analyze(raw) {
            Ok(plan) => plan,
            Err(err) => panic!("{err}"),
        };
        assert_eq!(plan.root.name, "Nested loop inner join");
        assert_eq!(plan.root.cost, Some(1.50));
        assert_eq!(plan.root.rows, Some(2.0));
        assert_eq!(plan.root.children.len(), 2);
        let filter = match plan.root.children.get(1) {
            Some(node) => node,
            None => panic!("missing filter"),
        };
        assert_eq!(filter.name, "Filter: (b.x = 1)");
        let scan = match filter.children.first() {
            Some(node) => node,
            None => panic!("missing scan"),
        };
        assert_eq!(scan.name, "Table scan on b");
        assert_eq!(plan.raw_format, "text");
        assert!(plan.raw.contains("Table scan on b"));
    }

    #[test]
    fn a_parent_whose_child_costs_almost_as_much_is_the_callers_job() {
        let plain = mysql_explain_sql("SELECT 1", false);
        let analyze = mysql_explain_sql("DELETE FROM t", true);
        assert_eq!(plain, "EXPLAIN FORMAT=JSON SELECT 1");
        assert_eq!(analyze, "EXPLAIN ANALYZE DELETE FROM t");
    }
}
