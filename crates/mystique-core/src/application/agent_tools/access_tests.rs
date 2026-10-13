use mystique_connector_api::HARD_MAX_ROWS;
use serde_json::json;

use super::*;
use crate::connectors::ConnectionManager;
use crate::domain::SaveConnectionInput;
use crate::secrets::SecretStore;
use crate::storage::AppDatabase;
use crate::AppServices;

fn app() -> AppServices {
    AppServices::with_secrets(
        AppDatabase::open_in_memory().unwrap(),
        ConnectionManager::bootstrap(),
        SecretStore::memory(),
    )
}

fn save_pg(app: &AppServices, name: &str) -> crate::domain::ConnectionProfile {
    app.profiles()
        .save(SaveConnectionInput {
            id: None,
            name: name.into(),
            engine: "postgres".into(),
            color_id: "teal".into(),
            environment: "production".into(),
            read_only: false,
            notes: None,
            plugin_config: json!({"host": "127.0.0.1", "database": "postgres"}),
            password: None,
            secret_ref: None,
            clear_secret: false,
            copy_secret_from: None,
            favorite: None,
            ssh_config: crate::domain::SshConfig::default(),
            ssh_password: None,
            clear_ssh_secret: false,
            tls_key_passphrase: None,
        })
        .unwrap()
}

#[test]
fn agent_max_rows_is_below_driver_ceiling() {
    assert!(
        AGENT_MAX_ROWS < HARD_MAX_ROWS,
        "{AGENT_MAX_ROWS} must be < {HARD_MAX_ROWS}"
    );
}

#[test]
fn tool_specs_omit_execute_query_when_read_only() {
    assert!(tool_specs_for(AgentWritePolicy::ReadOnly)
        .iter()
        .all(|t| t.name != "execute_query"));
    assert!(tool_specs_for(AgentWritePolicy::SafeWrite)
        .iter()
        .any(|t| t.name == "execute_query"));
}

#[tokio::test]
async fn agent_connection_list_has_no_mongo_password() {
    let app = app();
    let mut input = SaveConnectionInput {
        id: None,
        name: "mongo".into(),
        engine: "mongodb".into(),
        color_id: "teal".into(),
        environment: "local".into(),
        read_only: true,
        notes: None,
        plugin_config: json!({
            "uri": "mongodb://admin:s%40cret@127.0.0.1:27017/app?authSource=admin"
        }),
        password: None,
        secret_ref: None,
        clear_secret: false,
        copy_secret_from: None,
        favorite: None,
        ssh_config: crate::domain::SshConfig::default(),
        ssh_password: None,
        clear_ssh_secret: false,
        tls_key_passphrase: None,
    };
    input.engine = "documentdb".into();
    app.profiles().save(input).unwrap();
    let listed = app
        .agent_tools()
        .list_connections(ToolCaller::Agent)
        .await
        .unwrap();
    let json = serde_json::to_string(&listed).unwrap();
    assert!(!json.contains("s@cret"), "{json}");
    assert!(!json.contains("s%40cret"), "{json}");
    assert!(!json.contains("admin:"), "{json}");
}

#[tokio::test]
async fn mcp_list_connections_empty_allowlist_is_none() {
    let app = app();
    let saved = save_pg(&app, "prod");
    let tools = app.agent_tools();
    let mcp = tools.list_connections(ToolCaller::Mcp).await.unwrap();
    assert!(mcp.is_empty(), "empty allowlist must hide all connections");
    let agent = tools.list_connections(ToolCaller::Agent).await.unwrap();
    assert_eq!(agent.len(), 1);
    assert_eq!(agent[0].id, saved.id);
}

#[tokio::test]
async fn mcp_list_connections_returns_allowlisted_only() {
    let app = app();
    let a = save_pg(&app, "a");
    let _b = save_pg(&app, "b");
    app.settings()
        .set(SETTING_MCP_ALLOWLIST, &format!(r#"["{}"]"#, a.id))
        .unwrap();
    let tools = app.agent_tools();
    let mcp = tools.list_connections(ToolCaller::Mcp).await.unwrap();
    assert_eq!(mcp.len(), 1);
    assert_eq!(mcp[0].id, a.id);
}

#[tokio::test]
async fn mcp_rejects_connection_not_on_allowlist() {
    let app = app();
    let saved = save_pg(&app, "prod");
    let err = app
        .agent_tools()
        .call_tool(
            "execute_readonly_query",
            &json!({
                "connection_id": saved.id,
                "query": "SELECT 1"
            }),
            ToolCaller::Mcp,
        )
        .await
        .unwrap_err();
    assert!(
        err.to_string().contains("allowlist"),
        "expected allowlist error, got {err}"
    );
}

#[tokio::test]
async fn default_policy_blocks_insert_even_when_allowlisted() {
    let app = app();
    let saved = save_pg(&app, "prod");
    app.settings()
        .set(SETTING_MCP_ALLOWLIST, &format!(r#"["{}"]"#, saved.id))
        .unwrap();
    let err = app
        .agent_tools()
        .call_tool(
            "execute_query",
            &json!({
                "connection_id": saved.id,
                "query": "INSERT INTO t VALUES (1)"
            }),
            ToolCaller::Mcp,
        )
        .await
        .unwrap_err();
    let msg = err.to_string();
    assert!(
        msg.contains("read-only"),
        "expected read-only policy error, got {msg}"
    );
}

#[tokio::test]
async fn safe_write_blocks_drop() {
    let app = app();
    let saved = save_pg(&app, "prod");
    app.settings()
        .set(SETTING_MCP_ALLOWLIST, &format!(r#"["{}"]"#, saved.id))
        .unwrap();
    app.settings()
        .set(SETTING_MCP_POLICY, "safe_write")
        .unwrap();
    let err = app
        .agent_tools()
        .call_tool(
            "execute_query",
            &json!({
                "connection_id": saved.id,
                "query": "DROP TABLE t"
            }),
            ToolCaller::Mcp,
        )
        .await
        .unwrap_err();
    let msg = err.to_string();
    assert!(
        msg.contains("safe_write"),
        "expected safe_write policy error, got {msg}"
    );
}

#[tokio::test]
async fn safe_write_blocks_delete() {
    let app = app();
    let saved = save_pg(&app, "prod");
    app.settings()
        .set(SETTING_MCP_ALLOWLIST, &format!(r#"["{}"]"#, saved.id))
        .unwrap();
    app.settings()
        .set(SETTING_MCP_POLICY, "safe_write")
        .unwrap();
    let err = app
        .agent_tools()
        .call_tool(
            "execute_query",
            &json!({
                "connection_id": saved.id,
                "query": "DELETE FROM t"
            }),
            ToolCaller::Mcp,
        )
        .await
        .unwrap_err();
    let msg = err.to_string();
    assert!(
        msg.contains("safe_write"),
        "expected safe_write policy error, got {msg}"
    );
}

#[tokio::test]
async fn agent_caller_is_not_filtered_by_allowlist() {
    let app = app();
    let saved = save_pg(&app, "prod");
    let err = app
        .agent_tools()
        .call_tool(
            "execute_query",
            &json!({
                "connection_id": saved.id,
                "query": "INSERT INTO t VALUES (1)"
            }),
            ToolCaller::Agent,
        )
        .await
        .unwrap_err();
    let msg = err.to_string();
    assert!(
        !msg.contains("allowlist"),
        "in-app agent must not use the MCP allowlist, got {msg}"
    );
    assert!(
        msg.contains("read-only"),
        "in-app agent still uses the shared write policy, got {msg}"
    );
}

#[tokio::test]
async fn mcp_save_query_does_not_overwrite_another_connection() {
    let app = app();
    let scratch = save_pg(&app, "scratch");
    let prod = save_pg(&app, "prod");
    app.settings()
        .set(SETTING_MCP_ALLOWLIST, &format!(r#"["{}"]"#, scratch.id))
        .unwrap();
    let tools = app.agent_tools();
    let saved = tools
        .save_query(crate::domain::SaveQueryInput {
            id: None,
            connection_id: prod.id.clone(),
            name: "prod query".into(),
            body: "SELECT secret".into(),
            language: Some("sql".into()),
            sort_order: 0,
            favorite: None,
        })
        .unwrap();

    let err = tools
        .call_tool(
            "save_query",
            &json!({
                "id": saved.id,
                "connection_id": scratch.id,
                "name": "hijack",
                "body": "SELECT 1"
            }),
            ToolCaller::Mcp,
        )
        .await
        .unwrap_err();
    assert!(
        err.to_string().contains("allowlist"),
        "existing row must be allowlisted, got {err}"
    );
    let still = app
        .db
        .with_conn(|c| crate::storage::sqlite::repo::queries::get_query(c, &saved.id))
        .unwrap();
    assert_eq!(still.body, "SELECT secret");
    assert_eq!(still.name, "prod query");
}

#[tokio::test]
async fn mcp_save_query_updates_its_own_row() {
    let app = app();
    let scratch = save_pg(&app, "scratch");
    app.settings()
        .set(SETTING_MCP_ALLOWLIST, &format!(r#"["{}"]"#, scratch.id))
        .unwrap();
    let tools = app.agent_tools();
    let created = tools
        .call_tool(
            "save_query",
            &json!({
                "connection_id": scratch.id,
                "name": "mine",
                "body": "SELECT 1"
            }),
            ToolCaller::Mcp,
        )
        .await
        .unwrap();
    let id = created["id"].as_str().unwrap();
    let updated = tools
        .call_tool(
            "save_query",
            &json!({
                "id": id,
                "connection_id": scratch.id,
                "name": "mine",
                "body": "SELECT 2"
            }),
            ToolCaller::Mcp,
        )
        .await
        .unwrap();
    assert_eq!(updated["body"], "SELECT 2");
}
