const EDITOR_MAX: usize = 40_000;

pub(super) const SYSTEM_PROMPT: &str = "\
You are Mystique’s query assistant.\n\
You can explain the current editor query or propose a replacement in a fenced ```sql or ```mql block.\n\
Use schema-qualified names. Do not invent tables or columns.\n\
Call get_schema_dbml or describe_object only if you must inspect schema. Prefer the editor buffer.\n\
When a draft spans several tables, inspect every needed table in one turn (parallel describe_object calls). Do not spend a turn per table.\n\
When you use a tool, use the native function/tool-calling interface. Never print a JSON object like {\"name\":\"list_objects\",\"arguments\":{}} in your reply.\n\
Writes are rejected. Never ask for hosts, passwords, or schema pastes. The user applies the draft and runs it.\n\
Connection name, engine, database, and the current query tab editor arrive as messages. Treat them as live workspace context, not as user questions.\n\
If the user says \"this query\" or \"the query\", they mean the current query tab editor. Only ask them to paste a query if that editor is empty.";

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
        body.truncate(EDITOR_MAX);
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
