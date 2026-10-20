//! `EXPLAIN QUERY PLAN` as a tree.
//!
//! SQLite has no analyze plan that returns a tree. The statement is not
//! executed. `PRAGMA query_only` is on so a bug cannot fall through to the
//! real statement.

use std::sync::Arc;

use mystique_connector_api::{
    ConnectorError, ConnectorResult, ExplainPlan, PlanNode, QueryControl, ValidatedQuery,
    EXPLAIN_NOT_EXECUTED,
};
use rusqlite::Connection;

use super::interrupt;
use super::SqliteSession;

pub(super) async fn explain_query(
    session: &SqliteSession,
    query: &ValidatedQuery,
    analyze: bool,
) -> ConnectorResult<ExplainPlan> {
    session.ensure_open()?;
    let path = session.path();
    let sql = sqlite_explain_sql(&query.text);
    let control = Arc::clone(&query.control);
    session
        .blocking(move || read_plan(&path, &sql, analyze, &control))
        .await
}

pub(super) fn sqlite_explain_sql(subject: &str) -> String {
    let subject = subject.trim().trim_end_matches(';').trim();
    format!("EXPLAIN QUERY PLAN {subject}")
}

fn read_plan(
    path: &std::path::Path,
    sql: &str,
    analyze: bool,
    control: &Arc<QueryControl>,
) -> ConnectorResult<ExplainPlan> {
    let conn = Connection::open(path).map_err(|err| interrupt::map_err(control, err))?;
    conn.execute_batch("PRAGMA query_only = ON")
        .map_err(|err| interrupt::map_err(control, err))?;
    interrupt::arm(&conn, control);
    let _settle = interrupt::Settle {
        conn: &conn,
        control,
    };
    if control.is_cancelled() {
        return Err(ConnectorError::Query("Query cancelled.".into()));
    }
    let mut stmt = conn
        .prepare(sql)
        .map_err(|err| interrupt::map_err(control, err))?;
    let columns = stmt.column_count();
    let mut rows = stmt
        .query([])
        .map_err(|err| interrupt::map_err(control, err))?;
    let mut steps = Vec::new();
    let mut raw_lines = Vec::new();
    while let Some(row) = rows
        .next()
        .map_err(|err| interrupt::map_err(control, err))?
    {
        if columns >= 4 {
            let id: i64 = row.get(0).map_err(|err| interrupt::map_err(control, err))?;
            let parent: i64 = row.get(1).map_err(|err| interrupt::map_err(control, err))?;
            let detail: String = row.get(3).map_err(|err| interrupt::map_err(control, err))?;
            raw_lines.push(format!("{id}\t{parent}\t{detail}"));
            steps.push(Step { id, parent, detail });
        } else {
            let detail: String = row.get(0).map_err(|err| interrupt::map_err(control, err))?;
            raw_lines.push(detail.clone());
            steps.push(Step {
                id: i64::try_from(steps.len()).unwrap_or(i64::MAX),
                parent: 0,
                detail,
            });
        }
    }
    if steps.is_empty() {
        return Err(ConnectorError::Query("Explain returned no plan.".into()));
    }
    let mut warnings = Vec::new();
    if analyze {
        warnings.push(
            "SQLite has no execution stats. This is EXPLAIN QUERY PLAN, so the statement did not run."
                .into(),
        );
        warnings.push(EXPLAIN_NOT_EXECUTED.to_string());
    }
    Ok(ExplainPlan {
        raw: raw_lines.join("\n"),
        raw_format: "text".into(),
        root: sqlite_tree(&steps),
        warnings,
    })
}

struct Step {
    id: i64,
    parent: i64,
    detail: String,
}

fn sqlite_tree(steps: &[Step]) -> PlanNode {
    let ids: Vec<i64> = steps.iter().map(|step| step.id).collect();
    let roots = children_of(steps, &ids, None);
    single_root("Query plan", roots)
}

fn children_of(steps: &[Step], ids: &[i64], parent: Option<i64>) -> Vec<PlanNode> {
    let mut out = Vec::new();
    for step in steps {
        let is_child = match parent {
            None => step.parent == 0 || !ids.contains(&step.parent),
            Some(parent) => step.parent == parent,
        };
        if !is_child {
            continue;
        }
        out.push(PlanNode {
            name: step.detail.clone(),
            rows: None,
            cost: None,
            fields: Vec::new(),
            children: children_of(steps, ids, Some(step.id)),
        });
    }
    out
}

fn single_root(name: &str, mut nodes: Vec<PlanNode>) -> PlanNode {
    if nodes.len() == 1 {
        return nodes.pop().unwrap_or_else(|| PlanNode::leaf(name));
    }
    if nodes.is_empty() {
        return PlanNode::leaf(name);
    }
    PlanNode {
        name: name.into(),
        rows: None,
        cost: None,
        fields: Vec::new(),
        children: nodes,
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    use mystique_connector_api::{
        ConnectionConfig, DatabaseConnector, PoolConfig, QueryControl, QueryMode, ValidatedQuery,
    };

    use crate::SqliteConnector;

    use super::sqlite_tree;
    use super::Step;

    fn query(text: &str) -> ValidatedQuery {
        ValidatedQuery {
            text: text.into(),
            mode: QueryMode::Write,
            max_rows: 10,
            timeout_ms: 5_000,
            collection: None,
            database: None,
            control: QueryControl::new(),
            params: Vec::new(),
        }
    }

    fn temp_path() -> std::path::PathBuf {
        let nanos = match SystemTime::now().duration_since(UNIX_EPOCH) {
            Ok(duration) => duration.as_nanos(),
            Err(_) => 0,
        };
        std::env::temp_dir().join(format!(
            "mystique-explain-{}-{nanos}.sqlite",
            std::process::id()
        ))
    }

    #[test]
    fn a_child_step_hangs_under_its_parent() {
        let steps = vec![
            Step {
                id: 2,
                parent: 0,
                detail: "SCAN t".into(),
            },
            Step {
                id: 3,
                parent: 2,
                detail: "USE TEMP B-TREE FOR ORDER BY".into(),
            },
        ];
        let root = sqlite_tree(&steps);
        assert_eq!(root.name, "SCAN t");
        assert!(root.cost.is_none());
        let child = match root.children.first() {
            Some(child) => child,
            None => panic!("missing child"),
        };
        assert_eq!(child.name, "USE TEMP B-TREE FOR ORDER BY");
    }

    #[tokio::test]
    async fn explain_analyze_delete_does_not_delete() {
        let path = temp_path();
        let _ = fs::remove_file(&path);
        let connector = SqliteConnector;
        let config = ConnectionConfig {
            engine: "sqlite".into(),
            plugin_config: serde_json::json!({ "path": path.to_string_lossy() }),
            password: None,
            pool: PoolConfig {
                max_size: 1,
                min_idle: 0,
                acquire_timeout_ms: 3_000,
                idle_timeout_secs: 5,
            },
        };
        let session = match connector.connect(&config).await {
            Ok(session) => session,
            Err(err) => panic!("{err}"),
        };
        let seed = query("CREATE TABLE t (id INTEGER PRIMARY KEY, a INTEGER); INSERT INTO t (id, a) VALUES (1, 0)");
        // Seed through execute so the table exists. Explain itself must not write.
        if let Err(err) = session.execute_query(&seed).await {
            panic!("{err}");
        }
        let plan = match session
            .explain_query(&query("DELETE FROM t WHERE id = 1"), true)
            .await
        {
            Ok(plan) => plan,
            Err(err) => panic!("{err}"),
        };
        assert!(
            plan.warnings
                .iter()
                .any(|warning| warning.contains("did not")),
            "{:?}",
            plan.warnings
        );
        let count = match session
            .execute_query(&query("SELECT COUNT(*) FROM t"))
            .await
        {
            Ok(result) => result,
            Err(err) => panic!("{err}"),
        };
        let cell = count
            .rows
            .first()
            .and_then(|row| row.first())
            .and_then(|value| value.as_i64());
        assert_eq!(cell, Some(1), "{count:?}");
        let _ = session.close().await;
        let _ = fs::remove_file(&path);
    }

    #[tokio::test]
    async fn a_select_plan_names_the_scan() {
        let path = temp_path();
        let _ = fs::remove_file(&path);
        let connector = SqliteConnector;
        let config = ConnectionConfig {
            engine: "sqlite".into(),
            plugin_config: serde_json::json!({ "path": path.to_string_lossy() }),
            password: None,
            pool: PoolConfig {
                max_size: 1,
                min_idle: 0,
                acquire_timeout_ms: 3_000,
                idle_timeout_secs: 5,
            },
        };
        let session = match connector.connect(&config).await {
            Ok(session) => session,
            Err(err) => panic!("{err}"),
        };
        if let Err(err) = session
            .execute_query(&query(
                "CREATE TABLE t (id INTEGER PRIMARY KEY); INSERT INTO t (id) VALUES (1)",
            ))
            .await
        {
            panic!("{err}");
        }
        let plan = match session
            .explain_query(&query("SELECT * FROM t"), false)
            .await
        {
            Ok(plan) => plan,
            Err(err) => panic!("{err}"),
        };
        let rendered = format!("{:?}", plan.root);
        assert!(
            rendered.to_ascii_lowercase().contains("scan") || rendered.contains('t'),
            "{rendered}"
        );
        assert!(plan.warnings.is_empty(), "{:?}", plan.warnings);
        assert!(!plan.raw.is_empty());
        let _ = session.close().await;
        let _ = fs::remove_file(&path);
    }
}
