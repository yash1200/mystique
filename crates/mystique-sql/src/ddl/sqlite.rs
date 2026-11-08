//! SQLite: in-place changes only.

use super::changes::{fk_changes, pk_changed};
use super::statements::{index_statements, qualify, quote, rename_sql};
use super::types::{DdlPlan, TableDefinition};
use super::{finish, DdlTarget};

/// SQLite alters in place only. The old rebuild re-created the table from the
/// draft and lost DEFAULT, CHECK, UNIQUE, COLLATE, AUTOINCREMENT, STRICT,
/// foreign keys, indexes, and triggers (R-03). Until it rebuilds from the
/// original `CREATE TABLE`, anything that needs a rebuild is refused.
pub(super) fn plan_sqlite(draft: &TableDefinition) -> DdlPlan {
    let mut warnings = sqlite_refusals(draft);
    let mut statements = Vec::new();
    let table = qualify(DdlTarget::SQLITE, draft);
    for column in draft.columns.iter().filter(|column| !column.drop) {
        match &column.original {
            None => {
                if !column.nullable {
                    warnings.push(format!(
                        "Add column {} as nullable. SQLite cannot add a NOT NULL column without a default.",
                        column.name
                    ));
                }
                statements.push(format!(
                    "ALTER TABLE {table} ADD COLUMN {} {}",
                    quote(DdlTarget::SQLITE, &column.name),
                    column.data_type.trim()
                ));
            }
            Some(original) if !original.name.eq_ignore_ascii_case(&column.name) => {
                statements.push(format!(
                    "ALTER TABLE {table} RENAME COLUMN {} TO {}",
                    quote(DdlTarget::SQLITE, &original.name),
                    quote(DdlTarget::SQLITE, &column.name)
                ));
            }
            Some(_) => {}
        }
    }
    statements.extend(index_statements(DdlTarget::SQLITE, draft, true));
    if let Some(rename) = rename_sql(DdlTarget::SQLITE, draft) {
        statements.push(rename);
    }
    // SQLite DDL is transactional: a failed step leaves nothing half-applied.
    let transactional = statements.len() > 1;
    if transactional {
        statements.insert(0, "BEGIN".into());
        statements.push("COMMIT".into());
    }
    finish(
        DdlTarget::SQLITE,
        statements,
        warnings,
        transactional,
        false,
        None,
    )
}

const SQLITE_LIMIT: &str =
    "On SQLite, only adding columns, renaming the table or a column, and index changes can be applied for now.";

pub(super) fn sqlite_refusals(draft: &TableDefinition) -> Vec<String> {
    let mut refused = Vec::new();
    for column in &draft.columns {
        let Some(original) = &column.original else {
            continue;
        };
        let name = &original.name;
        if column.drop {
            refused.push(format!("Column {name} cannot be dropped. {SQLITE_LIMIT}"));
            continue;
        }
        if !original
            .data_type
            .eq_ignore_ascii_case(column.data_type.trim())
        {
            refused.push(format!(
                "The type of column {name} cannot be changed. {SQLITE_LIMIT}"
            ));
        }
        if original.nullable != column.nullable {
            refused.push(format!(
                "NOT NULL on column {name} cannot be changed. {SQLITE_LIMIT}"
            ));
        }
    }
    let new_pk = draft
        .columns
        .iter()
        .any(|column| column.original.is_none() && !column.drop && column.pk_position > 0);
    if pk_changed(draft) || new_pk {
        refused.push(format!("The primary key cannot be changed. {SQLITE_LIMIT}"));
    }
    if draft.foreign_keys.iter().any(fk_changes) {
        refused.push(format!(
            "Foreign keys cannot be added, changed, or dropped. {SQLITE_LIMIT}"
        ));
    }
    refused
}
