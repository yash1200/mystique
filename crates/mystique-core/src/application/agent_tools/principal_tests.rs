//! F-01: the agent and MCP never inherit the human's unlock or pinned tab.
//! F-02: levels, single-use approvals, and timeouts, run against SQLite.

use std::sync::Arc;
use std::time::Duration;

use mystique_connector_api::{ExecuteQueryRequest, QueryMode};
use serde_json::{json, Value as JsonValue};

use super::*;
use crate::connectors::{ConnectionManager, OpenSessionRequest};
use crate::domain::{ConnectionProfile, SaveConnectionInput};
use crate::secrets::SecretStore;
use crate::storage::AppDatabase;
use crate::AppServices;

struct Fixture {
    app: Arc<AppServices>,
    profile: ConnectionProfile,
    path: std::path::PathBuf,
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

fn fixture(environment: &str, read_only: bool) -> Fixture {
    let path = std::env::temp_dir().join(format!(
        "mystique-f01-{}-{}.sqlite",
        std::process::id(),
        uuid::Uuid::new_v4()
    ));
    let app = Arc::new(AppServices::with_secrets(
        AppDatabase::open_in_memory().unwrap(),
        ConnectionManager::bootstrap(),
        SecretStore::memory(),
    ));
    let profile = app
        .profiles()
        .save(SaveConnectionInput {
            id: None,
            name: "orders-db".into(),
            engine: "sqlite".into(),
            color_id: "teal".into(),
            environment: environment.into(),
            read_only,
            notes: None,
            plugin_config: json!({ "path": path.to_string_lossy() }),
            password: None,
            secret_ref: None,
            clear_secret: false,
            copy_secret_from: None,
            favorite: None,
            ssh_config: crate::domain::SshConfig::default(),
            ssh_password: None,
            clear_ssh_secret: false,
            tls_key_passphrase: None,
            query_timeout_secs: None,
            save_password: None,
        })
        .unwrap();
    Fixture { app, profile, path }
}

impl Fixture {
    fn level(&self, level: &str) {
        self.app
            .tool_access()
            .set_level(&self.profile.id, None, Some(level))
            .unwrap();
    }

    /// The desktop session: the query tab's id is the profile id.
    async fn open_ui(&self) {
        self.app
            .connections()
            .open_session(OpenSessionRequest {
                engine: "sqlite".into(),
                plugin_config: self.profile.plugin_config.clone(),
                password: None,
                pool: None,
                session_id: Some(self.profile.id.clone()),
                profile_id: Some(self.profile.id.clone()),
                ssh: None,
                tls_key_passphrase: None,
                environment: Some(self.profile.environment.clone()),
                connection_name: Some(self.profile.name.clone()),
                profile_read_only: self.profile.read_only,
                use_saved_ssh: true,
            })
            .await
            .unwrap();
    }

    async fn human(&self, text: &str) -> mystique_connector_api::ConnectorResult<()> {
        self.app
            .connections()
            .execute_query(request(&self.profile.id, text))
            .await
            .map(|_| ())
    }

    async fn tool(&self, caller: ToolCaller, query: &str) -> AppResult<JsonValue> {
        self.app
            .agent_tools()
            .call_tool(
                "execute_query",
                &json!({ "connection_id": self.profile.id, "query": query }),
                caller,
            )
            .await
    }

    async fn count(&self) -> i64 {
        let rows = self
            .app
            .agent_tools()
            .execute_readonly_query(
                &self.profile.id,
                "SELECT count(*) FROM t",
                None,
                None,
                None,
                "json",
                types::AgentCellWindow::default(),
                ToolCaller::Agent,
            )
            .await
            .unwrap()
            .rows
            .unwrap();
        rows[0][0].as_i64().unwrap()
    }
}

fn request(session_id: &str, text: &str) -> ExecuteQueryRequest {
    ExecuteQueryRequest {
        session_id: session_id.into(),
        text: text.into(),
        mode: QueryMode::Write,
        max_rows: None,
        timeout_ms: None,
        confirm_object_name: None,
        collection: None,
        database: None,
        execution_id: None,
        params: Vec::new(),
    }
}

/// Create the table as the human, through the unlock the human gives.
async fn seed(fx: &Fixture) {
    fx.open_ui().await;
    let _ = fx.human("CREATE TABLE t (id INTEGER)").await;
    fx.app
        .connections()
        .unlock_writes(&fx.profile.id)
        .await
        .unwrap();
    fx.human("CREATE TABLE IF NOT EXISTS t (id INTEGER)")
        .await
        .unwrap();
}

#[tokio::test]
async fn a_human_unlock_on_production_never_lets_mcp_or_the_agent_write() {
    let fx = fixture("production", false);
    seed(&fx).await;
    // The human is unlocked and writes.
    fx.human("INSERT INTO t VALUES (1)").await.unwrap();

    // MCP at read and the agent at its read default: denied, nothing ran.
    fx.level("read");
    for caller in [ToolCaller::Mcp, ToolCaller::Agent] {
        let err = fx
            .tool(caller, "INSERT INTO t VALUES (2)")
            .await
            .unwrap_err();
        assert!(matches!(err, AppError::AccessDenied(_)), "{err:?}");
        assert!(err.to_string().contains("needs `write`"), "{err}");
    }
    assert_eq!(fx.count().await, 1);

    // The tool session is separate from the human one.
    let ids: Vec<String> = fx
        .app
        .connections()
        .list_sessions()
        .await
        .into_iter()
        .map(|s| s.session_id)
        .collect();
    let agent = format!("agent:{}", fx.profile.id);
    assert!(ids.contains(&fx.profile.id), "{ids:?}");
    assert!(ids.contains(&agent), "{ids:?}");
    assert_ne!(agent, fx.profile.id);

    // A write sent straight to the tool session, past the access check, is
    // refused at the write guard even though the human session is unlocked.
    let err = fx
        .app
        .connections()
        .execute_query(request(&agent, "INSERT INTO t VALUES (3)"))
        .await
        .unwrap_err();
    assert!(err.to_string().contains("never applies to them"), "{err}");
    assert_eq!(fx.count().await, 1);

    // A tool session can be neither unlocked nor pinned to a tab, and a
    // desktop session cannot take over its id.
    let err = fx
        .app
        .connections()
        .unlock_writes(&agent)
        .await
        .unwrap_err();
    assert!(err.to_string().contains("cannot be unlocked"), "{err}");
    let err = fx
        .app
        .connections()
        .pin_tab(&agent, "tab-x")
        .await
        .unwrap_err();
    assert!(err.to_string().contains("cannot be pinned"), "{err}");
    let err = fx
        .app
        .connections()
        .open_session(OpenSessionRequest {
            engine: "sqlite".into(),
            plugin_config: fx.profile.plugin_config.clone(),
            password: None,
            pool: None,
            session_id: Some(format!("mcp:{}", fx.profile.id)),
            profile_id: None,
            ssh: None,
            tls_key_passphrase: None,
            environment: None,
            connection_name: None,
            profile_read_only: false,
            use_saved_ssh: true,
        })
        .await
        .unwrap_err();
    assert!(err.to_string().contains("reserved"), "{err}");
    let _ = fx.app.connections().close_session(&agent).await;
    let _ = fx.app.connections().close_session(&fx.profile.id).await;
}

#[tokio::test]
async fn an_explicit_level_is_the_grant_on_production_and_the_global_default_is_not() {
    let fx = fixture("production", false);
    seed(&fx).await;
    // The global default never writes on production.
    fx.app.tool_access().set_default("mcp", "full").unwrap();
    let err = fx
        .tool(ToolCaller::Mcp, "INSERT INTO t VALUES (1)")
        .await
        .unwrap_err();
    assert!(err.to_string().contains("production"), "{err}");

    // A level on the connection itself writes, with no dialog and no unlock
    // of the tool session.
    fx.level("write");
    fx.tool(ToolCaller::Mcp, "INSERT INTO t VALUES (1)")
        .await
        .unwrap();
    assert_eq!(fx.count().await, 1);
    // And still not a destructive statement.
    let err = fx.tool(ToolCaller::Mcp, "DELETE FROM t").await.unwrap_err();
    assert!(err.to_string().contains("needs `full`"), "{err}");
    assert_eq!(fx.count().await, 1);
    let _ = fx
        .app
        .connections()
        .close_session(&format!("mcp:{}", fx.profile.id))
        .await;
    let _ = fx.app.connections().close_session(&fx.profile.id).await;
}

#[tokio::test]
async fn a_pinned_tab_transaction_is_not_the_tools() {
    let fx = fixture("local", false);
    seed(&fx).await;
    let connections = fx.app.connections();
    connections.pin_tab(&fx.profile.id, "tab-1").await.unwrap();
    connections
        .execute_pinned_batch(
            request(&fx.profile.id, "BEGIN; INSERT INTO t VALUES (9)"),
            "tab-1".into(),
            None,
        )
        .await
        .unwrap();
    // The tool reads on its own pool: the open transaction is not visible.
    assert_eq!(fx.count().await, 0);
    connections.rollback_tab("tab-1").await.unwrap();
    assert_eq!(fx.count().await, 0);
    let _ = connections
        .close_session(&format!("agent:{}", fx.profile.id))
        .await;
    let _ = connections.close_session(&fx.profile.id).await;
}

#[tokio::test]
async fn a_read_only_profile_denies_writes_to_every_principal() {
    let fx = fixture("local", true);
    fx.level("full");
    fx.app.tool_access().set_ask(&fx.profile.id, true).unwrap();
    for caller in [ToolCaller::Mcp, ToolCaller::Agent] {
        let err = fx
            .tool(caller, "CREATE TABLE t (id INTEGER)")
            .await
            .unwrap_err();
        assert!(err.to_string().contains("read-only"), "{err}");
    }
    // Read-only wins before anything is asked.
    assert!(fx.app.tool_approvals().pending().is_empty());
}

#[tokio::test]
async fn closing_the_human_session_leaves_the_tool_session_running() {
    let fx = fixture("local", false);
    seed(&fx).await;
    fx.human("INSERT INTO t VALUES (1)").await.unwrap();
    assert_eq!(fx.count().await, 1);
    fx.app
        .connections()
        .close_session(&fx.profile.id)
        .await
        .unwrap();
    let agent = format!("agent:{}", fx.profile.id);
    assert!(fx.app.connections().get_session(&agent).await.is_some());
    assert_eq!(fx.count().await, 1);
    let _ = fx.app.connections().close_session(&agent).await;
}

async fn next_request(app: &AppServices) -> crate::domain::ToolApprovalRequest {
    for _ in 0..400 {
        if let Some(req) = app.tool_approvals().pending().into_iter().next() {
            return req;
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    panic!("nothing reached the approval dock");
}

#[tokio::test]
async fn ask_me_runs_an_approved_statement_once() {
    let fx = fixture("production", false);
    seed(&fx).await;
    fx.level("read");
    fx.app.tool_access().set_ask(&fx.profile.id, true).unwrap();

    let call = |query: &'static str| {
        let tools = fx.app.agent_tools();
        let id = fx.profile.id.clone();
        tokio::spawn(async move {
            tools
                .call_tool(
                    "execute_query",
                    &json!({ "connection_id": id, "query": query }),
                    ToolCaller::Mcp,
                )
                .await
        })
    };
    let first = call("INSERT INTO t VALUES (1)");
    let req = next_request(&fx.app).await;
    assert_eq!(req.sql, "INSERT INTO t VALUES (1)");
    assert_eq!(req.caller, "mcp");
    assert_eq!(req.connection_name, "orders-db");
    assert_eq!(req.environment, "production");
    assert_eq!(req.needed, "write");
    assert_eq!(req.granted, "read");
    fx.app.tool_approvals().decide(&req.id, true).unwrap();
    first.await.unwrap().unwrap();
    assert_eq!(fx.count().await, 1);

    // The same text again needs a new approval: this one is denied.
    let again = call("INSERT INTO t VALUES (1)");
    let req = next_request(&fx.app).await;
    fx.app.tool_approvals().decide(&req.id, false).unwrap();
    let err = again.await.unwrap().unwrap_err();
    assert!(err.to_string().contains("denied this statement"), "{err}");
    assert_eq!(fx.count().await, 1);

    // Approving A never allows B: B waits on its own and times out.
    fx.app
        .tool_approvals()
        .set_timeout(Duration::from_millis(400));
    let a = call("INSERT INTO t VALUES (10)");
    let b = call("INSERT INTO t VALUES (20)");
    let mut seen = Vec::new();
    for _ in 0..400 {
        seen = fx.app.tool_approvals().pending();
        if seen.len() == 2 {
            break;
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    assert_eq!(seen.len(), 2, "{seen:?}");
    let a_id = seen
        .iter()
        .find(|r| r.sql.contains("(10)"))
        .map(|r| r.id.clone())
        .unwrap();
    fx.app.tool_approvals().decide(&a_id, true).unwrap();
    a.await.unwrap().unwrap();
    let err = b.await.unwrap().unwrap_err();
    assert!(err.to_string().contains("nobody approved"), "{err}");
    assert_eq!(fx.count().await, 2);
    assert!(fx.app.tool_approvals().pending().is_empty());
    let _ = fx
        .app
        .connections()
        .close_session(&format!("mcp:{}", fx.profile.id))
        .await;
    let _ = fx.app.connections().close_session(&fx.profile.id).await;
}

#[tokio::test]
async fn a_new_process_holds_no_grants_or_requests() {
    let fx = fixture("local", false);
    let other = AppServices::with_secrets(
        AppDatabase::open_in_memory().unwrap(),
        ConnectionManager::bootstrap(),
        SecretStore::memory(),
    );
    assert!(fx.app.tool_approvals().pending().is_empty());
    assert!(other.tool_approvals().pending().is_empty());
    // Ask is a stored setting; grants are not.
    fx.app.tool_access().set_ask(&fx.profile.id, true).unwrap();
    let view = fx.app.tool_access().view().unwrap();
    assert!(view.connections[0].ask_high_risk);
}

#[tokio::test]
async fn an_unprovable_read_runs_only_where_writes_are_allowed() {
    let fx = fixture("local", false);
    seed(&fx).await;
    // Classified as a read, but ReadProof cannot prove an unknown function.
    let udf = "SELECT mq_fa_udf(1)";
    fx.level("read");
    for caller in [ToolCaller::Mcp, ToolCaller::Agent] {
        let err = fx.tool(caller, udf).await.unwrap_err();
        let msg = err.to_string();
        assert!(matches!(err, AppError::AccessDenied(_)), "{err:?}");
        assert!(msg.contains("needs `write`"), "{msg}");
        assert!(msg.contains("cannot prove it only reads"), "{msg}");
    }
    // The read tool says where it can run instead.
    let err = fx
        .app
        .agent_tools()
        .call_tool(
            "execute_readonly_query",
            &json!({ "connection_id": fx.profile.id, "query": udf }),
            ToolCaller::Mcp,
        )
        .await
        .unwrap_err();
    assert!(err.to_string().contains("Use execute_query"), "{err}");

    // With `write`, it is let through and runs in Write mode: SQLite itself
    // answers, not the Read-mode gate.
    fx.level("write");
    for caller in [ToolCaller::Mcp, ToolCaller::Agent] {
        let msg = fx.tool(caller, udf).await.unwrap_err().to_string();
        assert!(msg.contains("no such function"), "{msg}");
    }
    // A provable read still runs at `read`.
    fx.level("read");
    fx.tool(ToolCaller::Mcp, "SELECT count(*) FROM t")
        .await
        .unwrap();
    for sid in [
        format!("mcp:{}", fx.profile.id),
        format!("agent:{}", fx.profile.id),
        fx.profile.id.clone(),
    ] {
        let _ = fx.app.connections().close_session(&sid).await;
    }
}
