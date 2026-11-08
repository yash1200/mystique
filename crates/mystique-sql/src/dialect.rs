use sqlparser::dialect::{Dialect, GenericDialect, MySqlDialect, PostgreSqlDialect, SQLiteDialect};

/// SQL dialect for split / parse / quoting.
///
/// A fifth engine (ClickHouse, SQL Server, DuckDB) picks a variant here
/// instead of copying keyword lists.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SqlDialect {
    Postgres,
    Mysql,
    Sqlite,
    #[default]
    Generic,
}

impl SqlDialect {
    pub fn from_engine(id: &str) -> Self {
        match id.trim().to_ascii_lowercase().as_str() {
            "postgres" | "postgresql" | "pg" | "cockroachdb" => Self::Postgres,
            "mysql" | "mariadb" | "tidb" => Self::Mysql,
            "sqlite" | "turso" => Self::Sqlite,
            _ => Self::Generic,
        }
    }

    pub(crate) fn parser_dialect(self) -> Box<dyn Dialect> {
        match self {
            Self::Postgres => Box::new(PostgreSqlDialect {}),
            Self::Mysql => Box::new(MySqlDialect {}),
            Self::Sqlite => Box::new(SQLiteDialect {}),
            Self::Generic => Box::new(GenericDialect {}),
        }
    }
}

/// How a SQL engine spells bound parameters.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SqlPlaceholder {
    /// Postgres `$1`, `$2`, …
    Dollar,
    /// MySQL and SQLite `?`.
    Question,
}

impl SqlPlaceholder {
    /// The 1-based `index`th placeholder.
    pub fn slot(self, index: usize) -> String {
        match self {
            Self::Dollar => format!("${index}"),
            Self::Question => "?".to_string(),
        }
    }
}
