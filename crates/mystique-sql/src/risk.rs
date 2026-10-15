//! AST write-risk for a SQL statement. Parse failure is Write (fail closed).

use sqlparser::ast::{
    AlterTableOperation, ContextModifier, Expr, Function, FunctionArg, FunctionArgExpr,
    FunctionArguments, JoinConstraint, JoinOperator, MergeAction, ObjectName, Query, Select,
    SelectItem, Set, SetExpr, Statement, TableFactor,
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
    lower.starts_with("explain") && lower.contains("analyze")
}

fn ast_explain_executes(stmt: &Statement) -> bool {
    let Statement::Explain {
        analyze,
        query_plan,
        options,
        ..
    } = stmt
    else {
        return false;
    };
    if *query_plan && !*analyze && !options_enable_analyze(options) {
        return false;
    }
    *analyze || options_enable_analyze(options)
}

fn options_enable_analyze(options: &Option<Vec<sqlparser::ast::UtilityOption>>) -> bool {
    let Some(options) = options else {
        return false;
    };
    options.iter().any(|opt| {
        if !opt.name.value.eq_ignore_ascii_case("analyze") {
            return false;
        }
        match &opt.arg {
            None => true,
            Some(expr) => {
                let text = expr.to_string().to_ascii_lowercase();
                !matches!(text.as_str(), "false" | "off" | "0" | "no")
            }
        }
    })
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
        return TextRoute::opaque(StatementRisk::Write);
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
        Statement::Commit { end, .. } => {
            // `END` of a block sets `end`. `COMMIT` does not.
            route.txn_boundary = !end;
        }
        Statement::Rollback { savepoint, .. } => {
            route.txn_boundary = savepoint.is_none();
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

fn has_versioned_comment(sql: &str) -> bool {
    let bytes = sql.as_bytes();
    let mut i = 0;
    let mut in_single = false;
    while i + 1 < bytes.len() {
        if bytes[i] == b'\'' {
            in_single = !in_single;
            i += 1;
            continue;
        }
        if !in_single && bytes[i] == b'/' && bytes[i + 1] == b'*' {
            let rest = sql[i + 2..].trim_start();
            if rest.starts_with('!')
                || rest.len() >= 2
                    && (rest[..2].eq_ignore_ascii_case("m!")
                        || rest[..2].eq_ignore_ascii_case("t!"))
            {
                return true;
            }
        }
        i += 1;
    }
    false
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

fn looks_like_pragma_read(sql: &str) -> bool {
    let lower = sql.trim().to_ascii_lowercase();
    let Some(rest) = lower.strip_prefix("pragma") else {
        return false;
    };
    // `PRAGMA table_info(t)` reads. `PRAGMA query_only = ON` writes.
    rest.starts_with(|c: char| c.is_whitespace()) && !rest.contains('=') && !rest.contains(';')
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
    match stmt {
        Statement::Query(q) => classify_query(q),
        Statement::Explain {
            analyze,
            options,
            statement,
            ..
        } => {
            if *analyze || options_enable_analyze(options) {
                classify_ast(statement)
            } else {
                StatementRisk::Read
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
        // `PRAGMA table_info(t)` is a read. `PRAGMA query_only = ON` assigns.
        Statement::Pragma { is_eq, .. } => {
            if *is_eq {
                StatementRisk::Write
            } else {
                StatementRisk::Read
            }
        }

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

fn classify_query(q: &Query) -> StatementRisk {
    let mut risk = if q.locks.is_empty() {
        StatementRisk::Read
    } else {
        StatementRisk::Write
    };
    if let Some(with) = &q.with {
        for cte in &with.cte_tables {
            risk = risk.max(classify_query(&cte.query));
        }
    }
    risk.max(classify_set_expr(&q.body))
}

fn classify_set_expr(expr: &SetExpr) -> StatementRisk {
    match expr {
        SetExpr::Select(select) => classify_select(select),
        SetExpr::Query(inner) => classify_query(inner),
        SetExpr::SetOperation { left, right, .. } => {
            classify_set_expr(left).max(classify_set_expr(right))
        }
        SetExpr::Values(values) => values
            .rows
            .iter()
            .flatten()
            .fold(StatementRisk::Read, |risk, expr| risk.max(expr_risk(expr))),
        SetExpr::Table(_) => StatementRisk::Read,
        SetExpr::Insert(_) | SetExpr::Update(_) | SetExpr::Delete(_) => StatementRisk::Write,
    }
}

fn classify_select(select: &Select) -> StatementRisk {
    let mut risk = if select.into.is_some() {
        StatementRisk::Write
    } else {
        StatementRisk::Read
    };
    for item in &select.projection {
        risk = risk.max(select_item_risk(item));
    }
    for from in &select.from {
        risk = risk.max(table_factor_risk(&from.relation));
        for join in &from.joins {
            risk = risk.max(table_factor_risk(&join.relation));
            risk = risk.max(join_operator_risk(&join.join_operator));
        }
    }
    for expr in [
        select.selection.as_ref(),
        select.prewhere.as_ref(),
        select.having.as_ref(),
        select.qualify.as_ref(),
    ]
    .into_iter()
    .flatten()
    {
        risk = risk.max(expr_risk(expr));
    }
    risk
}

fn select_item_risk(item: &SelectItem) -> StatementRisk {
    match item {
        SelectItem::UnnamedExpr(expr) | SelectItem::ExprWithAlias { expr, .. } => expr_risk(expr),
        _ => StatementRisk::Read,
    }
}

fn table_factor_risk(factor: &TableFactor) -> StatementRisk {
    match factor {
        TableFactor::Derived { subquery, .. } => classify_query(subquery),
        TableFactor::TableFunction { expr, .. } => expr_risk(expr),
        TableFactor::Function { name, args, .. } => {
            function_name_risk(name).max(args.iter().fold(StatementRisk::Read, |risk, arg| {
                risk.max(function_arg_risk(arg))
            }))
        }
        TableFactor::UNNEST { array_exprs, .. } => array_exprs
            .iter()
            .fold(StatementRisk::Read, |risk, expr| risk.max(expr_risk(expr))),
        _ => StatementRisk::Read,
    }
}

fn join_operator_risk(op: &JoinOperator) -> StatementRisk {
    match op {
        JoinOperator::Join(constraint)
        | JoinOperator::Inner(constraint)
        | JoinOperator::Left(constraint)
        | JoinOperator::LeftOuter(constraint)
        | JoinOperator::Right(constraint)
        | JoinOperator::RightOuter(constraint)
        | JoinOperator::FullOuter(constraint)
        | JoinOperator::Semi(constraint)
        | JoinOperator::LeftSemi(constraint)
        | JoinOperator::RightSemi(constraint)
        | JoinOperator::Anti(constraint)
        | JoinOperator::LeftAnti(constraint)
        | JoinOperator::RightAnti(constraint)
        | JoinOperator::StraightJoin(constraint) => constraint_risk(constraint),
        JoinOperator::AsOf {
            match_condition,
            constraint,
        } => expr_risk(match_condition).max(constraint_risk(constraint)),
        JoinOperator::CrossJoin | JoinOperator::CrossApply | JoinOperator::OuterApply => {
            StatementRisk::Read
        }
    }
}

fn constraint_risk(constraint: &JoinConstraint) -> StatementRisk {
    match constraint {
        JoinConstraint::On(expr) => expr_risk(expr),
        _ => StatementRisk::Read,
    }
}

fn expr_risk(expr: &Expr) -> StatementRisk {
    match expr {
        Expr::Function(function) => function_risk(function),
        Expr::Subquery(query)
        | Expr::Exists {
            subquery: query, ..
        } => classify_query(query),
        Expr::InSubquery { expr, subquery, .. } => expr_risk(expr).max(classify_query(subquery)),
        Expr::Nested(inner)
        | Expr::IsNull(inner)
        | Expr::IsNotNull(inner)
        | Expr::IsTrue(inner)
        | Expr::IsNotTrue(inner)
        | Expr::IsFalse(inner)
        | Expr::IsNotFalse(inner) => expr_risk(inner),
        Expr::BinaryOp { left, right, .. }
        | Expr::IsDistinctFrom(left, right)
        | Expr::IsNotDistinctFrom(left, right) => expr_risk(left).max(expr_risk(right)),
        Expr::UnaryOp { expr, .. } | Expr::Cast { expr, .. } | Expr::Extract { expr, .. } => {
            expr_risk(expr)
        }
        Expr::InList { expr, list, .. } => list
            .iter()
            .fold(expr_risk(expr), |risk, item| risk.max(expr_risk(item))),
        Expr::Between {
            expr, low, high, ..
        } => expr_risk(expr).max(expr_risk(low)).max(expr_risk(high)),
        Expr::Like { expr, pattern, .. } | Expr::ILike { expr, pattern, .. } => {
            expr_risk(expr).max(expr_risk(pattern))
        }
        Expr::Case {
            operand,
            conditions,
            else_result,
            ..
        } => {
            let mut risk = operand
                .as_deref()
                .map(expr_risk)
                .unwrap_or(StatementRisk::Read);
            for when in conditions {
                risk = risk
                    .max(expr_risk(&when.condition))
                    .max(expr_risk(&when.result));
            }
            if let Some(else_result) = else_result {
                risk = risk.max(expr_risk(else_result));
            }
            risk
        }
        _ => StatementRisk::Read,
    }
}

fn function_risk(function: &Function) -> StatementRisk {
    function_name_risk(&function.name)
        .max(function_arguments_risk(&function.args))
        .max(function_arguments_risk(&function.parameters))
        .max(
            function
                .filter
                .as_deref()
                .map(expr_risk)
                .unwrap_or(StatementRisk::Read),
        )
}

fn function_arguments_risk(args: &FunctionArguments) -> StatementRisk {
    match args {
        FunctionArguments::None => StatementRisk::Read,
        FunctionArguments::Subquery(query) => classify_query(query),
        FunctionArguments::List(list) => list.args.iter().fold(StatementRisk::Read, |risk, arg| {
            risk.max(function_arg_risk(arg))
        }),
    }
}

fn function_arg_risk(arg: &FunctionArg) -> StatementRisk {
    match arg {
        FunctionArg::Named { arg, .. } | FunctionArg::Unnamed(arg) => function_arg_expr_risk(arg),
        FunctionArg::ExprNamed { name, arg, .. } => {
            expr_risk(name).max(function_arg_expr_risk(arg))
        }
    }
}

fn function_arg_expr_risk(arg: &FunctionArgExpr) -> StatementRisk {
    match arg {
        FunctionArgExpr::Expr(expr) => expr_risk(expr),
        _ => StatementRisk::Read,
    }
}

fn function_name_risk(name: &ObjectName) -> StatementRisk {
    match basename(name).as_str() {
        "pg_terminate_backend" | "pg_cancel_backend" | "load_extension" => {
            StatementRisk::Destructive
        }
        "set_config" | "dblink_exec" | "dblink_connect" | "nextval" | "lo_import" | "lo_export"
        | "lo_unlink" | "get_lock" | "release_lock" | "is_used_lock" | "sleep" | "benchmark" => {
            StatementRisk::Write
        }
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

fn ast_is_safe_dml(stmt: &Statement) -> bool {
    match stmt {
        Statement::Insert(_) | Statement::Update { .. } => true,
        Statement::Merge { clauses, .. } => clauses.iter().all(|clause| {
            matches!(
                clause.action,
                MergeAction::Insert(_) | MergeAction::Update { .. }
            )
        }),
        Statement::Query(query) => query_is_safe_dml(query),
        _ => false,
    }
}

fn query_is_safe_dml(query: &Query) -> bool {
    if let Some(with) = &query.with {
        if with
            .cte_tables
            .iter()
            .any(|cte| !query_is_safe_dml(&cte.query))
        {
            return false;
        }
    }
    set_expr_is_safe_dml(&query.body)
}

fn set_expr_is_safe_dml(expr: &SetExpr) -> bool {
    match expr {
        SetExpr::Insert(_) | SetExpr::Update(_) => true,
        SetExpr::Query(inner) => query_is_safe_dml(inner),
        SetExpr::SetOperation { left, right, .. } => {
            set_expr_is_safe_dml(left) && set_expr_is_safe_dml(right)
        }
        _ => false,
    }
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
        } if *analyze || options_enable_analyze(options) => ast_destructive_names(statement),
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
