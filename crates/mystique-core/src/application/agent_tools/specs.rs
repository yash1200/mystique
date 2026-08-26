use serde_json::json;

use super::types::AgentToolSpec;

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
        },
        AgentToolSpec {
            name: "execute_readonly_query",
            description:
                "Run a read-only query (SELECT / find). Writes are rejected. Returns JSON rows or CSV for download. Opens the connection if needed. Qualify tables with schema (SELECT * FROM main.boards), not bare names.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "connection_id": { "type": "string" },
                    "query": { "type": "string", "description": "SQL or Mongo find/aggregate / JSON filter" },
                    "database": { "type": "string" },
                    "collection": { "type": "string", "description": "Mongo collection when required" },
                    "max_rows": { "type": "integer", "minimum": 1, "maximum": 2000 },
                    "format": { "type": "string", "enum": ["json", "csv"], "description": "json (default) or csv" }
                },
                "required": ["connection_id", "query"],
                "additionalProperties": false
            }),
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
        },
    ]
}
