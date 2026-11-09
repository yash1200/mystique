//! Read-only statements the grid runs next to an edit (filter value lists).

use crate::filters::{sql_where_clause, TableFilter};
use crate::{qualify_table, quote_ident, SqlDialect};

/// `SELECT DISTINCT` for the grid filter. The caller checks that `column` is
/// one the catalog returned. `limit` is clamped to 1..=1000.
///
/// A non-empty `prefix` keeps values whose text starts with it (ASCII
/// case-insensitive, `%` and `_` literal), the same match as the grid's
/// "starts with" filter, so completion of a wide column finds rows past the
/// first `limit`.
pub fn distinct_values_sql(
    dialect: SqlDialect,
    database: Option<&str>,
    schema: Option<&str>,
    table: &str,
    column: &str,
    limit: u32,
    prefix: Option<&str>,
) -> String {
    let limit = limit.clamp(1, 1_000);
    let ident = quote_ident(dialect, column);
    let target = qualify_table(dialect, database, schema, table);
    let starts = prefix
        .filter(|p| !p.is_empty())
        .and_then(|value| {
            sql_where_clause(
                dialect,
                &[TableFilter {
                    column: column.to_string(),
                    op: "starts".into(),
                    value: value.to_string(),
                }],
            )
            .ok()
        })
        .map(|clause| format!(" AND{}", clause.trim_start_matches(" WHERE")))
        .unwrap_or_default();
    format!("SELECT DISTINCT {ident} FROM {target} WHERE {ident} IS NOT NULL{starts} LIMIT {limit}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn distinct_quotes_the_column_and_caps_the_limit() {
        assert_eq!(
            distinct_values_sql(
                SqlDialect::Mysql,
                Some("app"),
                None,
                "docs",
                "bo`dy",
                0,
                None
            ),
            "SELECT DISTINCT `bo``dy` FROM `app`.`docs` WHERE `bo``dy` IS NOT NULL LIMIT 1"
        );
    }

    #[test]
    fn a_prefix_is_a_literal_starts_with() {
        assert_eq!(
            distinct_values_sql(
                SqlDialect::Postgres,
                None,
                Some("public"),
                "users",
                "email",
                50,
                Some("a_b%'"),
            ),
            "SELECT DISTINCT \"email\" FROM \"public\".\"users\" WHERE \"email\" IS NOT NULL \
             AND LOWER(CAST(\"email\" AS TEXT)) LIKE LOWER('a!_b!%''%') ESCAPE '!' LIMIT 50"
        );
        assert_eq!(
            distinct_values_sql(SqlDialect::Sqlite, None, None, "t", "c", 10, Some("")),
            "SELECT DISTINCT \"c\" FROM \"t\" WHERE \"c\" IS NOT NULL LIMIT 10"
        );
    }
}
