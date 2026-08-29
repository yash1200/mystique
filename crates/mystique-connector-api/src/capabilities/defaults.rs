//! Per-engine default [`EngineCapabilities`] used at plugin bootstrap.

use super::{
    default_export_formats, sql_export_formats, ConnectionKind, EngineCapabilities, EngineFamily,
    ExplorerConfig, ModelViewKind, ObjectLabels, TableAdminFeatures,
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
            code_filter_placeholder: r#"{ "status": "active" } or status = 'active'"#.into(),
            export_formats: sql_export_formats(),
            supports_server_metrics: true,
            supports_terminate_session: false,
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
            code_filter_placeholder: r#"{ "email": "user@example.com" }"#.into(),
            export_formats: default_export_formats(),
            supports_server_metrics: true,
            supports_terminate_session: true,
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
            code_filter_placeholder: r#"{ "status": "active" } or status = 'active'"#.into(),
            export_formats: sql_export_formats(),
            supports_server_metrics: true,
            supports_terminate_session: true,
        }
    }
}

#[cfg(test)]
mod create_caps_tests {
    use super::*;

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
