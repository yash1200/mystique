use super::*;
use crate::catalog::{ColumnInfo, TableFilter};

fn f(column: &str, op: &str, value: &str) -> TableFilter {
    TableFilter {
        column: column.into(),
        op: op.into(),
        value: value.into(),
    }
}

#[test]
fn sql_eq_and_like() {
    let where_sql = sql_where_clause(
        SqlIdentStyle::Postgres,
        &[f("name", "contains", "a_b"), f("age", "gt", "10")],
    )
    .unwrap();
    assert!(where_sql.contains("LOWER(CAST(\"name\" AS TEXT)) LIKE LOWER('%a!_b%') ESCAPE '!'"));
    assert!(where_sql.contains("\"age\" > 10"));
}

#[test]
fn sql_eq_quotes_numeric_looking_text() {
    let where_sql = sql_where_clause(SqlIdentStyle::Postgres, &[f("status", "eq", "10")]).unwrap();
    assert_eq!(where_sql, " WHERE \"status\" = '10'");
}

#[test]
fn sql_quotes_ident_and_string() {
    let where_sql = sql_where_clause(SqlIdentStyle::Mysql, &[f("na`me", "eq", "O'Brien")]).unwrap();
    assert_eq!(where_sql, " WHERE `na``me` = 'O''Brien'");
}

#[test]
fn code_filter_json_equals() {
    let where_sql = sql_preview_where(
        SqlIdentStyle::Postgres,
        &[],
        Some(r#"{ "email": "ajay@x.com" }"#),
    )
    .unwrap();
    assert_eq!(where_sql, " WHERE \"email\" = 'ajay@x.com'");
}

#[test]
fn code_filter_expression() {
    let rules = parse_code_filter_rules("status = 'active' AND age > 10").unwrap();
    assert_eq!(rules.len(), 2);
    assert_eq!(rules[0].column, "status");
    assert_eq!(rules[0].op, "eq");
    assert_eq!(rules[1].op, "gt");
}

fn col(name: &str) -> ColumnInfo {
    ColumnInfo {
        name: name.into(),
        data_type: "text".into(),
        nullable: true,
        pk: false,
        enum_values: None,
        default_value: None,
        references: None,
    }
}

#[test]
fn preview_sort_sql_quotes_and_dir() {
    let cols = [col("na\"me")];
    let sql =
        preview_sort_sql(SqlIdentStyle::Postgres, &cols, Some("na\"me"), Some("desc")).unwrap();
    assert_eq!(sql, " ORDER BY \"na\"\"me\" DESC");
}

#[test]
fn preview_sort_sql_unknown_column_is_none() {
    let cols = [col("id")];
    assert!(preview_sort_sql(SqlIdentStyle::Postgres, &cols, Some("nope"), Some("asc")).is_none());
    assert!(preview_sort_sql(SqlIdentStyle::Postgres, &cols, None, Some("desc")).is_none());
}

#[test]
fn preview_sort_field_defaults_asc() {
    let cols = [col("age")];
    assert_eq!(
        preview_sort_field(&cols, Some("age"), None),
        Some(("age", 1))
    );
    assert_eq!(
        preview_sort_field(&cols, Some("age"), Some("DESC")),
        Some(("age", -1))
    );
}
