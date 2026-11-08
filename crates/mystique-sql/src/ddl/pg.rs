//! Postgres-only ALTER pieces.

use super::statements::quote;
use super::types::{ColumnDraft, ColumnOriginal};
use super::DdlTarget;

/// `ALTER COLUMN … TYPE` and `SET` / `DROP NOT NULL` for one changed column.
pub(super) fn alter_column(
    dialect: DdlTarget,
    table: &str,
    original: &ColumnOriginal,
    column: &ColumnDraft,
) -> Vec<String> {
    let mut statements = Vec::new();
    if !original
        .data_type
        .eq_ignore_ascii_case(column.data_type.trim())
    {
        statements.push(format!(
            "ALTER TABLE {table} ALTER COLUMN {} TYPE {}",
            quote(dialect, &column.name),
            column.data_type.trim()
        ));
    }
    if original.nullable != column.nullable {
        let verb = if column.nullable { "DROP" } else { "SET" };
        statements.push(format!(
            "ALTER TABLE {table} ALTER COLUMN {} {verb} NOT NULL",
            quote(dialect, &column.name)
        ));
    }
    statements
}
