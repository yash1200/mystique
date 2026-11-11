//! What the MySQL plugin supports (MariaDB and TiDB share it).

use mystique_connector_api::{EngineCapabilities, ObjectKindSpec};

pub fn default_capabilities() -> EngineCapabilities {
    EngineCapabilities {
        object_kinds: vec![
            ObjectKindSpec::table(),
            ObjectKindSpec::view(),
            ObjectKindSpec::new("function", "function", "Functions"),
            ObjectKindSpec::new("procedure", "procedure", "Procedures"),
            ObjectKindSpec::new("trigger", "trigger", "Triggers"),
        ],
        binds_session_to_database: true,
        name_example: "e.g. local-mysql".into(),
        ..EngineCapabilities::relational("mysql", "mysql")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mystique_connector_api::SupportDepth;

    #[test]
    fn mysql_operates_with_metrics_explain_and_cancel() {
        let caps = default_capabilities();
        assert_eq!(caps.engine, "mysql");
        assert_eq!(caps.support_depth, SupportDepth::Operate);
        assert!(caps.supports_cancel_query);
        assert!(caps.supports_explain_plan);
        assert!(caps.supports_server_metrics);
        assert!(caps.supports_terminate_session);
        assert!(caps.explorer.multi_database);
        assert_eq!(caps.explorer.layout, "flat");
    }
}
