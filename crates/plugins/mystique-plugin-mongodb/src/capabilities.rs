//! What the MongoDB plugin supports. A session turns transactions on when
//! the deployment is a replica set.

use mystique_connector_api::EngineCapabilities;

pub fn default_capabilities() -> EngineCapabilities {
    EngineCapabilities {
        prefer_database_from_uri: true,
        system_databases: vec!["admin".into(), "local".into(), "config".into()],
        name_example: "e.g. local-mongo".into(),
        ..EngineCapabilities::document("mongodb", "MQL")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mystique_connector_api::{ConnectionKind, SupportDepth};

    #[test]
    fn mongodb_is_a_uri_document_store_that_can_create_collections() {
        let caps = default_capabilities();
        assert_eq!(caps.engine, "mongodb");
        assert_eq!(caps.support_depth, SupportDepth::Operate);
        assert_eq!(caps.connection_kind, ConnectionKind::Uri);
        assert!(caps.supports_cancel_query);
        assert!(caps.supports_explain_plan);
        assert!(caps.supports_server_metrics);
        assert!(caps.supports_terminate_session);
        assert!(!caps.supports_transactions);
        assert!(caps.explorer.can_create_object);
        assert!(!caps.explorer.can_create_group);
        assert!(caps.explorer.can_create_database);
        assert_eq!(caps.explorer.object_label_singular, "collection");
    }
}
