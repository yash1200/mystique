use super::*;
use crate::dml::{ApplyTableEditsInput, TableEditColumn, TableRowUpdate};
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
        SqlDialect::Postgres,
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
        SqlDialect::Postgres,
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
        SqlDialect::Mysql,
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
        SqlDialect::Mysql,
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
    edits.deletes = vec![crate::dml::TableRowDelete {
        original: BTreeMap::from([
            ("id".into(), json!(1)),
            (
                "body".into(),
                json!({ "$large": true, "preview": "abc", "totalChars": 99999 }),
            ),
        ]),
    }];
    let plan =
        plan_table_edits(SqlDialect::Postgres, SqlPlaceholder::Dollar, &edits).expect("plan");
    assert!(plan.edits[0].inverse.is_none());
    assert!(plan.rollback_sql().is_none());
    assert!(plan.undo(&[Vec::new()]).is_none());
}

#[test]
fn undo_pairs_each_check_with_its_rows() {
    let plan = plan_table_edits(
        SqlDialect::Postgres,
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
        SqlDialect::Postgres,
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
