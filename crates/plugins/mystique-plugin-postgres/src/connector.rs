use std::time::Instant;

use async_trait::async_trait;
use deadpool_postgres::{Config as PoolCfg, ManagerConfig, RecyclingMethod, Runtime};
use mystique_connector_api::{
    ConnectionConfig, ConnectionFormSchema, ConnectorError, ConnectorResult, ConnectorSession,
    DatabaseConnector, EngineCapabilities, EngineFamily, FormField, FormFieldType, TestResult,
};
use tokio_postgres::NoTls;

use crate::session::PostgresSession;

pub struct PostgresConnector;

fn build_pool_config(config: &ConnectionConfig) -> ConnectorResult<PoolCfg> {
    let host = config
        .str_field("host")
        .unwrap_or_else(|| "127.0.0.1".into());
    let port = config.u16_field("port", 5432);
    let dbname = config.require_str("database")?;
    let user = config
        .str_field("username")
        .or_else(|| config.str_field("user"))
        .unwrap_or_else(|| "postgres".into());

    let mut cfg = PoolCfg::new();
    cfg.host = Some(host);
    cfg.port = Some(port);
    cfg.dbname = Some(dbname);
    cfg.user = Some(user);
    if let Some(pw) = config.password.clone() {
        cfg.password = Some(pw);
    } else if let Some(pw) = config.str_field("password") {
        cfg.password = Some(pw);
    }
    cfg.manager = Some(ManagerConfig {
        recycling_method: RecyclingMethod::Fast,
    });
    // deadpool PoolConfig
    let mut pool = deadpool_postgres::PoolConfig::new(config.pool.max_size as usize);
    pool.timeouts.wait = Some(std::time::Duration::from_millis(
        config.pool.acquire_timeout_ms,
    ));
    if config.pool.idle_timeout_secs > 0 {
        pool.timeouts.recycle = Some(std::time::Duration::from_secs(config.pool.idle_timeout_secs));
    }
    cfg.pool = Some(pool);
    Ok(cfg)
}

#[async_trait]
impl DatabaseConnector for PostgresConnector {
    fn id(&self) -> &'static str {
        "postgres"
    }

    fn display_name(&self) -> &'static str {
        "PostgreSQL"
    }

    fn family(&self) -> EngineFamily {
        EngineFamily::Relational
    }

    fn description(&self) -> &'static str {
        "Relational studio with connection pooling (deadpool + tokio-postgres)"
    }

    fn capabilities(&self) -> EngineCapabilities {
        EngineCapabilities::postgres_defaults()
    }

    fn connection_form_schema(&self) -> ConnectionFormSchema {
        ConnectionFormSchema {
            engine: "postgres".into(),
            fields: vec![
                FormField {
                    name: "host".into(),
                    label: "Host".into(),
                    field_type: FormFieldType::Text,
                    required: true,
                    secret: false,
                    default: Some(serde_json::json!("127.0.0.1")),
                    placeholder: None,
                    hint: None,
                },
                FormField {
                    name: "port".into(),
                    label: "Port".into(),
                    field_type: FormFieldType::Number,
                    required: true,
                    secret: false,
                    default: Some(serde_json::json!(5432)),
                    placeholder: None,
                    hint: None,
                },
                FormField {
                    name: "database".into(),
                    label: "Database".into(),
                    field_type: FormFieldType::Text,
                    required: true,
                    secret: false,
                    default: None,
                    placeholder: None,
                    hint: None,
                },
                FormField {
                    name: "username".into(),
                    label: "Username".into(),
                    field_type: FormFieldType::Text,
                    required: false,
                    secret: false,
                    default: Some(serde_json::json!("postgres")),
                    placeholder: None,
                    hint: None,
                },
                FormField {
                    name: "password".into(),
                    label: "Password".into(),
                    field_type: FormFieldType::Password,
                    required: false,
                    secret: true,
                    default: None,
                    placeholder: None,
                    hint: None,
                },
            ],
        }
    }

    async fn test_connection(&self, config: &ConnectionConfig) -> ConnectorResult<TestResult> {
        let start = Instant::now();
        let pool_cfg = build_pool_config(config)?;
        // tiny pool for a one-shot test
        let mut test_cfg = pool_cfg;
        let mut pc = deadpool_postgres::PoolConfig::new(1);
        pc.timeouts.wait = Some(std::time::Duration::from_millis(
            config.pool.acquire_timeout_ms,
        ));
        test_cfg.pool = Some(pc);

        let pool = test_cfg
            .create_pool(Some(Runtime::Tokio1), NoTls)
            .map_err(|e| ConnectorError::Pool(e.to_string()))?;

        let client = pool
            .get()
            .await
            .map_err(|e| ConnectorError::Connect(e.to_string()))?;

        let row = client
            .query_one("SELECT version()", &[])
            .await
            .map_err(|e| ConnectorError::Query(e.to_string()))?;
        let version: String = row.get(0);
        let latency_ms = start.elapsed().as_millis() as u64;
        pool.close();

        Ok(TestResult {
            ok: true,
            engine: "postgres".into(),
            server_version: Some(version),
            latency_ms,
            message: "Connected".into(),
        })
    }

    async fn connect(
        &self,
        config: &ConnectionConfig,
    ) -> ConnectorResult<Box<dyn ConnectorSession>> {
        let pool_cfg = build_pool_config(config)?;
        let pool = pool_cfg
            .create_pool(Some(Runtime::Tokio1), NoTls)
            .map_err(|e| ConnectorError::Pool(e.to_string()))?;

        // Warm / validate: acquire one connection
        {
            let client = pool
                .get()
                .await
                .map_err(|e| ConnectorError::Connect(e.to_string()))?;
            client
                .simple_query("SELECT 1")
                .await
                .map_err(|e| ConnectorError::Connect(e.to_string()))?;
        }

        Ok(Box::new(PostgresSession::new(
            pool,
            config.pool.max_size,
        )))
    }
}
