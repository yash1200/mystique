//! AST write-risk for a SQL statement. Parse failure is Write (fail closed).

use std::ops::ControlFlow;

use sqlparser::ast::{
    AlterTableOperation, ContextModifier, Expr, MergeAction, ObjectName, Query, Set, SetExpr,
    Statement, TableFactor, UtilityOption, Visit, Visitor,
};
use sqlparser::parser::{Parser, ParserError};

use super::{split_statements, SqlDialect};

/// Statement-level risk. Session (SET / BEGIN) is not a data write.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum StatementRisk {
    Read = 0,
    Session = 1,
    Write = 2,
    Destructive = 3,
}

impl StatementRisk {
    pub fn is_read_like(self) -> bool {
        matches!(self, Self::Read | Self::Session)
    }
}

/// Worst risk across the batch. Empty SQL is Read.
pub fn classify(sql: &str, dialect: SqlDialect) -> StatementRisk {
    let stmts = split_statements(sql, dialect);
    if stmts.is_empty() {
        return StatementRisk::Read;
    }
    let mut worst = StatementRisk::Read;
    for stmt in &stmts {
        let risk = classify_one(stmt, dialect);
        if risk > worst {
            worst = risk;
        }
    }
    worst
}

/// INSERT / UPDATE / MERGE / REPLACE only. All statements in the batch must match.
pub fn is_safe_dml(sql: &str, dialect: SqlDialect) -> bool {
    let stmts = split_statements(sql, dialect);
    if stmts.is_empty() {
        return false;
    }
    stmts.iter().all(|s| one_is_safe_dml(s, dialect))
}

/// Statement text to wrap in `EXPLAIN`. A leading `EXPLAIN` is peeled off
/// so the button does not plan `EXPLAIN EXPLAIN …`.
pub fn explain_subject(sql: &str, dialect: SqlDialect) -> Result<String, String> {
    let parsed = parse_result(sql, dialect).map_err(|err| err.to_string())?;
    let Some(stmt) = parsed.first() else {
        return Err("Query is empty.".into());
    };
    if parsed.len() != 1 {
        return Err("Explain one statement.".into());
    }
    match stmt {
        Statement::Explain { statement, .. } => Ok(trim_statement(&statement.to_string())),
        _ => Ok(trim_statement(sql)),
    }
}

fn trim_statement(sql: &str) -> String {
    sql.trim().trim_end_matches(';').trim().to_string()
}

/// True when a statement will execute its target, not only plan it.
///
/// `EXPLAIN ANALYZE` and `EXPLAIN (ANALYZE …)` run the inner statement.
/// Plain `EXPLAIN` and SQLite `EXPLAIN QUERY PLAN` do not. Parse failure
/// still returns true when the text is an `EXPLAIN` that mentions `ANALYZE`,
/// so a parenthesised form we cannot parse is not run unconstrained.
pub fn explain_executes(sql: &str, dialect: SqlDialect) -> bool {
    let Some(parsed) = parse(sql, dialect) else {
        return looks_like_explain_analyze(sql);
    };
    parsed.iter().any(ast_explain_executes)
}

fn looks_like_explain_analyze(sql: &str) -> bool {
    let lower = sql.trim_start().to_ascii_lowercase();
    lower.starts_with("explain") && (lower.contains("analyze") || lower.contains("analyse"))
}

fn ast_explain_executes(stmt: &Statement) -> bool {
    let Statement::Explain {
        analyze, options, ..
    } = stmt
    else {
        return false;
    };
    *analyze || options_may_execute(options)
}

/// Postgres accepts `ANALYZE` and `ANALYSE`. Any option we do not know
/// counts as executing, so a new spelling cannot hide a DELETE.
fn options_may_execute(options: &Option<Vec<UtilityOption>>) -> bool {
    let Some(options) = options else {
        return false;
    };
    options.iter().any(|opt| {
        let name = opt.name.value.to_ascii_lowercase();
        match name.as_str() {
            "analyze" | "analyse" => !opt.arg.as_ref().is_some_and(is_false_literal),
            "verbose" | "costs" | "settings" | "generic_plan" | "buffers" | "wal" | "timing"
            | "summary" | "memory" | "serialize" | "format" => false,
            _ => true,
        }
    })
}

fn is_false_literal(expr: &Expr) -> bool {
    let text = expr.to_string().to_ascii_lowercase();
    matches!(text.as_str(), "false" | "off" | "0" | "no")
}

/// Every destructive target in the batch, in source order, joined for type-to-confirm.
///
/// `DROP TABLE important, trivial` asks for `important, trivial`, not only the last name.
pub fn destructive_object_name(sql: &str, dialect: SqlDialect) -> Option<String> {
    let mut names = Vec::new();
    for stmt in split_statements(sql, dialect) {
        let Some(parsed) = parse(&stmt, dialect) else {
            continue;
        };
        for statement in &parsed {
            names.extend(ast_destructive_names(statement));
        }
    }
    if names.is_empty() {
        None
    } else {
        Some(names.join(", "))
    }
}

fn parse(sql: &str, dialect: SqlDialect) -> Option<Vec<Statement>> {
    parse_result(sql, dialect).ok()
}

fn parse_result(sql: &str, dialect: SqlDialect) -> Result<Vec<Statement>, ParserError> {
    let boxed = dialect.parser_dialect();
    match Parser::parse_sql(&*boxed, sql) {
        Ok(statements) => Ok(statements),
        Err(primary) => {
            if matches!(dialect, SqlDialect::Generic) {
                return Err(primary);
            }
            match Parser::parse_sql(&sqlparser::dialect::GenericDialect {}, sql) {
                Ok(statements) => Ok(statements),
                Err(_) => Err(primary),
            }
        }
    }
}

/// A `SET` that Read mode may run: session scope, a harmless setting, and a
/// value that calls nothing. Role, read-only, timeout and lexer settings
/// (`sql_mode`, `standard_conforming_strings`) are not on the list.
pub fn is_read_safe_set(sql: &str, dialect: SqlDialect) -> bool {
    if set_text_is_privileged(sql) {
        return false;
    }
    let Some(parsed) = parse(sql, dialect) else {
        return false;
    };
    !parsed.is_empty()
        && parsed.iter().all(|stmt| match stmt {
            Statement::Set(set) => set_is_read_safe(set) && deep_risk(stmt) == StatementRisk::Read,
            _ => false,
        })
}

fn set_is_read_safe(set: &Set) -> bool {
    match set {
        Set::SetTimeZone { .. } | Set::SetNames { .. } | Set::SetNamesDefault {} => true,
        Set::SingleAssignment {
            scope, variable, ..
        } => scope_is_session(scope) && variable_is_read_safe(variable),
        Set::MultipleAssignments { assignments } => assignments.iter().all(|assignment| {
            scope_is_session(&assignment.scope) && variable_is_read_safe(&assignment.name)
        }),
        _ => false,
    }
}

fn set_touches_autocommit(set: &Set) -> bool {
    let is_autocommit = |name: &ObjectName| {
        let text = name.to_string().to_ascii_lowercase();
        let text = text.trim_start_matches('@');
        let text = text
            .strip_prefix("session.")
            .or_else(|| text.strip_prefix("local."))
            .unwrap_or(text);
        text == "autocommit"
    };
    match set {
        Set::SingleAssignment { variable, .. } => is_autocommit(variable),
        Set::MultipleAssignments { assignments } => {
            assignments.iter().any(|assignment| is_autocommit(&assignment.name))
        }
        _ => false,
    }
}

fn scope_is_session(scope: &Option<ContextModifier>) -> bool {
    !matches!(scope, Some(ContextModifier::Global))
}

fn variable_is_read_safe(name: &ObjectName) -> bool {
    let text = name.to_string().to_ascii_lowercase();
    let name = match text.strip_prefix("@@") {
        Some(rest) => rest
            .strip_prefix("session.")
            .or_else(|| rest.strip_prefix("local."))
            .unwrap_or(rest),
        // MySQL user variable.
        None if text.starts_with('@') => return !text[1..].contains('@'),
        None => text.as_str(),
    };
    matches!(
        name,
        "search_path"
            | "timezone"
            | "time_zone"
            | "datestyle"
            | "intervalstyle"
            | "client_encoding"
            | "application_name"
            | "extra_float_digits"
            | "bytea_output"
            | "client_min_messages"
            | "lc_messages"
            | "lc_monetary"
            | "lc_numeric"
            | "lc_time"
            | "lc_time_names"
            | "default_text_search_config"
            | "character_set_client"
            | "character_set_connection"
            | "character_set_results"
            | "collation_connection"
    )
}

/// A statement the planner still accepts when sqlparser cannot read it.
pub(crate) fn skip_syntax_check(sql: &str) -> bool {
    has_versioned_comment(sql)
        || looks_like_do_block(sql)
        || looks_like_table_read(sql)
        || looks_like_pragma_read(sql)
}

/// Parse failure that should not be sent to the engine.
///
/// `offset` is a byte offset in `sql`. Known fallbacks (`TABLE`, `PRAGMA`,
/// versioned comments, `DO` blocks) return none.
pub(crate) fn syntax_fault_in(sql: &str, dialect: SqlDialect) -> Option<(String, u32)> {
    if skip_syntax_check(sql) {
        return None;
    }
    let Err(err) = parse_result(sql, dialect) else {
        return None;
    };
    let raw = err.to_string();
    let (line, column) = location_from_message(&raw).unwrap_or((1, 1));
    let offset = byte_at(sql, line.max(1), column.max(1));
    Some((
        strip_location(&raw),
        u32::try_from(offset).unwrap_or(u32::MAX),
    ))
}

fn location_from_message(message: &str) -> Option<(u32, u32)> {
    let marker = "Line:";
    let line_at = message.rfind(marker)?;
    let rest = message[line_at + marker.len()..].trim_start();
    let line = rest
        .split(|ch: char| !ch.is_ascii_digit())
        .next()
        .unwrap_or("")
        .parse()
        .ok()?;
    let col_marker = "Column:";
    let col_at = rest.find(col_marker)?;
    let col_rest = rest[col_at + col_marker.len()..].trim_start();
    let column = col_rest
        .split(|ch: char| !ch.is_ascii_digit())
        .next()
        .unwrap_or("")
        .parse()
        .ok()?;
    Some((line, column))
}

fn strip_location(message: &str) -> String {
    let Some(at) = message.rfind(" at Line:") else {
        return message.trim().to_string();
    };
    message[..at].trim().to_string()
}

fn byte_at(text: &str, line: u32, column: u32) -> usize {
    let mut cur_line = 1u32;
    let mut cur_col = 1u32;
    for (index, ch) in text.char_indices() {
        if cur_line == line && cur_col == column {
            return index;
        }
        if ch == '\n' {
            cur_line += 1;
            cur_col = 1;
        } else {
            cur_col += 1;
        }
    }
    text.len()
}

fn classify_one(sql: &str, dialect: SqlDialect) -> StatementRisk {
    route_text(sql, dialect).risk
}

/// How one statement should be executed. Built from the same parse as [`classify_one`].
#[derive(Debug, Clone)]
pub(crate) struct TextRoute {
    pub risk: StatementRisk,
    pub returns_rows: bool,
    pub returns_count: bool,
    /// Postgres `DECLARE CURSOR` is only safe for a plain read query.
    pub cursor: bool,
    /// MySQL `CALL` can emit more than one result set.
    pub multi_set: bool,
    pub txn_boundary: bool,
    pub explain_executes: bool,
    pub destructive_names: Vec<String>,
}

impl TextRoute {
    fn opaque(risk: StatementRisk) -> Self {
        Self {
            risk,
            returns_rows: false,
            returns_count: false,
            cursor: false,
            multi_set: false,
            txn_boundary: false,
            explain_executes: false,
            destructive_names: Vec::new(),
        }
    }

    fn read_grid(cursor: bool) -> Self {
        Self {
            returns_rows: true,
            cursor,
            ..Self::opaque(StatementRisk::Read)
        }
    }
}

/// One parse. Callers that need both the risk and the execution route use this
/// instead of classifying and parsing again.
pub(crate) fn route_text(sql: &str, dialect: SqlDialect) -> TextRoute {
    // MySQL executes the body of `/*! */`, `/*M! */`, and `/*T! */`. The parser
    // drops the comment, so the AST looks like a read.
    if has_versioned_comment(sql) {
        let mut route = TextRoute::opaque(StatementRisk::Write);
        // The hidden body may be a SELECT. Read mode still rejects the risk.
        // The row path can show that body; a cursor would quote the comment.
        route.returns_rows = true;
        return route;
    }
    // `DO $$ … $$` often fails to parse. It can hide a DROP. Fail closed.
    if looks_like_do_block(sql) {
        return TextRoute::opaque(StatementRisk::Destructive);
    }
    let Some(parsed) = parse(sql, dialect) else {
        // Postgres `TABLE t` is `SELECT * FROM t`. sqlparser 0.58 does not parse it.
        if looks_like_table_read(sql) {
            return TextRoute::read_grid(true);
        }
        if looks_like_pragma_read(sql) {
            return TextRoute::read_grid(false);
        }
        // Opaque write. Write mode sends it to the engine, so transaction
        // control we cannot parse must still be refused on auto-commit.
        let mut route = TextRoute::opaque(StatementRisk::Write);
        route.txn_boundary = looks_like_txn_control(sql);
        return route;
    };
    let mut iter = parsed.iter();
    let Some(first) = iter.next() else {
        return TextRoute::opaque(StatementRisk::Read);
    };
    let mut acc = route_ast(first, sql);
    for stmt in iter {
        acc.merge(route_ast(stmt, sql));
    }
    acc
}

impl TextRoute {
    fn merge(&mut self, next: TextRoute) {
        self.risk = self.risk.max(next.risk);
        self.returns_rows |= next.returns_rows;
        self.returns_count |= next.returns_count;
        self.cursor = self.cursor && next.cursor;
        self.multi_set |= next.multi_set;
        self.txn_boundary |= next.txn_boundary;
        self.explain_executes |= next.explain_executes;
        self.destructive_names.extend(next.destructive_names);
        if self.risk != StatementRisk::Read {
            self.cursor = false;
        }
    }
}

fn route_ast(stmt: &Statement, raw_sql: &str) -> TextRoute {
    let mut risk = classify_ast(stmt);
    if matches!(stmt, Statement::Set(_)) && set_text_is_privileged(raw_sql) {
        risk = risk.max(StatementRisk::Write);
    }
    let mut route = TextRoute::opaque(risk);
    route.destructive_names = ast_destructive_names(stmt);
    route.explain_executes = ast_explain_executes(stmt);
    match stmt {
        Statement::Query(_) => {
            route.returns_rows = true;
            route.cursor = risk == StatementRisk::Read;
        }
        Statement::Explain { .. }
        | Statement::ExplainTable { .. }
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
        | Statement::ShowObjects(_) => {
            route.returns_rows = true;
        }
        Statement::Insert(insert) => {
            route.returns_count = true;
            route.returns_rows = insert.returning.is_some();
        }
        Statement::Update { returning, .. } => {
            route.returns_count = true;
            route.returns_rows = returning.is_some();
        }
        Statement::Delete(delete) => {
            route.returns_count = true;
            route.returns_rows = delete.returning.is_some();
        }
        Statement::Merge { .. } => {
            route.returns_count = true;
        }
        Statement::Call(_) => {
            route.returns_rows = true;
            route.multi_set = true;
        }
        Statement::StartTransaction { statements, .. } => {
            // A procedural `BEGIN … END` block carries a body. A transaction
            // opener does not. Pinned sessions are a later ticket.
            route.txn_boundary = statements.is_empty();
        }
        Statement::Commit { .. } => {
            // A top-level `END` / `END TRANSACTION` commits on Postgres and
            // SQLite. Block ends only appear inside a parsed body.
            route.txn_boundary = true;
        }
        Statement::Rollback { savepoint, .. } => {
            route.txn_boundary = savepoint.is_none();
        }
        Statement::Set(set) if set_touches_autocommit(set) => {
            // `SET autocommit = 0` opens an implicit transaction that would
            // outlive the statement on a pooled connection.
            route.txn_boundary = true;
        }
        Statement::Pragma { is_eq, .. } => {
            route.returns_rows = !is_eq;
        }
        _ => {}
    }
    if route.risk != StatementRisk::Read {
        route.cursor = false;
    }
    route
}

/// Any `/*!`, `/*M!` or `/*T!`, even inside a string.
///
/// MySQL's lexer is not ours: `\'`, `#`, `--` without a space and `sql_mode`
/// all change what counts as quoted. Matching the bytes fails closed.
fn has_versioned_comment(sql: &str) -> bool {
    let mut rest = sql;
    while let Some(at) = rest.find("/*") {
        rest = &rest[at + 2..];
        let body = rest.trim_start();
        if body.starts_with('!')
            || body.get(..2).is_some_and(|head| {
                head.eq_ignore_ascii_case("m!") || head.eq_ignore_ascii_case("t!")
            })
        {
            return true;
        }
    }
    false
}

/// `ABORT`, `XA START`, `START TRANSACTION WITH CONSISTENT SNAPSHOT`,
/// `PREPARE TRANSACTION`, `COMMIT PREPARED`: sqlparser 0.58 reads none of them.
fn looks_like_txn_control(sql: &str) -> bool {
    let lower = skip_leading_comments(sql).to_ascii_lowercase();
    let mut words = lower
        .split(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
        .filter(|word| !word.is_empty());
    matches!(
        (words.next(), words.next()),
        (
            Some("begin" | "commit" | "end" | "rollback" | "abort" | "xa"),
            _
        ) | (Some("start" | "prepare"), Some("transaction"))
    )
}

fn skip_leading_comments(sql: &str) -> &str {
    let mut rest = sql.trim_start();
    loop {
        if let Some(after) = rest.strip_prefix("--") {
            rest = after
                .split_once('\n')
                .map_or("", |(_, tail)| tail)
                .trim_start();
        } else if let Some(after) = rest.strip_prefix("/*") {
            rest = after
                .split_once("*/")
                .map_or("", |(_, tail)| tail)
                .trim_start();
        } else {
            return rest;
        }
    }
}

fn looks_like_table_read(sql: &str) -> bool {
    let trimmed = sql.trim();
    let lower = trimmed.to_ascii_lowercase();
    let Some(rest) = lower.strip_prefix("table") else {
        return false;
    };
    rest.starts_with(|c: char| c.is_whitespace())
        && !rest.contains(';')
        && !rest.contains("for update")
        && !rest.contains("for share")
}

/// Text fallback for a pragma sqlparser cannot read. Same rule as the AST path.
fn looks_like_pragma_read(sql: &str) -> bool {
    let lower = sql.trim().to_ascii_lowercase();
    let Some(rest) = lower.strip_prefix("pragma") else {
        return false;
    };
    if !rest.starts_with(|c: char| c.is_whitespace()) {
        return false;
    }
    let rest = rest.trim_start();
    let name_end = rest
        .find(|c: char| !(c.is_ascii_alphanumeric() || c == '_' || c == '.'))
        .unwrap_or(rest.len());
    let name = rest[..name_end].rsplit('.').next().unwrap_or("");
    let tail = rest[name_end..].trim();
    let has_arg = match tail {
        "" => false,
        _ if tail.starts_with('(') && tail.ends_with(')') && !tail.contains(';') => true,
        _ => return false,
    };
    pragma_risk(name, has_arg, false) == StatementRisk::Read
}

/// `PRAGMA user_version(42)` writes as surely as `= 42`. Only listed pragmas
/// read: introspection ones take a table or schema argument, settings take none.
fn pragma_risk(name: &str, has_arg: bool, is_eq: bool) -> StatementRisk {
    if is_eq {
        return StatementRisk::Write;
    }
    let introspection = matches!(
        name,
        "table_info"
            | "table_xinfo"
            | "table_list"
            | "index_info"
            | "index_xinfo"
            | "index_list"
            | "foreign_key_list"
            | "foreign_key_check"
            | "integrity_check"
            | "quick_check"
            | "collation_list"
            | "database_list"
            | "function_list"
            | "module_list"
            | "pragma_list"
            | "compile_options"
    );
    let setting = matches!(
        name,
        "application_id"
            | "auto_vacuum"
            | "automatic_index"
            | "busy_timeout"
            | "cache_size"
            | "cache_spill"
            | "cell_size_check"
            | "data_version"
            | "defer_foreign_keys"
            | "encoding"
            | "foreign_keys"
            | "freelist_count"
            | "journal_mode"
            | "journal_size_limit"
            | "legacy_alter_table"
            | "locking_mode"
            | "max_page_count"
            | "mmap_size"
            | "page_count"
            | "page_size"
            | "query_only"
            | "recursive_triggers"
            | "schema_version"
            | "secure_delete"
            | "synchronous"
            | "temp_store"
            | "trusted_schema"
            | "user_version"
            | "wal_autocheckpoint"
            | "writable_schema"
    );
    if introspection || (setting && !has_arg) {
        StatementRisk::Read
    } else {
        StatementRisk::Write
    }
}

fn looks_like_do_block(sql: &str) -> bool {
    let trimmed = sql.trim_start();
    let Some(rest) = trimmed.get(2..) else {
        return false;
    };
    trimmed[..2].eq_ignore_ascii_case("do")
        && rest
            .chars()
            .next()
            .is_some_and(|c| c.is_whitespace() || c == '$' || c == '\'')
}

fn set_text_is_privileged(sql: &str) -> bool {
    let lower = sql.trim_start().to_ascii_lowercase();
    lower.starts_with("set persist")
        || lower.starts_with("set global")
        || lower.starts_with("set @@global")
        || lower.starts_with("set @@persist")
        || lower.starts_with("set @@session.transaction_read_only")
}

fn one_is_safe_dml(sql: &str, dialect: SqlDialect) -> bool {
    let Some(parsed) = parse(sql, dialect) else {
        return false;
    };
    !parsed.is_empty() && parsed.iter().all(ast_is_safe_dml)
}

fn classify_ast(stmt: &Statement) -> StatementRisk {
    // A plain EXPLAIN plans its statement and calls nothing in it.
    if let Statement::Explain { statement, .. } = stmt {
        return if ast_explain_executes(stmt) {
            classify_ast(statement)
        } else {
            StatementRisk::Read
        };
    }
    statement_risk(stmt).max(deep_risk(stmt))
}

/// Risk of the statement kind. Functions and nested queries are [`deep_risk`].
fn statement_risk(stmt: &Statement) -> StatementRisk {
    match stmt {
        Statement::Query(_) => StatementRisk::Read,
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
        | Statement::ShowObjects(_) => StatementRisk::Read,

        Statement::Insert(_)
        | Statement::Update { .. }
        | Statement::Delete(_)
        | Statement::Merge { .. } => StatementRisk::Write,
        Statement::Copy { .. } => StatementRisk::Write,
        Statement::Directory { .. } => StatementRisk::Write,

        Statement::Truncate { .. }
        | Statement::Drop { .. }
        | Statement::DropFunction { .. }
        | Statement::DropProcedure { .. }
        | Statement::DropTrigger { .. }
        | Statement::DropExtension { .. }
        | Statement::DropDomain(_)
        | Statement::DropPolicy { .. }
        | Statement::DropSecret { .. }
        | Statement::DropConnector { .. } => StatementRisk::Destructive,

        Statement::Set(set) => set_risk(set),
        Statement::Use(_)
        | Statement::StartTransaction { .. }
        | Statement::Commit { .. }
        | Statement::Rollback { .. }
        | Statement::Discard { .. }
        | Statement::Deallocate { .. }
        | Statement::Prepare { .. }
        | Statement::Declare { .. }
        | Statement::Fetch { .. }
        | Statement::Close { .. }
        | Statement::Open(_)
        | Statement::AlterSession { .. } => StatementRisk::Session,

        // EXECUTE may run a prepared write — fail closed.
        Statement::Execute { .. } => StatementRisk::Write,
        // CALL / anonymous DO can hide DDL. Type-to-confirm on production.
        Statement::Call(_) => StatementRisk::Destructive,
        Statement::Pragma {
            name, value, is_eq, ..
        } => pragma_risk(&basename(name), value.is_some(), *is_eq),

        Statement::AlterTable { operations, .. } => {
            if operations
                .iter()
                .any(|op| matches!(op, AlterTableOperation::DropColumn { .. }))
            {
                StatementRisk::Destructive
            } else {
                StatementRisk::Write
            }
        }

        Statement::CreateView { .. }
        | Statement::CreateTable(_)
        | Statement::CreateVirtualTable { .. }
        | Statement::CreateIndex(_)
        | Statement::CreateRole { .. }
        | Statement::CreateSchema { .. }
        | Statement::CreateDatabase { .. }
        | Statement::CreateFunction(_)
        | Statement::CreateProcedure { .. }
        | Statement::CreateTrigger { .. }
        | Statement::CreateExtension { .. }
        | Statement::CreateMacro { .. }
        | Statement::CreateStage { .. }
        | Statement::CreateSecret { .. }
        | Statement::CreatePolicy { .. }
        | Statement::CreateServer(_)
        | Statement::CreateConnector(_)
        | Statement::AlterIndex { .. }
        | Statement::AlterView { .. }
        | Statement::AlterType(_)
        | Statement::AlterRole { .. }
        | Statement::AlterPolicy { .. }
        | Statement::AlterConnector { .. }
        | Statement::Grant { .. }
        | Statement::Revoke { .. }
        | Statement::Deny(_)
        | Statement::Comment { .. }
        | Statement::Analyze { .. }
        | Statement::AttachDatabase { .. }
        | Statement::AttachDuckDBDatabase { .. }
        | Statement::DetachDuckDBDatabase { .. } => StatementRisk::Write,

        _ => StatementRisk::Write,
    }
}

fn set_risk(set: &Set) -> StatementRisk {
    match set {
        Set::SetRole { .. } => StatementRisk::Write,
        Set::SingleAssignment {
            scope, variable, ..
        } => {
            if matches!(scope, Some(ContextModifier::Global)) || variable_is_sensitive(variable) {
                StatementRisk::Write
            } else {
                StatementRisk::Session
            }
        }
        Set::MultipleAssignments { assignments } => {
            if assignments.iter().any(|assignment| {
                matches!(assignment.scope, Some(ContextModifier::Global))
                    || variable_is_sensitive(&assignment.name)
            }) {
                StatementRisk::Write
            } else {
                StatementRisk::Session
            }
        }
        _ => StatementRisk::Session,
    }
}

fn variable_is_sensitive(name: &ObjectName) -> bool {
    let text = name.to_string().to_ascii_lowercase();
    if text.contains("@@global") || text.contains("@@persist") {
        return true;
    }
    let last = text.rsplit('.').next().unwrap_or(&text);
    let last = last.trim_start_matches('@');
    matches!(
        last,
        "default_transaction_read_only"
            | "transaction_read_only"
            | "statement_timeout"
            | "lock_timeout"
            | "idle_in_transaction_session_timeout"
            | "idle_session_timeout"
            | "session_authorization"
            | "role"
            | "read_only"
    )
}

/// Worst risk of everything the statement runs: every function call in any
/// clause, row locks, `SELECT INTO`, and DML nested in a query.
fn deep_risk(stmt: &Statement) -> StatementRisk {
    let mut scan = RiskScan {
        risk: StatementRisk::Read,
    };
    let _ = stmt.visit(&mut scan);
    scan.risk
}

struct RiskScan {
    risk: StatementRisk,
}

impl Visitor for RiskScan {
    type Break = ();

    fn pre_visit_query(&mut self, query: &Query) -> ControlFlow<()> {
        if !query.locks.is_empty() {
            self.risk = self.risk.max(StatementRisk::Write);
        }
        self.risk = self.risk.max(set_expr_shape_risk(&query.body));
        ControlFlow::Continue(())
    }

    fn pre_visit_table_factor(&mut self, factor: &TableFactor) -> ControlFlow<()> {
        // `FROM dblink(…)` is a table with arguments, not an expression.
        match factor {
            TableFactor::Table {
                name,
                args: Some(_),
                ..
            }
            | TableFactor::Function { name, .. } => {
                self.risk = self.risk.max(function_name_risk(name));
            }
            _ => {}
        }
        ControlFlow::Continue(())
    }

    fn pre_visit_expr(&mut self, expr: &Expr) -> ControlFlow<()> {
        if let Expr::Function(function) = expr {
            self.risk = self.risk.max(function_name_risk(&function.name));
        }
        ControlFlow::Continue(())
    }
}

/// The query body itself. Nested queries are visited on their own.
fn set_expr_shape_risk(expr: &SetExpr) -> StatementRisk {
    match expr {
        SetExpr::Select(select) if select.into.is_some() => StatementRisk::Write,
        SetExpr::SetOperation { left, right, .. } => {
            set_expr_shape_risk(left).max(set_expr_shape_risk(right))
        }
        SetExpr::Insert(_) | SetExpr::Update(_) | SetExpr::Delete(_) => StatementRisk::Write,
        _ => StatementRisk::Read,
    }
}

/// Denylist of functions with side effects. Phase F replaces it with an allowlist.
fn function_name_risk(name: &ObjectName) -> StatementRisk {
    let name = basename(name);
    let name = name.as_str();
    if matches!(
        name,
        "pg_terminate_backend" | "pg_cancel_backend" | "load_extension"
    ) {
        return StatementRisk::Destructive;
    }
    // dblink and fdw run SQL on their own autocommit connection, outside
    // the read-only transaction.
    const WRITE_PREFIXES: &[&str] = &[
        "dblink",
        "postgres_fdw_",
        "pg_advisory_",
        "pg_try_advisory_",
        "pg_sleep",
        "pg_ls_",
        "pg_stat_reset",
        "pg_create_",
        "pg_drop_replication_slot",
        "pg_copy_",
        "pg_replication_origin_",
        "pg_replication_slot_advance",
        "pg_file_",
        "pg_wal_replay_",
        "lo_",
    ];
    if WRITE_PREFIXES.iter().any(|prefix| name.starts_with(prefix)) {
        return StatementRisk::Write;
    }
    match name {
        // Postgres
        "set_config" | "nextval" | "setval" | "lowrite" | "pg_reload_conf"
        | "pg_rotate_logfile" | "pg_read_file" | "pg_read_binary_file" | "pg_stat_file"
        | "pg_notify" | "pg_switch_wal" | "pg_promote" | "pg_backup_start" | "pg_backup_stop"
        | "pg_start_backup" | "pg_stop_backup" | "pg_logical_emit_message"
        | "pg_import_system_collations" | "pg_log_backend_memory_contexts"
        | "pg_stat_statements_reset"
        // MySQL
        | "get_lock" | "release_lock" | "release_all_locks" | "is_used_lock" | "is_free_lock"
        | "sleep" | "benchmark" | "load_file" | "master_pos_wait" | "source_pos_wait"
        | "wait_for_executed_gtid_set" | "wait_until_sql_thread_after_gtids"
        // SQLite
        | "writefile" => StatementRisk::Write,
        _ => StatementRisk::Read,
    }
}

fn basename(name: &ObjectName) -> String {
    let raw = name
        .0
        .last()
        .map(|part| part.to_string())
        .unwrap_or_default();
    let trimmed = raw.trim_matches(|c| matches!(c, '"' | '`' | '[' | ']'));
    trimmed
        .rsplit('.')
        .next()
        .unwrap_or(trimmed)
        .to_ascii_lowercase()
}

/// INSERT / UPDATE / MERGE whose every source and expression is a read.
fn ast_is_safe_dml(stmt: &Statement) -> bool {
    let shape = match stmt {
        Statement::Insert(_) | Statement::Update { .. } => true,
        Statement::Merge { clauses, .. } => clauses.iter().all(|clause| {
            matches!(
                clause.action,
                MergeAction::Insert(_) | MergeAction::Update { .. }
            )
        }),
        Statement::Query(query) => return query_is_safe_dml(query),
        _ => false,
    };
    // The statement's own write is not visited, only what it reads and calls.
    shape && deep_risk(stmt) == StatementRisk::Read
}

fn query_is_safe_dml(query: &Query) -> bool {
    if !query.locks.is_empty() {
        return false;
    }
    if let Some(with) = &query.with {
        if !with.cte_tables.iter().all(|cte| {
            query_is_safe_dml(&cte.query) || classify_query(&cte.query) == StatementRisk::Read
        }) {
            return false;
        }
    }
    set_expr_is_safe_dml(&query.body)
}

fn set_expr_is_safe_dml(expr: &SetExpr) -> bool {
    match expr {
        SetExpr::Insert(stmt) | SetExpr::Update(stmt) => ast_is_safe_dml(stmt),
        SetExpr::Query(inner) => query_is_safe_dml(inner),
        SetExpr::SetOperation { left, right, .. } => {
            set_expr_is_safe_dml(left) && set_expr_is_safe_dml(right)
        }
        _ => false,
    }
}

fn classify_query(query: &Query) -> StatementRisk {
    let mut scan = RiskScan {
        risk: StatementRisk::Read,
    };
    let _ = query.visit(&mut scan);
    scan.risk
}

fn ast_destructive_names(stmt: &Statement) -> Vec<String> {
    match stmt {
        Statement::Drop { names, .. } => names.iter().filter_map(last_object_ident).collect(),
        Statement::Truncate { table_names, .. } => table_names
            .iter()
            .filter_map(|target| last_object_ident(&target.name))
            .collect(),
        Statement::DropFunction { func_desc, .. } => func_desc
            .iter()
            .filter_map(|desc| last_object_ident(&desc.name))
            .collect(),
        Statement::DropProcedure { proc_desc, .. } => proc_desc
            .iter()
            .filter_map(|desc| last_object_ident(&desc.name))
            .collect(),
        Statement::DropTrigger { trigger_name, .. } => {
            last_object_ident(trigger_name).into_iter().collect()
        }
        Statement::AlterTable { operations, .. } => operations
            .iter()
            .flat_map(|op| match op {
                AlterTableOperation::DropColumn { column_names, .. } => column_names
                    .iter()
                    .map(|ident| ident.value.clone())
                    .collect::<Vec<_>>(),
                _ => Vec::new(),
            })
            .collect(),
        Statement::Call(function) => {
            let name = basename(&function.name);
            if name.is_empty() {
                vec!["CALL".to_string()]
            } else {
                vec![name]
            }
        }
        Statement::Explain {
            analyze,
            options,
            statement,
            ..
        } if *analyze || options_may_execute(options) => ast_destructive_names(statement),
        _ => Vec::new(),
    }
}

fn last_object_ident(name: &ObjectName) -> Option<String> {
    let ident = name.0.last()?;
    let raw = ident.to_string();
    let trimmed = raw.trim_matches(|c| matches!(c, '"' | '`' | '[' | ']'));
    let last = trimmed.rsplit('.').next().unwrap_or(trimmed);
    if last.is_empty() {
        None
    } else {
        Some(last.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn explain_subject_peels_explain_and_keeps_a_plain_select() {
        let subject = explain_subject("EXPLAIN SELECT 1", SqlDialect::Postgres).expect("subject");
        let lower = subject.to_ascii_lowercase();
        assert!(lower.contains("select"), "{subject}");
        assert!(!lower.trim_start().starts_with("explain"), "{subject}");
        let plain = explain_subject("SELECT 1;", SqlDialect::Sqlite).expect("plain");
        assert_eq!(plain, "SELECT 1");
    }

    fn pg(sql: &str) -> StatementRisk {
        classify(sql, SqlDialect::Postgres)
    }

    fn mysql(sql: &str) -> StatementRisk {
        classify(sql, SqlDialect::Mysql)
    }

    #[test]
    fn select_is_read() {
        assert_eq!(pg("SELECT 1"), StatementRisk::Read);
    }

    #[test]
    fn with_select_is_read() {
        assert_eq!(
            pg("WITH cte AS (SELECT 1) SELECT * FROM cte"),
            StatementRisk::Read
        );
    }

    #[test]
    fn with_insert_is_write() {
        assert_eq!(
            pg("WITH x AS (SELECT 1) INSERT INTO t SELECT * FROM x"),
            StatementRisk::Write
        );
    }

    #[test]
    fn select_into_is_write() {
        assert_eq!(pg("SELECT * INTO new_table FROM old"), StatementRisk::Write);
        assert_eq!(
            classify("SELECT * INTO new_table FROM old", SqlDialect::Generic),
            StatementRisk::Write
        );
    }

    #[test]
    fn explain_analyze_delete_is_write() {
        assert_eq!(pg("EXPLAIN ANALYZE DELETE FROM t"), StatementRisk::Write);
    }

    #[test]
    fn explain_select_is_read() {
        assert_eq!(pg("EXPLAIN SELECT 1"), StatementRisk::Read);
    }

    #[test]
    fn explain_analyze_executes_including_parenthesised_options() {
        assert!(explain_executes(
            "EXPLAIN ANALYZE DELETE FROM t",
            SqlDialect::Postgres
        ));
        assert!(explain_executes(
            "EXPLAIN (ANALYZE) DELETE FROM t",
            SqlDialect::Postgres
        ));
        assert!(explain_executes(
            "EXPLAIN (ANALYZE true, FORMAT JSON) DELETE FROM t",
            SqlDialect::Postgres
        ));
        assert!(explain_executes(
            "EXPLAIN ANALYZE SELECT 1",
            SqlDialect::Mysql
        ));
    }

    #[test]
    fn end_and_autocommit_are_transaction_boundaries() {
        for (sql, dialect) in [
            ("END", SqlDialect::Postgres),
            ("END TRANSACTION", SqlDialect::Sqlite),
            ("COMMIT", SqlDialect::Mysql),
            ("SET autocommit = 0", SqlDialect::Mysql),
            ("SET @@session.autocommit = 0", SqlDialect::Mysql),
        ] {
            assert!(route_text(sql, dialect).txn_boundary, "{sql}");
        }
        assert!(!route_text("SET time_zone = '+00:00'", SqlDialect::Mysql).txn_boundary);
    }

    #[test]
    fn explain_analyse_spelling_and_unknown_options_execute() {
        for sql in [
            "EXPLAIN (ANALYSE) DELETE FROM t",
            "EXPLAIN (analyse on) DELETE FROM t",
            "EXPLAIN (FOO) DELETE FROM t",
            "EXPLAIN (ANALYZE 'off') DELETE FROM t",
        ] {
            assert!(explain_executes(sql, SqlDialect::Postgres), "{sql}");
            assert_eq!(pg(sql), StatementRisk::Write, "{sql}");
        }
        // Unparsed, but the text still says ANALYSE.
        assert!(explain_executes(
            "EXPLAIN ANALYSE DELETE FROM t",
            SqlDialect::Postgres
        ));
        assert!(!explain_executes(
            "EXPLAIN (VERBOSE, COSTS off, FORMAT JSON) DELETE FROM t",
            SqlDialect::Postgres
        ));
    }

    #[test]
    fn functions_in_any_clause_raise_the_risk() {
        assert_eq!(
            pg("SELECT a FROM t ORDER BY pg_sleep(1)"),
            StatementRisk::Write
        );
        assert_eq!(
            pg("SELECT 1 LIMIT (SELECT nextval('s'))"),
            StatementRisk::Write
        );
        assert_eq!(
            pg("SELECT ARRAY[pg_terminate_backend(1)]"),
            StatementRisk::Destructive
        );
        assert_eq!(
            pg("SELECT * FROM dblink('c', 'DELETE FROM t') AS x(a int)"),
            StatementRisk::Write
        );
        // A plain EXPLAIN calls nothing.
        assert_eq!(pg("EXPLAIN SELECT pg_sleep(1)"), StatementRisk::Read);
        assert_eq!(pg("SELECT now(), lower(a) FROM t"), StatementRisk::Read);
    }

    #[test]
    fn safe_dml_walks_sources_and_expressions() {
        assert!(!is_safe_dml(
            "INSERT INTO t SELECT pg_terminate_backend(pid) FROM pg_stat_activity",
            SqlDialect::Postgres
        ));
        assert!(!is_safe_dml(
            "UPDATE t SET a = pg_sleep(1)",
            SqlDialect::Postgres
        ));
        assert!(!is_safe_dml("VACUUM", SqlDialect::Postgres));
        assert!(is_safe_dml(
            "INSERT INTO t SELECT a FROM s",
            SqlDialect::Postgres
        ));
        assert!(is_safe_dml(
            "WITH x AS (SELECT 1) INSERT INTO t SELECT * FROM x",
            SqlDialect::Postgres
        ));
    }

    #[test]
    fn pragma_with_an_argument_to_a_setting_is_a_write() {
        let lite = |sql| classify(sql, SqlDialect::Sqlite);
        assert_eq!(lite("PRAGMA user_version(42)"), StatementRisk::Write);
        assert_eq!(lite("PRAGMA journal_mode(DELETE)"), StatementRisk::Write);
        assert_eq!(lite("PRAGMA optimize"), StatementRisk::Write);
        assert_eq!(lite("PRAGMA user_version"), StatementRisk::Read);
        assert_eq!(lite("PRAGMA table_info(t)"), StatementRisk::Read);
        assert!(looks_like_pragma_read("PRAGMA main.index_list(\"t\")"));
        assert!(!looks_like_pragma_read("PRAGMA journal_mode = WAL"));
        assert!(!looks_like_pragma_read("PRAGMA writable_schema(1)"));
    }

    #[test]
    fn read_safe_set_is_session_scope_and_listed() {
        assert!(is_read_safe_set(
            "SET search_path TO public",
            SqlDialect::Postgres
        ));
        assert!(is_read_safe_set(
            "SET TIME ZONE 'UTC'",
            SqlDialect::Postgres
        ));
        assert!(is_read_safe_set(
            "SET @@session.time_zone = '+00:00'",
            SqlDialect::Mysql
        ));
        assert!(is_read_safe_set("SET @a = 1", SqlDialect::Mysql));
        for (sql, dialect) in [
            ("SET statement_timeout = 0", SqlDialect::Postgres),
            ("SET ROLE admin", SqlDialect::Postgres),
            ("SET TRANSACTION READ WRITE", SqlDialect::Postgres),
            ("SET GLOBAL time_zone = '+00:00'", SqlDialect::Mysql),
            ("SET sql_mode = ''", SqlDialect::Mysql),
            ("SET @a = (SELECT SLEEP(1))", SqlDialect::Mysql),
            ("SELECT 1", SqlDialect::Postgres),
        ] {
            assert!(!is_read_safe_set(sql, dialect), "{sql}");
        }
    }

    #[test]
    fn plain_explain_does_not_execute() {
        assert!(!explain_executes("EXPLAIN SELECT 1", SqlDialect::Postgres));
        assert!(!explain_executes(
            "EXPLAIN (ANALYZE false) DELETE FROM t",
            SqlDialect::Postgres
        ));
        assert!(!explain_executes(
            "EXPLAIN QUERY PLAN SELECT 1",
            SqlDialect::Sqlite
        ));
        assert!(!explain_executes("SELECT 1", SqlDialect::Postgres));
    }

    #[test]
    fn delete_is_write_drop_is_destructive() {
        assert_eq!(pg("DELETE FROM users"), StatementRisk::Write);
        assert_eq!(pg("DROP TABLE public.users"), StatementRisk::Destructive);
        assert_eq!(
            destructive_object_name("DROP TABLE public.users", SqlDialect::Postgres).as_deref(),
            Some("users")
        );
    }

    #[test]
    fn batch_takes_worst() {
        assert_eq!(pg("SELECT 1; DELETE FROM t;"), StatementRisk::Write);
        assert_eq!(
            pg("INSERT INTO t VALUES (1); DROP TABLE t;"),
            StatementRisk::Destructive
        );
    }

    #[test]
    fn mysql_with_insert_is_write() {
        assert_eq!(
            mysql("WITH x AS (SELECT 1) INSERT INTO t SELECT * FROM x"),
            StatementRisk::Write
        );
        assert!(mysql("SELECT 1 FROM t").is_read_like());
    }

    #[test]
    fn safe_dml_insert_not_delete() {
        assert!(is_safe_dml(
            "INSERT INTO t VALUES (1)",
            SqlDialect::Postgres
        ));
        assert!(is_safe_dml("UPDATE t SET a = 1", SqlDialect::Postgres));
        assert!(!is_safe_dml("DELETE FROM t", SqlDialect::Postgres));
        assert!(!is_safe_dml("CREATE TABLE t (a int)", SqlDialect::Postgres));
        assert!(!is_safe_dml(
            "INSERT INTO t VALUES (1); DELETE FROM t",
            SqlDialect::Postgres
        ));
    }

    #[test]
    fn merge_is_write_and_safe_dml() {
        let sql = "MERGE INTO t USING s ON t.id = s.id WHEN MATCHED THEN UPDATE SET a = s.a";
        assert_eq!(classify(sql, SqlDialect::Generic), StatementRisk::Write);
        assert!(is_safe_dml(sql, SqlDialect::Generic));
    }

    #[test]
    fn set_is_session_read_like() {
        assert!(pg("SET search_path TO public").is_read_like());
    }
}
