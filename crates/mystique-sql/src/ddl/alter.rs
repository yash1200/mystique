//! In-place ALTER for Postgres and MySQL. Column changes that differ by
//! engine are in `pg` and `mysql`.

use super::changes::{
    default_changed, identity_changed, index_change_count, pk_changed, type_changed,
};
use super::statements::{
    column_definition, constraint_statements, foreign_key_statements, index_statements, qualify,
    quote, rename_sql,
};
use super::types::{DdlPlan, TableDefinition};
use super::{finish, mysql, pg, DdlTarget};

pub(super) fn plan_sql(dialect: DdlTarget, draft: &TableDefinition, postgres: bool) -> DdlPlan {
    let mut warnings = Vec::new();
    if pk_changed(draft) {
        warnings.push("Changing the primary key is not supported on this engine.".into());
    }
    let mut statements = Vec::new();
    let table = qualify(dialect, draft);
    // Constraints and keys that use a dropped column go first.
    let mut drops = Vec::new();
    for column in &draft.columns {
        if column.drop {
            if column.original.is_some() {
                drops.push(format!(
                    "ALTER TABLE {table} DROP COLUMN {}",
                    quote(dialect, &column.name)
                ));
            }
            continue;
        }
        match &column.original {
            None => {
                let has_default = column
                    .default_value
                    .as_deref()
                    .is_some_and(|text| !text.trim().is_empty());
                if !column.nullable && !has_default && column.identity.is_none() {
                    warnings.push(format!(
                        "Add column {} as nullable. NOT NULL without a default fails when the table already has rows.",
                        column.name
                    ));
                }
                statements.push(format!(
                    "ALTER TABLE {table} ADD COLUMN {}",
                    column_definition(dialect, column)
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
                let changed = type_changed(original, column)
                    || original.nullable != column.nullable
                    || default_changed(original, column)
                    || identity_changed(original, column);
                if changed {
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
    let (mut key_drops, mut key_adds) = foreign_key_statements(dialect, draft);
    let (check_drops, check_adds) = constraint_statements(dialect, draft);
    key_drops.extend(check_drops);
    key_adds.extend(check_adds);
    let mut ordered = key_drops;
    ordered.extend(drops);
    ordered.extend(statements);
    let mut statements = ordered;
    if concurrent && (statements.len() + key_adds.len() + index_change_count(draft) > 1) {
        warnings.push(
            "CREATE INDEX CONCURRENTLY cannot share a transaction with other changes.".into(),
        );
    }
    statements.extend(index_statements(dialect, draft, true));
    statements.extend(key_adds);
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
