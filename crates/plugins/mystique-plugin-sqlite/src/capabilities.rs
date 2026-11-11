//! What the SQLite plugin supports.

use mystique_connector_api::{ConnectionKind, EngineCapabilities, ExplorerConfig, ObjectKindSpec};

pub fn default_capabilities() -> EngineCapabilities {
    EngineCapabilities {
        object_kinds: vec![
            ObjectKindSpec::table(),
            ObjectKindSpec::view(),
            ObjectKindSpec::new("trigger", "trigger", "Triggers"),
        ],
        connection_kind: ConnectionKind::File,
        name_example: "e.g. app-dev.sqlite".into(),
        explorer: ExplorerConfig::relational_flat(false, false),
        // One file: no other backends to terminate.
        supports_terminate_session: false,
        ..EngineCapabilities::relational("sqlite", "sqlite")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mystique_connector_api::SupportDepth;

    #[test]
    fn sqlite_is_a_file_with_metrics_and_no_terminate() {
        let caps = default_capabilities();
        assert_eq!(caps.engine, "sqlite");
        assert_eq!(caps.support_depth, SupportDepth::Operate);
        assert_eq!(caps.connection_kind, ConnectionKind::File);
        assert!(caps.supports_cancel_query);
        assert!(caps.supports_explain_plan);
        assert!(caps.supports_server_metrics);
        assert!(!caps.supports_terminate_session);
        assert!(!caps.binds_session_to_database);
        assert!(!caps.structure_edit_limited);
    }
}
