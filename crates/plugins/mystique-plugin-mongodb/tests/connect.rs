use mystique_connector_api::{ConnectionConfig, DatabaseConnector, PoolConfig};
use mystique_plugin_mongodb::MongoConnector;
use serde_json::json;

#[tokio::test]
#[ignore = "needs local MongoDB"]
async fn test_connection_against_local_docker() {
    let config = ConnectionConfig {
        engine: "mongodb".into(),
        plugin_config: json!({
            "uri": "mongodb://admin:123@127.0.0.1:27017/mystique?authSource=admin",
            "database": "mystique",
        }),
        password: None,
        pool: PoolConfig {
            max_size: 2,
            min_idle: 0,
            acquire_timeout_ms: 5000,
            idle_timeout_secs: 30,
        },
    };
    match MongoConnector.test_connection(&config).await {
        Ok(r) => assert!(r.ok, "expected ok: {}", r.message),
        Err(e) => panic!("mongo test_connection failed: {e}"),
    }
}
