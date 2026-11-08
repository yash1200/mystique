//! Read-only statements the grid runs next to an edit (filter value lists).

use crate::{qualify_table, quote_ident, SqlDialect};

/// `SELECT DISTINCT` for the grid filter. The caller checks that `column` is
/// one the catalog returned. `limit` is clamped to 1..=1000.
pub fn distinct_values_sql(
    dialect: SqlDialect,
    database: Option<&str>,
    schema: Option<&str>,
    table: &str,
    column: &str,
    limit: u32,
) -> String {
    let limit = limit.clamp(1, 1_000);
    let ident = quote_ident(dialect, column);
    let target = qualify_table(dialect, database, schema, table);
    format!("SELECT DISTINCT {ident} FROM {target} WHERE {ident} IS NOT NULL LIMIT {limit}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn distinct_quotes_the_column_and_caps_the_limit() {
        assert_eq!(
            distinct_values_sql(SqlDialect::Mysql, Some("app"), None, "docs", "bo`dy", 0),
            "SELECT DISTINCT `bo``dy` FROM `app`.`docs` WHERE `bo``dy` IS NOT NULL LIMIT 1"
        );
    }
}
