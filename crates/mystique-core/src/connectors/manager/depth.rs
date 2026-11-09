//! Reject IPC that sits outside a product's support depth.
//!
//! The UI hides the chrome. This is the lock: a `browse` product cannot
//! reach table admin, metrics, or ER by calling the command directly.

use mystique_connector_api::{ConnectorError, ConnectorResult, EngineCapabilities, TableAdminOp};

pub(super) fn reject_beyond_depth(
    caps: &EngineCapabilities,
    feature: DepthFeature,
) -> ConnectorResult<()> {
    if feature.allowed(caps) {
        Ok(())
    } else {
        Err(ConnectorError::NotSupported(feature.message().into()))
    }
}

#[derive(Clone, Copy)]
pub(super) enum DepthFeature {
    Metrics,
    Terminate,
    Er,
    TableAdmin(TableAdminOp),
}

impl DepthFeature {
    fn allowed(self, caps: &EngineCapabilities) -> bool {
        match self {
            Self::Metrics => caps.supports_server_metrics,
            Self::Terminate => caps.supports_terminate_session,
            Self::Er => caps.supports_er_diagram || caps.supports_model_view,
            Self::TableAdmin(op) => admin_allowed(caps, op),
        }
    }

    fn message(self) -> &'static str {
        match self {
            Self::Metrics => "Server metrics are outside this product's support depth.",
            Self::Terminate => "Terminating a backend session is not available for this product.",
            Self::Er => "ER and model view are outside this product's support depth.",
            Self::TableAdmin(_) => "Table admin is outside this product's support depth.",
        }
    }
}

fn admin_allowed(caps: &EngineCapabilities, op: TableAdminOp) -> bool {
    match op {
        TableAdminOp::CreateStatement => caps.table_admin.create_statement,
        TableAdminOp::CreateGroup => caps.explorer.can_create_group,
        TableAdminOp::CreateObject => caps.explorer.can_create_object,
        TableAdminOp::Rename => caps.table_admin.rename,
        TableAdminOp::Drop => caps.table_admin.drop,
        TableAdminOp::Truncate => caps.table_admin.truncate,
        TableAdminOp::Duplicate => caps.table_admin.duplicate,
    }
}

#[cfg(test)]
mod tests {
    use mystique_connector_api::SupportDepth;

    use super::*;

    /// Postgres stays operate. A product pinned to browse is rejected.
    fn caps_at(depth: SupportDepth) -> EngineCapabilities {
        let mut caps = EngineCapabilities::postgres_defaults();
        caps.apply_support_depth(depth);
        caps
    }

    #[test]
    fn browse_rejects_admin_metrics_and_er() {
        let caps = caps_at(SupportDepth::Browse);
        assert!(reject_beyond_depth(&caps, DepthFeature::Metrics).is_err());
        assert!(reject_beyond_depth(&caps, DepthFeature::Terminate).is_err());
        assert!(reject_beyond_depth(&caps, DepthFeature::Er).is_err());
        assert!(reject_beyond_depth(&caps, DepthFeature::TableAdmin(TableAdminOp::Drop)).is_err());
        let err = reject_beyond_depth(&caps, DepthFeature::Metrics).unwrap_err();
        assert!(matches!(err, ConnectorError::NotSupported(_)), "{err}");
    }

    #[test]
    fn operate_without_terminate_still_serves_metrics() {
        let mut caps = caps_at(SupportDepth::Operate);
        caps.supports_terminate_session = false;
        assert!(reject_beyond_depth(&caps, DepthFeature::Metrics).is_ok());
        assert!(reject_beyond_depth(&caps, DepthFeature::Terminate).is_err());
    }

    #[test]
    fn operate_postgres_allows_admin_metrics_and_er() {
        let caps = caps_at(SupportDepth::Operate);
        assert!(reject_beyond_depth(&caps, DepthFeature::Metrics).is_ok());
        assert!(reject_beyond_depth(&caps, DepthFeature::Terminate).is_ok());
        assert!(reject_beyond_depth(&caps, DepthFeature::Er).is_ok());
        assert!(reject_beyond_depth(&caps, DepthFeature::TableAdmin(TableAdminOp::Drop)).is_ok());
        assert!(reject_beyond_depth(
            &caps,
            DepthFeature::TableAdmin(TableAdminOp::CreateStatement)
        )
        .is_ok());
    }
}
