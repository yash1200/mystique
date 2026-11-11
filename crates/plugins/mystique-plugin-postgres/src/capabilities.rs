//! What the Postgres plugin supports. Product overlays (CockroachDB, …) are
//! applied by core on top of this.

use mystique_connector_api::{EngineCapabilities, ExplorerConfig};

pub fn default_capabilities() -> EngineCapabilities {
    EngineCapabilities {
        binds_session_to_database: true,
        name_example: "e.g. local-pg".into(),
        explorer: ExplorerConfig::relational_schema_tree(),
        ..EngineCapabilities::relational("postgres", "postgresql")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mystique_connector_api::SupportDepth;

    #[test]
    fn postgres_operates_with_metrics_explain_and_cancel() {
        let caps = default_capabilities();
        assert_eq!(caps.engine, "postgres");
        assert_eq!(caps.support_depth, SupportDepth::Operate);
        assert!(caps.supports_cancel_query);
        assert!(caps.supports_explain_plan);
        assert!(caps.supports_server_metrics);
        assert!(caps.supports_terminate_session);
        assert!(!caps.structure_edit_limited);
        assert_eq!(caps.explorer.layout, "schema-tree");
    }
}
