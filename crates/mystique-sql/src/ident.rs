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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn postgres_quotes_and_escapes() {
        assert_eq!(SqlDialect::Postgres.quote_ident("users"), "\"users\"");
        assert_eq!(SqlDialect::Postgres.quote_ident("a\"b"), "\"a\"\"b\"");
    }

    #[test]
    fn mysql_uses_backticks() {
        assert_eq!(SqlDialect::Mysql.quote_ident("users"), "`users`");
        assert_eq!(SqlDialect::Mysql.quote_path(["db", "t"]), "`db`.`t`");
    }
}
