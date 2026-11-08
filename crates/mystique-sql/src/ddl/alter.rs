//! In-place ALTER for Postgres and MySQL. Column changes that differ by
//! engine are in `pg` and `mysql`.

use super::changes::{index_change_count, pk_changed};
use super::statements::{foreign_key_statements, index_statements, qualify, quote, rename_sql};
use super::types::{DdlPlan, TableDefinition};
use super::{finish, mysql, pg, DdlTarget};

pub(super) fn plan_sql(dialect: DdlTarget, draft: &TableDefinition, postgres: bool) -> DdlPlan {
    let mut warnings = Vec::new();
    if pk_changed(draft) {
        warnings.push("Changing the primary key is not supported on this engine.".into());
    }
    let mut statements = Vec::new();
    let table = qualify(dialect, draft);
    for column in &draft.columns {
        if column.drop {
            if column.original.is_some() {
                statements.push(format!(
                    "ALTER TABLE {table} DROP COLUMN {}",
                    quote(dialect, &column.name)
                ));
            }
            continue;
        }
        match &column.original {
            None => {
                if !column.nullable {
                    warnings.push(format!(
                        "Add column {} as nullable. NOT NULL without a default fails when the table already has rows.",
                        column.name
                    ));
                }
                statements.push(format!(
                    "ALTER TABLE {table} ADD COLUMN {} {}",
                    quote(dialect, &column.name),
                    column.data_type.trim()
                ));
            }
            Some(original) => {
                if !original.name.eq_ignore_ascii_case(&column.name) {
                    statements.push(format!(
                        "ALTER TABLE {table} RENAME COLUMN {} TO {}",
                        quote(dialect, &original.name),
                        quote(dialect, &column.name)
                    ));
                }
                if !original
                    .data_type
                    .eq_ignore_ascii_case(column.data_type.trim())
                    || original.nullable != column.nullable
                {
                    if postgres {
                        statements.extend(pg::alter_column(dialect, &table, original, column));
                    } else {
                        match mysql::modify_column(dialect, &table, original, column) {
                            Ok(sql) => statements.push(sql),
                            Err(warning) => warnings.push(warning),
                        }
                    }
                }
            }
        }
    }
    let concurrent = postgres
        && draft
            .indexes
            .iter()
            .any(|index| index.concurrent && !index.drop);
    if concurrent && (statements.len() + index_change_count(draft) > 1) {
        warnings.push(
            "CREATE INDEX CONCURRENTLY cannot share a transaction with other changes.".into(),
        );
    }
    statements.extend(index_statements(dialect, draft, true));
    statements.extend(foreign_key_statements(dialect, draft));
    if let Some(rename) = rename_sql(dialect, draft) {
        statements.push(rename);
    }
    let note = if postgres {
        None
    } else {
        Some(
            "MySQL applies each statement immediately. A later failure leaves the earlier ones in place."
                .into(),
        )
    };
    finish(
        dialect,
        statements,
        warnings,
        postgres && !concurrent,
        false,
        note,
    )
}
