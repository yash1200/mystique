use std::collections::HashMap;
use std::sync::Arc;

use mystique_connector_api::{
    product, runtime_id, DatabaseConnector, EngineDescriptor, EngineQueryPolicy, SqlQueryPolicy,
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
        // Mongo's policy reads the same shell parse its executor runs.
        reg.register_policy(mystique_plugin_mongodb::query_policy());
        reg
    }

    /// Crate id that opens the pool for this product (`cockroachdb` → `postgres`).
    pub fn runtime_id(id: &str) -> Option<&'static str> {
        runtime_id(id)
    }

    pub fn product(id: &str) -> Option<&'static mystique_connector_api::EngineProduct> {
        product(id)
    }

    /// Resolve a **product** id (or alias) to the runtime connector.
    pub fn get(&self, id: &str) -> Option<Arc<dyn DatabaseConnector>> {
        let runtime = runtime_id(id).unwrap_or(id.trim());
        self.connectors
            .get(runtime)
            .or_else(|| self.connectors.get(id.trim()))
            .cloned()
    }

    pub fn policy(&self, id: &str) -> Option<Arc<dyn EngineQueryPolicy>> {
        let runtime = runtime_id(id).unwrap_or(id.trim());
        self.policies
            .get(runtime)
            .or_else(|| self.policies.get(id.trim()))
            .cloned()
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
    fn get_resolves_products_to_runtime_connectors() {
        let reg = ConnectorRegistry::bootstrap();
        assert_eq!(reg.get("mariadb").unwrap().id(), "mysql");
        assert_eq!(reg.get("tidb").unwrap().id(), "mysql");
        assert_eq!(reg.get("cockroachdb").unwrap().id(), "postgres");
        assert_eq!(reg.get("documentdb").unwrap().id(), "mongodb");
        assert_eq!(reg.get("postgresql").unwrap().id(), "postgres");
        assert_eq!(reg.get("PG").unwrap().id(), "postgres");
        assert_eq!(reg.get("mongo").unwrap().id(), "mongodb");
        assert_eq!(reg.get("MYSQL").unwrap().id(), "mysql");
        assert!(reg.get("oracle").is_none());
        assert_eq!(
            ConnectorRegistry::runtime_id("cockroachdb"),
            Some("postgres")
        );
        assert!(reg.policy("mariadb").is_some());
        assert!(reg.policy("cockroachdb").is_some());
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
        assert!(p
            .validate(
                "WITH x AS (SELECT 1) INSERT INTO t SELECT * FROM x",
                QueryMode::Read
            )
            .is_err());
        assert!(p
            .validate("WITH cte AS (SELECT 1) SELECT * FROM cte", QueryMode::Read)
            .is_ok());
        assert!(p
            .validate("EXPLAIN ANALYZE DELETE FROM t", QueryMode::Read)
            .is_err());

        let mysql = reg.policy("mysql").unwrap();
        assert!(mysql
            .validate(
                "WITH x AS (SELECT 1) INSERT INTO t SELECT * FROM x",
                QueryMode::Read
            )
            .is_err());

        let mongo = reg.policy("mongodb").unwrap();
        assert!(mongo
            .validate(r#"{"op":"deleteOne","filter":{"_id":1}}"#, QueryMode::Write)
            .is_ok());
        assert!(mongo
            .validate(r#"{"op":"deleteOne","filter":{"_id":1}}"#, QueryMode::Read)
            .is_err());
    }
}
