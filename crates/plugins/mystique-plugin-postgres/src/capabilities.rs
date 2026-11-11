//! What the Postgres plugin supports. Product overlays (CockroachDB, …) are
//! applied by core on top of this.

use mystique_connector_api::{EngineCapabilities, ExplorerConfig, ObjectKindSpec};

pub fn default_capabilities() -> EngineCapabilities {
    EngineCapabilities {
        object_kinds: vec![
            ObjectKindSpec::table(),
            ObjectKindSpec::view(),
            ObjectKindSpec::new("matview", "materialized view", "Materialized views").browsable(),
            ObjectKindSpec::new("sequence", "sequence", "Sequences").browsable(),
            ObjectKindSpec::new("function", "function", "Functions"),
            ObjectKindSpec::new("procedure", "procedure", "Procedures"),
            ObjectKindSpec::new("type", "type", "Types"),
        ],
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

    #[test]
    fn only_tables_views_and_matviews_or_sequences_open_a_grid() {
        let caps = default_capabilities();
        let browsable: Vec<_> = caps
            .object_kinds
            .iter()
            .filter(|k| k.browsable)
            .map(|k| k.id.as_str())
            .collect();
        assert_eq!(browsable, ["table", "view", "matview", "sequence"]);
        let editable: Vec<_> = caps
            .object_kinds
            .iter()
            .filter(|k| k.editable)
            .map(|k| k.id.as_str())
            .collect();
        assert_eq!(editable, ["table"]);
    }
}
