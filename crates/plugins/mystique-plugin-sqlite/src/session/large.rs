//! Re-read one clipped cell by primary key.

use std::sync::Arc;

use mystique_connector_api::{
    large_cell_select, page_cell, ConnectorError, ConnectorResult, FetchLargeValueInput,
    FetchLargeValueResult, SqlIdentStyle, SqlPlaceholder, SqlTextCast,
};
use rusqlite::OptionalExtension;

use super::encode::value_ref_json;
use super::interrupt::{self, map_err};
use super::SqliteSession;

pub(super) async fn fetch_large_value(
    session: &SqliteSession,
    input: &FetchLargeValueInput,
) -> ConnectorResult<FetchLargeValueResult> {
    session.ensure_open()?;
    let select = large_cell_select(
        SqlIdentStyle::Postgres,
        SqlPlaceholder::Question,
        SqlTextCast::Text,
        &input.locator,
    )?;
    let pool = session.pool();
    let control = Arc::clone(&input.control);
    let offset = input.offset;
    let limit = input.limit;
    let params = select.params.clone();
    let sql = select.sql.clone();
    session
        .blocking(move || -> ConnectorResult<FetchLargeValueResult> {
            let conn = pool
                .get()
                .map_err(|e| ConnectorError::Pool(e.to_string()))?;
            interrupt::arm(&conn, &control);
            let _settle = interrupt::Settle {
                conn: &conn,
                control: &control,
            };
            let cell = conn
                .query_row(&sql, rusqlite::params_from_iter(params.iter()), |row| {
                    let value = row.get_ref(0)?;
                    Ok(value_ref_json(value))
                })
                .optional()
                .map_err(|err| map_err(&control, err))?;
            let Some(cell) = cell else {
                return Err(ConnectorError::Query("No row matched that key.".into()));
            };
            Ok(page_cell(&cell, offset, limit))
        })
        .await
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    use mystique_connector_api::{
        clip_table_preview, ConnectionConfig, DatabaseConnector, FetchLargeValueInput,
        FetchTablePreviewInput, LargeCellLocator, PoolConfig, QueryControl, QueryMode,
        ValidatedQuery, LARGE_CELL_BYTES,
    };
    use serde_json::json;

    use crate::SqliteConnector;

    fn temp_path() -> std::path::PathBuf {
        let nanos = match SystemTime::now().duration_since(UNIX_EPOCH) {
            Ok(duration) => duration.as_nanos(),
            Err(_) => 0,
        };
        std::env::temp_dir().join(format!(
            "mystique-large-{}-{nanos}.sqlite",
            std::process::id()
        ))
    }

    #[tokio::test]
    async fn a_big_text_cell_stays_out_of_the_preview_and_loads_by_int8_key() {
        let path = temp_path();
        let _ = fs::remove_file(&path);
        let session = match SqliteConnector
            .connect(&ConnectionConfig {
                engine: "sqlite".into(),
                plugin_config: json!({ "path": path.to_string_lossy() }),
                password: None,
                pool: PoolConfig {
                    max_size: 1,
                    min_idle: 0,
                    acquire_timeout_ms: 3_000,
                    idle_timeout_secs: 5,
                },
            })
            .await
        {
            Ok(session) => session,
            Err(err) => panic!("connect: {err}"),
        };

        let one = format!("{}NEEDLE-ONE", "x".repeat(200_000));
        let two = format!("{}NEEDLE-TWO", "y".repeat(200_000));
        let seed = ValidatedQuery {
            text: format!(
                "CREATE TABLE docs (id INTEGER PRIMARY KEY, body TEXT);\
                 INSERT INTO docs (id, body) VALUES (9007199254740993, '{one}');\
                 INSERT INTO docs (id, body) VALUES (9007199254740995, '{two}');"
            ),
            mode: QueryMode::Write,
            max_rows: 10,
            timeout_ms: 5_000,
            collection: None,
            database: None,
            control: QueryControl::new(),
        };
        if let Err(err) = session.execute_query(&seed).await {
            panic!("seed: {err}");
        }

        let mut page = match session
            .fetch_table_preview(&FetchTablePreviewInput {
                table: "docs".into(),
                schema: None,
                database: None,
                max_rows: 10,
                offset: 0,
                filters: Vec::new(),
                code_filter: None,
                sort_column: None,
                sort_dir: None,
                control: QueryControl::new(),
            })
            .await
        {
            Ok(page) => page,
            Err(err) => panic!("preview: {err}"),
        };
        clip_table_preview(&mut page, None, None, "docs");
        let encoded = serde_json::to_string(&page.rows).unwrap_or_default();
        assert!(
            !encoded.contains("NEEDLE-ONE") && !encoded.contains("NEEDLE-TWO"),
            "preview ipc still holds the payload ({len} bytes)",
            len = encoded.len()
        );
        assert!(encoded.len() < LARGE_CELL_BYTES * 2);

        let locator = page
            .rows
            .iter()
            .find_map(|row| -> Option<LargeCellLocator> {
                let body = row.get(1)?;
                let keys = body.get("locator")?.get("keys")?.as_array()?;
                let id = keys.first()?.get("value")?.as_str()?;
                if id == "9007199254740993" {
                    serde_json::from_value(body.get("locator")?.clone()).ok()
                } else {
                    None
                }
            })
            .unwrap_or_else(|| panic!("missing locator for the int8 key"));

        let loaded = match session
            .fetch_large_value(&FetchLargeValueInput {
                locator: locator.clone(),
                offset: 0,
                limit: 32_000,
                control: QueryControl::new(),
            })
            .await
        {
            Ok(loaded) => loaded,
            Err(err) => panic!("fetch: {err}"),
        };
        assert!(loaded.total_chars > 200_000);
        assert!(!loaded.text.contains("NEEDLE"));
        assert!(loaded.next_offset.is_some());

        let mut offset = 0;
        let mut whole = String::new();
        loop {
            let page = match session
                .fetch_large_value(&FetchLargeValueInput {
                    locator: locator.clone(),
                    offset,
                    limit: 32_000,
                    control: QueryControl::new(),
                })
                .await
            {
                Ok(page) => page,
                Err(err) => panic!("page: {err}"),
            };
            whole.push_str(&page.text);
            match page.next_offset {
                Some(next) => offset = next,
                None => break,
            }
        }
        assert!(whole.ends_with("NEEDLE-ONE"), "loaded the wrong row");
        assert!(!whole.contains("NEEDLE-TWO"));
        assert_eq!(whole.chars().count() as u64, loaded.total_chars);

        let _ = session.close().await;
        let _ = fs::remove_file(&path);
    }
}
