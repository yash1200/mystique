use std::collections::HashMap;
use std::sync::Arc;

use mystique_connector_api::{
    DatabaseConnector, EngineDescriptor, EngineQueryPolicy, MongoQueryPolicy, SqlQueryPolicy,
};

/// Plugin catalog. Adding a DB = implement a plugin + policy + register both.
pub struct ConnectorRegistry {
    connectors: HashMap<&'static str, Arc<dyn DatabaseConnector>>,
    policies: HashMap<&'static str, Arc<dyn EngineQueryPolicy>>,
}

impl Default for ConnectorRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl ConnectorRegistry {
    pub fn new() -> Self {
        Self {
            connectors: HashMap::new(),
            policies: HashMap::new(),
        }
    }

    pub fn register(&mut self, connector: Arc<dyn DatabaseConnector>) {
        self.connectors.insert(connector.id(), connector);
    }

    pub fn register_policy(&mut self, policy: Arc<dyn EngineQueryPolicy>) {
        self.policies.insert(policy.engine_id(), policy);
    }

    /// Bootstrap default engines shipped with Mystique.
    pub fn bootstrap() -> Self {
        let mut reg = Self::new();
        reg.register(mystique_plugin_postgres::connector());
        reg.register(mystique_plugin_sqlite::connector());
        reg.register(mystique_plugin_mongodb::connector());
        reg.register(mystique_plugin_mysql::connector());

        // Per-engine query policies (SPI) — not one global SQL allowlist.
        reg.register_policy(Arc::new(SqlQueryPolicy::new("postgres")));
        reg.register_policy(Arc::new(SqlQueryPolicy::new("sqlite")));
        reg.register_policy(Arc::new(SqlQueryPolicy::new("mysql")));
        reg.register_policy(Arc::new(MongoQueryPolicy));
        reg
    }

    pub fn get(&self, id: &str) -> Option<Arc<dyn DatabaseConnector>> {
        self.connectors.get(id).cloned()
    }

    pub fn policy(&self, id: &str) -> Option<Arc<dyn EngineQueryPolicy>> {
        self.policies.get(id).cloned()
    }

    pub fn list(&self) -> Vec<EngineDescriptor> {
        let mut list: Vec<_> = self.connectors.values().map(|c| c.descriptor()).collect();
        list.sort_by(|a, b| a.id.cmp(&b.id));
        list
    }

    pub fn ids(&self) -> Vec<&'static str> {
        let mut ids: Vec<_> = self.connectors.keys().copied().collect();
        ids.sort_unstable();
        ids
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mystique_connector_api::QueryMode;

    #[test]
    fn bootstrap_registers_engines() {
        let reg = ConnectorRegistry::bootstrap();
        let ids = reg.ids();
        assert!(ids.contains(&"postgres"));
        assert!(ids.contains(&"sqlite"));
        assert!(ids.contains(&"mongodb"));
        assert!(ids.contains(&"mysql"));
        assert_eq!(ids.len(), 4);
        assert!(reg.get("postgres").is_some());
        assert!(reg.get("mysql").is_some());
        let list = reg.list();
        assert_eq!(list.len(), 4);
        assert!(list.iter().all(|e| e.supports_pooling));
        assert!(reg.policy("mysql").is_some());
    }

    #[test]
    fn policies_allow_writes_and_deletes_unless_read_only() {
        let reg = ConnectorRegistry::bootstrap();
        let p = reg.policy("postgres").unwrap();
        assert!(p.validate("SELECT 1", QueryMode::Read).is_ok());
        assert!(p
            .validate("INSERT INTO t VALUES (1)", QueryMode::Write)
            .is_ok());
        assert!(p.validate("DELETE FROM t", QueryMode::Write).is_ok());
        assert!(p.validate("DROP TABLE t", QueryMode::Write).is_ok());
        assert!(p
            .validate("INSERT INTO t VALUES (1)", QueryMode::Read)
            .is_err());
        assert!(p.validate("DELETE FROM t", QueryMode::Read).is_err());

        let mongo = reg.policy("mongodb").unwrap();
        assert!(mongo
            .validate(r#"{"op":"deleteOne","filter":{"_id":1}}"#, QueryMode::Write)
            .is_ok());
        assert!(mongo
            .validate(r#"{"op":"deleteOne","filter":{"_id":1}}"#, QueryMode::Read)
            .is_err());
    }
}
