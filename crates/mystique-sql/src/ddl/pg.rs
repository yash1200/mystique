//! Postgres-only ALTER pieces.

use super::changes::{default_changed, identity_changed, type_changed};
use super::statements::quote;
use super::types::{ColumnDraft, ColumnOriginal};
use super::DdlTarget;

fn identity_kind(identity: &str) -> &'static str {
    if identity.trim().eq_ignore_ascii_case("always") {
        "ALWAYS"
    } else {
        "BY DEFAULT"
    }
}

/// `ALTER COLUMN …` for one changed column: type, NOT NULL, default, and
/// identity. Identity is dropped before a new default and added after the
/// old default is gone.
pub(super) fn alter_column(
    dialect: DdlTarget,
    table: &str,
    original: &ColumnOriginal,
    column: &ColumnDraft,
) -> Vec<String> {
    let name = quote(dialect, &column.name);
    let alter = |rest: String| format!("ALTER TABLE {table} ALTER COLUMN {name} {rest}");
    let mut statements = Vec::new();
    let old_identity = original
        .identity
        .as_deref()
        .filter(|text| !text.trim().is_empty());
    let new_identity = column
        .identity
        .as_deref()
        .filter(|text| !text.trim().is_empty());
    let identity_moves = identity_changed(original, column);
    if identity_moves && old_identity.is_some() && new_identity.is_none() {
        statements.push(alter("DROP IDENTITY IF EXISTS".into()));
    }
    if type_changed(original, column) {
        statements.push(alter(format!("TYPE {}", column.data_type.trim())));
    }
    if original.nullable != column.nullable {
        let verb = if column.nullable { "DROP" } else { "SET" };
        statements.push(alter(format!("{verb} NOT NULL")));
    }
    if default_changed(original, column) {
        match column
            .default_value
            .as_deref()
            .map(str::trim)
            .filter(|text| !text.is_empty())
        {
            Some(default) => statements.push(alter(format!("SET DEFAULT {default}"))),
            None => statements.push(alter("DROP DEFAULT".into())),
        }
    }
    if identity_moves {
        match (old_identity, new_identity) {
            (None, Some(identity)) => statements.push(alter(format!(
                "ADD GENERATED {} AS IDENTITY",
                identity_kind(identity)
            ))),
            (Some(_), Some(identity)) => {
                statements.push(alter(format!("SET GENERATED {}", identity_kind(identity))));
            }
            _ => {}
        }
    }
    statements
}
