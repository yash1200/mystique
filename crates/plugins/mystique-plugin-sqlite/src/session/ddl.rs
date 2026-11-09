//! Read a table definition and run a DDL plan on one connection.
//!
//! A rebuild plan (see `mystique_sql::ddl::sqlite_rebuild_steps`) runs with
//! foreign keys off, checks that the schema still matches the draft right
//! after `BEGIN IMMEDIATE`, and rolls back when `PRAGMA foreign_key_check`
//! finds a violation or a view no longer compiles.

use mystique_connector_api::{
    ApplyDdlResult, ColumnDraft, ConnectorError, ConnectorResult, ConstraintDraft, DdlPlan,
    ForeignKeyDraft, IndexDraft, TableDefinition, TriggerDefinition,
};
use mystique_sql::ddl::{sqlite_rebuild_steps as steps, sqlite_table_extras};
use rusqlite::{Connection, OptionalExtension};

use super::catalog::sqlite_pragma_invoke;
use super::query_failed;
use super::SqliteSession;

#[cfg(test)]
mod tests;

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

/// `(type, name, sql)` of the indexes and triggers on a table.
fn dependants(conn: &Connection, table: &str) -> ConnectorResult<Vec<(String, String, String)>> {
    let mut stmt = conn
        .prepare(
            "SELECT type, name, sql FROM sqlite_master \
             WHERE tbl_name = ?1 AND type IN ('index', 'trigger') AND sql IS NOT NULL \
             ORDER BY type, name",
        )
        .map_err(query_failed)?;
    let rows = stmt
        .query_map([table], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))
        .map_err(query_failed)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(query_failed)?;
    Ok(rows)
}

fn table_sql(conn: &Connection, table: &str) -> ConnectorResult<Option<String>> {
    conn.query_row(
        "SELECT sql FROM sqlite_master WHERE type = 'table' AND name = ?1",
        [table],
        |row| row.get(0),
    )
    .optional()
    .map_err(query_failed)
    .map(Option::flatten)
}

/// The parent's primary key, for a REFERENCES clause that names no column.
fn primary_key_of(conn: &Connection, table: &str) -> ConnectorResult<Vec<String>> {
    let info = sqlite_pragma_invoke("table_info", table)?;
    let mut stmt = conn.prepare(&info).map_err(query_failed)?;
    let mut rows = stmt.query([]).map_err(query_failed)?;
    let mut keys: Vec<(i64, String)> = Vec::new();
    while let Some(row) = rows.next().map_err(query_failed)? {
        let pk: i64 = row.get(5).map_err(query_failed)?;
        if pk > 0 {
            keys.push((pk, row.get(1).map_err(query_failed)?));
        }
    }
    keys.sort();
    Ok(keys.into_iter().map(|(_, name)| name).collect())
}

pub(super) fn describe_on(conn: &Connection, table: &str) -> ConnectorResult<TableDefinition> {
    let revision = table_sql(conn, table)?;
    let extras = revision
        .as_deref()
        .and_then(sqlite_table_extras)
        .unwrap_or_default();
    let mut columns = Vec::new();
    let info = sqlite_pragma_invoke("table_info", table)?;
    let mut stmt = conn.prepare(&info).map_err(query_failed)?;
    let mut rows = stmt.query([]).map_err(query_failed)?;
    while let Some(row) = rows.next().map_err(query_failed)? {
        let name: String = row.get(1).map_err(query_failed)?;
        // An untyped column stays untyped: "ANY" would mean something else
        // in a STRICT table and change affinity on a rebuild.
        let data_type: String = row
            .get::<_, Option<String>>(2)
            .map_err(query_failed)?
            .unwrap_or_default();
        let not_null: i64 = row.get(3).map_err(query_failed)?;
        let default: Option<String> = row.get(4).map_err(query_failed)?;
        let pk: i64 = row.get(5).map_err(query_failed)?;
        let identity = extras
            .autoincrement
            .as_deref()
            .filter(|column| column.eq_ignore_ascii_case(&name))
            .map(|_| "autoincrement".to_string());
        columns.push(
            ColumnDraft::loaded(
                name,
                data_type,
                not_null == 0,
                u32::try_from(pk).unwrap_or(0),
            )
            .with_default(
                default
                    .map(|text| text.trim().to_string())
                    .filter(|text| !text.is_empty()),
                identity,
            ),
        );
    }
    drop(rows);
    drop(stmt);

    let deps = dependants(conn, table)?;
    let mut indexes = Vec::new();
    let list = sqlite_pragma_invoke("index_list", table)?;
    let mut list_stmt = conn.prepare(&list).map_err(query_failed)?;
    let mut list_rows = list_stmt.query([]).map_err(query_failed)?;
    let mut index_meta = Vec::new();
    while let Some(row) = list_rows.next().map_err(query_failed)? {
        let name: String = row.get(1).map_err(query_failed)?;
        // UNIQUE / PRIMARY KEY constraints: they live in the CREATE TABLE.
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
            // NULL for an expression; the definition carries it.
            let col: Option<String> = row.get(2).map_err(query_failed)?;
            cols.push(col.unwrap_or_else(|| "(expression)".into()));
        }
        let definition = deps
            .iter()
            .find(|(kind, item, _)| kind == "index" && *item == name)
            .map(|(_, _, sql)| sql.clone());
        indexes.push(IndexDraft::loaded(name, cols, unique, false).with_definition(definition));
    }

    let mut foreign_keys = Vec::new();
    let fk_sql = sqlite_pragma_invoke("foreign_key_list", table)?;
    let mut fk_stmt = conn.prepare(&fk_sql).map_err(query_failed)?;
    let mut fk_rows = fk_stmt.query([]).map_err(query_failed)?;
    // id, seq, table, from, to, on_update, on_delete, match
    type Group = (
        i64,
        String,
        Vec<String>,
        Vec<Option<String>>,
        String,
        String,
    );
    let mut grouped: Vec<Group> = Vec::new();
    while let Some(row) = fk_rows.next().map_err(query_failed)? {
        let id: i64 = row.get(0).map_err(query_failed)?;
        let ref_table: String = row.get(2).map_err(query_failed)?;
        let from: String = row.get(3).map_err(query_failed)?;
        let to: Option<String> = row.get(4).map_err(query_failed)?;
        let on_update: String = row.get(5).map_err(query_failed)?;
        let on_delete: String = row.get(6).map_err(query_failed)?;
        match grouped.last_mut() {
            Some(item) if item.0 == id => {
                item.2.push(from);
                item.3.push(to);
            }
            _ => grouped.push((id, ref_table, vec![from], vec![to], on_delete, on_update)),
        }
    }
    drop(fk_rows);
    drop(fk_stmt);
    // SQLite numbers keys from the last one written; list them in text order.
    grouped.reverse();
    for (id, ref_table, columns_from, to, on_delete, on_update) in grouped {
        let ref_columns: Vec<String> = if to.iter().all(Option::is_none) {
            primary_key_of(conn, &ref_table)?
        } else {
            to.into_iter().map(Option::unwrap_or_default).collect()
        };
        let name = extras
            .foreign_keys
            .iter()
            .find(|fk| {
                fk.ref_table.eq_ignore_ascii_case(&ref_table)
                    && fk.columns.len() == columns_from.len()
                    && fk
                        .columns
                        .iter()
                        .zip(&columns_from)
                        .all(|(a, b)| a.eq_ignore_ascii_case(b))
            })
            .and_then(|fk| fk.name.clone())
            .unwrap_or_else(|| format!("fk_{table}_{id}"));
        foreign_keys.push(
            ForeignKeyDraft::loaded(name, columns_from, None, ref_table, ref_columns)
                .with_actions(Some(on_delete), Some(on_update)),
        );
    }

    let mut constraints = Vec::new();
    for (name, expression) in &extras.checks {
        constraints.push(ConstraintDraft::loaded(
            "check",
            name.clone(),
            Vec::new(),
            Some(expression.clone()),
        ));
    }
    for (name, cols) in &extras.uniques {
        constraints.push(ConstraintDraft::loaded(
            "unique",
            name.clone(),
            cols.clone(),
            None,
        ));
    }
    let triggers = deps
        .iter()
        .filter(|(kind, _, _)| kind == "trigger")
        .map(|(_, name, sql)| TriggerDefinition {
            name: name.clone(),
            sql: sql.clone(),
        })
        .collect();

    let mut definition = TableDefinition::loaded(None, None, table, "table");
    definition.columns = columns;
    definition.indexes = indexes;
    definition.foreign_keys = foreign_keys;
    definition.constraints = constraints;
    definition.triggers = triggers;
    definition.schema_revision = revision;
    Ok(definition)
}

/// The rebuild was planned from this draft: the table text, its indexes,
/// and its triggers must still be the ones the draft was read with.
fn ensure_schema_matches(conn: &Connection, draft: &TableDefinition) -> ConnectorResult<()> {
    let stale = || {
        ConnectorError::Query(
            "The table schema changed. Reload the structure and try again.".into(),
        )
    };
    let live = table_sql(conn, draft.name.trim())?.ok_or_else(stale)?;
    let expected = draft.schema_revision.as_deref().unwrap_or("").trim();
    if live.trim() != expected {
        return Err(stale());
    }
    let mut live_deps: Vec<(String, String)> = dependants(conn, draft.name.trim())?
        .into_iter()
        .map(|(kind, name, sql)| (format!("{kind}:{name}"), sql.trim().to_string()))
        .collect();
    let mut expected_deps: Vec<(String, String)> = draft
        .indexes
        .iter()
        .filter_map(|index| index.original.as_ref())
        .filter_map(|original| {
            original
                .definition
                .as_deref()
                .map(|sql| (format!("index:{}", original.name), sql.trim().to_string()))
        })
        .chain(draft.triggers.iter().map(|trigger| {
            (
                format!("trigger:{}", trigger.name),
                trigger.sql.trim().to_string(),
            )
        }))
        .collect();
    live_deps.sort();
    expected_deps.sort();
    if live_deps != expected_deps {
        return Err(stale());
    }
    Ok(())
}

/// Rows of `PRAGMA foreign_key_check`, as `table rowid → parent`.
fn foreign_key_violations(conn: &Connection, table: Option<&str>) -> ConnectorResult<Vec<String>> {
    let sql = match table {
        Some(table) => sqlite_pragma_invoke("foreign_key_check", table)?,
        None => steps::FOREIGN_KEY_CHECK.to_string(),
    };
    let mut stmt = conn.prepare(&sql).map_err(query_failed)?;
    let mut rows = stmt.query([]).map_err(query_failed)?;
    let mut out = Vec::new();
    while let Some(row) = rows.next().map_err(query_failed)? {
        let child: String = row.get(0).map_err(query_failed)?;
        let rowid: Option<i64> = row.get(1).map_err(query_failed)?;
        let parent: String = row.get(2).map_err(query_failed)?;
        let at = rowid.map(|id| format!(" row {id}")).unwrap_or_default();
        out.push(format!("{child}{at} → {parent}"));
    }
    Ok(out)
}

/// Views that no longer compile after the rebuild (a dropped column).
fn broken_views(conn: &Connection) -> ConnectorResult<Vec<String>> {
    let mut stmt = conn
        .prepare("SELECT name FROM sqlite_master WHERE type = 'view'")
        .map_err(query_failed)?;
    let names: Vec<String> = stmt
        .query_map([], |row| row.get(0))
        .map_err(query_failed)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(query_failed)?;
    let mut broken = Vec::new();
    for name in names {
        let probe = format!(
            "SELECT * FROM {} LIMIT 0",
            mystique_sql::quote_ident(mystique_sql::SqlDialect::Sqlite, &name)
        );
        if let Err(err) = conn.prepare(&probe) {
            broken.push(format!("{name} ({err})"));
        }
    }
    Ok(broken)
}

fn foreign_keys_on(conn: &Connection) -> ConnectorResult<bool> {
    conn.query_row("PRAGMA foreign_keys", [], |row| row.get::<_, i64>(0))
        .map(|value| value != 0)
        .map_err(query_failed)
}

pub(super) fn apply_on(
    conn: &Connection,
    plan: &DdlPlan,
    draft: &TableDefinition,
) -> ConnectorResult<ApplyDdlResult> {
    if plan.requires_rebuild {
        return rebuild_on(conn, plan, draft);
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

/// Run a rebuild plan. Everything between `BEGIN IMMEDIATE` and `COMMIT`
/// is one transaction; any failure rolls it back and restores the
/// connection's pragmas.
fn rebuild_on(
    conn: &Connection,
    plan: &DdlPlan,
    draft: &TableDefinition,
) -> ConnectorResult<ApplyDdlResult> {
    let fk_were_on = foreign_keys_on(conn)?;
    let final_name = draft
        .rename_to
        .as_deref()
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .unwrap_or(draft.name.trim())
        .to_string();
    let mut applied = Vec::new();
    let mut in_transaction = false;
    let restore = |in_transaction: bool| {
        if in_transaction {
            let _ = conn.execute_batch("ROLLBACK");
        }
        let _ = conn.execute_batch(steps::LEGACY_ALTER_OFF);
        if fk_were_on {
            let _ = conn.execute_batch(steps::FOREIGN_KEYS_ON);
        }
    };
    let fail = |applied: &[String], in_transaction: bool, reason: String| {
        restore(in_transaction);
        let rolled_back = if applied.is_empty() {
            "Nothing was applied."
        } else {
            "The rebuild was rolled back."
        };
        ConnectorError::Query(format!("{reason} {rolled_back}"))
    };
    for sql in &plan.statements {
        let step = sql.trim();
        if step == steps::FOREIGN_KEYS_ON {
            if fk_were_on {
                if let Err(err) = conn.execute_batch(step) {
                    return Err(ConnectorError::Query(format!(
                        "The rebuild was committed, but foreign keys could not be turned back on: {err}"
                    )));
                }
                applied.push(sql.clone());
            }
            continue;
        }
        if step == steps::FOREIGN_KEY_CHECK {
            let scope = (!fk_were_on).then_some(final_name.as_str());
            let violations = match foreign_key_violations(conn, scope) {
                Ok(rows) => rows,
                Err(err) => return Err(fail(&applied, in_transaction, err.to_string())),
            };
            if !violations.is_empty() {
                let shown: Vec<&str> = violations.iter().take(5).map(String::as_str).collect();
                let more = violations.len().saturating_sub(shown.len());
                let more = if more > 0 {
                    format!(" and {more} more")
                } else {
                    String::new()
                };
                return Err(fail(
                    &applied,
                    in_transaction,
                    format!(
                        "The change breaks foreign keys: {}{more}.",
                        shown.join(", ")
                    ),
                ));
            }
            match broken_views(conn) {
                Ok(views) if !views.is_empty() => {
                    return Err(fail(
                        &applied,
                        in_transaction,
                        format!(
                            "These views stop working after the change: {}.",
                            views.join(", ")
                        ),
                    ));
                }
                Ok(_) => {}
                Err(err) => return Err(fail(&applied, in_transaction, err.to_string())),
            }
            applied.push(sql.clone());
            continue;
        }
        if let Err(err) = conn.execute_batch(step) {
            let at = applied.len();
            return Err(fail(
                &applied,
                in_transaction,
                format!("Statement {} failed: {err}.", at + 1),
            ));
        }
        applied.push(sql.clone());
        if step == steps::BEGIN {
            in_transaction = true;
            if let Err(err) = ensure_schema_matches(conn, draft) {
                return Err(fail(&applied, true, err.to_string()));
            }
        } else if step == steps::COMMIT {
            in_transaction = false;
        }
    }
    if in_transaction {
        return Err(fail(
            &applied,
            true,
            "The rebuild plan did not commit.".into(),
        ));
    }
    let count = applied.len();
    Ok(ApplyDdlResult {
        applied,
        message: format!("Rebuilt {final_name} ({count} statement(s))."),
    })
}
