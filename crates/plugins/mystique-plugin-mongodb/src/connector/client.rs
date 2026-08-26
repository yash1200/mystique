use std::time::Duration;

use mongodb::options::{ClientOptions, Credential, ServerAddress};
use mongodb::Client;
use mystique_connector_api::{ConnectionConfig, ConnectorError, ConnectorResult};

use super::url::database_from_mongo_uri;

/// Build a Mongo client and resolve the default application database.
///
/// Default DB resolution order:
/// 1. explicit `pluginConfig.database`
/// 2. path segment in the connection URI (`…/mystique?…`)
/// 3. `None` (caller / explorer must pick a database)
pub(super) async fn build_client(
    config: &ConnectionConfig,
) -> ConnectorResult<(Client, Option<String>)> {
    let mut options = if let Some(uri) = config.str_field("uri") {
        ClientOptions::parse(&uri)
            .await
            .map_err(|e| ConnectorError::InvalidConfig(e.to_string()))?
    } else {
        let host = config
            .str_field("host")
            .unwrap_or_else(|| "127.0.0.1".into());
        let port = config.u16_field("port", 27017);
        let mut opts = ClientOptions::default();
        opts.hosts = vec![ServerAddress::Tcp {
            host,
            port: Some(port),
        }];
        opts
    };

    // Pool sizing (driver-managed)
    options.max_pool_size = Some(config.pool.max_size);
    options.min_pool_size = Some(config.pool.min_idle.min(config.pool.max_size));
    options.connect_timeout = Some(Duration::from_millis(config.pool.acquire_timeout_ms));
    if config.pool.idle_timeout_secs > 0 {
        options.max_idle_time = Some(Duration::from_secs(config.pool.idle_timeout_secs));
    }

    // Auth from fields if not already embedded in URI
    let user = config.str_field("username");
    let password = config
        .password
        .clone()
        .or_else(|| config.str_field("password"));
    if let (Some(username), Some(password)) = (user, password) {
        let mut cred = Credential::default();
        cred.username = Some(username);
        cred.password = Some(password);
        // Prefer explicit authSource; default to admin (root-style users), not the app DB name.
        cred.source = Some(
            config
                .str_field("authSource")
                .or_else(|| config.str_field("auth_source"))
                .unwrap_or_else(|| "admin".into()),
        );
        options.credential = Some(cred);
    }

    options.app_name = Some("Mystique".into());

    let default_db = config
        .str_field("database")
        .filter(|s| !s.is_empty())
        .or_else(|| options.default_database.clone())
        .or_else(|| {
            config
                .str_field("uri")
                .and_then(|u| database_from_mongo_uri(&u))
        });

    let client =
        Client::with_options(options).map_err(|e| ConnectorError::Connect(e.to_string()))?;
    Ok((client, default_db))
}
