//! One save is many statements inside one transaction.
//!
//! Each changed row still compiles to its own INSERT, UPDATE, or DELETE.
//! Placeholders carry the values. `preview` is the same statement with
//! bound-safe literals so the confirm dialog can show it. `inverse` is the
//! statement that puts that row back, also with literals. `check` reads the
//! row as it stands after the save, so rollback can tell if it changed since.

use serde_json::Value as JsonValue;
use std::collections::BTreeMap;

use super::super::edits::{
    rows_hash, ApplyTableEditsInput, GridUndo, RowCheck, TableRowDelete, TableRowInsert,
    TableRowUpdate,
};
use super::{col_type, is_empty_value, qualify_table, quote_ident, sql_literal, SqlIdentStyle};
use crate::error::{ConnectorError, ConnectorResult};
use crate::large_cell::{is_large_marker, SqlPlaceholder};
use crate::ColumnInfo;

/// `SELECT COUNT(*)` that must return 1 before a keyless write.
#[derive(Debug, Clone, PartialEq)]
pub struct PlannedCount {
    pub sql: String,
    pub params: Vec<JsonValue>,
}

/// `SELECT *` that finds one row after the save.
#[derive(Debug, Clone, PartialEq)]
pub struct PlannedRead {
    pub sql: String,
    pub params: Vec<JsonValue>,
}

impl PlannedRead {
    pub fn checked(&self, rows: &[Vec<JsonValue>]) -> RowCheck {
        RowCheck {
            sql: self.sql.clone(),
            params: self.params.clone(),
            hash: rows_hash(rows),
        }
    }
}

/// One row write. `sql` is what runs. `preview` is what the user reads.
#[derive(Debug, Clone, PartialEq)]
pub struct PlannedEdit {
    pub sql: String,
    pub params: Vec<JsonValue>,
    pub preview: String,
    /// Puts the row back. `None` when a value is only a large-cell preview.
    pub inverse: Option<String>,
    pub check: PlannedRead,
    pub uniqueness: Option<PlannedCount>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct EditPlan {
    pub edits: Vec<PlannedEdit>,
}

impl EditPlan {
    pub fn previews(&self) -> Vec<String> {
        self.edits.iter().map(|edit| edit.preview.clone()).collect()
    }

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

/// Catalog primary keys replace whatever the frontend sent.
pub fn overlay_catalog_pk(columns: &mut Vec<crate::TableEditColumn>, described: &[ColumnInfo]) {
    let pk_names: Vec<&str> = described
        .iter()
        .filter(|column| column.pk)
        .map(|column| column.name.as_str())
        .collect();
    if pk_names.is_empty() {
        for column in columns.iter_mut() {
            column.pk = false;
        }
        return;
    }
    for column in columns.iter_mut() {
        column.pk = pk_names.iter().any(|name| *name == column.name);
    }
    for described in described.iter().filter(|column| column.pk) {
        if columns.iter().any(|column| column.name == described.name) {
            continue;
        }
        columns.push(crate::TableEditColumn {
            name: described.name.clone(),
            data_type: described.data_type.clone(),
            nullable: described.nullable,
            pk: true,
        });
    }
}

pub fn plan_table_edits(
    style: SqlIdentStyle,
    placeholder: SqlPlaceholder,
    input: &ApplyTableEditsInput,
) -> ConnectorResult<EditPlan> {
    let mut edits = Vec::new();
    for update in &input.updates {
        edits.push(plan_update(style, placeholder, input, update)?);
    }
    for insert in &input.inserts {
        edits.push(plan_insert(style, placeholder, input, insert)?);
    }
    for delete in &input.deletes {
        edits.push(plan_delete(style, placeholder, input, delete)?);
    }
    if edits.is_empty() {
        return Err(ConnectorError::Query("Nothing to save.".into()));
    }
    Ok(EditPlan { edits })
}

fn plan_update(
    style: SqlIdentStyle,
    placeholder: SqlPlaceholder,
    input: &ApplyTableEditsInput,
    update: &TableRowUpdate,
) -> ConnectorResult<PlannedEdit> {
    if update.values.is_empty() {
        return Err(ConnectorError::Query(
            "Update has no changed columns.".into(),
        ));
    }
    let target = target(style, input);
    let set_pairs = update.values.iter().collect::<Vec<_>>();
    let set = assign_list(style, placeholder, input, &set_pairs, 1)?;
    let pred = row_predicate(
        style,
        placeholder,
        input,
        &update.original,
        set.params.len() + 1,
    )?;
    let uniqueness = keyless_count(style, placeholder, input, &update.original)?;
    let sql = format!("UPDATE {target} SET {} WHERE {}", set.sql, pred.sql);
    let preview = format!("UPDATE {target} SET {} WHERE {}", set.preview, pred.preview);
    let old_owned: Vec<(String, JsonValue)> = update
        .values
        .keys()
        .map(|name| {
            (
                name.clone(),
                update
                    .original
                    .get(name)
                    .cloned()
                    .unwrap_or(JsonValue::Null),
            )
        })
        .collect();
    let old_pairs: Vec<(&String, &JsonValue)> = old_owned
        .iter()
        .map(|(name, value)| (name, value))
        .collect();
    let old = assign_list(style, placeholder, input, &old_pairs, 1)?;
    // The row is found by its values after the save (a keyless row or a new key).
    let mut after = update.original.clone();
    for (name, value) in &update.values {
        after.insert(name.clone(), value.clone());
    }
    let after_pred = row_predicate(style, placeholder, input, &after, 1)?;
    let restorable = !any_large(old_owned.iter().map(|(_, value)| value))
        && !any_large(after_pred.params.iter());
    let inverse = restorable.then(|| {
        format!(
            "UPDATE {target} SET {} WHERE {}",
            old.preview, after_pred.preview
        )
    });
    let check = read_row(&target, &after_pred);
    let mut params = set.params;
    params.extend(pred.params);
    Ok(PlannedEdit {
        sql,
        params,
        preview,
        inverse,
        check,
        uniqueness,
    })
}

fn plan_insert(
    style: SqlIdentStyle,
    placeholder: SqlPlaceholder,
    input: &ApplyTableEditsInput,
    insert: &TableRowInsert,
) -> ConnectorResult<PlannedEdit> {
    let pairs: Vec<(&String, &JsonValue)> = insert
        .values
        .iter()
        .filter(|(_, value)| !is_empty_value(value))
        .collect();
    if pairs.is_empty() {
        return Err(ConnectorError::Query(
            "New row is empty — fill at least one column before saving.".into(),
        ));
    }
    let target = target(style, input);
    let mut params = Vec::new();
    let mut names = Vec::new();
    let mut slots = Vec::new();
    let mut literals = Vec::new();
    for (name, value) in &pairs {
        params.push(param_json(value, col_type(&input.columns, name)));
        names.push(quote_ident(style, name));
        slots.push(slot(placeholder, params.len()));
        literals.push(sql_literal(style, value, col_type(&input.columns, name)));
    }
    let sql = format!(
        "INSERT INTO {target} ({}) VALUES ({})",
        names.join(", "),
        slots.join(", ")
    );
    let preview = format!(
        "INSERT INTO {target} ({}) VALUES ({})",
        names.join(", "),
        literals.join(", ")
    );
    let owned: Vec<(String, JsonValue)> = pairs
        .iter()
        .map(|(name, value)| ((*name).clone(), (*value).clone()))
        .collect();
    let inverse_pred = predicate(style, placeholder, input, &owned, 1)?;
    let inverse = (!any_large(owned.iter().map(|(_, value)| value)))
        .then(|| format!("DELETE FROM {target} WHERE {}", inverse_pred.preview));
    let check = read_row(&target, &inverse_pred);
    Ok(PlannedEdit {
        sql,
        params,
        preview,
        inverse,
        check,
        uniqueness: None,
    })
}

fn plan_delete(
    style: SqlIdentStyle,
    placeholder: SqlPlaceholder,
    input: &ApplyTableEditsInput,
    delete: &TableRowDelete,
) -> ConnectorResult<PlannedEdit> {
    let target = target(style, input);
    let pred = row_predicate(style, placeholder, input, &delete.original, 1)?;
    let uniqueness = keyless_count(style, placeholder, input, &delete.original)?;
    let sql = format!("DELETE FROM {target} WHERE {}", pred.sql);
    let preview = format!("DELETE FROM {target} WHERE {}", pred.preview);
    let inverse = if any_large(delete.original.values()) {
        None
    } else {
        Some(insert_original(style, input, &delete.original)?)
    };
    let check = read_row(&target, &pred);
    Ok(PlannedEdit {
        sql,
        params: pred.params,
        preview,
        inverse,
        check,
        uniqueness,
    })
}

struct Piece {
    sql: String,
    preview: String,
    params: Vec<JsonValue>,
}

fn assign_list(
    style: SqlIdentStyle,
    placeholder: SqlPlaceholder,
    input: &ApplyTableEditsInput,
    pairs: &[(&String, &JsonValue)],
    start: usize,
) -> ConnectorResult<Piece> {
    if pairs.is_empty() {
        return Err(ConnectorError::Query(
            "Update has no original values to restore.".into(),
        ));
    }
    let mut params = Vec::new();
    let mut sql = Vec::new();
    let mut preview = Vec::new();
    for (name, value) in pairs {
        params.push(param_json(value, col_type(&input.columns, name)));
        let ident = quote_ident(style, name);
        sql.push(format!(
            "{ident} = {}",
            slot(placeholder, start + params.len() - 1)
        ));
        preview.push(format!(
            "{ident} = {}",
            sql_literal(style, value, col_type(&input.columns, name))
        ));
    }
    Ok(Piece {
        sql: sql.join(", "),
        preview: preview.join(", "),
        params,
    })
}

fn row_predicate(
    style: SqlIdentStyle,
    placeholder: SqlPlaceholder,
    input: &ApplyTableEditsInput,
    original: &BTreeMap<String, JsonValue>,
    start: usize,
) -> ConnectorResult<Piece> {
    let pks: Vec<&crate::TableEditColumn> =
        input.columns.iter().filter(|column| column.pk).collect();
    let pairs: Vec<(String, JsonValue)> = if pks.is_empty() {
        if original.is_empty() {
            return Err(ConnectorError::Query(format!(
                "Cannot save “{}”: no primary key and the original row is empty.",
                input.table
            )));
        }
        original
            .iter()
            .map(|(name, value)| (name.clone(), value.clone()))
            .collect()
    } else {
        let mut out = Vec::new();
        for pk in pks {
            let value = original.get(&pk.name).ok_or_else(|| {
                ConnectorError::Query(format!(
                    "Cannot save row: primary key “{}” missing from original row.",
                    pk.name
                ))
            })?;
            if is_empty_value(value) {
                return Err(ConnectorError::Query(format!(
                    "Cannot save row: primary key “{}” is empty on the original row.",
                    pk.name
                )));
            }
            out.push((pk.name.clone(), value.clone()));
        }
        out
    };
    predicate(style, placeholder, input, &pairs, start)
}

fn keyless_count(
    style: SqlIdentStyle,
    placeholder: SqlPlaceholder,
    input: &ApplyTableEditsInput,
    original: &BTreeMap<String, JsonValue>,
) -> ConnectorResult<Option<PlannedCount>> {
    if input.columns.iter().any(|column| column.pk) {
        return Ok(None);
    }
    let pairs: Vec<(String, JsonValue)> = original
        .iter()
        .map(|(name, value)| (name.clone(), value.clone()))
        .collect();
    let pred = predicate(style, placeholder, input, &pairs, 1)?;
    let target = target(style, input);
    Ok(Some(PlannedCount {
        sql: format!("SELECT COUNT(*) FROM {target} WHERE {}", pred.sql),
        params: pred.params,
    }))
}

fn predicate(
    style: SqlIdentStyle,
    placeholder: SqlPlaceholder,
    input: &ApplyTableEditsInput,
    pairs: &[(String, JsonValue)],
    start: usize,
) -> ConnectorResult<Piece> {
    if pairs.is_empty() {
        return Err(ConnectorError::Query("Cannot locate the row.".into()));
    }
    let mut params = Vec::new();
    let mut sql = Vec::new();
    let mut preview = Vec::new();
    for (name, value) in pairs {
        let ident = quote_ident(style, name);
        let data_type = col_type(&input.columns, name);
        if is_empty_value(value) {
            sql.push(format!("{ident} IS NULL"));
            preview.push(format!("{ident} IS NULL"));
            continue;
        }
        params.push(param_json(value, data_type));
        sql.push(format!(
            "{ident} = {}",
            slot(placeholder, start + params.len() - 1)
        ));
        preview.push(format!(
            "{ident} = {}",
            sql_literal(style, value, data_type)
        ));
    }
    Ok(Piece {
        sql: sql.join(" AND "),
        preview: preview.join(" AND "),
        params,
    })
}

fn insert_original(
    style: SqlIdentStyle,
    input: &ApplyTableEditsInput,
    original: &BTreeMap<String, JsonValue>,
) -> ConnectorResult<String> {
    if original.is_empty() {
        return Err(ConnectorError::Query(
            "Cannot restore a deleted row with no original values.".into(),
        ));
    }
    let target = target(style, input);
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

fn read_row(target: &str, pred: &Piece) -> PlannedRead {
    PlannedRead {
        sql: format!("SELECT * FROM {target} WHERE {}", pred.sql),
        params: pred.params.clone(),
    }
}

fn any_large<'a>(values: impl IntoIterator<Item = &'a JsonValue>) -> bool {
    values.into_iter().any(is_large_marker)
}

fn target(style: SqlIdentStyle, input: &ApplyTableEditsInput) -> String {
    qualify_table(
        style,
        input.database.as_deref(),
        input.schema.as_deref(),
        &input.table,
    )
}

fn slot(placeholder: SqlPlaceholder, index: usize) -> String {
    match placeholder {
        SqlPlaceholder::Dollar => format!("${index}"),
        SqlPlaceholder::Question => "?".to_string(),
    }
}

/// Numeric text stays a JSON number when the column is numeric, so an int8
/// past 2^53 is still the decimal string (JSON numbers cannot hold it).
fn param_json(value: &JsonValue, data_type: &str) -> JsonValue {
    let JsonValue::String(text) = value else {
        return value.clone();
    };
    if !is_numeric_data_type(data_type) {
        return value.clone();
    }
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return value.clone();
    }
    if let Ok(number) = trimmed.parse::<i64>() {
        if trimmed == number.to_string() {
            return JsonValue::from(number);
        }
    }
    if let Ok(number) = trimmed.parse::<u64>() {
        if trimmed == number.to_string() {
            if let Ok(signed) = i64::try_from(number) {
                return JsonValue::from(signed);
            }
        }
    }
    value.clone()
}

fn is_numeric_data_type(data_type: &str) -> bool {
    let data_type = data_type.to_ascii_lowercase();
    data_type.contains("int")
        || data_type.contains("serial")
        || data_type.contains("numeric")
        || data_type.contains("decimal")
        || data_type.contains("float")
        || data_type.contains("double")
        || data_type.contains("real")
        || data_type.contains("number")
        || data_type.contains("oid")
}

pub const KEYLESS_AMBIGUOUS: &str = "keyless_ambiguous";
pub const AFFECTED_MISMATCH: &str = "affected_mismatch";

pub fn keyless_ambiguous(count: i64) -> ConnectorError {
    ConnectorError::Query(format!(
        "{KEYLESS_AMBIGUOUS}: {count} rows match every column. Refusing the change."
    ))
}

pub fn affected_mismatch(kind: &str, count: u64) -> ConnectorError {
    ConnectorError::Query(format!("{AFFECTED_MISMATCH}: {kind} changed {count} rows."))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::table::edits::{ApplyTableEditsInput, TableEditColumn, TableRowUpdate};
    use serde_json::json;

    fn column(name: &str, data_type: &str, pk: bool) -> TableEditColumn {
        TableEditColumn {
            name: name.into(),
            data_type: data_type.into(),
            nullable: !pk,
            pk,
        }
    }

    fn input(columns: Vec<TableEditColumn>, updates: Vec<TableRowUpdate>) -> ApplyTableEditsInput {
        ApplyTableEditsInput {
            table: "docs".into(),
            schema: None,
            database: None,
            columns,
            updates,
            inserts: Vec::new(),
            deletes: Vec::new(),
            dry_run: false,
        }
    }

    #[test]
    fn two_dirty_rows_are_two_statements() {
        let plan = plan_table_edits(
            SqlIdentStyle::Postgres,
            SqlPlaceholder::Dollar,
            &input(
                vec![column("id", "int8", true), column("body", "text", false)],
                vec![
                    TableRowUpdate {
                        original: BTreeMap::from([("id".into(), json!(1))]),
                        values: BTreeMap::from([("body".into(), json!("a"))]),
                    },
                    TableRowUpdate {
                        original: BTreeMap::from([("id".into(), json!(2))]),
                        values: BTreeMap::from([("body".into(), json!("b"))]),
                    },
                ],
            ),
        )
        .expect("plan");
        assert_eq!(plan.edits.len(), 2);
        assert!(plan.edits[0]
            .sql
            .contains("UPDATE \"docs\" SET \"body\" = $1 WHERE \"id\" = $2"));
        assert!(!plan.edits[0].sql.contains("DROP"));
        assert_eq!(plan.edits[0].params, vec![json!("a"), json!(1)]);
        assert!(plan.edits[0].uniqueness.is_none());
        let rollback = plan.rollback_sql().expect("rollback");
        assert_eq!(rollback.len(), 2);
        assert!(rollback[0].contains("WHERE \"id\" = 2"));
        assert_eq!(
            plan.edits[0].check.sql,
            "SELECT * FROM \"docs\" WHERE \"id\" = $1"
        );
        assert_eq!(plan.edits[0].check.params, vec![json!(1)]);
    }

    #[test]
    fn int8_past_2_53_stays_a_string_parameter() {
        let id = "9007199254740993";
        let plan = plan_table_edits(
            SqlIdentStyle::Postgres,
            SqlPlaceholder::Dollar,
            &input(
                vec![column("id", "int8", true), column("body", "text", false)],
                vec![TableRowUpdate {
                    original: BTreeMap::from([("id".into(), json!(id))]),
                    values: BTreeMap::from([("body".into(), json!("x"))]),
                }],
            ),
        )
        .expect("plan");
        assert_eq!(plan.edits[0].params[1], json!(9_007_199_254_740_993_i64));
        assert!(plan.edits[0].preview.contains(id));
        assert!(!plan.edits[0].preview.contains("9007199254740992"));
    }

    #[test]
    fn a_keyless_table_matches_every_column() {
        let plan = plan_table_edits(
            SqlIdentStyle::Mysql,
            SqlPlaceholder::Question,
            &input(
                vec![column("name", "text", false), column("n", "int", false)],
                vec![TableRowUpdate {
                    original: BTreeMap::from([("name".into(), json!("a")), ("n".into(), json!(1))]),
                    values: BTreeMap::from([("n".into(), json!(2))]),
                }],
            ),
        )
        .expect("plan");
        let count = plan.edits[0].uniqueness.as_ref().expect("count");
        assert!(count.sql.contains("SELECT COUNT(*)"));
        assert!(count.sql.contains("`name` = ?"));
        assert!(count.sql.contains("`n` = ?"));
        assert!(plan.edits[0].sql.contains("`n` = ?"));
        assert!(!plan.edits[0].sql.contains("= 2"));
    }

    #[test]
    fn a_keyless_inverse_finds_the_row_by_its_saved_values() {
        let plan = plan_table_edits(
            SqlIdentStyle::Mysql,
            SqlPlaceholder::Question,
            &input(
                vec![column("name", "text", false), column("n", "int", false)],
                vec![TableRowUpdate {
                    original: BTreeMap::from([("name".into(), json!("a")), ("n".into(), json!(1))]),
                    values: BTreeMap::from([("n".into(), json!(2))]),
                }],
            ),
        )
        .expect("plan");
        let inverse = plan.edits[0].inverse.clone().expect("inverse");
        assert_eq!(
            inverse,
            "UPDATE `docs` SET `n` = 1 WHERE `n` = 2 AND `name` = 'a'"
        );
        assert_eq!(plan.edits[0].check.params, vec![json!(2), json!("a")]);
    }

    #[test]
    fn a_deleted_row_with_a_large_preview_has_no_undo() {
        let mut edits = input(
            vec![column("id", "int", true), column("body", "text", false)],
            Vec::new(),
        );
        edits.deletes = vec![crate::table::edits::TableRowDelete {
            original: BTreeMap::from([
                ("id".into(), json!(1)),
                (
                    "body".into(),
                    json!({ "$large": true, "preview": "abc", "totalChars": 99999 }),
                ),
            ]),
        }];
        let plan = plan_table_edits(SqlIdentStyle::Postgres, SqlPlaceholder::Dollar, &edits)
            .expect("plan");
        assert!(plan.edits[0].inverse.is_none());
        assert!(plan.rollback_sql().is_none());
        assert!(plan.undo(&[Vec::new()]).is_none());
    }

    #[test]
    fn undo_pairs_each_check_with_its_rows() {
        let plan = plan_table_edits(
            SqlIdentStyle::Postgres,
            SqlPlaceholder::Dollar,
            &input(
                vec![column("id", "int8", true), column("body", "text", false)],
                vec![TableRowUpdate {
                    original: BTreeMap::from([("id".into(), json!(1))]),
                    values: BTreeMap::from([("body".into(), json!("a"))]),
                }],
            ),
        )
        .expect("plan");
        let rows = vec![vec![vec![json!(1), json!("a")]]];
        let undo = plan.undo(&rows).expect("undo");
        assert_eq!(undo.checks.len(), 1);
        assert_eq!(undo.checks[0].hash, rows_hash(&rows[0]));
        assert!(plan.undo(&[]).is_none());
    }

    #[test]
    fn a_quote_in_a_value_stays_out_of_the_executed_sql() {
        let plan = plan_table_edits(
            SqlIdentStyle::Postgres,
            SqlPlaceholder::Question,
            &input(
                vec![column("id", "int", true), column("body", "text", false)],
                vec![TableRowUpdate {
                    original: BTreeMap::from([("id".into(), json!(1))]),
                    values: BTreeMap::from([("body".into(), json!("a'); DROP TABLE docs;--"))]),
                }],
            ),
        )
        .expect("plan");
        assert!(!plan.edits[0].sql.contains("DROP"));
        assert!(plan.edits[0].preview.contains("''"));
    }
}
