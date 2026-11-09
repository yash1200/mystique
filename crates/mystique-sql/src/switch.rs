//! Whether a batch moves its connection to another database (`USE`).
//!
//! The row preview names an `INSERT`'s columns from the table in the
//! database the session opened with; after a `USE` that guess is unsafe.

use sqlparser::ast::Statement;
use sqlparser::parser::Parser;

use super::{split_statements, SqlDialect};

/// True when a statement in `sql` is a `USE`, so later batches on the same
/// connection may not run in the database it opened with. Text that does
/// not parse but starts with `USE` counts.
pub fn switches_database(sql: &str, dialect: SqlDialect) -> bool {
    split_statements(sql, dialect)
        .iter()
        .any(|stmt| match parse(stmt, dialect) {
            Some(statements) => statements.iter().any(|s| matches!(s, Statement::Use(_))),
            None => {
                let head: String = stmt
                    .trim_start()
                    .chars()
                    .take(4)
                    .collect::<String>()
                    .to_ascii_lowercase();
                head.starts_with("use") && !head[3..].starts_with(|c: char| c.is_alphanumeric())
            }
        })
}

fn parse(sql: &str, dialect: SqlDialect) -> Option<Vec<Statement>> {
    let boxed = dialect.parser_dialect();
    match Parser::parse_sql(&*boxed, sql) {
        Ok(statements) => Some(statements),
        Err(_) if !matches!(dialect, SqlDialect::Generic) => {
            Parser::parse_sql(&sqlparser::dialect::GenericDialect {}, sql).ok()
        }
        Err(_) => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn use_switches_the_database() {
        assert!(switches_database("SELECT 1; USE prod", SqlDialect::Mysql));
        assert!(switches_database("use `prod`", SqlDialect::Mysql));
        assert!(!switches_database("SELECT * FROM users", SqlDialect::Mysql));
        assert!(!switches_database(
            "UPDATE user_settings SET a = 1",
            SqlDialect::Mysql
        ));
    }
}
