//! Statements that put a saved row back, for rollback and undo.
//!
//! A row whose values are only a large-cell preview cannot be restored, so
//! its inverse is `None` and the whole save has no rollback.

use serde_json::Value as JsonValue;
use std::collections::BTreeMap;

use super::edits::{ApplyTableEditsInput, GridUndo};
use super::plan::EditPlan;
use super::{col_type, is_large_marker};
use crate::error::{BuildError, BuildResult};
use crate::{qualify_table, quote_ident, sql_literal, SqlDialect};

impl EditPlan {
    /// Inverse statements in reverse apply order. `None` when any row cannot be put back.
    pub fn rollback_sql(&self) -> Option<Vec<String>> {
        self.edits
            .iter()
            .rev()
            .map(|edit| edit.inverse.clone())
            .collect()
    }

    /// Undo for a committed save. `rows[i]` is what `edits[i].check` returned
    /// inside the save transaction.
    pub fn undo(&self, rows: &[Vec<Vec<JsonValue>>]) -> Option<GridUndo> {
        let inverse = self.rollback_sql()?;
        if rows.len() != self.edits.len() {
            return None;
        }
        let checks = self
            .edits
            .iter()
            .zip(rows)
            .map(|(edit, rows)| edit.check.checked(rows))
            .collect();
        Some(GridUndo { checks, inverse })
    }
}

pub(super) fn insert_original(
    style: SqlDialect,
    input: &ApplyTableEditsInput,
    original: &BTreeMap<String, JsonValue>,
) -> BuildResult<String> {
    if original.is_empty() {
        return Err(BuildError::new(
            "Cannot restore a deleted row with no original values.".into(),
        ));
    }
    let target = qualify_table(
        style,
        input.database.as_deref(),
        input.schema.as_deref(),
        &input.table,
    );
    let mut names = Vec::new();
    let mut literals = Vec::new();
    for (name, value) in original {
        names.push(quote_ident(style, name));
        literals.push(sql_literal(style, value, col_type(&input.columns, name)));
    }
    Ok(format!(
        "INSERT INTO {target} ({}) VALUES ({})",
        names.join(", "),
        literals.join(", ")
    ))
}

pub(super) fn any_large<'a>(values: impl IntoIterator<Item = &'a JsonValue>) -> bool {
    values.into_iter().any(is_large_marker)
}
