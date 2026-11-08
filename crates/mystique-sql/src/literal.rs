//! SQL value literals for previews, inverse statements and exports.
//!
//! MySQL differs from Postgres / SQLite / generic SQL in two places: booleans
//! are `1` / `0`, and `\` is escaped because `\'` closes a string there.

use serde_json::Value as JsonValue;

use crate::SqlDialect;

pub(crate) fn is_numeric_type(t: &str) -> bool {
    let t = t.to_ascii_lowercase();
    t.contains("int")
        || t.contains("serial")
        || t.contains("numeric")
        || t.contains("decimal")
        || t.contains("float")
        || t.contains("double")
        || t.contains("real")
        || t.contains("money")
        || t.contains("number")
        || t.contains("oid")
}

fn is_bool_type(t: &str) -> bool {
    let t = t.to_ascii_lowercase();
    t == "bool" || t == "boolean" || t == "bit" || t == "bit(1)" || t.contains("tinyint(1)")
}

fn bool_sql(dialect: SqlDialect, v: bool) -> &'static str {
    match dialect {
        SqlDialect::Mysql => {
            if v {
                "1"
            } else {
                "0"
            }
        }
        SqlDialect::Postgres | SqlDialect::Sqlite | SqlDialect::Generic => {
            if v {
                "TRUE"
            } else {
                "FALSE"
            }
        }
    }
}

/// One SQL string literal. MySQL also escapes `\` because `\'` is a quote there.
pub fn sql_string_literal(dialect: SqlDialect, s: &str) -> String {
    match dialect {
        SqlDialect::Mysql => {
            let mut out = String::with_capacity(s.len() + 2);
            out.push('\'');
            for c in s.chars() {
                match c {
                    '\\' => out.push_str("\\\\"),
                    '\'' => out.push_str("''"),
                    '\0' => out.push_str("\\0"),
                    _ => out.push(c),
                }
            }
            out.push('\'');
            out
        }
        SqlDialect::Postgres | SqlDialect::Sqlite | SqlDialect::Generic => {
            format!("'{}'", s.replace('\'', "''"))
        }
    }
}

pub fn sql_literal(dialect: SqlDialect, value: &JsonValue, data_type: &str) -> String {
    match value {
        JsonValue::Null => "NULL".into(),
        JsonValue::Bool(b) => bool_sql(dialect, *b).into(),
        JsonValue::Number(n) => n.to_string(),
        JsonValue::String(s) => {
            if is_bool_type(data_type) {
                let b = matches!(s.to_ascii_lowercase().as_str(), "true" | "1" | "yes" | "t");
                return bool_sql(dialect, b).into();
            }
            let t = s.trim();
            if is_numeric_type(data_type) && !t.is_empty() && is_plain_number(t) {
                return t.to_string();
            }
            sql_string_literal(dialect, s)
        }
        other => sql_string_literal(dialect, &other.to_string()),
    }
}

fn is_plain_number(s: &str) -> bool {
    let mut chars = s.chars().peekable();
    if matches!(chars.peek(), Some('+') | Some('-')) {
        chars.next();
    }
    let rest: String = chars.collect();
    if rest.is_empty() {
        return false;
    }
    let mut seen_dot = false;
    rest.chars().all(|c| {
        if c == '.' {
            if seen_dot {
                return false;
            }
            seen_dot = true;
            true
        } else {
            c.is_ascii_digit()
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn strings_escape_quotes_per_dialect() {
        assert_eq!(
            sql_string_literal(SqlDialect::Postgres, "O'Brien"),
            "'O''Brien'"
        );
        assert_eq!(sql_string_literal(SqlDialect::Sqlite, "a\\b"), "'a\\b'");
        assert_eq!(
            sql_string_literal(SqlDialect::Mysql, "a\\'b\0"),
            "'a\\\\''b\\0'"
        );
    }

    #[test]
    fn booleans_and_numbers_follow_the_column_type() {
        assert_eq!(sql_literal(SqlDialect::Mysql, &json!(true), ""), "1");
        assert_eq!(sql_literal(SqlDialect::Postgres, &json!(true), ""), "TRUE");
        assert_eq!(
            sql_literal(SqlDialect::Sqlite, &json!("yes"), "boolean"),
            "TRUE"
        );
        assert_eq!(
            sql_literal(SqlDialect::Postgres, &json!(" 12.5 "), "numeric"),
            "12.5"
        );
        assert_eq!(
            sql_literal(SqlDialect::Postgres, &json!("12.5.1"), "numeric"),
            "'12.5.1'"
        );
        assert_eq!(
            sql_literal(SqlDialect::Postgres, &json!(null), "int"),
            "NULL"
        );
    }
}
