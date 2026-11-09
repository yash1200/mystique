//! The agent's `execute_query`: the same `AgentTools` path, levels and
//! approval dock as MCP, run against SQLite.

use std::sync::Arc;
use std::time::Duration;

use mystique_core::{
    AppDatabase, AppServices, ConnectionManager, SaveConnectionInput, SecretStore, SshConfig,
    ToolApprovalRequest, ToolCaller,
};
use serde_json::{json, Value as JsonValue};

use super::{BoundTools, ToolErr, WRITE_TOOL};

struct Fixture {
    app: Arc<AppServices>,
    id: String,
    path: std::path::PathBuf,
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

fn fixture(environment: &str) -> Fixture {
    static NEXT: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
    let path = std::env::temp_dir().join(format!(
        "mystique-agent-write-{}-{}.sqlite",
        std::process::id(),
        NEXT.fetch_add(1, std::sync::atomic::Ordering::SeqCst)
    ));
    let _ = std::fs::remove_file(&path);
    let app = Arc::new(AppServices::with_secrets(
        AppDatabase::open_in_memory().unwrap(),
        ConnectionManager::bootstrap(),
        SecretStore::memory(),
    ));
    let id = app
        .profiles()
        .save(SaveConnectionInput {
            id: None,
            name: "shop".into(),
            engine: "sqlite".into(),
            color_id: "teal".into(),
            environment: environment.into(),
            read_only: false,
            notes: None,
            plugin_config: json!({ "path": path.to_string_lossy() }),
            password: None,
            secret_ref: None,
            clear_secret: false,
            copy_secret_from: None,
            favorite: None,
            ssh_config: SshConfig::default(),
            ssh_password: None,
            clear_ssh_secret: false,
            tls_key_passphrase: None,
            query_timeout_secs: None,
            save_password: None,
        })
        .unwrap()
        .id;
    Fixture { app, id, path }
}

impl Fixture {
    fn bound(&self) -> BoundTools {
        BoundTools::new(self.app.agent_tools(), self.id.clone(), String::new())
    }

    fn level(&self, level: &str) {
        self.app
            .tool_access()
            .set_level(&self.id, None, Some(level))
            .unwrap();
    }

    async fn write(&self, query: &str) -> Result<JsonValue, ToolErr> {
        self.bound()
            .dispatch(WRITE_TOOL, json!({ "query": query }))
            .await
    }

    /// Seed `t` with one row at `full`, then put the level back to unset.
    async fn seed(&self) {
        self.level("full");
        self.write("CREATE TABLE t (id INTEGER)").await.unwrap();
        self.write("INSERT INTO t VALUES (1)").await.unwrap();
        self.app
            .tool_access()
            .set_level(&self.id, None, None)
            .unwrap();
    }

    async fn count(&self) -> i64 {
        let out = self
            .bound()
            .dispatch(
                "execute_readonly_query",
                json!({ "query": "SELECT count(*) FROM t" }),
            )
            .await
            .unwrap();
        out["rows"][0][0].as_i64().unwrap()
    }

    async fn close(&self) {
        for prefix in ["agent:", "mcp:"] {
            let _ = self
                .app
                .connections()
                .close_session(&format!("{prefix}{}", self.id))
                .await;
        }
    }
}

async fn next_request(app: &AppServices) -> ToolApprovalRequest {
    for _ in 0..400 {
        if let Some(req) = app.tool_approvals().pending().into_iter().next() {
            return req;
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    panic!("nothing reached the approval dock");
}

#[tokio::test]
async fn at_read_the_write_tool_is_not_listed_and_a_write_is_denied_like_mcp() {
    let fx = fixture("local");
    fx.seed().await;
    // The agent's default level is read.
    let bound = fx.bound();
    assert!(!bound.may_write());
    assert!(!bound.listed().contains(&WRITE_TOOL));
    assert!(bound.listed().contains(&"sample_rows"));
    assert!(bound.listed().contains(&"explain_query"));
    assert!(bound.listed().contains(&"infer_fields"));
    // Called anyway (a leaked call): the level denies it, as on MCP.
    let err = fx.write("INSERT INTO t VALUES (2)").await.unwrap_err();
    assert!(err.0.contains("needs `write`"), "{err}");
    assert!(err.0.contains("(the global default)"), "{err}");

    // Listed once any level allows writes; this connection still reads.
    fx.level("read");
    assert!(!fx.bound().may_write());
    fx.app
        .tool_access()
        .set_level(&fx.id, Some("other"), Some("write"))
        .unwrap();
    assert!(fx.bound().may_write());
    let agent = fx.write("INSERT INTO t VALUES (2)").await.unwrap_err().0;
    fx.app.tool_access().set_default("mcp", "read").unwrap();
    let mcp = fx
        .app
        .agent_tools()
        .call_tool(
            "execute_query",
            &json!({ "connection_id": fx.id, "query": "INSERT INTO t VALUES (2)" }),
            ToolCaller::Mcp,
        )
        .await
        .unwrap_err()
        .to_string();
    for msg in [&agent, &mcp] {
        assert!(msg.contains("needs `write`"), "{msg}");
        assert!(msg.contains("Nothing ran"), "{msg}");
    }
    // One message, with the caller named.
    assert_eq!(
        agent.replace("The built-in agent has", "X has"),
        mcp.replace("MCP has", "X has"),
    );
    assert_eq!(fx.count().await, 1);
    fx.close().await;
}

#[tokio::test]
async fn write_allows_insert_and_full_allows_delete() {
    let fx = fixture("production");
    fx.seed().await;
    fx.level("write");
    assert!(fx.bound().listed().contains(&WRITE_TOOL));
    fx.write("INSERT INTO t VALUES (2)").await.unwrap();
    assert_eq!(fx.count().await, 2);
    let err = fx.write("DELETE FROM t WHERE id = 2").await.unwrap_err();
    assert!(err.0.contains("needs `full`"), "{err}");
    assert_eq!(fx.count().await, 2);

    fx.level("full");
    fx.write("DELETE FROM t WHERE id = 2").await.unwrap();
    assert_eq!(fx.count().await, 1);
    fx.close().await;
}

#[tokio::test]
async fn ask_me_puts_the_agent_in_the_dock_and_honours_the_answer() {
    let fx = fixture("production");
    fx.seed().await;
    fx.level("read");
    fx.app.tool_access().set_ask(&fx.id, true).unwrap();
    // Ask on: the write tool is offered even at read.
    assert!(fx.bound().may_write());

    let call = |query: &'static str| {
        let bound = fx.bound();
        tokio::spawn(async move { bound.dispatch(WRITE_TOOL, json!({ "query": query })).await })
    };

    // Approve: runs once.
    let first = call("INSERT INTO t VALUES (2)");
    let req = next_request(&fx.app).await;
    assert_eq!(req.caller, "agent");
    assert_eq!(req.sql, "INSERT INTO t VALUES (2)");
    assert_eq!(req.needed, "write");
    assert_eq!(req.granted, "read");
    assert_eq!(req.connection_name, "shop");
    fx.app.tool_approvals().decide(&req.id, true).unwrap();
    first.await.unwrap().unwrap();
    assert_eq!(fx.count().await, 2);

    // Deny: nothing runs.
    let denied = call("INSERT INTO t VALUES (3)");
    let req = next_request(&fx.app).await;
    fx.app.tool_approvals().decide(&req.id, false).unwrap();
    let err = denied.await.unwrap().unwrap_err();
    assert!(err.0.contains("denied this statement"), "{err}");
    assert_eq!(fx.count().await, 2);

    // Timeout: nobody answers.
    fx.app
        .tool_approvals()
        .set_timeout(Duration::from_millis(200));
    let err = fx.write("INSERT INTO t VALUES (4)").await.unwrap_err();
    assert!(err.0.contains("nobody approved"), "{err}");
    assert_eq!(fx.count().await, 2);
    assert!(fx.app.tool_approvals().pending().is_empty());
    fx.close().await;
}
