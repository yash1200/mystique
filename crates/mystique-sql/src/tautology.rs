//! `UPDATE` / `DELETE` that touch every row: no `WHERE`, or a `WHERE` that is
//! always true (`1 = 1`, `id = id`, `x OR NOT x`). A warning for the
//! production dialog, never a block.
//!
//! Folding is deliberately small: literals, arithmetic on numbers, casts,
//! `AND` / `OR` / `NOT`, comparisons, `IS [NOT] NULL`, and the same
//! expression on both sides. Anything else is "unknown" and not reported.

use std::ops::ControlFlow;

use sqlparser::ast::{
    BinaryOperator, Expr, FromTable, Statement, TableFactor, TableWithJoins, UnaryOperator, Value,
    Visit, Visitor,
};

use super::risk::parse_result;
use super::{split_statements, SqlDialect};

/// One line per `UPDATE` / `DELETE` in `sql` that changes every row.
///
/// Statements that do not parse are skipped: the dialog still shows the SQL.
pub fn where_warnings(sql: &str, dialect: SqlDialect) -> Vec<String> {
    let mut out = Vec::new();
    for stmt in split_statements(sql, dialect) {
        let Ok(parsed) = parse_result(&stmt, dialect) else {
            continue;
        };
        for statement in &parsed {
            let mut scan = WhereScan { out: &mut out };
            let _ = statement.visit(&mut scan);
        }
    }
    out
}

struct WhereScan<'a> {
    out: &'a mut Vec<String>,
}

impl Visitor for WhereScan<'_> {
    type Break = ();

    // Also reached for DML nested in a CTE (`WITH … DELETE`).
    fn pre_visit_statement(&mut self, stmt: &Statement) -> ControlFlow<()> {
        let found = match stmt {
            Statement::Update {
                table, selection, ..
            } => Some(("UPDATE", "changes", table_name(table), selection.as_ref())),
            Statement::Delete(delete) if delete.limit.is_none() => {
                let from = match &delete.from {
                    FromTable::WithFromKeyword(tables) | FromTable::WithoutKeyword(tables) => {
                        tables
                    }
                };
                let name = delete
                    .tables
                    .first()
                    .map(ToString::to_string)
                    .or_else(|| from.first().map(table_name))
                    .unwrap_or_else(|| "the table".into());
                Some(("DELETE", "removes", name, delete.selection.as_ref()))
            }
            _ => None,
        };
        if let Some((kind, verb, table, selection)) = found {
            match selection {
                None => self.out.push(format!(
                    "{kind} on {table} has no WHERE: it {verb} every row."
                )),
                Some(expr) if truth(expr) == Truth::True => self.out.push(format!(
                    "WHERE {} is always true: this {kind} {verb} every row of {table}.",
                    clip(&expr.to_string())
                )),
                Some(_) => {}
            }
        }
        ControlFlow::Continue(())
    }
}

fn table_name(table: &TableWithJoins) -> String {
    match &table.relation {
        TableFactor::Table { name, .. } => name.to_string(),
        other => other.to_string(),
    }
}

fn clip(text: &str) -> String {
    const MAX: usize = 60;
    if text.chars().count() <= MAX {
        text.to_string()
    } else {
        let cut: String = text.chars().take(MAX - 1).collect();
        format!("{cut}…")
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Truth {
    True,
    False,
    Unknown,
}

impl From<bool> for Truth {
    fn from(value: bool) -> Self {
        if value {
            Self::True
        } else {
            Self::False
        }
    }
}

fn truth(expr: &Expr) -> Truth {
    match expr {
        Expr::Nested(inner) => truth(inner),
        Expr::UnaryOp {
            op: UnaryOperator::Not,
            expr,
        } => match truth(expr) {
            Truth::True => Truth::False,
            Truth::False => Truth::True,
            Truth::Unknown => Truth::Unknown,
        },
        Expr::BinaryOp {
            left,
            op: BinaryOperator::And,
            right,
        } => match (truth(left), truth(right)) {
            (Truth::True, Truth::True) => Truth::True,
            (Truth::False, _) | (_, Truth::False) => Truth::False,
            _ => Truth::Unknown,
        },
        Expr::BinaryOp {
            left,
            op: BinaryOperator::Or,
            right,
        } => {
            if complementary(left, right) {
                return Truth::True;
            }
            match (truth(left), truth(right)) {
                (Truth::True, _) | (_, Truth::True) => Truth::True,
                (Truth::False, Truth::False) => Truth::False,
                _ => Truth::Unknown,
            }
        }
        Expr::BinaryOp { left, op, right } => compare(left, op, right),
        Expr::IsNull(inner) => match literal(inner) {
            Some(Lit::Null) => Truth::True,
            Some(_) => Truth::False,
            None => Truth::Unknown,
        },
        Expr::IsNotNull(inner) => match literal(inner) {
            Some(Lit::Null) => Truth::False,
            Some(_) => Truth::True,
            None => Truth::Unknown,
        },
        Expr::IsTrue(inner) | Expr::IsNotFalse(inner) => match truth(inner) {
            Truth::True => Truth::True,
            _ => Truth::Unknown,
        },
        // MySQL and SQLite read a bare number as a boolean: `WHERE 1`.
        _ => match literal(expr) {
            Some(Lit::Bool(value)) => value.into(),
            Some(Lit::Num(value)) => (value != 0.0).into(),
            _ => Truth::Unknown,
        },
    }
}

/// `x OR NOT x` and `x IS NULL OR x IS NOT NULL`, either way round.
fn complementary(left: &Expr, right: &Expr) -> bool {
    let left = strip(left);
    let right = strip(right);
    let negates = |a: &Expr, b: &Expr| match a {
        Expr::UnaryOp {
            op: UnaryOperator::Not,
            expr,
        } => strip(expr) == b,
        _ => false,
    };
    let null_pair = |a: &Expr, b: &Expr| match (a, b) {
        (Expr::IsNull(x), Expr::IsNotNull(y)) => strip(x) == strip(y),
        _ => false,
    };
    negates(left, right) || negates(right, left) || null_pair(left, right) || null_pair(right, left)
}

fn strip(expr: &Expr) -> &Expr {
    match expr {
        Expr::Nested(inner) => strip(inner),
        other => other,
    }
}

fn compare(left: &Expr, op: &BinaryOperator, right: &Expr) -> Truth {
    use std::cmp::Ordering;
    let ordering = match (literal(left), literal(right)) {
        (Some(a), Some(b)) => match a.cmp(&b) {
            Some(ordering) => ordering,
            None => return Truth::Unknown,
        },
        // `id = id`: the same column (or pure expression) on both sides.
        // NULL rows still fall out, but every other row matches.
        _ if strip(left) == strip(right) && is_pure(left) => Ordering::Equal,
        _ => return Truth::Unknown,
    };
    let result = match op {
        BinaryOperator::Eq | BinaryOperator::Spaceship => ordering == Ordering::Equal,
        BinaryOperator::NotEq => ordering != Ordering::Equal,
        BinaryOperator::Lt => ordering == Ordering::Less,
        BinaryOperator::LtEq => ordering != Ordering::Greater,
        BinaryOperator::Gt => ordering == Ordering::Greater,
        BinaryOperator::GtEq => ordering != Ordering::Less,
        _ => return Truth::Unknown,
    };
    result.into()
}

/// No call and no subquery: `random() = random()` is not always true.
fn is_pure(expr: &Expr) -> bool {
    struct Impure;
    impl Visitor for Impure {
        type Break = ();
        fn pre_visit_expr(&mut self, expr: &Expr) -> ControlFlow<()> {
            match expr {
                Expr::Function(_)
                | Expr::Subquery(_)
                | Expr::Exists { .. }
                | Expr::InSubquery { .. } => ControlFlow::Break(()),
                _ => ControlFlow::Continue(()),
            }
        }
    }
    expr.visit(&mut Impure).is_continue()
}

#[derive(Debug, Clone, PartialEq)]
enum Lit {
    Num(f64),
    Str(String),
    Bool(bool),
    Null,
}

impl Lit {
    fn as_num(&self) -> Option<f64> {
        match self {
            Self::Num(value) => Some(*value),
            Self::Bool(value) => Some(if *value { 1.0 } else { 0.0 }),
            Self::Str(text) => text.trim().parse().ok(),
            Self::Null => None,
        }
    }

    fn cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        match (self, other) {
            (Self::Null, _) | (_, Self::Null) => None,
            (Self::Str(a), Self::Str(b)) => Some(a.cmp(b)),
            (Self::Bool(a), Self::Bool(b)) => Some(a.cmp(b)),
            (a, b) => a.as_num()?.partial_cmp(&b.as_num()?),
        }
    }
}

fn literal(expr: &Expr) -> Option<Lit> {
    match expr {
        Expr::Nested(inner) => literal(inner),
        Expr::Value(value) => match &value.value {
            Value::Number(text, _) => text.parse().ok().map(Lit::Num),
            Value::SingleQuotedString(text)
            | Value::DoubleQuotedString(text)
            | Value::EscapedStringLiteral(text)
            | Value::NationalStringLiteral(text) => Some(Lit::Str(text.clone())),
            Value::Boolean(value) => Some(Lit::Bool(*value)),
            Value::Null => Some(Lit::Null),
            _ => None,
        },
        Expr::UnaryOp {
            op: UnaryOperator::Minus,
            expr,
        } => match literal(expr)? {
            Lit::Num(value) => Some(Lit::Num(-value)),
            _ => None,
        },
        Expr::UnaryOp {
            op: UnaryOperator::Plus,
            expr,
        } => literal(expr),
        Expr::BinaryOp { left, op, right } => {
            let (a, b) = (literal(left)?, literal(right)?);
            if let (BinaryOperator::StringConcat, Lit::Str(a), Lit::Str(b)) = (op, &a, &b) {
                return Some(Lit::Str(format!("{a}{b}")));
            }
            let (Lit::Num(a), Lit::Num(b)) = (a, b) else {
                return None;
            };
            let value = match op {
                BinaryOperator::Plus => a + b,
                BinaryOperator::Minus => a - b,
                BinaryOperator::Multiply => a * b,
                BinaryOperator::Divide if b != 0.0 => a / b,
                _ => return None,
            };
            Some(Lit::Num(value))
        }
        Expr::Cast { expr, .. } => literal(expr),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn warns(sql: &str) -> bool {
        !where_warnings(sql, SqlDialect::Postgres).is_empty()
    }

    #[test]
    fn always_true_where_is_reported() {
        for sql in [
            "UPDATE users SET a = 1 WHERE 1=1",
            "UPDATE users SET a = 1 WHERE 1 = 1",
            "DELETE FROM users WHERE true",
            "DELETE FROM users WHERE TRUE AND 2 > 1",
            "DELETE FROM users WHERE id = id",
            "DELETE FROM users WHERE users.id = users.id",
            "DELETE FROM users WHERE (id) = id",
            "DELETE FROM users WHERE x OR NOT x",
            "DELETE FROM users WHERE NOT x OR x",
            "DELETE FROM users WHERE 'a'='a'",
            "DELETE FROM users WHERE col IS NULL OR col IS NOT NULL",
            "DELETE FROM users WHERE id > 5 OR 1 = 1",
            "DELETE FROM users WHERE 1 + 1 = 2",
            "DELETE FROM users WHERE -1 < 0",
            "DELETE FROM users WHERE '1'::int = 1",
            "DELETE FROM users WHERE NOT (1 = 0)",
            "DELETE FROM users WHERE 1 <> 2",
            "DELETE FROM users WHERE 'a' || 'b' = 'ab'",
            "DELETE FROM users WHERE id >= id",
            "DELETE FROM users WHERE NULL IS NULL",
            "DELETE FROM users WHERE 1 IS NOT NULL",
        ] {
            assert!(warns(sql), "{sql}");
        }
        assert!(!where_warnings("DELETE FROM t WHERE 1", SqlDialect::Mysql).is_empty());
    }

    #[test]
    fn real_filters_are_not_reported() {
        for sql in [
            "UPDATE users SET a = 1 WHERE id = 1",
            "DELETE FROM users WHERE id = 5 AND 1 = 1",
            "DELETE FROM users WHERE 1 = 0",
            "DELETE FROM users WHERE false",
            "DELETE FROM users WHERE id = other_id",
            "DELETE FROM users WHERE random() = random()",
            "DELETE FROM users WHERE (SELECT 1) = (SELECT 1)",
            "DELETE FROM users WHERE x OR NOT y",
            "DELETE FROM users WHERE col IS NULL OR other IS NOT NULL",
            "DELETE FROM users WHERE NULL = NULL",
            "DELETE FROM users WHERE 'a' = 'b'",
            "DELETE FROM users WHERE id <> id",
            "DELETE FROM users WHERE id IN (SELECT id FROM banned)",
            "DELETE FROM users WHERE 1 / 0 = 1",
            "INSERT INTO users VALUES (1)",
            "SELECT * FROM users WHERE 1 = 1",
        ] {
            assert!(!warns(sql), "{sql}");
        }
        assert!(where_warnings("DELETE FROM t LIMIT 10", SqlDialect::Mysql).is_empty());
    }

    #[test]
    fn missing_where_is_reported_with_the_table() {
        assert_eq!(
            where_warnings("DELETE FROM public.users", SqlDialect::Postgres),
            vec!["DELETE on public.users has no WHERE: it removes every row."]
        );
        assert_eq!(
            where_warnings("UPDATE users SET a = 1", SqlDialect::Postgres),
            vec!["UPDATE on users has no WHERE: it changes every row."]
        );
        assert_eq!(
            where_warnings("UPDATE users SET a = 1 WHERE 1=1", SqlDialect::Postgres),
            vec!["WHERE 1 = 1 is always true: this UPDATE changes every row of users."]
        );
    }

    #[test]
    fn every_statement_and_nested_dml_is_checked() {
        let warnings = where_warnings(
            "SELECT 1; DELETE FROM a; UPDATE b SET x = 1 WHERE id = 2; UPDATE c SET x = 1 WHERE true",
            SqlDialect::Postgres,
        );
        assert_eq!(warnings.len(), 2, "{warnings:?}");
        assert!(warnings[0].contains(" a "), "{warnings:?}");
        assert!(warnings[1].contains("of c."), "{warnings:?}");
        assert!(warns(
            "WITH gone AS (DELETE FROM t RETURNING id) SELECT count(*) FROM gone"
        ));
        assert!(where_warnings("DELETE FROM", SqlDialect::Postgres).is_empty());
    }
}
