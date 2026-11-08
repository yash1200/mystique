//! Quoting and the statement builders every SQL dialect shares.

use super::types::{ColumnDraft, TableDefinition};
use super::DdlTarget;
use crate::quote_ident;

pub(super) fn quote(dialect: DdlTarget, name: &str) -> String {
    quote_ident(dialect.sql(), name.trim())
}

pub(super) fn qualify(dialect: DdlTarget, draft: &TableDefinition) -> String {
    let mut parts: Vec<&str> = Vec::new();
    if dialect.is_mysql() {
        if let Some(database) = draft
            .database
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
        {
            parts.push(database);
        }
    }
    if !dialect.is_sqlite() {
        if let Some(schema) = draft
            .schema
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
        {
            if !schema.eq_ignore_ascii_case("main") {
                parts.push(schema);
            }
        }
    }
    parts.push(draft.name.trim());
    parts
        .into_iter()
        .filter(|part| !part.is_empty())
        .map(|part| quote(dialect, part))
        .collect::<Vec<_>>()
        .join(".")
}

pub(super) fn create_table_sql(dialect: DdlTarget, draft: &TableDefinition) -> String {
    let mut lines = Vec::new();
    for column in draft.columns.iter().filter(|column| !column.drop) {
        let null_sql = if column.nullable { "NULL" } else { "NOT NULL" };
        lines.push(format!(
            "{} {} {null_sql}",
            quote(dialect, &column.name),
            column.data_type.trim()
        ));
    }
    let mut pk: Vec<&ColumnDraft> = draft
        .columns
        .iter()
        .filter(|column| !column.drop && column.pk_position > 0)
        .collect();
    pk.sort_by_key(|column| column.pk_position);
    if !pk.is_empty() {
        let cols = pk
            .iter()
            .map(|column| quote(dialect, &column.name))
            .collect::<Vec<_>>()
            .join(", ");
        lines.push(format!("PRIMARY KEY ({cols})"));
    }
    format!(
        "CREATE TABLE {} ({})",
        qualify(dialect, draft),
        lines.join(", ")
    )
}

pub(super) fn drop_index_sql(dialect: DdlTarget, draft: &TableDefinition, name: &str) -> String {
    let quoted = quote(dialect, name);
    if dialect.is_mysql() {
        format!(
            "ALTER TABLE {} DROP INDEX {quoted}",
            qualify(dialect, draft)
        )
    } else if dialect.is_postgres() {
        let index = match draft
            .schema
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
        {
            Some(schema) => format!("{}.{}", quote(dialect, schema), quoted),
            None => quoted,
        };
        format!("DROP INDEX {index}")
    } else {
        format!("DROP INDEX {quoted}")
    }
}

pub(super) fn index_statements(
    dialect: DdlTarget,
    draft: &TableDefinition,
    include_drops: bool,
) -> Vec<String> {
    let mut out = Vec::new();
    let table = qualify(dialect, draft);
    for index in &draft.indexes {
        if index.primary {
            continue;
        }
        if include_drops && index.drop && index.original.is_some() {
            out.push(drop_index_sql(dialect, draft, &index.name));
            continue;
        }
        let changed = index.original.as_ref().is_some_and(|original| {
            original.unique != index.unique || original.columns != index.columns
        });
        if changed && include_drops {
            out.push(drop_index_sql(dialect, draft, &index.name));
        }
        if (index.original.is_none() || changed) && !index.drop {
            let unique = if index.unique { "UNIQUE " } else { "" };
            let concurrently = if index.concurrent && dialect.is_postgres() {
                "CONCURRENTLY "
            } else {
                ""
            };
            let cols = index
                .columns
                .iter()
                .map(|column| quote(dialect, column))
                .collect::<Vec<_>>()
                .join(", ");
            out.push(format!(
                "CREATE {unique}INDEX {concurrently}{} ON {table} ({cols})",
                quote(dialect, &index.name)
            ));
        }
    }
    out
}

pub(super) fn foreign_key_statements(dialect: DdlTarget, draft: &TableDefinition) -> Vec<String> {
    let mut out = Vec::new();
    let table = qualify(dialect, draft);
    for fk in &draft.foreign_keys {
        if fk.drop && fk.original.is_some() {
            let sql = if dialect.is_mysql() {
                format!(
                    "ALTER TABLE {table} DROP FOREIGN KEY {}",
                    quote(dialect, &fk.name)
                )
            } else {
                format!(
                    "ALTER TABLE {table} DROP CONSTRAINT {}",
                    quote(dialect, &fk.name)
                )
            };
            out.push(sql);
            continue;
        }
        if fk.original.is_none() && !fk.drop {
            let cols = fk
                .columns
                .iter()
                .map(|column| quote(dialect, column))
                .collect::<Vec<_>>()
                .join(", ");
            let refs = fk
                .ref_columns
                .iter()
                .map(|column| quote(dialect, column))
                .collect::<Vec<_>>()
                .join(", ");
            let ref_table = dialect.sql().quote_path(
                [
                    fk.ref_schema.as_deref().unwrap_or(""),
                    fk.ref_table.as_str(),
                ]
                .into_iter()
                .filter(|part| !part.is_empty()),
            );
            out.push(format!(
                "ALTER TABLE {table} ADD CONSTRAINT {} FOREIGN KEY ({cols}) REFERENCES {ref_table} ({refs})",
                quote(dialect, &fk.name)
            ));
        }
    }
    out
}

pub(super) fn rename_sql(dialect: DdlTarget, draft: &TableDefinition) -> Option<String> {
    let next = draft
        .rename_to
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())?;
    if next == draft.name.trim() {
        return None;
    }
    Some(format!(
        "ALTER TABLE {} RENAME TO {}",
        qualify(dialect, draft),
        quote(dialect, next)
    ))
}
