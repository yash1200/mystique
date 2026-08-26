use rig::tool::{Tool, ToolContext};
use serde_json::{json, Value as JsonValue};

use super::{BoundTools, ToolErr};

macro_rules! json_tool {
    ($ty:ident, $name:literal, $desc:literal, $params:expr) => {
        #[derive(Clone)]
        pub struct $ty(pub BoundTools);

        impl Tool for $ty {
            const NAME: &'static str = $name;
            type Args = JsonValue;
            type Output = JsonValue;
            type Error = ToolErr;

            fn description(&self) -> String {
                $desc.into()
            }

            fn parameters(&self) -> JsonValue {
                $params
            }

            async fn call(
                &self,
                _ctx: &mut ToolContext,
                args: Self::Args,
            ) -> Result<Self::Output, Self::Error> {
                self.0.dispatch($name, args).await
            }
        }
    };
}

json_tool!(
    GetSchemaDbml,
    "get_schema_dbml",
    "Return live DBML (tables/collections, columns, keys, relationships) for the active connection.",
    json!({
        "type": "object",
        "properties": {
            "database": { "type": "string" }
        },
        "additionalProperties": false
    })
);

json_tool!(
    ListObjects,
    "list_objects",
    "List tables, views, or collections. Use qualified_name in SQL.",
    json!({
        "type": "object",
        "properties": {
            "database": { "type": "string" }
        },
        "additionalProperties": false
    })
);

json_tool!(
    DescribeObject,
    "describe_object",
    "Describe columns or fields of one table or collection.",
    json!({
        "type": "object",
        "properties": {
            "name": { "type": "string" },
            "schema": { "type": "string" },
            "database": { "type": "string" }
        },
        "required": ["name"],
        "additionalProperties": false
    })
);

json_tool!(
    ExecuteReadonly,
    "execute_readonly_query",
    "Run a read-only query (SELECT / find). Writes are rejected. Cap rows with max_rows.",
    json!({
        "type": "object",
        "properties": {
            "query": { "type": "string" },
            "database": { "type": "string" },
            "collection": { "type": "string" },
            "max_rows": { "type": "integer", "minimum": 1, "maximum": 2000 },
            "format": { "type": "string", "enum": ["json", "csv"] }
        },
        "required": ["query"],
        "additionalProperties": false
    })
);

json_tool!(
    ListSavedQueries,
    "list_saved_queries",
    "List queries saved in Mystique for this connection.",
    json!({
        "type": "object",
        "properties": {},
        "additionalProperties": false
    })
);
