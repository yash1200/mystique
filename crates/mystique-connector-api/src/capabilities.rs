//! Engine capability flags returned to the UI / API consumers.

use serde::{Deserialize, Serialize};

mod defaults;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum EngineFamily {
    Relational,
    Document,
    WideColumn,
    KeyValue,
    Other,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ModelViewKind {
    Er,
    CollectionMap,
    KeyModel,
    None,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct ObjectLabels {
    pub namespace: String,
    pub container: String,
    pub field: String,
}

impl ObjectLabels {
    pub fn relational() -> Self {
        Self {
            namespace: "Schema".into(),
            container: "Table".into(),
            field: "Column".into(),
        }
    }

    pub fn document() -> Self {
        Self {
            namespace: "Database".into(),
            container: "Collection".into(),
            field: "Field".into(),
        }
    }
}

/// Explorer chrome inside one database (plugin-owned; FE must not hardcode).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct ExplorerConfig {
    /// `schema-tree` (Postgres) or `flat` (MySQL / SQLite / Mongo).
    pub layout: String,
    pub multi_database: bool,
    pub can_create_database: bool,
    /// Create a group/namespace (e.g. Postgres schema).
    #[serde(default)]
    pub can_create_group: bool,
    /// Create a leaf object (table / collection).
    #[serde(default = "default_true")]
    pub can_create_object: bool,
    pub database_label: String,
    pub group_label: String,
    pub object_label: String,
    pub object_label_singular: String,
    /// Heading above the explorer list (e.g. "Tables", "Collections").
    #[serde(default)]
    pub list_title: String,
    pub filter_placeholder: String,
}

/// Catalog-object actions the plugin implements (`apply_table_admin`).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct TableAdminFeatures {
    pub create_statement: bool,
    pub rename: bool,
    pub drop: bool,
    pub truncate: bool,
    pub duplicate: bool,
    /// Right-click “View structure” (plugin `describe_object_structure`).
    #[serde(default = "default_true")]
    pub view_structure: bool,
}

fn default_true() -> bool {
    true
}

/// How Home collects / displays connection coordinates.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub enum ConnectionKind {
    #[default]
    Host,
    Uri,
    File,
}

/// How much of the IDE this product actually supports.
///
/// Chrome reads the boolean flags this depth clamps — never `if engine ===`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "lowercase")]
pub enum SupportDepth {
    /// Full chrome (today’s four runtimes).
    #[default]
    Operate,
    /// Catalog, query, grid, export. No ER/metrics/mutating admin.
    Browse,
    /// Test connection + query. No explorer/ER/metrics/admin.
    Connect,
}

fn default_supports_query() -> bool {
    true
}

/// Capability flags returned to the UI / API consumers.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct EngineCapabilities {
    pub engine: String,
    pub family: EngineFamily,
    pub query_language: String,
    /// sql-formatter dialect (`postgresql` / `mysql` / `sqlite` / `sql`). Empty for non-SQL.
    #[serde(default)]
    pub sql_dialect: String,
    pub supports_schema: bool,
    pub supports_foreign_keys: bool,
    pub supports_er_diagram: bool,
    pub supports_model_view: bool,
    pub model_view_kind: ModelViewKind,
    pub supports_pagination: bool,
    pub supports_write: bool,
    pub supports_cancel_query: bool,
    /// Manual commit on a pinned connection. Mongo is on only for a replica set.
    #[serde(default = "default_true")]
    pub supports_transactions: bool,
    /// Explain button. The UI does not parse engine text itself. Off unless
    /// the plugin says so: the default `explain_query` is unsupported.
    #[serde(default)]
    pub supports_explain_plan: bool,
    /// Structure tab can preview and apply a DDL plan. The UI does not build SQL.
    #[serde(default = "default_true")]
    pub supports_structure_edit: bool,
    /// Structure edits are limited to adding columns, renaming, and indexes.
    /// No built-in engine sets it since the SQLite rebuild keeps the schema.
    #[serde(default)]
    #[cfg_attr(feature = "ts", ts(optional = nullable))]
    pub structure_edit_limited: bool,
    /// Grid filter can ask for distinct column values. Hidden when false.
    #[serde(default = "default_true")]
    #[cfg_attr(feature = "ts", ts(optional = nullable))]
    pub supports_distinct_values: bool,
    /// Query editor gutter. Every runtime can run a statement.
    #[serde(default = "default_supports_query")]
    pub supports_query: bool,
    pub supports_pooling: bool,
    /// Postgres / MySQL: switching database needs a new pool session.
    pub binds_session_to_database: bool,
    /// Default database is the path segment of the connection URI (Mongo).
    #[serde(default)]
    pub prefer_database_from_uri: bool,
    /// Home form + connection-detail facts layout.
    #[serde(default)]
    pub connection_kind: ConnectionKind,
    /// Catalog names skipped when picking a default database.
    #[serde(default)]
    pub system_databases: Vec<String>,
    /// Example used as the connection-name placeholder.
    #[serde(default)]
    pub name_example: String,
    pub object_labels: ObjectLabels,
    pub explorer: ExplorerConfig,
    pub table_admin: TableAdminFeatures,
    /// Placeholder for the grid code-filter box (plugin-owned).
    #[serde(default = "default_code_filter_placeholder")]
    pub code_filter_placeholder: String,
    /// File formats this plugin can export (`csv` / `json` / `jsonl` / `sql`).
    #[serde(default = "default_export_formats")]
    pub export_formats: Vec<String>,
    /// Session-rail “Server metrics” tab (plugin `fetch_server_metrics`).
    #[serde(default)]
    pub supports_server_metrics: bool,
    /// Terminate a backend session from the metrics tab.
    #[serde(default)]
    pub supports_terminate_session: bool,
    /// Object explorer (catalog tree). Off at [`SupportDepth::Connect`].
    #[serde(default = "default_true")]
    pub supports_explorer: bool,
    /// Product honesty: `operate` | `browse` | `connect`.
    #[serde(default)]
    pub support_depth: SupportDepth,
}

impl EngineCapabilities {
    /// Clamp feature flags to `depth`. `operate` leaves plugin flags as-is.
    pub fn apply_support_depth(&mut self, depth: SupportDepth) {
        self.support_depth = depth;
        match depth {
            SupportDepth::Operate => {
                self.supports_explorer = true;
            }
            SupportDepth::Browse => {
                self.supports_explorer = true;
                self.supports_structure_edit = false;
                self.supports_er_diagram = false;
                self.supports_model_view = false;
                self.model_view_kind = ModelViewKind::None;
                self.supports_server_metrics = false;
                self.supports_terminate_session = false;
                self.table_admin.create_statement = false;
                self.table_admin.rename = false;
                self.table_admin.drop = false;
                self.table_admin.truncate = false;
                self.table_admin.duplicate = false;
                self.explorer.can_create_database = false;
                self.explorer.can_create_group = false;
                self.explorer.can_create_object = false;
            }
            SupportDepth::Connect => {
                self.apply_support_depth(SupportDepth::Browse);
                self.support_depth = SupportDepth::Connect;
                self.supports_explorer = false;
                self.table_admin.view_structure = false;
            }
        }
    }
}

fn default_code_filter_placeholder() -> String {
    r#"{ "email": "user@example.com" }"#.into()
}

pub(super) fn default_export_formats() -> Vec<String> {
    vec!["csv".into(), "json".into(), "jsonl".into()]
}

pub(super) fn sql_export_formats() -> Vec<String> {
    vec!["csv".into(), "json".into(), "jsonl".into(), "sql".into()]
}

impl ExplorerConfig {
    pub fn relational_schema_tree() -> Self {
        Self {
            layout: "schema-tree".into(),
            multi_database: true,
            can_create_database: true,
            can_create_group: true,
            can_create_object: true,
            database_label: "Database".into(),
            group_label: "Schema".into(),
            object_label: "tables".into(),
            object_label_singular: "table".into(),
            list_title: "Tables".into(),
            filter_placeholder: "Filter schemas & tables…".into(),
        }
    }

    pub fn relational_flat(multi_database: bool, can_create_database: bool) -> Self {
        Self {
            layout: "flat".into(),
            multi_database,
            can_create_database,
            can_create_group: false,
            can_create_object: true,
            database_label: "Database".into(),
            group_label: String::new(),
            object_label: "tables".into(),
            object_label_singular: "table".into(),
            list_title: "Tables".into(),
            filter_placeholder: "Filter tables…".into(),
        }
    }

    pub fn document() -> Self {
        Self {
            layout: "flat".into(),
            multi_database: true,
            can_create_database: true,
            can_create_group: false,
            can_create_object: true,
            database_label: "Database".into(),
            group_label: String::new(),
            object_label: "collections".into(),
            object_label_singular: "collection".into(),
            list_title: "Collections".into(),
            filter_placeholder: "Filter collections…".into(),
        }
    }
}

impl TableAdminFeatures {
    pub fn sql_all() -> Self {
        Self {
            create_statement: true,
            rename: true,
            drop: true,
            truncate: true,
            duplicate: true,
            view_structure: true,
        }
    }

    pub fn mongodb() -> Self {
        Self {
            create_statement: true,
            rename: true,
            drop: true,
            truncate: true,
            duplicate: true,
            view_structure: true,
        }
    }
}
