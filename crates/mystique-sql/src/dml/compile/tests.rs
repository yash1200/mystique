use super::compile_update;
use crate::dml::{ApplyTableEditsInput, TableEditColumn, TableRowUpdate};
use crate::SqlDialect;
use serde_json::json;
use std::collections::BTreeMap;

fn column(name: &str, data_type: &str, pk: bool) -> TableEditColumn {
    TableEditColumn {
        name: name.into(),
        data_type: data_type.into(),
        nullable: false,
        pk,
    }
}

fn update_sql(id: &str, data_type: &str) -> String {
    let mut original = BTreeMap::new();
    original.insert("id".into(), json!(id));
    let mut values = BTreeMap::new();
    values.insert("note".into(), json!("x"));
    compile_update(
        SqlDialect::Postgres,
        &ApplyTableEditsInput {
            table: "t".into(),
            schema: None,
            database: None,
            columns: vec![column("id", data_type, true), column("note", "text", false)],
            updates: vec![],
            inserts: vec![],
            deletes: vec![],
            dry_run: false,
        },
        &TableRowUpdate { original, values },
    )
    .expect("update")
}

#[test]
fn int8_identity_matches_one_row_not_its_neighbor() {
    let sql = update_sql("9007199254740993", "int8");
    assert!(sql.contains("\"id\" = 9007199254740993"), "{sql}");
    assert!(!sql.contains("9007199254740992"), "{sql}");

    let max = update_sql("9223372036854775807", "int8");
    assert!(max.contains("\"id\" = 9223372036854775807"), "{max}");
    assert!(!max.contains("9223372036854775806"), "{max}");
}

#[test]
fn numeric_identity_keeps_the_fraction() {
    let sql = update_sql("12345678901234567890.25", "numeric");
    assert!(sql.contains("\"id\" = 12345678901234567890.25"), "{sql}");
}

/// SQLite used to pass the Postgres ident style. Its statements must not change.
#[test]
fn sqlite_statements_match_the_old_postgres_style() {
    use crate::dml::{compile_upsert, TableRowInsert};
    let input = ApplyTableEditsInput {
        table: "t\"x".into(),
        schema: Some("main".into()),
        database: None,
        columns: vec![
            column("id", "integer", true),
            column("ok", "boolean", false),
        ],
        updates: vec![],
        inserts: vec![],
        deletes: vec![],
        dry_run: false,
    };
    let insert = TableRowInsert {
        values: BTreeMap::from([
            ("id".into(), json!("7")),
            ("ok".into(), json!(true)),
            ("note".into(), json!("it's")),
        ]),
    };
    let sqlite = compile_upsert(SqlDialect::Sqlite, &input, &insert).expect("sqlite");
    let postgres = compile_upsert(SqlDialect::Postgres, &input, &insert).expect("postgres");
    assert_eq!(sqlite, postgres);
    assert_eq!(
        sqlite,
        "INSERT INTO \"main\".\"t\"\"x\" (\"id\", \"note\", \"ok\") VALUES (7, 'it''s', TRUE) \
         ON CONFLICT (\"id\") DO UPDATE SET \"note\" = EXCLUDED.\"note\", \"ok\" = EXCLUDED.\"ok\""
    );
}
