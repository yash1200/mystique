//! Read a table definition and run a DDL plan on one connection.

use mystique_connector_api::{
    ApplyDdlResult, ColumnDraft, ConnectorError, ConnectorResult, DdlPlan, ForeignKeyDraft,
    IndexDraft, TableDefinition,
};
use rusqlite::Connection;

use super::catalog::sqlite_pragma_invoke;
use super::query_failed;
use super::SqliteSession;

pub(super) async fn describe_table_definition(
    session: &SqliteSession,
    table: &str,
) -> ConnectorResult<TableDefinition> {
    session.ensure_open()?;
    let pool = session.pool();
    let table = table.to_string();
    session
        .blocking(move || {
            let conn = pool
                .get()
                .map_err(|err| ConnectorError::Pool(err.to_string()))?;
            describe_on(&conn, &table)
        })
        .await
}

pub(super) async fn list_table_definitions(
    session: &SqliteSession,
) -> ConnectorResult<Vec<TableDefinition>> {
    session.ensure_open()?;
    let pool = session.pool();
    session
        .blocking(move || {
            let conn = pool
                .get()
                .map_err(|err| ConnectorError::Pool(err.to_string()))?;
            let mut stmt = conn
                .prepare(
                    "SELECT name FROM sqlite_master \
                     WHERE type = 'table' AND name NOT LIKE 'sqlite_%' ORDER BY name",
                )
                .map_err(query_failed)?;
            let names: Vec<String> = stmt
                .query_map([], |row| row.get(0))
                .map_err(query_failed)?
                .collect::<Result<Vec<_>, _>>()
                .map_err(query_failed)?;
            let mut out = Vec::new();
            for name in names {
                out.push(describe_on(&conn, &name)?);
            }
            Ok(out)
        })
        .await
}

pub(super) async fn apply_ddl(
    session: &SqliteSession,
    plan: &DdlPlan,
    draft: &TableDefinition,
) -> ConnectorResult<ApplyDdlResult> {
    session.ensure_open()?;
    let pool = session.pool();
    let plan = plan.clone();
    let draft = draft.clone();
    session
        .blocking(move || {
            let conn = pool
                .get()
                .map_err(|err| ConnectorError::Pool(err.to_string()))?;
            apply_on(&conn, &plan, &draft)
        })
        .await
}

fn describe_on(conn: &Connection, table: &str) -> ConnectorResult<TableDefinition> {
    let revision: Option<String> = conn
        .query_row(
            "SELECT sql FROM sqlite_master WHERE type = 'table' AND name = ?1",
            [table],
            |row| row.get(0),
        )
        .map_err(query_failed)?;
    let mut columns = Vec::new();
    let info = sqlite_pragma_invoke("table_info", table)?;
    let mut stmt = conn.prepare(&info).map_err(query_failed)?;
    let mut rows = stmt.query([]).map_err(query_failed)?;
    while let Some(row) = rows.next().map_err(query_failed)? {
        let name: String = row.get(1).map_err(query_failed)?;
        let data_type: String = row
            .get::<_, Option<String>>(2)
            .map_err(query_failed)?
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| "ANY".into());
        let not_null: i64 = row.get(3).map_err(query_failed)?;
        let pk: i64 = row.get(5).map_err(query_failed)?;
        columns.push(ColumnDraft::loaded(
            name,
            data_type,
            not_null == 0,
            u32::try_from(pk).unwrap_or(0),
        ));
    }
    drop(rows);
    drop(stmt);

    let mut indexes = Vec::new();
    let list = sqlite_pragma_invoke("index_list", table)?;
    let mut list_stmt = conn.prepare(&list).map_err(query_failed)?;
    let mut list_rows = list_stmt.query([]).map_err(query_failed)?;
    let mut index_meta = Vec::new();
    while let Some(row) = list_rows.next().map_err(query_failed)? {
        let name: String = row.get(1).map_err(query_failed)?;
        if name.starts_with("sqlite_autoindex") {
            continue;
        }
        let unique: i64 = row.get(2).map_err(query_failed)?;
        index_meta.push((name, unique != 0));
    }
    drop(list_rows);
    drop(list_stmt);
    for (name, unique) in index_meta {
        let info = sqlite_pragma_invoke("index_info", &name)?;
        let mut info_stmt = conn.prepare(&info).map_err(query_failed)?;
        let mut info_rows = info_stmt.query([]).map_err(query_failed)?;
        let mut cols = Vec::new();
        while let Some(row) = info_rows.next().map_err(query_failed)? {
            let col: String = row.get(2).map_err(query_failed)?;
            cols.push(col);
        }
        indexes.push(IndexDraft::loaded(name, cols, unique, false));
    }

    let mut foreign_keys = Vec::new();
    let fk_sql = sqlite_pragma_invoke("foreign_key_list", table)?;
    let mut fk_stmt = conn.prepare(&fk_sql).map_err(query_failed)?;
    let mut fk_rows = fk_stmt.query([]).map_err(query_failed)?;
    let mut grouped: Vec<(i64, String, Vec<String>, Vec<String>)> = Vec::new();
    while let Some(row) = fk_rows.next().map_err(query_failed)? {
        let id: i64 = row.get(0).map_err(query_failed)?;
        let ref_table: String = row.get(2).map_err(query_failed)?;
        let from: String = row.get(3).map_err(query_failed)?;
        let to: String = row.get(4).map_err(query_failed)?;
        if grouped.last().is_some_and(|item| item.0 == id) {
            if let Some(item) = grouped.last_mut() {
                item.2.push(from);
                item.3.push(to);
            }
        } else {
            grouped.push((id, ref_table, vec![from], vec![to]));
        }
    }
    for (id, ref_table, columns, ref_columns) in grouped {
        foreign_keys.push(ForeignKeyDraft::loaded(
            format!("fk_{table}_{id}"),
            columns,
            None,
            ref_table,
            ref_columns,
        ));
    }

    let mut definition = TableDefinition::loaded(None, None, table, "table");
    definition.columns = columns;
    definition.indexes = indexes;
    definition.foreign_keys = foreign_keys;
    definition.schema_revision = revision;
    Ok(definition)
}

fn apply_on(
    conn: &Connection,
    plan: &DdlPlan,
    draft: &TableDefinition,
) -> ConnectorResult<ApplyDdlResult> {
    if plan.requires_rebuild {
        let live: String = conn
            .query_row(
                "SELECT sql FROM sqlite_master WHERE type = 'table' AND name = ?1",
                [draft.name.as_str()],
                |row| row.get(0),
            )
            .map_err(query_failed)?;
        let expected = draft.schema_revision.as_deref().unwrap_or("").trim();
        if live.trim() != expected {
            return Err(ConnectorError::Query(
                "The table schema changed. Reload the structure and try again.".into(),
            ));
        }
    }
    let mut applied = Vec::new();
    let mut began = false;
    for sql in &plan.statements {
        if sql.trim().eq_ignore_ascii_case("BEGIN") {
            began = true;
        }
        if let Err(err) = conn.execute_batch(sql) {
            if began {
                let _ = conn.execute_batch("ROLLBACK");
            }
            if plan.requires_rebuild {
                let _ = conn.execute_batch("PRAGMA foreign_keys=ON");
            }
            let tail = if began {
                "The transaction was rolled back."
            } else if applied.is_empty() {
                "Nothing was applied."
            } else {
                "Earlier statements were not rolled back."
            };
            return Err(ConnectorError::Query(format!(
                "Applied {} statement(s), then failed. {tail} {err}",
                applied.len()
            )));
        }
        applied.push(sql.clone());
        if sql.trim().eq_ignore_ascii_case("COMMIT") {
            began = false;
        }
    }
    let count = applied.len();
    Ok(ApplyDdlResult {
        applied,
        message: format!("Applied {count} statement(s)."),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use mystique_connector_api::{plan_table_alter, DdlDialect};

    #[test]
    fn add_nullable_text_column_matches_the_preview() {
        let conn = match Connection::open_in_memory() {
            Ok(conn) => conn,
            Err(err) => panic!("{err}"),
        };
        if let Err(err) = conn.execute_batch("CREATE TABLE orders (id INTEGER PRIMARY KEY)") {
            panic!("{err}");
        }
        let mut draft = match describe_on(&conn, "orders") {
            Ok(draft) => draft,
            Err(err) => panic!("{err}"),
        };
        draft.columns.push(ColumnDraft {
            name: "note".into(),
            data_type: "text".into(),
            nullable: true,
            pk_position: 0,
            original: None,
            drop: false,
        });
        let plan = plan_table_alter(DdlDialect::Sqlite, &draft);
        assert!(plan.applyable(), "{:?}", plan.warnings);
        let preview = plan.statements.clone();
        let applied = match apply_on(&conn, &plan, &draft) {
            Ok(result) => result,
            Err(err) => panic!("{err}"),
        };
        assert_eq!(applied.applied, preview);
        let again = match describe_on(&conn, "orders") {
            Ok(draft) => draft,
            Err(err) => panic!("{err}"),
        };
        assert!(again.columns.iter().any(|column| {
            column.name == "note"
                && column.nullable
                && column.data_type.eq_ignore_ascii_case("text")
        }));
    }

    #[test]
    fn rebuild_aborts_when_the_schema_changed() {
        let conn = match Connection::open_in_memory() {
            Ok(conn) => conn,
            Err(err) => panic!("{err}"),
        };
        if let Err(err) = conn.execute_batch("CREATE TABLE orders (id INTEGER PRIMARY KEY)") {
            panic!("{err}");
        }
        let mut draft = match describe_on(&conn, "orders") {
            Ok(draft) => draft,
            Err(err) => panic!("{err}"),
        };
        draft.schema_revision = Some("CREATE TABLE orders (id INTEGER)".into());
        if let Some(column) = draft.columns.first_mut() {
            column.data_type = "text".into();
        }
        let plan = plan_table_alter(DdlDialect::Sqlite, &draft);
        assert!(plan.requires_rebuild, "{plan:?}");
        let err = match apply_on(&conn, &plan, &draft) {
            Err(err) => err,
            Ok(result) => panic!("rebuild ran: {result:?}"),
        };
        assert!(
            err.to_string().to_ascii_lowercase().contains("schema"),
            "{err}"
        );
        let again = match describe_on(&conn, "orders") {
            Ok(draft) => draft,
            Err(err) => panic!("{err}"),
        };
        assert!(again.columns.iter().any(|column| {
            column.name == "id" && column.data_type.eq_ignore_ascii_case("integer")
        }));
        assert!(!again.columns.iter().any(|column| column.name == "note"));
    }
}
