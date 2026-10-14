//! One parse per batch: split, classify, and decide how each statement runs.

use super::risk::{route_text, StatementRisk};
use super::{split_statement_spans, SqlDialect};

/// One statement in a batch, with the route taken from its AST.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlannedStatement {
    /// 0-based position in the batch.
    pub index: u32,
    pub sql: String,
    /// Byte offsets of `sql` in the original batch (`batch[start..end] == sql`).
    pub start: u32,
    pub end: u32,
    pub risk: StatementRisk,
    pub returns_rows: bool,
    pub returns_count: bool,
    pub cursor: bool,
    pub multi_set: bool,
    pub txn_boundary: bool,
    pub explain_executes: bool,
    pub destructive_names: Vec<String>,
}

/// Split `sql` and route every statement from a single parse.
///
/// Empty and comment-only chunks are dropped. An empty batch is an empty vec.
pub fn plan_batch(sql: &str, dialect: SqlDialect) -> Vec<PlannedStatement> {
    let mut out = Vec::new();
    for (index, span) in split_statement_spans(sql, dialect).into_iter().enumerate() {
        let Ok(index) = u32::try_from(index) else {
            break;
        };
        let route = route_text(&span.text, dialect);
        out.push(PlannedStatement {
            index,
            sql: span.text,
            start: u32::try_from(span.start).unwrap_or(u32::MAX),
            end: u32::try_from(span.end).unwrap_or(u32::MAX),
            risk: route.risk,
            returns_rows: route.returns_rows,
            returns_count: route.returns_count,
            cursor: route.cursor,
            multi_set: route.multi_set,
            txn_boundary: route.txn_boundary,
            explain_executes: route.explain_executes,
            destructive_names: route.destructive_names,
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pg(sql: &str) -> Vec<PlannedStatement> {
        plan_batch(sql, SqlDialect::Postgres)
    }

    #[test]
    fn two_selects_are_read_cursors() {
        let plan = pg("SELECT 1; SELECT 2;");
        assert_eq!(plan.len(), 2);
        assert!(plan.iter().all(|stmt| stmt.risk == StatementRisk::Read));
        assert!(plan.iter().all(|stmt| stmt.returns_rows && stmt.cursor));
        assert!(plan.iter().all(|stmt| !stmt.txn_boundary));
        assert_eq!(plan[1].index, 1);
        assert_eq!(plan[1].sql, "SELECT 2");
    }

    #[test]
    fn delete_after_select_is_a_count_not_a_cursor() {
        let plan = pg("SELECT 1; DELETE FROM t;");
        assert_eq!(plan[0].risk, StatementRisk::Read);
        assert_eq!(plan[1].risk, StatementRisk::Write);
        assert!(plan[1].returns_count);
        assert!(!plan[1].returns_rows);
        assert!(!plan[1].cursor);
        assert_eq!(plan[1].index, 1);
    }

    #[test]
    fn leading_comment_still_routes_as_a_select() {
        let plan = pg("-- note\nSELECT 1");
        assert_eq!(plan.len(), 1);
        assert!(plan[0].sql.starts_with("-- note"));
        assert!(plan[0].returns_rows);
        assert!(plan[0].cursor);
        assert_eq!(plan[0].risk, StatementRisk::Read);
    }

    #[test]
    fn insert_returning_returns_rows_and_a_count() {
        let plan = pg("INSERT INTO t (id) VALUES (1) RETURNING id");
        assert_eq!(plan.len(), 1);
        assert!(plan[0].returns_rows);
        assert!(plan[0].returns_count);
        assert!(!plan[0].cursor);
        assert_eq!(plan[0].risk, StatementRisk::Write);
    }

    #[test]
    fn parenthesised_select_is_a_cursor() {
        let plan = pg("(SELECT 1)");
        assert_eq!(plan.len(), 1);
        assert!(plan[0].returns_rows, "{:?}", plan[0]);
        assert!(plan[0].cursor);
        assert_eq!(plan[0].risk, StatementRisk::Read);
    }

    #[test]
    fn transaction_openers_are_boundaries_and_rollback_to_is_not() {
        assert!(pg("BEGIN")[0].txn_boundary);
        assert!(pg("START TRANSACTION")[0].txn_boundary);
        assert!(pg("COMMIT")[0].txn_boundary);
        assert!(pg("ROLLBACK")[0].txn_boundary);
        assert!(!pg("ROLLBACK TO mystique_row_cap")[0].txn_boundary);
    }

    #[test]
    fn call_drains_multiple_sets_and_is_not_a_cursor() {
        let plan = plan_batch("CALL app.sync()", SqlDialect::Mysql);
        assert!(plan[0].multi_set);
        assert!(plan[0].returns_rows);
        assert!(!plan[0].cursor);
        assert_eq!(plan[0].risk, StatementRisk::Destructive);
    }

    #[test]
    fn versioned_comment_stays_a_write_even_when_the_body_looks_like_a_read() {
        let plan = plan_batch("/*!50000 SELECT 1 */", SqlDialect::Mysql);
        assert_eq!(plan[0].risk, StatementRisk::Write);
        assert!(plan[0].returns_rows);
        assert!(!plan[0].cursor);
    }

    #[test]
    fn explain_analyze_executes_and_plain_explain_does_not() {
        let plain = pg("EXPLAIN SELECT 1");
        assert!(plain[0].returns_rows);
        assert!(!plain[0].cursor);
        assert!(!plain[0].explain_executes);
        let analyze = pg("EXPLAIN ANALYZE SELECT 1");
        assert!(analyze[0].explain_executes);
        assert_eq!(analyze[0].risk, StatementRisk::Read);
    }
}
