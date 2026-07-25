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

/// Capability flags returned to the UI / API consumers.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EngineCapabilities {
    pub engine: String,
    pub family: EngineFamily,
    pub query_language: String,
    pub supports_schema: bool,
    pub supports_foreign_keys: bool,
    pub supports_er_diagram: bool,
    pub supports_model_view: bool,
    pub model_view_kind: ModelViewKind,
    pub supports_pagination: bool,
    pub supports_write: bool,
    pub supports_cancel_query: bool,
    pub supports_pooling: bool,
    pub object_labels: ObjectLabels,
}

impl EngineCapabilities {
    pub fn postgres_defaults() -> Self {
        Self {
            engine: "postgres".into(),
            family: EngineFamily::Relational,
            query_language: "SQL".into(),
            supports_schema: true,
            supports_foreign_keys: true,
            supports_er_diagram: true,
            supports_model_view: true,
            model_view_kind: ModelViewKind::Er,
            supports_pagination: true,
            supports_write: false,
            supports_cancel_query: true,
            supports_pooling: true,
            object_labels: ObjectLabels::relational(),
        }
    }

    pub fn sqlite_defaults() -> Self {
        Self {
            engine: "sqlite".into(),
            family: EngineFamily::Relational,
            query_language: "SQL".into(),
            supports_schema: true,
            supports_foreign_keys: true,
            supports_er_diagram: true,
            supports_model_view: true,
            model_view_kind: ModelViewKind::Er,
            supports_pagination: true,
            supports_write: false,
            supports_cancel_query: false,
            supports_pooling: true,
            object_labels: ObjectLabels::relational(),
        }
    }

    pub fn mongodb_defaults() -> Self {
        Self {
            engine: "mongodb".into(),
            family: EngineFamily::Document,
            query_language: "MQL".into(),
            supports_schema: false,
            supports_foreign_keys: false,
            supports_er_diagram: false,
            supports_model_view: true,
            model_view_kind: ModelViewKind::CollectionMap,
            supports_pagination: true,
            supports_write: false,
            supports_cancel_query: true,
            supports_pooling: true,
            object_labels: ObjectLabels::document(),
        }
    }
}
