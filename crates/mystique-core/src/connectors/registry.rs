use std::collections::HashMap;
use std::sync::Arc;

use mystique_connector_api::{DatabaseConnector, EngineDescriptor};

/// Plugin catalog. Adding a DB = implement a plugin + one `register` call.
pub struct ConnectorRegistry {
    connectors: HashMap<&'static str, Arc<dyn DatabaseConnector>>,
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
        }
    }

    pub fn register(&mut self, connector: Arc<dyn DatabaseConnector>) {
        self.connectors.insert(connector.id(), connector);
    }

    /// Bootstrap default engines shipped with MystiqueDB.
    pub fn bootstrap() -> Self {
        let mut reg = Self::new();
        reg.register(mystique_plugin_postgres::connector());
        reg.register(mystique_plugin_sqlite::connector());
        reg.register(mystique_plugin_mongodb::connector());
        reg
    }

    pub fn get(&self, id: &str) -> Option<Arc<dyn DatabaseConnector>> {
        self.connectors.get(id).cloned()
    }

    pub fn list(&self) -> Vec<EngineDescriptor> {
        let mut list: Vec<_> = self
            .connectors
            .values()
            .map(|c| c.descriptor())
            .collect();
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

    #[test]
    fn bootstrap_registers_three_engines() {
        let reg = ConnectorRegistry::bootstrap();
        let ids = reg.ids();
        assert!(ids.contains(&"postgres"));
        assert!(ids.contains(&"sqlite"));
        assert!(ids.contains(&"mongodb"));
        assert_eq!(ids.len(), 3);
        assert!(reg.get("postgres").is_some());
        let list = reg.list();
        assert_eq!(list.len(), 3);
        assert!(list.iter().all(|e| e.supports_pooling));
    }
}
