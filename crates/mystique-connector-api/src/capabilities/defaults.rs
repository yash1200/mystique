//! Per-engine default [`EngineCapabilities`] used at plugin bootstrap.

use super::{
    default_export_formats, sql_export_formats, ConnectionKind, EngineCapabilities, EngineFamily,
    ExplorerConfig, ModelViewKind, ObjectLabels, SupportDepth, TableAdminFeatures,
};

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
            supports_query: true,
            supports_pooling: true,
            binds_session_to_database: true,
            prefer_database_from_uri: false,
            connection_kind: ConnectionKind::Host,
            system_databases: vec![],
            name_example: "e.g. local-pg".into(),
            object_labels: ObjectLabels::relational(),
            explorer: ExplorerConfig::relational_schema_tree(),
            table_admin: TableAdminFeatures::sql_all(),
            code_filter_placeholder: r#"{ "status": "active" } or status = 'active'"#.into(),
            export_formats: sql_export_formats(),
            supports_server_metrics: true,
            supports_terminate_session: true,
            supports_explorer: true,
            support_depth: SupportDepth::Operate,
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
            supports_cancel_query: true,
            supports_query: true,
            supports_pooling: true,
            binds_session_to_database: false,
            prefer_database_from_uri: false,
            connection_kind: ConnectionKind::File,
            system_databases: vec![],
            name_example: "e.g. app-dev.sqlite".into(),
            object_labels: ObjectLabels::relational(),
            explorer: ExplorerConfig::relational_flat(false, false),
            table_admin: TableAdminFeatures::sql_all(),
            code_filter_placeholder: r#"{ "status": "active" } or status = 'active'"#.into(),
            export_formats: sql_export_formats(),
            supports_server_metrics: true,
            supports_terminate_session: false,
            supports_explorer: true,
            support_depth: SupportDepth::Operate,
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
            supports_query: true,
            supports_pooling: true,
            binds_session_to_database: false,
            prefer_database_from_uri: true,
            connection_kind: ConnectionKind::Uri,
            system_databases: vec!["admin".into(), "local".into(), "config".into()],
            name_example: "e.g. local-mongo".into(),
            object_labels: ObjectLabels::document(),
            explorer: ExplorerConfig::document(),
            table_admin: TableAdminFeatures::mongodb(),
            code_filter_placeholder: r#"{ "email": "user@example.com" }"#.into(),
            export_formats: default_export_formats(),
            supports_server_metrics: true,
            supports_terminate_session: true,
            supports_explorer: true,
            support_depth: SupportDepth::Operate,
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
            supports_query: true,
            supports_pooling: true,
            binds_session_to_database: true,
            prefer_database_from_uri: false,
            connection_kind: ConnectionKind::Host,
            system_databases: vec![],
            name_example: "e.g. local-mysql".into(),
            object_labels: ObjectLabels::relational(),
            explorer: ExplorerConfig::relational_flat(true, true),
            table_admin: TableAdminFeatures::sql_all(),
            code_filter_placeholder: r#"{ "status": "active" } or status = 'active'"#.into(),
            export_formats: sql_export_formats(),
            supports_server_metrics: true,
            supports_terminate_session: true,
            supports_explorer: true,
            support_depth: SupportDepth::Operate,
        }
    }
}

#[cfg(test)]
mod create_caps_tests {
    use super::*;

    #[test]
    fn cancel_is_on_for_the_four_runtimes() {
        for caps in [
            EngineCapabilities::postgres_defaults(),
            EngineCapabilities::mysql_defaults(),
            EngineCapabilities::sqlite_defaults(),
            EngineCapabilities::mongodb_defaults(),
        ] {
            assert!(
                caps.supports_cancel_query,
                "{} does not advertise cancel",
                caps.engine
            );
        }
    }

    #[test]
    fn server_metrics_flags_per_engine() {
        let pg = EngineCapabilities::postgres_defaults();
        assert!(pg.supports_server_metrics);
        assert!(pg.supports_terminate_session);
        let my = EngineCapabilities::mysql_defaults();
        assert!(my.supports_server_metrics);
        assert!(my.supports_terminate_session);
        let mo = EngineCapabilities::mongodb_defaults();
        assert!(mo.supports_server_metrics);
        assert!(mo.supports_terminate_session);
        let sq = EngineCapabilities::sqlite_defaults();
        assert!(sq.supports_server_metrics);
        assert!(!sq.supports_terminate_session);
    }

    #[test]
    fn four_runtimes_default_to_operate() {
        for caps in [
            EngineCapabilities::postgres_defaults(),
            EngineCapabilities::mysql_defaults(),
            EngineCapabilities::sqlite_defaults(),
            EngineCapabilities::mongodb_defaults(),
        ] {
            assert_eq!(caps.support_depth, SupportDepth::Operate);
            assert!(caps.supports_explorer);
        }
    }

    #[test]
    fn browse_hides_model_metrics_and_mutating_admin() {
        let mut caps = EngineCapabilities::postgres_defaults();
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
        let mut caps = EngineCapabilities::postgres_defaults();
        caps.apply_support_depth(SupportDepth::Connect);
        assert_eq!(caps.support_depth, SupportDepth::Connect);
        assert!(!caps.supports_explorer);
        assert!(!caps.supports_er_diagram);
        assert!(!caps.table_admin.view_structure);
    }

    #[test]
    fn missing_support_depth_deserializes_as_operate() {
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
        let caps: EngineCapabilities = serde_json::from_value(v).unwrap();
        assert_eq!(caps.support_depth, SupportDepth::Operate);
        assert!(caps.supports_explorer);
    }

    #[test]
    fn mongodb_explorer_can_create_object() {
        let caps = EngineCapabilities::mongodb_defaults();
        assert!(caps.explorer.can_create_object);
        assert!(!caps.explorer.can_create_group);
        assert!(caps.explorer.can_create_database);
        let json = serde_json::to_value(&caps).unwrap();
        let explorer = json.get("explorer").unwrap();
        assert_eq!(
            explorer.get("canCreateObject").and_then(|v| v.as_bool()),
            Some(true)
        );
        assert_eq!(
            explorer.get("objectLabelSingular").and_then(|v| v.as_str()),
            Some("collection")
        );
        println!("{}", serde_json::to_string_pretty(&caps.explorer).unwrap());
    }
}
