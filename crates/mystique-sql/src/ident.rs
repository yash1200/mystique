//! The one identifier-quoting module. Every crate quotes through here.

use super::SqlDialect;

impl SqlDialect {
    /// Quote a single identifier (`"users"` / `` `users` ``).
    pub fn quote_ident(self, name: &str) -> String {
        match self {
            Self::Mysql => format!("`{}`", name.replace('`', "``")),
            Self::Postgres | Self::Sqlite | Self::Generic => {
                format!("\"{}\"", name.replace('"', "\"\""))
            }
        }
    }

    /// Quote a dotted path (`schema.table`). Empty parts are skipped.
    pub fn quote_path<I, S>(self, parts: I) -> String
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        parts
            .into_iter()
            .map(|p| p.as_ref().trim().to_string())
            .filter(|p| !p.is_empty())
            .map(|p| self.quote_ident(&p))
            .collect::<Vec<_>>()
            .join(".")
    }
}

pub fn quote_ident(dialect: SqlDialect, name: &str) -> String {
    dialect.quote_ident(name)
}

pub fn quote_path<I, S>(dialect: SqlDialect, parts: I) -> String
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    dialect.quote_path(parts)
}

/// Quoted table name for a grid / transfer statement.
///
/// MySQL has no schemas: `schema` (or else `database`) is the database
/// qualifier. Other dialects qualify by `schema` only; `database` is the
/// connection's and is never written.
pub fn qualify_table(
    dialect: SqlDialect,
    database: Option<&str>,
    schema: Option<&str>,
    table: &str,
) -> String {
    let t = quote_ident(dialect, table);
    match dialect {
        SqlDialect::Mysql => {
            let db = schema
                .filter(|s| !s.is_empty())
                .or_else(|| database.filter(|s| !s.is_empty()));
            match db {
                Some(d) => format!("{}.{}", quote_ident(dialect, d), t),
                None => t,
            }
        }
        SqlDialect::Postgres | SqlDialect::Sqlite | SqlDialect::Generic => {
            match schema.filter(|s| !s.is_empty()) {
                Some(s) => format!("{}.{}", quote_ident(dialect, s), t),
                None => t,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn postgres_quotes_and_escapes() {
        assert_eq!(SqlDialect::Postgres.quote_ident("users"), "\"users\"");
        assert_eq!(SqlDialect::Postgres.quote_ident("a\"b"), "\"a\"\"b\"");
    }

    #[test]
    fn sqlite_and_generic_use_double_quotes() {
        for dialect in [SqlDialect::Sqlite, SqlDialect::Generic] {
            assert_eq!(quote_ident(dialect, "users"), "\"users\"");
            assert_eq!(quote_ident(dialect, "we\"ird"), "\"we\"\"ird\"");
            // A backtick is an ordinary character inside double quotes.
            assert_eq!(quote_ident(dialect, "a`b"), "\"a`b\"");
        }
    }

    #[test]
    fn mysql_uses_backticks() {
        assert_eq!(SqlDialect::Mysql.quote_ident("users"), "`users`");
        assert_eq!(SqlDialect::Mysql.quote_path(["db", "t"]), "`db`.`t`");
    }

    #[test]
    fn mysql_doubles_embedded_backticks_and_keeps_double_quotes() {
        assert_eq!(quote_ident(SqlDialect::Mysql, "na`me"), "`na``me`");
        assert_eq!(quote_ident(SqlDialect::Mysql, "``"), "``````");
        assert_eq!(quote_ident(SqlDialect::Mysql, "a\"b"), "`a\"b`");
    }

    #[test]
    fn empty_and_spaced_names_are_still_quoted() {
        assert_eq!(quote_ident(SqlDialect::Postgres, ""), "\"\"");
        assert_eq!(quote_ident(SqlDialect::Postgres, "my col"), "\"my col\"");
        assert_eq!(
            quote_path(SqlDialect::Postgres, [" s ", "", "t"]),
            "\"s\".\"t\""
        );
    }

    #[test]
    fn qualify_table_uses_schema_or_mysql_database() {
        assert_eq!(
            qualify_table(SqlDialect::Postgres, Some("db"), Some("public"), "t"),
            "\"public\".\"t\""
        );
        assert_eq!(
            qualify_table(SqlDialect::Postgres, Some("db"), None, "t"),
            "\"t\""
        );
        assert_eq!(
            qualify_table(SqlDialect::Sqlite, None, Some("main"), "t\"x"),
            "\"main\".\"t\"\"x\""
        );
        assert_eq!(
            qualify_table(SqlDialect::Mysql, Some("app"), None, "t"),
            "`app`.`t`"
        );
        assert_eq!(
            qualify_table(SqlDialect::Mysql, Some("app"), Some("other"), "t"),
            "`other`.`t`"
        );
        assert_eq!(
            qualify_table(SqlDialect::Mysql, Some(""), Some(""), "t"),
            "`t`"
        );
    }
}
