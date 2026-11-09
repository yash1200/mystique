//! JSON-RPC checks for access levels (F-02): tool annotations, deny
//! messages, and single-use approvals with their timeout.

use std::sync::Arc;
use std::time::Duration;

use mystique_core::{
    AppDatabase, AppServices, ConnectionManager, SaveConnectionInput, SecretStore, SshConfig,
};
use serde_json::{json, Value as JsonValue};

use super::rpc::handle_method;

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
    let path = std::env::temp_dir().join(format!(
        "mystique-mcp-levels-{}-{}.sqlite",
        std::process::id(),
        uuid::Uuid::new_v4()
    ));
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
    async fn rpc(&self, method: &str, params: JsonValue) -> JsonValue {
        handle_method(&self.app.agent_tools(), method, params)
            .await
            .unwrap_or_else(|(code, msg)| json!({ "rpcError": code, "message": msg }))
    }

    async fn call(&self, query: &str) -> JsonValue {
        self.rpc(
            "tools/call",
            json!({
                "name": "execute_query",
                "arguments": { "connection_id": self.id, "query": query }
            }),
        )
        .await
    }

    fn level(&self, level: &str) {
        self.app
            .tool_access()
            .set_level(&self.id, None, Some(level))
            .unwrap();
    }

    async fn close(&self) {
        let _ = self
            .app
            .connections()
            .close_session(&format!("mcp:{}", self.id))
            .await;
    }
}

fn text(result: &JsonValue) -> String {
    result["content"][0]["text"]
        .as_str()
        .unwrap_or("")
        .to_string()
}

#[tokio::test]
async fn tools_carry_annotations_and_the_write_tool_follows_the_levels() {
    let fx = fixture("local");
    let listed = fx.rpc("tools/list", json!({})).await;
    let tools = listed["tools"].as_array().unwrap();
    assert!(tools.iter().all(|t| t["name"] != "execute_query"));
    for tool in tools {
        let a = &tool["annotations"];
        assert_eq!(a["openWorldHint"], json!(false), "{tool}");
        if tool["name"] != "save_query" {
            assert_eq!(a["readOnlyHint"], json!(true), "{tool}");
            assert_eq!(a["destructiveHint"], json!(false), "{tool}");
        }
        assert!(tool["title"].as_str().is_some_and(|t| !t.is_empty()));
    }

    fx.level("write");
    let listed = fx.rpc("tools/list", json!({})).await;
    let write = listed["tools"]
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["name"] == "execute_query")
        .cloned()
        .expect("listed once a connection may write");
    assert_eq!(write["annotations"]["readOnlyHint"], json!(false));
    assert_eq!(write["annotations"]["destructiveHint"], json!(true));

    let init = fx
        .rpc("initialize", json!({ "protocolVersion": "2025-06-18" }))
        .await;
    let instructions = init["instructions"].as_str().unwrap();
    assert!(instructions.contains("full (adds DELETE, DDL, DROP)"));
}

#[tokio::test]
async fn a_call_above_the_level_is_denied_with_a_readable_message() {
    let fx = fixture("local");
    // Hidden: same message for a real and an unknown id, no name.
    for id in [fx.id.as_str(), "424242"] {
        let out = fx
            .rpc(
                "tools/call",
                json!({
                    "name": "execute_readonly_query",
                    "arguments": { "connection_id": id, "query": "SELECT 1" }
                }),
            )
            .await;
        assert_eq!(out["isError"], json!(true));
        assert!(text(&out).contains("has no access"), "{out}");
        assert!(!text(&out).contains("shop"), "{out}");
    }
    let schema = fx
        .rpc(
            "resources/read",
            json!({ "uri": format!("mystique://connections/{}/schema.dbml", fx.id) }),
        )
        .await;
    assert_eq!(schema["rpcError"], json!(-32000), "{schema}");

    fx.level("read");
    let out = fx.call("CREATE TABLE t (id INTEGER)").await;
    assert_eq!(out["isError"], json!(true));
    let msg = text(&out);
    assert!(msg.contains("needs `full`"), "{msg}");
    assert!(msg.contains("MCP has `read`"), "{msg}");
    assert!(msg.contains("connection \"shop\""), "{msg}");
    assert!(msg.contains("Settings → MCP → Access"), "{msg}");
    assert!(msg.contains("Nothing ran"), "{msg}");

    // The level is the grant: no dialog.
    fx.level("full");
    let out = fx.call("CREATE TABLE t (id INTEGER)").await;
    assert_eq!(out["isError"], json!(false), "{out}");
    let listed = fx
        .rpc("tools/call", json!({ "name": "list_connections" }))
        .await;
    assert_eq!(
        listed["structuredContent"]["connections"][0]["access"],
        json!("full")
    );
    fx.close().await;
}

async fn wait_for_request(app: &AppServices) -> mystique_core::ToolApprovalRequest {
    for _ in 0..400 {
        if let Some(req) = app.tool_approvals().pending().into_iter().next() {
            return req;
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    panic!("nothing reached the approval dock");
}

#[tokio::test]
async fn an_approval_is_single_use_and_times_out() {
    let fx = Arc::new(fixture("production"));
    fx.level("full");
    assert_eq!(
        fx.call("CREATE TABLE t (id INTEGER)").await["isError"],
        json!(false)
    );
    fx.level("read");
    fx.app.tool_access().set_ask(&fx.id, true).unwrap();
    fx.app
        .tool_approvals()
        .set_timeout(Duration::from_millis(300));

    let pending = {
        let fx = Arc::clone(&fx);
        tokio::spawn(async move { fx.call("INSERT INTO t VALUES (1)").await })
    };
    let req = wait_for_request(&fx.app).await;
    assert_eq!(req.sql, "INSERT INTO t VALUES (1)");
    assert_eq!(req.environment, "production");
    fx.app.tool_approvals().decide(&req.id, true).unwrap();
    let out = pending.await.unwrap();
    assert_eq!(out["isError"], json!(false), "{out}");

    // Same text again: a new request, unanswered, denied at the timeout.
    let out = fx.call("INSERT INTO t VALUES (1)").await;
    assert_eq!(out["isError"], json!(true));
    assert!(text(&out).contains("nobody approved"), "{out}");
    assert!(fx.app.tool_approvals().pending().is_empty());

    let count = fx
        .rpc(
            "tools/call",
            json!({
                "name": "execute_readonly_query",
                "arguments": { "connection_id": fx.id, "query": "SELECT count(*) FROM t" }
            }),
        )
        .await;
    assert_eq!(
        count["structuredContent"]["rows"][0][0],
        json!(1),
        "{count}"
    );
    fx.close().await;
}
