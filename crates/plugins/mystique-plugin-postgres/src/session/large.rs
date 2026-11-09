//! Name the source columns of a query result, so a clipped cell can carry a
//! locator. The value itself is re-read by core through the normal bound
//! query path (`"id" = $1`, the parameter typed by the server).

use std::collections::HashMap;

use mystique_connector_api::{ColumnOrigin, ConnectorResult};

use super::{pg_col, query_failed, PostgresSession};

/// RowDescription gives `(table oid, attnum)`. One catalog read names every
/// column. Relations other than plain and partitioned tables are left out:
/// a view or a foreign table has no key to match on.
pub(super) async fn name_column_origins(
    session: &PostgresSession,
    origins: Vec<Option<ColumnOrigin>>,
) -> ConnectorResult<Vec<Option<ColumnOrigin>>> {
    let mut oids: Vec<u32> = origins
        .iter()
        .flatten()
        .filter_map(|origin| origin.table_oid)
        .collect();
    oids.sort_unstable();
    oids.dedup();
    if oids.is_empty() {
        return Ok(origins);
    }
    session.ensure_open()?;
    let client = session.client().await?;
    let rows = client
        .query(
            r#"
            SELECT c.oid, n.nspname, c.relname, a.attnum, a.attname
            FROM pg_catalog.pg_class c
            JOIN pg_catalog.pg_namespace n ON n.oid = c.relnamespace
            JOIN pg_catalog.pg_attribute a ON a.attrelid = c.oid
            WHERE c.oid = ANY($1)
              AND c.relkind IN ('r', 'p')
              AND a.attnum > 0
              AND NOT a.attisdropped
            "#,
            &[&oids],
        )
        .await
        .map_err(query_failed)?;
    let mut names: HashMap<(u32, i16), (String, String, String)> = HashMap::new();
    for row in rows {
        let oid: u32 = pg_col(&row, 0)?;
        let schema: String = pg_col(&row, 1)?;
        let table: String = pg_col(&row, 2)?;
        let attnum: i16 = pg_col(&row, 3)?;
        let column: String = pg_col(&row, 4)?;
        names.insert((oid, attnum), (schema, table, column));
    }
    Ok(origins
        .into_iter()
        .map(|origin| {
            let origin = origin?;
            if origin.has_names() {
                return Some(origin);
            }
            let key = (origin.table_oid?, origin.column_id?);
            let (schema, table, column) = names.get(&key)?.clone();
            Some(ColumnOrigin::named(None, Some(schema), table, column))
        })
        .collect())
}
