use mystique_connector_api::QueryResult;
use serde_json::{Map, Value as JsonValue};

use super::types::{AgentCellWindow, AgentQueryDownload};

/// Model-bound query payload. The grid and file export do not call this.
pub(super) fn download_from_result(
    result: QueryResult,
    format: &str,
    window: AgentCellWindow,
) -> AgentQueryDownload {
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
                window,
            )),
        }
    } else {
        let rows = result
            .rows
            .iter()
            .map(|row| {
                row.iter()
                    .map(|cell| window_cell_json(cell, window))
                    .collect()
            })
            .collect();
        AgentQueryDownload {
            engine: result.engine,
            columns,
            row_count: result.row_count,
            truncated: result.truncated,
            duration_ms: result.duration_ms,
            format: "json".into(),
            rows: Some(rows),
            csv: None,
        }
    }
}

struct CellSlice {
    text: String,
    next_offset: Option<u32>,
    /// Offset was non-zero, or the value continues past this slice.
    partial: bool,
}

fn slice_cell(value: &JsonValue, window: AgentCellWindow) -> CellSlice {
    let rendered = render_cell(value);
    let mut chars = rendered.chars().skip(window.offset as usize);
    let mut text = String::new();
    let mut count = 0u32;
    for ch in chars.by_ref().take(window.limit as usize) {
        text.push(ch);
        count = count.saturating_add(1);
    }
    let has_more = chars.next().is_some();
    CellSlice {
        text,
        next_offset: has_more.then(|| window.offset.saturating_add(count)),
        partial: window.offset > 0 || has_more,
    }
}

/// JSON cell for a model. A full value keeps its original JSON type.
/// A slice is `{ text, next_offset }` or `{ text, end: true }`.
pub(super) fn window_cell_json(value: &JsonValue, window: AgentCellWindow) -> JsonValue {
    let slice = slice_cell(value, window);
    if !slice.partial {
        return value.clone();
    }
    let mut obj = Map::new();
    obj.insert("text".into(), JsonValue::String(slice.text));
    if let Some(next) = slice.next_offset {
        obj.insert("next_offset".into(), JsonValue::from(next));
    } else {
        obj.insert("end".into(), JsonValue::Bool(true));
    }
    JsonValue::Object(obj)
}

fn window_cell_text(value: &JsonValue, window: AgentCellWindow) -> String {
    let slice = slice_cell(value, window);
    if !slice.partial {
        return slice.text;
    }
    match slice.next_offset {
        Some(next) => format!("{} [next_offset={next}]", slice.text),
        None => format!("{} [end]", slice.text),
    }
}

fn render_cell(value: &JsonValue) -> String {
    match value {
        JsonValue::Null => String::new(),
        JsonValue::String(s) => s.clone(),
        JsonValue::Bool(b) => b.to_string(),
        JsonValue::Number(n) => n.to_string(),
        other => other.to_string(),
    }
}

fn rows_to_csv(headers: &[&str], rows: &[Vec<JsonValue>], window: AgentCellWindow) -> String {
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
                csv_escape(&window_cell_text(cell, window))
            })
            .collect();
        out.push_str(&line.join(","));
        out.push('\n');
    }
    out
}

fn csv_escape(s: &str) -> String {
    if s.contains(',') || s.contains('"') || s.contains('\n') {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mystique_connector_api::{QueryColumn, QueryMode, QueryResult};
    use serde_json::json;

    use crate::application::agent_tools::types::{
        AgentCellWindow, AGENT_DEFAULT_CELL_CHARS, AGENT_MAX_CELL_CHARS,
    };

    fn result_with(cell: JsonValue) -> QueryResult {
        QueryResult {
            engine: "postgres".into(),
            mode: QueryMode::Read,
            columns: vec![QueryColumn {
                name: "doc".into(),
                data_type: Some("json".into()),
            }],
            rows: vec![vec![cell]],
            row_count: 1,
            truncated: false,
            duration_ms: 1,
            message: None,
        }
    }

    fn json_cell(download: &AgentQueryDownload) -> &JsonValue {
        download
            .rows
            .as_ref()
            .and_then(|rows| rows.first())
            .and_then(|row| row.first())
            .expect("cell")
    }

    #[test]
    fn default_window_returns_200_chars_and_next_offset() {
        let cell = JsonValue::String(format!("{}TAIL", "a".repeat(20_000)));
        let out = download_from_result(result_with(cell), "json", AgentCellWindow::default());
        let shown = json_cell(&out);
        let text = shown["text"].as_str().expect("text");
        assert_eq!(text.chars().count(), AGENT_DEFAULT_CELL_CHARS as usize);
        assert!(!text.contains("TAIL"));
        assert_eq!(shown["next_offset"], json!(AGENT_DEFAULT_CELL_CHARS));
    }

    #[test]
    fn huge_limit_is_clamped_to_4000() {
        let cell = JsonValue::String("b".repeat(20_000));
        let out = download_from_result(
            result_with(cell),
            "json",
            AgentCellWindow::from_options(None, Some(100_000)),
        );
        let shown = json_cell(&out);
        assert_eq!(
            shown["text"].as_str().expect("text").chars().count(),
            AGENT_MAX_CELL_CHARS as usize
        );
        assert_eq!(shown["next_offset"], json!(AGENT_MAX_CELL_CHARS));
    }

    #[test]
    fn follow_up_offset_returns_the_next_slice_then_the_end() {
        let cell = JsonValue::String("a".repeat(20_000));
        let mid = window_cell_json(&cell, AgentCellWindow::from_options(Some(200), Some(200)));
        assert_eq!(mid["text"].as_str().expect("text"), "a".repeat(200));
        assert_eq!(mid["next_offset"], json!(400));

        let end = window_cell_json(
            &cell,
            AgentCellWindow::from_options(Some(19_800), Some(400)),
        );
        assert_eq!(end["text"].as_str().expect("text").chars().count(), 200);
        assert!(end.get("next_offset").is_none());
        assert_eq!(end["end"], json!(true));
    }

    #[test]
    fn window_counts_unicode_characters() {
        let cell = JsonValue::String("甲乙丙丁戊己庚辛".into());
        let shown = window_cell_json(&cell, AgentCellWindow::from_options(Some(2), Some(3)));
        assert_eq!(shown["text"], json!("丙丁戊"));
        assert_eq!(shown["next_offset"], json!(5));
    }

    #[test]
    fn json_object_cell_is_windowed_as_text() {
        let cell = json!({ "k": "a".repeat(20_000) });
        let shown = window_cell_json(&cell, AgentCellWindow::default());
        let text = shown["text"].as_str().expect("text");
        assert_eq!(text.chars().count(), 200);
        assert!(text.starts_with("{\"k\":"));
        assert_eq!(shown["next_offset"], json!(200));
    }

    #[test]
    fn short_cell_keeps_its_json_type() {
        let cell = json!({"ok": true});
        let shown = window_cell_json(&cell, AgentCellWindow::default());
        assert_eq!(shown, cell);
    }

    #[test]
    fn csv_for_the_model_does_not_include_the_tail() {
        let cell = JsonValue::String(format!("{}TAIL", "a".repeat(20_000)));
        let out = download_from_result(result_with(cell), "csv", AgentCellWindow::default());
        let csv = out.csv.expect("csv");
        assert!(!csv.contains("TAIL"));
        assert!(csv.contains("next_offset=200"));
        assert_eq!(csv.matches('a').count(), 200);
    }
}
