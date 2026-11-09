use std::path::PathBuf;
use std::time::Duration;

use mystique_connector_api::{
    ConnectionConfig, ConnectorError, ConnectorResult, FormattedConnectionUrl,
};
use r2d2::Pool;
use r2d2_sqlite::SqliteConnectionManager;
use rusqlite::OpenFlags;

pub(crate) fn resolve_path(config: &ConnectionConfig) -> ConnectorResult<PathBuf> {
    let path = config
        .str_field("path")
        .or_else(|| config.str_field("database"))
        .ok_or_else(|| ConnectorError::InvalidConfig("missing `path`".into()))?;
    Ok(PathBuf::from(path))
}

/// Format SQLite path as a `file://` URL (or plain path for relative files).
pub(crate) fn format_sqlite_url(
    config: &ConnectionConfig,
) -> ConnectorResult<FormattedConnectionUrl> {
    let path = config
        .str_field("path")
        .or_else(|| config.str_field("database"))
        .ok_or_else(|| ConnectorError::InvalidConfig("missing `path`".into()))?;
    let path = path.trim();
    if path.is_empty() {
        return Err(ConnectorError::InvalidConfig("missing `path`".into()));
    }

    let bytes = path.as_bytes();
    let windows_drive = bytes.first().is_some_and(|b| b.is_ascii_alphabetic())
        && bytes.get(1) == Some(&b':')
        && matches!(bytes.get(2), Some(b'\\' | b'/'));
    let url = if path.starts_with('/') {
        format!("file://{path}")
    } else if windows_drive {
        let normalized = path.replace('\\', "/");
        format!("file:///{normalized}")
    } else {
        path.to_string()
    };

    Ok(FormattedConnectionUrl {
        engine: "sqlite".into(),
        url,
    })
}

pub(super) fn build_pool(
    config: &ConnectionConfig,
) -> ConnectorResult<Pool<SqliteConnectionManager>> {
    let path = resolve_path(config)?;
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() && !parent.exists() {
            return Err(ConnectorError::InvalidConfig(format!(
                "parent directory does not exist: {}",
                parent.display()
            )));
        }
    }

    let flags = if config.bool_field("readOnly", false) || config.bool_field("read_only", false) {
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX
    } else {
        OpenFlags::SQLITE_OPEN_READ_WRITE
            | OpenFlags::SQLITE_OPEN_CREATE
            | OpenFlags::SQLITE_OPEN_NO_MUTEX
    };

    let manager = SqliteConnectionManager::file(&path).with_flags(flags);
    let pool = Pool::builder()
        .max_size(config.pool.max_size.max(1))
        .min_idle(Some(config.pool.min_idle.min(config.pool.max_size)))
        .connection_timeout(Duration::from_millis(config.pool.acquire_timeout_ms))
        .build(manager)
        .map_err(|e| ConnectorError::Pool(e.to_string()))?;
    Ok(pool)
}

#[cfg(test)]
mod tests {
    use super::*;
    use mystique_connector_api::PoolConfig;
    use serde_json::json;

    #[test]
    fn format_absolute_unix_path() {
        let config = mystique_connector_api::ConnectionConfig {
            engine: "sqlite".into(),
            plugin_config: json!({ "path": "/tmp/app.sqlite" }),
            password: None,
            pool: PoolConfig::default(),
        };
        let formatted = format_sqlite_url(&config).expect("format");
        assert_eq!(formatted.url, "file:///tmp/app.sqlite");
    }

    #[test]
    fn resolve_path_requires_field() {
        let config = mystique_connector_api::ConnectionConfig {
            engine: "sqlite".into(),
            plugin_config: json!({}),
            password: None,
            pool: PoolConfig::default(),
        };
        let err = resolve_path(&config).unwrap_err();
        assert!(err.to_string().contains("path"), "{err}");
    }
}
