use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum EngineFamily {
    Relational,
    Document,
    WideColumn,
    KeyValue,
    Other,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ModelViewKind {
    Er,
    CollectionMap,
    KeyModel,
    None,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
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
#[serde(rename_all = "camelCase")]
pub struct ExplorerConfig {
    /// `schema-tree` (Postgres) or `flat` (MySQL / SQLite / Mongo).
    pub layout: String,
    pub multi_database: bool,
    pub can_create_database: bool,
    pub database_label: String,
    pub group_label: String,
    pub object_label: String,
    pub object_label_singular: String,
    pub filter_placeholder: String,
}

/// Catalog-object actions the plugin implements (`apply_table_admin`).
#[derive(Debug, Clone, Serialize, Deserialize)]
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
#[serde(rename_all = "camelCase")]
pub enum ConnectionKind {
    #[default]
    Host,
    Uri,
    File,
}

/// Capability flags returned to the UI / API consumers.
#[derive(Debug, Clone, Serialize, Deserialize)]
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
}

impl ExplorerConfig {
    pub fn relational_schema_tree() -> Self {
        Self {
            layout: "schema-tree".into(),
            multi_database: true,
            can_create_database: true,
            database_label: "Database".into(),
            group_label: "Schema".into(),
            object_label: "tables".into(),
            object_label_singular: "table".into(),
            filter_placeholder: "Filter schemas & tables…".into(),
        }
    }

    pub fn relational_flat(multi_database: bool, can_create_database: bool) -> Self {
        Self {
            layout: "flat".into(),
            multi_database,
            can_create_database,
            database_label: "Database".into(),
            group_label: String::new(),
            object_label: "tables".into(),
            object_label_singular: "table".into(),
            filter_placeholder: "Filter tables…".into(),
        }
    }

    pub fn document() -> Self {
        Self {
            layout: "flat".into(),
            multi_database: true,
            can_create_database: true,
            database_label: "Database".into(),
            group_label: String::new(),
            object_label: "collections".into(),
            object_label_singular: "collection".into(),
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

impl EngineCapabilities {
    pub fn postgres_defaults() -> Self {
        Self {
            engine: "postgres".into(),
            family: EngineFamily::Relational,
            query_language: "SQL".into(),
            sql_dialect: "postgresql".into(),
            supports_schema: true,
            supports_foreign_keys: true,
            supports_er_diagram: true,
            supports_model_view: true,
            model_view_kind: ModelViewKind::Er,
            supports_pagination: true,
            supports_write: false,
            supports_cancel_query: true,
            supports_pooling: true,
            binds_session_to_database: true,
            prefer_database_from_uri: false,
            connection_kind: ConnectionKind::Host,
            system_databases: vec![],
            name_example: "e.g. local-pg".into(),
            object_labels: ObjectLabels::relational(),
            explorer: ExplorerConfig::relational_schema_tree(),
            table_admin: TableAdminFeatures::sql_all(),
        }
    }

    pub fn sqlite_defaults() -> Self {
        Self {
            engine: "sqlite".into(),
            family: EngineFamily::Relational,
            query_language: "SQL".into(),
            sql_dialect: "sqlite".into(),
            supports_schema: true,
            supports_foreign_keys: true,
            supports_er_diagram: true,
            supports_model_view: true,
            model_view_kind: ModelViewKind::Er,
            supports_pagination: true,
            supports_write: false,
            supports_cancel_query: false,
            supports_pooling: true,
            binds_session_to_database: false,
            prefer_database_from_uri: false,
            connection_kind: ConnectionKind::File,
            system_databases: vec![],
            name_example: "e.g. app-dev.sqlite".into(),
            object_labels: ObjectLabels::relational(),
            explorer: ExplorerConfig::relational_flat(false, false),
            table_admin: TableAdminFeatures::sql_all(),
        }
    }

    pub fn mongodb_defaults() -> Self {
        Self {
            engine: "mongodb".into(),
            family: EngineFamily::Document,
            query_language: "MQL".into(),
            sql_dialect: String::new(),
            supports_schema: false,
            supports_foreign_keys: false,
            supports_er_diagram: false,
            supports_model_view: true,
            model_view_kind: ModelViewKind::CollectionMap,
            supports_pagination: true,
            supports_write: false,
            supports_cancel_query: true,
            supports_pooling: true,
            binds_session_to_database: false,
            prefer_database_from_uri: true,
            connection_kind: ConnectionKind::Uri,
            system_databases: vec!["admin".into(), "local".into(), "config".into()],
            name_example: "e.g. local-mongo".into(),
            object_labels: ObjectLabels::document(),
            explorer: ExplorerConfig::document(),
            table_admin: TableAdminFeatures::mongodb(),
        }
    }

    pub fn mysql_defaults() -> Self {
        Self {
            engine: "mysql".into(),
            family: EngineFamily::Relational,
            query_language: "SQL".into(),
            sql_dialect: "mysql".into(),
            supports_schema: true,
            supports_foreign_keys: true,
            supports_er_diagram: true,
            supports_model_view: true,
            model_view_kind: ModelViewKind::Er,
            supports_pagination: true,
            supports_write: false,
            supports_cancel_query: true,
            supports_pooling: true,
            binds_session_to_database: true,
            prefer_database_from_uri: false,
            connection_kind: ConnectionKind::Host,
            system_databases: vec![],
            name_example: "e.g. local-mysql".into(),
            object_labels: ObjectLabels::relational(),
            explorer: ExplorerConfig::relational_flat(true, true),
            table_admin: TableAdminFeatures::sql_all(),
        }
    }
}
