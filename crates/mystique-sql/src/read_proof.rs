//! Fail-closed ReadProof: a statement is a read only when we can show it.
//!
//! [`crate::classify`] answers "how risky is this?" with a denylist of known
//! side effects, so an unknown function counts as a read. ReadProof answers
//! "is this provably read-only?" and an unknown function is not. Read mode
//! runs it before execution on top of the engine's read-only transaction,
//! and MCP / agent read tools call it through
//! `EngineQueryPolicy::read_proof`.
//!
//! A statement passes when it is a `SELECT` / `WITH … SELECT` / `VALUES` /
//! `TABLE`, a `SHOW`, `DESCRIBE`, a plain `EXPLAIN`, an `EXPLAIN ANALYZE` of a
//! provable read, a read-only `PRAGMA`, or a harmless session `SET` (the same
//! list Read mode already allowed), and every function it calls is on the
//! dialect's allowlist in [`allowlist`]. Row locks, `SELECT … INTO`, DML in a
//! CTE or subquery, MySQL versioned comments and parse failures fail.
//!
//! Sleep, wait and lock functions do not write, but they hold a connection
//! or a lock that outlives the statement, so they are not reads here.

mod allowlist;

use std::fmt;
use std::ops::ControlFlow;

use sqlparser::ast::{
    Expr, Ident, ObjectName, ObjectNamePart, Query, SetExpr, Spanned, Statement, TableFactor,
    Visit, Visitor,
};
use sqlparser::tokenizer::Span;

pub use allowlist::DenyKind;

use super::risk::{
    ast_explain_executes, basename, byte_at, has_versioned_comment, looks_like_do_block,
    looks_like_pragma_read, looks_like_table_read, parse_result, pragma_risk, StatementRisk,
};
use super::{is_read_safe_set, mask_placeholders, split_statement_spans, SqlDialect};

/// What kept a statement from being proven read-only.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReadProofReason {
    /// A function with a known side effect.
    Function { name: String, kind: DenyKind },
    /// A function not on the dialect's allowlist: user-defined, from an
    /// extension, or a built-in nobody has listed yet.
    UnknownFunction { name: String },
    /// `SELECT … FOR UPDATE` / `FOR SHARE` (the lock clause as written).
    RowLock(String),
    /// `SELECT … INTO target`.
    SelectInto(String),
    /// `INSERT` / `UPDATE` / `DELETE` inside a query, such as a data-modifying CTE.
    NestedWrite(String),
    /// The statement itself is not a read (`DELETE`, `ATTACH`, `CALL`, …).
    NotARead(String),
    /// MySQL `/*! … */` runs whatever is inside it.
    VersionedComment,
    /// The parser could not read the statement.
    Unparsed,
}

impl fmt::Display for ReadProofReason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Function { name, kind } => write!(f, "`{name}` {}", kind.phrase()),
            Self::UnknownFunction { name } => {
                write!(f, "`{name}` is not a known read-only function")
            }
            Self::RowLock(clause) => write!(f, "`{clause}` locks rows"),
            Self::SelectInto(target) => write!(f, "`SELECT … INTO {target}` writes"),
            Self::NestedWrite(kind) => write!(f, "the `{kind}` inside this query writes"),
            Self::NotARead(kind) => write!(f, "`{kind}` is not a read"),
            Self::VersionedComment => {
                f.write_str("a MySQL `/*! … */` comment can run any statement")
            }
            Self::Unparsed => f.write_str("this statement could not be parsed"),
        }
    }
}

/// A batch that is not provably read-only.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReadProofError {
    /// 0-based statement in the batch.
    pub statement_index: usize,
    /// Byte offset of the fault in the text that was checked: the function
    /// name or construct when the parser located it, else the statement start.
    pub offset: usize,
    pub reason: ReadProofReason,
}

impl fmt::Display for ReadProofError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let reason = self.reason.to_string();
        let mut chars = reason.chars();
        if let Some(first) = chars.next() {
            write!(f, "{}{}", first.to_uppercase(), chars.as_str())?;
        }
        f.write_str("; switch to Write mode to run it.")
    }
}

impl std::error::Error for ReadProofError {}

/// Ok only when every statement in `sql` is provably read-only.
///
/// Placeholders are masked first, so `:id` and `$1` do not fail the parse.
/// An empty batch runs nothing and is Ok.
pub fn read_proof(dialect: SqlDialect, sql: &str) -> Result<(), ReadProofError> {
    let masked = mask_placeholders(sql, dialect);
    for (index, span) in split_statement_spans(&masked, dialect)
        .into_iter()
        .enumerate()
    {
        read_proof_statement(dialect, &span.text).map_err(|err| ReadProofError {
            statement_index: index,
            offset: span.start + err.offset,
            reason: err.reason,
        })?;
    }
    Ok(())
}

/// [`read_proof`] for one already-split, already-masked statement.
/// `statement_index` is 0 and `offset` is relative to `sql`.
pub fn read_proof_statement(dialect: SqlDialect, sql: &str) -> Result<(), ReadProofError> {
    let fail = |reason, offset| ReadProofError {
        statement_index: 0,
        offset,
        reason,
    };
    if has_versioned_comment(sql) {
        let at = sql.find("/*").unwrap_or(0);
        return Err(fail(ReadProofReason::VersionedComment, at));
    }
    if looks_like_do_block(sql) {
        return Err(fail(ReadProofReason::NotARead("DO".into()), 0));
    }
    let parsed = match parse_result(sql, dialect) {
        Ok(parsed) => parsed,
        // The same text fallbacks the classifier and planner accept.
        Err(_) if looks_like_table_read(sql) || looks_like_pragma_read(sql) => return Ok(()),
        Err(_) => return Err(fail(ReadProofReason::Unparsed, 0)),
    };
    for stmt in &parsed {
        if let Err(fault) = prove_statement(stmt, dialect, sql) {
            return Err(fail(fault.reason, offset_of(sql, fault.span)));
        }
    }
    Ok(())
}

/// A fault and where the parser saw it.
struct Fault {
    reason: ReadProofReason,
    span: Span,
}

impl Fault {
    fn new(reason: ReadProofReason, span: Span) -> Self {
        Self { reason, span }
    }
}

fn offset_of(sql: &str, span: Span) -> usize {
    let start = span.start;
    if start.line == 0 {
        return 0;
    }
    let line = u32::try_from(start.line).unwrap_or(u32::MAX);
    let column = u32::try_from(start.column).unwrap_or(u32::MAX);
    byte_at(sql, line, column)
}

fn prove_statement(stmt: &Statement, dialect: SqlDialect, raw: &str) -> Result<(), Fault> {
    match stmt {
        Statement::Query(_) => walk(stmt, dialect, Walk::Full),
        Statement::Explain { statement, .. } => {
            if ast_explain_executes(stmt) {
                return prove_statement(statement, dialect, raw).map_err(|fault| {
                    match fault.reason {
                        ReadProofReason::NotARead(kind) => Fault::new(
                            ReadProofReason::NotARead(format!("EXPLAIN ANALYZE {kind}")),
                            fault.span,
                        ),
                        _ => fault,
                    }
                });
            }
            match dialect {
                // Postgres plans without calling volatile functions; SQLite
                // only compiles. Nothing in the statement runs.
                SqlDialect::Postgres | SqlDialect::Sqlite => Ok(()),
                // MySQL evaluates constant subqueries while it optimizes, so
                // a plain EXPLAIN can still call what they contain.
                SqlDialect::Mysql | SqlDialect::Generic => {
                    walk(statement, dialect, Walk::FunctionsOnly)
                }
            }
        }
        Statement::ExplainTable { .. }
        | Statement::ShowVariable { .. }
        | Statement::ShowStatus { .. }
        | Statement::ShowVariables { .. }
        | Statement::ShowCreate { .. }
        | Statement::ShowColumns { .. }
        | Statement::ShowTables { .. }
        | Statement::ShowFunctions { .. }
        | Statement::ShowDatabases { .. }
        | Statement::ShowSchemas { .. }
        | Statement::ShowViews { .. }
        | Statement::ShowCollation { .. }
        | Statement::ShowObjects(_) => walk(stmt, dialect, Walk::FunctionsOnly),
        Statement::Pragma {
            name, value, is_eq, ..
        } => {
            if pragma_risk(&basename(name), value.is_some(), *is_eq) == StatementRisk::Read {
                Ok(())
            } else {
                Err(Fault::new(
                    ReadProofReason::NotARead("PRAGMA".into()),
                    Span::empty(),
                ))
            }
        }
        Statement::Set(_) if is_read_safe_set(raw, dialect) => walk(stmt, dialect, Walk::Full),
        _ => Err(Fault::new(
            ReadProofReason::NotARead(statement_keyword(stmt)),
            Span::empty(),
        )),
    }
}

/// `DELETE`, `CREATE TABLE`, `ATTACH`: the start of the statement as sqlparser prints it.
fn statement_keyword(stmt: &Statement) -> String {
    let text = stmt.to_string();
    let mut words = text.split_whitespace();
    let first = words.next().unwrap_or("statement").to_ascii_uppercase();
    match first.as_str() {
        "CREATE" | "ALTER" | "DROP" => match words.next() {
            Some(second) => format!("{first} {}", second.to_ascii_uppercase()),
            None => first,
        },
        _ => first,
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Walk {
    /// Every rule: functions, row locks, `SELECT INTO`, nested DML.
    Full,
    /// Only what gets called. For statements that are planned, not run.
    FunctionsOnly,
}

fn walk(stmt: &Statement, dialect: SqlDialect, mode: Walk) -> Result<(), Fault> {
    let mut scan = ProofScan { dialect, mode };
    match stmt.visit(&mut scan) {
        ControlFlow::Continue(()) => Ok(()),
        ControlFlow::Break(fault) => Err(fault),
    }
}

struct ProofScan {
    dialect: SqlDialect,
    mode: Walk,
}

impl Visitor for ProofScan {
    type Break = Fault;

    fn pre_visit_query(&mut self, query: &Query) -> ControlFlow<Fault> {
        if self.mode == Walk::FunctionsOnly {
            return ControlFlow::Continue(());
        }
        if let Some(lock) = query.locks.first() {
            return ControlFlow::Break(Fault::new(
                ReadProofReason::RowLock(format!("FOR {}", lock.lock_type)),
                query.span(),
            ));
        }
        match query_shape(&query.body) {
            Ok(()) => ControlFlow::Continue(()),
            Err(fault) => ControlFlow::Break(fault),
        }
    }

    fn pre_visit_table_factor(&mut self, factor: &TableFactor) -> ControlFlow<Fault> {
        // `FROM dblink(…)` is a table with arguments, not an expression.
        match factor {
            TableFactor::Table {
                name,
                args: Some(_),
                ..
            }
            | TableFactor::Function { name, .. } => self.check(name),
            _ => ControlFlow::Continue(()),
        }
    }

    fn pre_visit_expr(&mut self, expr: &Expr) -> ControlFlow<Fault> {
        match expr {
            Expr::Function(function) => self.check(&function.name),
            _ => ControlFlow::Continue(()),
        }
    }
}

impl ProofScan {
    fn check(&self, name: &ObjectName) -> ControlFlow<Fault> {
        match check_function(self.dialect, name) {
            Ok(()) => ControlFlow::Continue(()),
            Err(fault) => ControlFlow::Break(fault),
        }
    }
}

/// The query body itself. Nested queries are visited on their own.
fn query_shape(body: &SetExpr) -> Result<(), Fault> {
    match body {
        SetExpr::Select(select) => match &select.into {
            Some(into) => Err(Fault::new(
                ReadProofReason::SelectInto(into.name.to_string()),
                into.name.span(),
            )),
            None => Ok(()),
        },
        SetExpr::SetOperation { left, right, .. } => {
            query_shape(left)?;
            query_shape(right)
        }
        SetExpr::Insert(stmt) | SetExpr::Update(stmt) | SetExpr::Delete(stmt) => Err(Fault::new(
            ReadProofReason::NestedWrite(statement_keyword(stmt)),
            stmt.span(),
        )),
        _ => Ok(()),
    }
}

/// Deny list first, then the schema, then the allowlist. Unknown fails.
fn check_function(dialect: SqlDialect, name: &ObjectName) -> Result<(), Fault> {
    let shown = name.to_string();
    let idents: Option<Vec<&Ident>> = name
        .0
        .iter()
        .map(|part| match part {
            ObjectNamePart::Identifier(ident) => Some(ident),
            _ => None,
        })
        .collect();
    let Some((last, qualifier)) = idents.as_deref().and_then(<[&Ident]>::split_last) else {
        return Err(Fault::new(
            ReadProofReason::UnknownFunction { name: shown },
            name.span(),
        ));
    };
    let base = last.value.to_ascii_lowercase();
    let span = last.span;
    if let Some(kind) = allowlist::denied(&base) {
        return Err(Fault::new(
            ReadProofReason::Function { name: shown, kind },
            span,
        ));
    }
    let unknown = || {
        Fault::new(
            ReadProofReason::UnknownFunction {
                name: shown.clone(),
            },
            span,
        )
    };
    if !qualifier.is_empty() {
        let schema = qualifier
            .iter()
            .map(|ident| ident.value.to_ascii_lowercase())
            .collect::<Vec<_>>()
            .join(".");
        if !allowlist::builtin_schemas(dialect).contains(&schema.as_str()) {
            return Err(unknown());
        }
    }
    // Postgres keeps the case of a quoted name: `"LOWER"` is not `lower`.
    if dialect == SqlDialect::Postgres && last.quote_style.is_some() && last.value != base {
        return Err(unknown());
    }
    // SQLite table-valued pragmas cannot set anything; the read list decides.
    if dialect == SqlDialect::Sqlite {
        if let Some(pragma) = base.strip_prefix("pragma_") {
            return if pragma_risk(pragma, false, false) == StatementRisk::Read {
                Ok(())
            } else {
                Err(unknown())
            };
        }
    }
    if allowlist::allowed(dialect, &base) {
        Ok(())
    } else {
        Err(unknown())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn deny(dialect: SqlDialect, sql: &str) -> ReadProofError {
        read_proof(dialect, sql).expect_err(sql)
    }

    #[test]
    fn message_names_the_function_and_the_way_out() {
        let err = deny(SqlDialect::Postgres, "SELECT nextval('s')");
        assert_eq!(
            err.to_string(),
            "`nextval` may write; switch to Write mode to run it."
        );
        assert_eq!(err.offset, 7);
        assert_eq!(err.statement_index, 0);
    }

    #[test]
    fn offset_and_index_point_into_the_batch() {
        let sql = "SELECT 1;\nSELECT a,\n  my_func(a) FROM t";
        let err = deny(SqlDialect::Postgres, sql);
        assert_eq!(err.statement_index, 1);
        assert_eq!(&sql[err.offset..err.offset + 7], "my_func");
        assert_eq!(
            err.reason,
            ReadProofReason::UnknownFunction {
                name: "my_func".into()
            }
        );
    }

    #[test]
    fn placeholders_do_not_fail_the_parse() {
        assert!(read_proof(SqlDialect::Postgres, "SELECT * FROM t WHERE id = :id").is_ok());
        assert!(read_proof(SqlDialect::Postgres, "SELECT * FROM t WHERE id = $1").is_ok());
        assert!(read_proof(SqlDialect::Mysql, "SELECT * FROM t WHERE id = ?").is_ok());
    }

    const DIALECTS: [SqlDialect; 4] = [
        SqlDialect::Postgres,
        SqlDialect::Mysql,
        SqlDialect::Sqlite,
        SqlDialect::Generic,
    ];

    /// Every allowlist entry, called in a SELECT, passes for its dialect.
    /// A name the parser reads as syntax (`current_date`, `if`) calls nothing.
    #[test]
    fn every_allowed_function_is_a_read() {
        let mut failures = Vec::new();
        for dialect in DIALECTS {
            let (tables, _) = allowlist::allowlist(dialect);
            for name in tables.iter().flat_map(|table| table.iter()) {
                let sql = format!("SELECT {name}(1)");
                match read_proof(dialect, &sql) {
                    Ok(()) => {}
                    Err(err) if err.reason == ReadProofReason::Unparsed => {}
                    Err(err) => failures.push(format!("{dialect:?} {sql}: {err}")),
                }
            }
        }
        assert!(failures.is_empty(), "\n{}", failures.join("\n"));
    }

    /// Every deny entry fails with its own reason, in every dialect, also
    /// as a table function and schema-qualified.
    #[test]
    fn every_denied_function_names_its_kind() {
        let mut failures = Vec::new();
        let prefixed: Vec<(String, DenyKind)> = allowlist::DENY_PREFIXES
            .iter()
            .map(|(prefix, kind)| (format!("{prefix}x"), *kind))
            .collect();
        let entries = allowlist::DENY
            .iter()
            .map(|(name, kind)| ((*name).to_string(), *kind))
            .chain(prefixed);
        for (name, kind) in entries {
            for dialect in DIALECTS {
                for sql in [
                    format!("SELECT {name}(1)"),
                    format!("SELECT * FROM t WHERE a = (SELECT {name}(1))"),
                    format!("SELECT * FROM {name}(1) AS f"),
                ] {
                    match read_proof(dialect, &sql) {
                        Err(ReadProofError {
                            reason: ReadProofReason::Function { kind: got, .. },
                            ..
                        }) if got == kind => {}
                        // A deny name the parser cannot read as a call fails anyway.
                        Err(ReadProofError {
                            reason: ReadProofReason::Unparsed,
                            ..
                        }) => {}
                        other => failures.push(format!("{dialect:?} {sql}: {other:?}")),
                    }
                }
            }
        }
        assert!(failures.is_empty(), "\n{}", failures.join("\n"));
    }

    #[test]
    fn unknown_keyword_wording() {
        let err = deny(SqlDialect::Postgres, "DELETE FROM t");
        assert_eq!(err.reason, ReadProofReason::NotARead("DELETE".into()));
        let err = deny(SqlDialect::Postgres, "EXPLAIN ANALYZE DELETE FROM t");
        assert_eq!(
            err.reason,
            ReadProofReason::NotARead("EXPLAIN ANALYZE DELETE".into())
        );
        let err = deny(SqlDialect::Postgres, "CREATE TABLE t (a int)");
        assert_eq!(err.reason, ReadProofReason::NotARead("CREATE TABLE".into()));
        let err = deny(SqlDialect::Postgres, "SELECT * FORM t");
        assert_eq!(
            err.to_string(),
            "This statement could not be parsed; switch to Write mode to run it."
        );
    }
}
