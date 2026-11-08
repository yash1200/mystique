use super::*;

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
        SqlDialect::Postgres,
        &[f("name", "contains", "a_b"), f("age", "gt", "10")],
    )
    .unwrap();
    assert!(where_sql.contains("LOWER(CAST(\"name\" AS TEXT)) LIKE LOWER('%a!_b%') ESCAPE '!'"));
    assert!(where_sql.contains("\"age\" > 10"));
}

#[test]
fn sql_eq_quotes_numeric_looking_text() {
    let where_sql = sql_where_clause(SqlDialect::Postgres, &[f("status", "eq", "10")]).unwrap();
    assert_eq!(where_sql, " WHERE \"status\" = '10'");
}

#[test]
fn sql_quotes_ident_and_string() {
    let where_sql = sql_where_clause(SqlDialect::Mysql, &[f("na`me", "eq", "O'Brien")]).unwrap();
    assert_eq!(where_sql, " WHERE `na``me` = 'O''Brien'");
}

/// MySQL treats `\'` as a quote. Doubling `'` alone lets the value close the literal.
#[test]
fn mysql_filter_backslash_quote_stays_inside_the_literal() {
    let payload = "\\' OR 1=1 --";
    let where_sql = sql_where_clause(SqlDialect::Mysql, &[f("name", "eq", payload)]).unwrap();
    assert_eq!(where_sql, " WHERE `name` = '\\\\'' OR 1=1 --'");
    assert!(mysql_literal_is_closed(
        where_sql.trim_start_matches(" WHERE `name` = ")
    ));
}

#[test]
fn mysql_filter_backslash_does_not_escape_the_closing_quote() {
    let where_sql = sql_where_clause(SqlDialect::Mysql, &[f("name", "eq", "\\\\")]).unwrap();
    assert_eq!(where_sql, " WHERE `name` = '\\\\\\\\'");
    assert!(mysql_literal_is_closed(
        where_sql.trim_start_matches(" WHERE `name` = ")
    ));
}

#[test]
fn filter_column_rejects_nul() {
    let err = sql_where_clause(SqlDialect::Mysql, &[f("a\0b", "eq", "1")]).unwrap_err();
    assert!(err.to_string().contains("identifier"), "{err}");
}

/// True when `lit` is exactly one MySQL string literal (backslash escapes on).
fn mysql_literal_is_closed(lit: &str) -> bool {
    let b = lit.as_bytes();
    if b.first() != Some(&b'\'') {
        return false;
    }
    let mut i = 1;
    while i < b.len() {
        if b[i] == b'\\' {
            i += 2;
            continue;
        }
        if b[i] == b'\'' {
            if i + 1 < b.len() && b[i + 1] == b'\'' {
                i += 2;
                continue;
            }
            return i + 1 == b.len();
        }
        i += 1;
    }
    false
}

#[test]
fn code_filter_json_equals() {
    let where_sql = sql_preview_where(
        SqlDialect::Postgres,
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

#[test]
fn preview_sort_sql_quotes_and_dir() {
    let cols = ["na\"me"];
    let sql = preview_sort_sql(SqlDialect::Postgres, cols, Some("na\"me"), Some("desc")).unwrap();
    assert_eq!(sql, " ORDER BY \"na\"\"me\" DESC");
}

#[test]
fn preview_sort_sql_unknown_column_is_none() {
    let cols = ["id"];
    assert!(preview_sort_sql(SqlDialect::Postgres, cols, Some("nope"), Some("asc")).is_none());
    assert!(preview_sort_sql(SqlDialect::Postgres, cols, None, Some("desc")).is_none());
}

#[test]
fn preview_sort_field_defaults_asc() {
    let cols = ["age"];
    assert_eq!(
        preview_sort_field(cols, Some("age"), None),
        Some(("age", 1))
    );
    assert_eq!(
        preview_sort_field(cols, Some("age"), Some("DESC")),
        Some(("age", -1))
    );
}
