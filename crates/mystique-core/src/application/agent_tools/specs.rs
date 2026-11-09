use serde_json::json;

use mystique_connector_api::{INFER_DEFAULT_SAMPLE, INFER_MAX_SAMPLE};

use super::inspect::{AGENT_RAW_DEFAULT_CHARS, AGENT_SAMPLE_DEFAULT_ROWS, AGENT_SAMPLE_MAX_ROWS};
use super::types::{
    AgentToolSpec, ToolAnnotations, AGENT_DEFAULT_CELL_CHARS, AGENT_MAX_CELL_CHARS,
    AGENT_MAX_CELL_OFFSET, AGENT_MAX_ROWS,
};

/// The listed tools. `execute_query` only when the caller may write somewhere.
pub fn tool_specs_for(may_write: bool) -> Vec<AgentToolSpec> {
    let mut specs = tool_specs();
    if !may_write {
        specs.retain(|t| t.name != "execute_query");
    }
    specs
}

/// Reads: no change to the database or to Mystique.
const fn reads(title: &'static str) -> ToolAnnotations {
    ToolAnnotations {
        title,
        read_only: true,
        destructive: false,
        idempotent: true,
    }
}

pub fn tool_specs() -> Vec<AgentToolSpec> {
    vec![
        AgentToolSpec {
            name: "list_connections",
            description:
                "List saved Mystique connections: id, name, engine, database. No hosts or passwords.",
            input_schema: json!({
                "type": "object",
                "properties": {},
                "additionalProperties": false
            }),
            annotations: reads("List connections"),
        },
        AgentToolSpec {
            name: "get_schema_dbml",
            description:
                "Return live DBML (tables/collections, columns, keys, relationships) for a connection. Opens the connection if needed. Use schema-qualified names from the Table headers (e.g. main.boards) — unqualified names fail when the table is not on search_path.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "connection_id": { "type": "string", "description": "Connection profile id from list_connections" },
                    "database": { "type": "string", "description": "Optional database / catalog name" }
                },
                "required": ["connection_id"],
                "additionalProperties": false
            }),
            annotations: reads("Schema as DBML"),
        },
        AgentToolSpec {
            name: "list_objects",
            description:
                "List tables, views, or collections in a connection (opens it if needed). Use qualified_name in SQL (e.g. main.boards).",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "connection_id": { "type": "string" },
                    "database": { "type": "string" }
                },
                "required": ["connection_id"],
                "additionalProperties": false
            }),
            annotations: reads("List tables and collections"),
        },
        AgentToolSpec {
            name: "describe_object",
            description:
                "Describe columns / fields of one table or collection.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "connection_id": { "type": "string" },
                    "name": { "type": "string", "description": "Table or collection name" },
                    "schema": { "type": "string" },
                    "database": { "type": "string" }
                },
                "required": ["connection_id", "name"],
                "additionalProperties": false
            }),
            annotations: reads("Describe a table"),
        },
        AgentToolSpec {
            name: "sample_rows",
            description:
                "Return a few rows (at most 20, 10 by default) of one table, view or collection, read through Mystique's table preview instead of a query. Use it to see real values before writing a query; do not SELECT * to learn a table's shape. Each cell is a character window (200 by default, 4000 max). A truncated cell includes next_offset; pass that value as cell_char_offset to read the next slice.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "connection_id": { "type": "string" },
                    "object": { "type": "string", "description": "Table, view or collection name. SQL names may be schema-qualified (main.boards)." },
                    "schema": { "type": "string" },
                    "database": { "type": "string" },
                    "limit": {
                        "type": "integer",
                        "minimum": 1,
                        "maximum": AGENT_SAMPLE_MAX_ROWS,
                        "description": format!("Rows to return (default {AGENT_SAMPLE_DEFAULT_ROWS}, max {AGENT_SAMPLE_MAX_ROWS}).")
                    },
                    "cell_char_offset": cell_offset_schema(),
                    "cell_char_limit": cell_limit_schema()
                },
                "required": ["connection_id", "object"],
                "additionalProperties": false
            }),
            annotations: reads("Sample rows"),
        },
        AgentToolSpec {
            name: "explain_query",
            description:
                "Return the plan for one statement: a normalised tree (operators, estimated rows and cost) plus the engine's raw plan text. It never returns the statement's rows. A plain explain plans the statement without running it, so explaining an INSERT, UPDATE or DELETE changes nothing. analyze: true runs the statement for actual rows and timings, and is allowed only for statements Mystique can prove are reads. The raw text is a character window (2000 by default, 4000 max); a truncated raw includes next_offset; pass that value as raw_char_offset to read on.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "connection_id": { "type": "string" },
                    "sql": { "type": "string", "description": "One SQL statement, or a Mongo find/aggregate" },
                    "params": {
                        "type": "array",
                        "description": "Values for $1 / ? / :name placeholders, bound like a normal run",
                        "items": {}
                    },
                    "database": { "type": "string" },
                    "collection": { "type": "string", "description": "Mongo collection when required" },
                    "analyze": {
                        "type": "boolean",
                        "description": "Run the statement for actual rows and timings. Proven reads only; default false."
                    },
                    "raw_char_offset": {
                        "type": "integer",
                        "minimum": 0,
                        "maximum": AGENT_MAX_CELL_OFFSET,
                        "description": "Character offset into the raw plan text (default 0). Pass next_offset from a truncated raw."
                    },
                    "raw_char_limit": {
                        "type": "integer",
                        "minimum": 1,
                        "maximum": AGENT_MAX_CELL_CHARS,
                        "description": format!("Characters of raw plan text (default {AGENT_RAW_DEFAULT_CHARS}, max {AGENT_MAX_CELL_CHARS}).")
                    }
                },
                "required": ["connection_id", "sql"],
                "additionalProperties": false
            }),
            annotations: reads("Explain a query"),
        },
        AgentToolSpec {
            name: "infer_fields",
            description:
                "MongoDB and DocumentDB: sample a collection (100 documents by default, 500 max) and return every field path with how many sampled documents have it and a count of its value types. Nested fields are dotted (address.city); array elements end in [] (items[].sku). Read-only: one $sample stage, nothing else runs. For SQL tables use describe_object.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "connection_id": { "type": "string" },
                    "collection": { "type": "string" },
                    "database": { "type": "string" },
                    "sample": {
                        "type": "integer",
                        "minimum": 1,
                        "maximum": INFER_MAX_SAMPLE,
                        "description": format!("Documents to sample (default {INFER_DEFAULT_SAMPLE}, max {INFER_MAX_SAMPLE}).")
                    }
                },
                "required": ["connection_id", "collection"],
                "additionalProperties": false
            }),
            annotations: reads("Infer collection fields"),
        },
        AgentToolSpec {
            name: "execute_readonly_query",
            description:
                "Run a read-only query (SELECT / find). Writes are rejected. Returns JSON rows or CSV for the model. Opens the connection if needed. Qualify tables with schema (SELECT * FROM main.boards), not bare names. Each cell is a character window (200 by default, 4000 max). A truncated cell includes next_offset; pass that value as cell_char_offset, with the same query, to read the next slice. Pages come from the result cached for 5 minutes; a write is never run again to page it. The grid and file export are not capped.",
            input_schema: query_input_schema("SQL or Mongo find/aggregate / JSON filter"),
            annotations: reads("Run a read-only query"),
        },
        AgentToolSpec {
            name: "execute_query",
            description:
                "Run a query that may write. Allowed up to your access level on the connection or database: read runs SELECT/find only, write adds INSERT/UPDATE, full adds DELETE, DDL and DROP. Above the level the call is denied and nothing runs, unless the connection asks the user to approve it in Mystique (the call then waits up to 90 seconds). Prefer execute_readonly_query for SELECT/find. Each cell is a character window (200 by default, 4000 max). A truncated cell includes next_offset; pass that value as cell_char_offset, with the same query, to read the next slice. Pages come from the result cached for 5 minutes; a write is never run again to page it.",
            input_schema: query_input_schema("SQL or Mongo write / read"),
            annotations: ToolAnnotations {
                title: "Run a query that may write",
                read_only: false,
                // full access allows DELETE, DDL and DROP.
                destructive: true,
                idempotent: false,
            },
        },
        AgentToolSpec {
            name: "list_saved_queries",
            description: "List queries saved in Mystique. Optionally filter by connection_id.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "connection_id": { "type": "string" }
                },
                "additionalProperties": false
            }),
            annotations: reads("List saved queries"),
        },
        AgentToolSpec {
            name: "save_query",
            description: "Save a named query on a connection so it appears in Mystique.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "connection_id": { "type": "string" },
                    "name": { "type": "string" },
                    "body": { "type": "string", "description": "Query text" },
                    "language": { "type": "string", "description": "sql | mql" },
                    "id": { "type": "string", "description": "Existing id to update" }
                },
                "required": ["connection_id", "name", "body"],
                "additionalProperties": false
            }),
            annotations: ToolAnnotations {
                title: "Save a query in Mystique",
                read_only: false,
                // With an id it replaces that saved query's text.
                destructive: true,
                idempotent: true,
            },
        },
    ]
}

fn query_input_schema(query_description: &str) -> serde_json::Value {
    json!({
        "type": "object",
        "properties": {
            "connection_id": { "type": "string" },
            "query": { "type": "string", "description": query_description },
            "database": { "type": "string" },
            "collection": { "type": "string", "description": "Mongo collection when required" },
            "max_rows": { "type": "integer", "minimum": 1, "maximum": AGENT_MAX_ROWS },
            "format": {
                "type": "string",
                "enum": ["json", "csv"],
                "description": "json (default) or csv. Both are cell-windowed for the model."
            },
            "cell_char_offset": {
                "type": "integer",
                "minimum": 0,
                "maximum": AGENT_MAX_CELL_OFFSET,
                "description": "Character offset for every cell (default 0). Pass next_offset from a truncated cell to read the next slice of the cached result. A write is never run again to page it."
            },
            "cell_char_limit": cell_limit_schema()
        },
        "required": ["connection_id", "query"],
        "additionalProperties": false
    })
}

fn cell_offset_schema() -> serde_json::Value {
    json!({
        "type": "integer",
        "minimum": 0,
        "maximum": AGENT_MAX_CELL_OFFSET,
        "description": "Character offset for every cell (default 0). Pass next_offset from a truncated cell to read the next slice."
    })
}

fn cell_limit_schema() -> serde_json::Value {
    json!({
        "type": "integer",
        "minimum": 1,
        "maximum": AGENT_MAX_CELL_CHARS,
        "description": format!("Characters returned per cell (default {AGENT_DEFAULT_CELL_CHARS}, max {AGENT_MAX_CELL_CHARS}). A larger request is clamped.")
    })
}
