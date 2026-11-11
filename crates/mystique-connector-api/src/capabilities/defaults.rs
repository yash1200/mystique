//! Shared starting points for a plugin's [`EngineCapabilities`].
//!
//! Each plugin owns its own defaults (`default_capabilities()` in the plugin
//! crate). It starts from [`EngineCapabilities::relational`] or
//! [`EngineCapabilities::document`] and overrides what differs, so adding an
//! engine never edits this crate.

use super::{
    default_export_formats, sql_export_formats, ConnectionKind, EngineCapabilities, EngineFamily,
    ExplorerConfig, ModelViewKind, ObjectKindSpec, ObjectLabels, SupportDepth, TableAdminFeatures,
};

impl EngineCapabilities {
    /// A pooled, host-based SQL engine at [`SupportDepth::Operate`]: schemas,
    /// foreign keys, ER, transactions, explain, structure edit, metrics.
    ///
    /// `sql_dialect` is the sql-formatter dialect (`postgresql`, `mysql`,
    /// `sqlite`, `transactsql`, `duckdb`, or `sql`).
    pub fn relational(engine: &str, sql_dialect: &str) -> Self {
        Self {
            engine: engine.into(),
            family: EngineFamily::Relational,
            query_language: "SQL".into(),
            sql_dialect: sql_dialect.into(),
            supports_schema: true,
            supports_foreign_keys: true,
            supports_er_diagram: true,
            supports_model_view: true,
            model_view_kind: ModelViewKind::Er,
            supports_pagination: true,
            supports_write: false,
            supports_cancel_query: true,
            supports_transactions: true,
            supports_explain_plan: true,
            supports_structure_edit: true,
            structure_edit_limited: false,
            supports_distinct_values: true,
            supports_query: true,
            supports_pooling: true,
            binds_session_to_database: false,
            prefer_database_from_uri: false,
            connection_kind: ConnectionKind::Host,
            system_databases: vec![],
            name_example: format!("e.g. local-{engine}"),
            object_labels: ObjectLabels::relational(),
            explorer: ExplorerConfig::relational_flat(true, true),
            table_admin: TableAdminFeatures::sql_all(),
            code_filter_placeholder: r#"{ "status": "active" } or status = 'active'"#.into(),
            export_formats: sql_export_formats(),
            supports_server_metrics: true,
            supports_terminate_session: true,
            supports_explorer: true,
            support_depth: SupportDepth::Operate,
            object_kinds: ObjectKindSpec::relational(),
        }
    }

    /// A pooled document store at [`SupportDepth::Operate`]: databases of
    /// collections, a collection map instead of ER, no SQL dialect.
    pub fn document(engine: &str, query_language: &str) -> Self {
        Self {
            engine: engine.into(),
            family: EngineFamily::Document,
            query_language: query_language.into(),
            sql_dialect: String::new(),
            supports_schema: false,
            supports_foreign_keys: false,
            supports_er_diagram: false,
            supports_model_view: true,
            model_view_kind: ModelViewKind::CollectionMap,
            supports_pagination: true,
            supports_write: false,
            supports_cancel_query: true,
            supports_transactions: false,
            supports_explain_plan: true,
            supports_structure_edit: true,
            structure_edit_limited: false,
            supports_distinct_values: true,
            supports_query: true,
            supports_pooling: true,
            binds_session_to_database: false,
            prefer_database_from_uri: false,
            connection_kind: ConnectionKind::Uri,
            system_databases: vec![],
            name_example: format!("e.g. local-{engine}"),
            object_labels: ObjectLabels::document(),
            explorer: ExplorerConfig::document(),
            table_admin: TableAdminFeatures::mongodb(),
            code_filter_placeholder: r#"{ "email": "user@example.com" }"#.into(),
            export_formats: default_export_formats(),
            supports_server_metrics: true,
            supports_terminate_session: true,
            supports_explorer: true,
            support_depth: SupportDepth::Operate,
            object_kinds: ObjectKindSpec::document(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_plugin_that_does_not_declare_explain_has_none() {
        let mut raw = serde_json::to_value(EngineCapabilities::relational("x", "sql"))
            .unwrap_or_else(|err| panic!("{err}"));
        let Some(map) = raw.as_object_mut() else {
            panic!("capabilities are an object");
        };
        map.remove("supportsExplainPlan");
        let caps: EngineCapabilities =
            serde_json::from_value(raw).unwrap_or_else(|err| panic!("{err}"));
        assert!(!caps.supports_explain_plan);
    }

    #[test]
    fn bases_default_to_operate_with_object_kinds() {
        for caps in [
            EngineCapabilities::relational("x", "sql"),
            EngineCapabilities::document("y", "MQL"),
        ] {
            assert_eq!(caps.support_depth, SupportDepth::Operate);
            assert!(caps.supports_explorer);
            assert!(!caps.object_kinds.is_empty());
        }
    }

    #[test]
    fn browse_hides_model_metrics_and_mutating_admin() {
        let mut caps = EngineCapabilities::relational("x", "sql");
        caps.apply_support_depth(SupportDepth::Browse);
        assert_eq!(caps.support_depth, SupportDepth::Browse);
        assert!(caps.supports_explorer);
        assert!(!caps.supports_er_diagram);
        assert!(!caps.supports_model_view);
        assert!(!caps.supports_server_metrics);
        assert!(!caps.table_admin.drop);
        assert!(caps.table_admin.view_structure);
        assert!(!caps.explorer.can_create_object);
    }

    #[test]
    fn connect_hides_explorer() {
        let mut caps = EngineCapabilities::relational("x", "sql");
        caps.apply_support_depth(SupportDepth::Connect);
        assert_eq!(caps.support_depth, SupportDepth::Connect);
        assert!(!caps.supports_explorer);
        assert!(!caps.supports_er_diagram);
        assert!(!caps.table_admin.view_structure);
    }

    #[test]
    fn missing_new_fields_deserialize_to_defaults() {
        let v = serde_json::json!({
            "engine": "postgres",
            "family": "RELATIONAL",
            "queryLanguage": "SQL",
            "supportsSchema": true,
            "supportsForeignKeys": true,
            "supportsErDiagram": true,
            "supportsModelView": true,
            "modelViewKind": "ER",
            "supportsPagination": true,
            "supportsWrite": false,
            "supportsCancelQuery": true,
            "supportsPooling": true,
            "bindsSessionToDatabase": true,
            "objectLabels": { "namespace": "Schema", "container": "Table", "field": "Column" },
            "explorer": {
                "layout": "schema-tree",
                "multiDatabase": true,
                "canCreateDatabase": true,
                "canCreateObject": true,
                "databaseLabel": "Database",
                "groupLabel": "Schema",
                "objectLabel": "tables",
                "objectLabelSingular": "table",
                "filterPlaceholder": "Filter"
            },
            "tableAdmin": {
                "createStatement": true,
                "rename": true,
                "drop": true,
                "truncate": true,
                "duplicate": true
            }
        });
        let caps: EngineCapabilities =
            serde_json::from_value(v).unwrap_or_else(|err| panic!("{err}"));
        assert_eq!(caps.support_depth, SupportDepth::Operate);
        assert!(caps.supports_explorer);
        assert!(caps.object_kinds.is_empty());
    }
}
