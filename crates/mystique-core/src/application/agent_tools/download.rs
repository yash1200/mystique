use mystique_connector_api::QueryResult;
use serde_json::Value as JsonValue;

use super::types::AgentQueryDownload;

pub(super) fn download_from_result(result: QueryResult, format: &str) -> AgentQueryDownload {
    let columns: Vec<String> = result.columns.iter().map(|c| c.name.clone()).collect();
    let csv_wanted = format.eq_ignore_ascii_case("csv");
    if csv_wanted {
        AgentQueryDownload {
            engine: result.engine,
            columns,
            row_count: result.row_count,
            truncated: result.truncated,
            duration_ms: result.duration_ms,
            format: "csv".into(),
            rows: None,
            csv: Some(rows_to_csv(
                &result
                    .columns
                    .iter()
                    .map(|c| c.name.as_str())
                    .collect::<Vec<_>>(),
                &result.rows,
            )),
        }
    } else {
        AgentQueryDownload {
            engine: result.engine,
            columns,
            row_count: result.row_count,
            truncated: result.truncated,
            duration_ms: result.duration_ms,
            format: "json".into(),
            rows: Some(result.rows),
            csv: None,
        }
    }
}

fn rows_to_csv(headers: &[&str], rows: &[Vec<JsonValue>]) -> String {
    let mut out = String::new();
    out.push_str(
        &headers
            .iter()
            .map(|h| csv_escape(h))
            .collect::<Vec<_>>()
            .join(","),
    );
    out.push('\n');
    for row in rows {
        let line: Vec<String> = (0..headers.len())
            .map(|i| {
                let cell = row.get(i).unwrap_or(&JsonValue::Null);
                csv_escape(&json_cell(cell))
            })
            .collect();
        out.push_str(&line.join(","));
        out.push('\n');
    }
    out
}

fn json_cell(v: &JsonValue) -> String {
    match v {
        JsonValue::Null => String::new(),
        JsonValue::String(s) => s.clone(),
        JsonValue::Bool(b) => b.to_string(),
        JsonValue::Number(n) => n.to_string(),
        other => other.to_string(),
    }
}

fn csv_escape(s: &str) -> String {
    if s.contains(',') || s.contains('"') || s.contains('\n') {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}
