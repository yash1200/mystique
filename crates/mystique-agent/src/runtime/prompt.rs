//! The one task contract (system prompt) and the one repair prompt.
//!
//! Every correction the run sends the model — a failed tool call, a failed
//! query from the editor ("Fix with AI"), tool calls printed as text — goes
//! through [`repair_prompt`], so the model always sees the same shape: what
//! failed, the statement, the error, and where in the statement it points.

use serde_json::Value as JsonValue;

const EDITOR_MAX: usize = 40_000;

pub(super) const TASK_CONTRACT: &str = "\
You are Mystique’s query assistant for one database connection.\n\
\n\
Task contract:\n\
1. Answer the user's question about their data or query. When a query is the answer, finish with exactly one fenced ```sql or ```mql block holding the complete draft. The user applies and runs it; you never run the final draft for them.\n\
2. Use schema-qualified names. Never invent tables or columns: check them with describe_object (several tables in one turn, in parallel) or get_schema_dbml. Prefer the editor buffer and the referenced tables below when they already answer it.\n\
3. Use the native tool-calling interface. Never print a JSON tool call such as {\"name\":\"list_objects\",\"arguments\":{}} in your reply.\n\
4. Tools run under the user's access level. A refused or failed call says why; fix the call or explain the limit. Never ask for hosts, passwords, or schema pastes.\n\
5. When a correction arrives (marked “Repair”), fix exactly what it names, using the statement, error and position it gives.\n\
6. Keep it short. Connection, engine, database, referenced tables and the current editor arrive below as workspace context, not as user questions. \"This query\" means the current editor; ask for a paste only if the editor is empty.";

fn editor_fence(engine: &str) -> &'static str {
    match engine.to_ascii_lowercase().as_str() {
        "mongodb" | "mongo" | "mql" => "mql",
        _ => "sql",
    }
}

fn editor_block(engine: &str, raw: &str) -> String {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return "Current query tab editor: (empty). Only ask the user to paste a query if they refer to one and this block is empty.".into();
    }
    let fence = editor_fence(engine);
    let mut body = trimmed.to_string();
    let mut note = String::new();
    if body.len() > EDITOR_MAX {
        let mut end = EDITOR_MAX;
        while end > 0 && !body.is_char_boundary(end) {
            end -= 1;
        }
        body.truncate(end);
        note = "\n… truncated.".into();
    }
    format!("Current query tab editor (live buffer):\n```{fence}\n{body}{note}\n```")
}

pub(crate) fn workspace_context(
    name: &str,
    engine: &str,
    database: &str,
    editor_query: &str,
) -> String {
    let db = if database.is_empty() {
        "(default catalog)"
    } else {
        database
    };
    format!(
        "Workspace context (not a user question).\n\
         Connection: {name}\n\
         Engine: {engine}\n\
         Database: {db}\n\
         \n\
         {}",
        editor_block(engine, editor_query)
    )
}

/// The `@table` references of this turn, with the columns we looked up.
pub(crate) fn referenced_tables(tables: &[(String, Result<JsonValue, String>)]) -> String {
    if tables.is_empty() {
        return String::new();
    }
    let mut out = String::from("Tables the user referenced with @ (columns already looked up):");
    for (label, described) in tables {
        out.push_str("\n- ");
        out.push_str(label);
        match described {
            Ok(value) => {
                let cols = value
                    .get("columns")
                    .and_then(|c| c.as_array())
                    .map(|cols| {
                        cols.iter()
                            .map(|c| {
                                let name = c.get("name").and_then(|v| v.as_str()).unwrap_or("?");
                                let ty = c
                                    .get("data_type")
                                    .or_else(|| c.get("dataType"))
                                    .and_then(|v| v.as_str())
                                    .unwrap_or("");
                                if ty.is_empty() {
                                    name.to_string()
                                } else {
                                    format!("{name} {ty}")
                                }
                            })
                            .collect::<Vec<_>>()
                            .join(", ")
                    })
                    .unwrap_or_default();
                out.push_str(": ");
                out.push_str(if cols.is_empty() {
                    "(no columns)"
                } else {
                    &cols
                });
            }
            Err(err) => {
                out.push_str(": could not describe (");
                out.push_str(err);
                out.push(')');
            }
        }
    }
    out
}

/// 1-based line and column of an error inside the statement.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ErrorAt {
    pub line: u32,
    pub column: u32,
}

/// Everything one correction can carry. Only `what` and `error` are required.
pub(crate) struct Repair<'a> {
    /// What failed, e.g. "The tool call execute_readonly_query failed".
    pub what: &'a str,
    pub statement: Option<&'a str>,
    pub error: &'a str,
    pub at: Option<ErrorAt>,
    /// Extra material the fix needs (tool results for printed tool calls).
    pub extra: Option<&'a str>,
}

/// The one repair prompt.
pub(crate) fn repair_prompt(r: &Repair<'_>) -> String {
    let mut out = format!("Repair: {}.", r.what);
    if let Some(sql) = r.statement.map(str::trim).filter(|s| !s.is_empty()) {
        out.push_str("\n\nStatement:\n```\n");
        out.push_str(sql);
        out.push_str("\n```");
    }
    out.push_str("\n\nError: ");
    out.push_str(r.error.trim());
    if let Some(at) = r.at {
        out.push_str(&format!(
            "\nPosition: line {}, column {}",
            at.line, at.column
        ));
        if let Some(line) = r
            .statement
            .and_then(|s| s.lines().nth(at.line.saturating_sub(1) as usize))
        {
            let caret = " ".repeat(at.column.saturating_sub(1) as usize);
            out.push_str(&format!("\n```\n{line}\n{caret}^\n```"));
        }
    }
    if let Some(extra) = r.extra.map(str::trim).filter(|s| !s.is_empty()) {
        out.push_str("\n\n");
        out.push_str(extra);
    }
    out.push_str("\n\nFix the cause and continue with the task contract. Do not repeat the same statement unchanged.");
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn context_includes_workspace_fields() {
        let c = workspace_context("prod", "postgres", "app", "select 1");
        assert!(c.contains("prod"));
        assert!(c.contains("postgres"));
        assert!(c.contains("app"));
        assert!(c.contains("select 1"));
        assert!(c.contains("```sql"));
    }

    #[test]
    fn context_marks_empty_editor_and_default_catalog() {
        let c = workspace_context("local", "sqlite", "", "");
        assert!(c.contains("(empty)"));
        assert!(c.contains("(default catalog)"));
    }

    #[test]
    fn context_uses_mql_fence_for_mongo() {
        let c = workspace_context("atlas", "mongodb", "shop", "db.x.find()");
        assert!(c.contains("```mql"));
    }

    #[test]
    fn repair_carries_statement_error_and_position() {
        let text = repair_prompt(&Repair {
            what: "The query in the editor failed",
            statement: Some("select id,\n  nme from users"),
            error: "column \"nme\" does not exist",
            at: Some(ErrorAt { line: 2, column: 3 }),
            extra: None,
        });
        assert!(text.starts_with("Repair:"));
        assert!(text.contains("select id,"));
        assert!(text.contains("column \"nme\" does not exist"));
        assert!(text.contains("line 2, column 3"));
        assert!(text.contains("  nme from users\n  ^"), "{text}");
    }

    #[test]
    fn referenced_tables_list_columns() {
        let text = referenced_tables(&[
            (
                "public.orders".into(),
                Ok(json!({ "columns": [{ "name": "id", "data_type": "int4" }] })),
            ),
            ("public.gone".into(), Err("not found".into())),
        ]);
        assert!(text.contains("public.orders: id int4"));
        assert!(text.contains("public.gone: could not describe (not found)"));
    }
}
