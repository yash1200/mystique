//! Grid edits compiled to dialect-aware INSERT / UPDATE / DELETE / UPSERT.
//! Plugins still execute the SQL; this only builds statements.
//!
//! - [`compile`]: one literal statement per row (import, upsert).
//! - [`plan`]: one save as bound statements plus the literal preview.
//! - [`inverse`]: the statements that put a saved row back.
//! - [`preview`]: read-only helper statements for the grid.
//! - [`row_preview`]: the rows a typed UPDATE / DELETE / INSERT would touch.

mod compile;
mod edits;
mod inverse;
mod plan;
mod preview;
mod row_preview;

use serde_json::Value as JsonValue;

pub use compile::{compile_delete, compile_insert, compile_update, compile_upsert};
pub use edits::{
    rows_hash, ApplyTableEditsInput, GridUndo, RowCheck, TableEditColumn, TableRowDelete,
    TableRowInsert, TableRowUpdate,
};
pub use plan::{plan_table_edits, EditPlan, PlannedCount, PlannedEdit, PlannedRead};
pub use preview::distinct_values_sql;
pub use row_preview::{
    insert_row_count, row_preview_plan, RowPreviewAction, RowPreviewPlan, TablePath,
};

/// A grid cell that only holds a large-value preview (`{"$large": true, "preview": …}`).
/// It cannot be written back or used to find a row.
pub fn is_large_marker(value: &JsonValue) -> bool {
    value
        .get("$large")
        .and_then(JsonValue::as_bool)
        .unwrap_or(false)
        && value.get("preview").and_then(JsonValue::as_str).is_some()
}

fn col_type<'a>(columns: &'a [TableEditColumn], name: &str) -> &'a str {
    columns
        .iter()
        .find(|c| c.name == name)
        .map(|c| c.data_type.as_str())
        .unwrap_or("")
}

fn pk_columns(columns: &[TableEditColumn]) -> Vec<&TableEditColumn> {
    let pks: Vec<_> = columns.iter().filter(|c| c.pk).collect();
    if !pks.is_empty() {
        return pks;
    }
    columns
        .iter()
        .filter(|c| c.name == "id" || c.name == "_id" || c.name == "rowid")
        .collect()
}

fn is_empty_value(v: &JsonValue) -> bool {
    match v {
        JsonValue::Null => true,
        JsonValue::String(s) => s.is_empty(),
        _ => false,
    }
}
