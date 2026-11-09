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
            query_timeout_secs: None,
            save_password: None,
        })
        .unwrap()
}

const _: () = assert!(
    AGENT_MAX_ROWS < HARD_MAX_ROWS,
    "AGENT_MAX_ROWS must be below HARD_MAX_ROWS"
);

/// A local Postgres profile on database `dev` with database `prod` marked
/// production (F-03).
fn save_dev_with_prod(app: &AppServices) -> crate::domain::ConnectionProfile {
    let mut saved = save_pg(app, "dev server");
    saved = app
        .profiles()
        .save(SaveConnectionInput {
            id: Some(saved.id.clone()),
            name: saved.name.clone(),
            engine: "postgres".into(),
            color_id: "teal".into(),
            environment: "local".into(),
            read_only: false,
            notes: None,
            plugin_config: json!({"host": "127.0.0.1", "port": 1, "database": "dev"}),
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
        .unwrap_or(saved);
    app.set_database_environment(&crate::domain::SetDatabaseEnvironmentInput {
        profile_id: saved.id.clone(),
        database: "prod".into(),
        schema: None,
        environment: Some("production".into()),
    })
    .unwrap()
}

#[test]
fn production_target_reads_marks_and_qualified_names() {
    let app = app();
    let profile = save_dev_with_prod(&app);
    let tools = app.agent_tools();
    let prod = |text: Option<&str>, db: Option<&str>| tools.production_target(&profile, db, text);
    assert!(!prod(Some("INSERT INTO t VALUES (1)"), Some("dev")));
    assert!(prod(
        Some("INSERT INTO prod.public.t VALUES (1)"),
        Some("dev")
    ));
    assert!(prod(Some("INSERT INTO t VALUES (1)"), Some("prod")));
    // A write the parser cannot place counts as production.
    assert!(prod(Some("SELECT nextval('s')"), Some("dev")));
    assert!(prod(Some("DO $$ BEGIN DELETE FROM t; END $$"), Some("dev")));
    // Reads and no statement: the database they run in.
    assert!(!prod(Some("SELECT * FROM prod.public.t"), Some("dev")));
    assert!(!prod(None, Some("dev")));
    assert!(prod(None, Some("prod")));
}

#[tokio::test]
async fn an_inherited_write_level_is_capped_on_a_production_marked_database() {
    let app = app();
    app.tool_access().set_default("mcp", "write").unwrap();
    let profile = save_dev_with_prod(&app);
    let tools = app.agent_tools();
    let run = |query: &'static str| {
        let tools = tools.clone();
        let id = profile.id.clone();
        async move {
            tools
                .call_tool(
                    "execute_query",
                    &json!({ "connection_id": id, "query": query }),
                    ToolCaller::Mcp,
                )
                .await
        }
    };
    // The dev database: the inherited `write` applies. The access check
    // passes; the run then fails to connect (nothing listens on port 1).
    let dev = run("INSERT INTO t VALUES (1)").await.unwrap_err();
    assert!(!matches!(dev, AppError::AccessDenied(_)), "{dev:?}");
    // A qualified write into the production database is capped at read.
    let prod = run("INSERT INTO prod.public.t VALUES (1)")
        .await
        .unwrap_err();
    let msg = prod.to_string();
    assert!(matches!(prod, AppError::AccessDenied(_)), "{prod:?}");
    assert!(msg.contains("needs `write`"), "{msg}");
    assert!(msg.contains("production"), "{msg}");
    // An unresolved write counts as production too.
    let hidden = run("DO $$ BEGIN DELETE FROM t; END $$").await.unwrap_err();
    assert!(matches!(hidden, AppError::AccessDenied(_)), "{hidden:?}");
    // A level set on the connection itself is not capped.
    level(&app, &profile.id, None, "write");
    let set = run("INSERT INTO prod.public.t VALUES (1)")
        .await
        .unwrap_err();
    assert!(!matches!(set, AppError::AccessDenied(_)), "{set:?}");
}

#[test]
fn query_tool_spec_documents_the_cell_window() {
    let spec = tool_specs()
        .into_iter()
        .find(|tool| tool.name == "execute_readonly_query")
        .expect("execute_readonly_query");
    assert!(spec.description.contains("next_offset"));
    assert!(spec.description.contains("200"));
    assert!(spec.description.contains("4000"));
    let limit = &spec.input_schema["properties"]["cell_char_limit"];
    assert_eq!(limit["maximum"], json!(AGENT_MAX_CELL_CHARS));
    assert_eq!(
        spec.input_schema["properties"]["cell_char_offset"]["maximum"],
        json!(1_000_000)
    );
    let write = tool_specs()
        .into_iter()
        .find(|tool| tool.name == "execute_query")
        .expect("execute_query");
    assert!(write.description.contains("next_offset"));
}

#[test]
fn tool_specs_omit_execute_query_when_nothing_may_write() {
    assert!(tool_specs_for(false)
        .iter()
        .all(|t| t.name != "execute_query"));
    assert!(tool_specs_for(true)
        .iter()
        .any(|t| t.name == "execute_query"));
}

#[test]
fn tool_annotations_follow_the_mcp_spec() {
    for spec in tool_specs() {
        let a = spec.annotations.to_json();
        assert_eq!(a["openWorldHint"], json!(false), "{}", spec.name);
        assert!(!spec.annotations.title.is_empty(), "{}", spec.name);
        match spec.name {
            "execute_query" => {
                assert_eq!(a["readOnlyHint"], json!(false));
                assert_eq!(a["destructiveHint"], json!(true));
                assert_eq!(a["idempotentHint"], json!(false));
            }
            "save_query" => assert_eq!(a["readOnlyHint"], json!(false)),
            _ => {
                assert_eq!(a["readOnlyHint"], json!(true), "{}", spec.name);
                assert_eq!(a["destructiveHint"], json!(false), "{}", spec.name);
            }
        }
    }
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
        query_timeout_secs: None,
        save_password: None,
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

fn level(app: &AppServices, id: &str, database: Option<&str>, level: &str) {
    app.tool_access()
        .set_level(id, database, Some(level))
        .unwrap();
}

#[tokio::test]
async fn mcp_sees_no_connection_by_default() {
    let app = app();
    let saved = save_pg(&app, "prod");
    let tools = app.agent_tools();
    let mcp = tools.list_connections(ToolCaller::Mcp).await.unwrap();
    assert!(mcp.is_empty(), "the MCP default is none");
    let agent = tools.list_connections(ToolCaller::Agent).await.unwrap();
    assert_eq!(agent.len(), 1);
    assert_eq!(agent[0].id, saved.id);
    assert_eq!(agent[0].access, "read");
}

#[tokio::test]
async fn mcp_lists_connections_above_none_with_their_level() {
    let app = app();
    let a = save_pg(&app, "a");
    let b = save_pg(&app, "b");
    let c = save_pg(&app, "c");
    level(&app, &a.id, None, "read");
    // Only one database open: the connection is listed, at its own level.
    level(&app, &c.id, Some("app"), "read");
    let tools = app.agent_tools();
    let mcp = tools.list_connections(ToolCaller::Mcp).await.unwrap();
    let ids: Vec<_> = mcp.iter().map(|c| c.id.as_str()).collect();
    assert_eq!(ids, vec![a.id.as_str(), c.id.as_str()]);
    assert_eq!(mcp[0].access, "read");
    assert_eq!(mcp[1].access, "none");
    // A connection set to none is hidden from the agent too.
    level(&app, &b.id, None, "none");
    let agent = tools.list_connections(ToolCaller::Agent).await.unwrap();
    assert!(agent.iter().all(|c| c.id != b.id));
}

#[tokio::test]
async fn mcp_is_denied_a_hidden_connection_without_its_name() {
    let app = app();
    let saved = save_pg(&app, "secret-prod");
    for id in [saved.id.as_str(), "999", "not-a-number"] {
        let err = app
            .agent_tools()
            .call_tool(
                "execute_readonly_query",
                &json!({ "connection_id": id, "query": "SELECT 1" }),
                ToolCaller::Mcp,
            )
            .await
            .unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("no access"), "{msg}");
        assert!(msg.contains("Settings → MCP → Access"), "{msg}");
        assert!(!msg.contains("secret-prod"), "{msg}");
        assert!(matches!(err, AppError::AccessDenied(_)), "{err:?}");
    }
}

#[tokio::test]
async fn read_level_denies_a_write_and_says_which_level_is_needed() {
    let app = app();
    let saved = save_pg(&app, "prod");
    level(&app, &saved.id, None, "read");
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
    assert!(msg.contains("needs `write`"), "{msg}");
    assert!(msg.contains("MCP has `read`"), "{msg}");
    assert!(msg.contains("connection \"prod\""), "{msg}");
    assert!(msg.contains("Nothing ran"), "{msg}");
    assert!(msg.contains("Ask me for high-risk writes"), "{msg}");
}

#[tokio::test]
async fn write_level_denies_delete_and_drop() {
    let app = app();
    let saved = save_pg(&app, "prod");
    level(&app, &saved.id, None, "write");
    for query in [
        "DROP TABLE t",
        "DELETE FROM t",
        "DELETE FROM t WHERE id = 1",
    ] {
        let err = app
            .agent_tools()
            .call_tool(
                "execute_query",
                &json!({ "connection_id": saved.id, "query": query }),
                ToolCaller::Mcp,
            )
            .await
            .unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("needs `full`"), "{query}: {msg}");
        assert!(msg.contains("has `write`"), "{query}: {msg}");
    }
}

#[tokio::test]
async fn a_database_level_overrides_its_connection() {
    let app = app();
    let saved = save_pg(&app, "prod");
    level(&app, &saved.id, None, "read");
    level(&app, &saved.id, Some("audit"), "none");
    let err = app
        .agent_tools()
        .call_tool(
            "execute_readonly_query",
            &json!({ "connection_id": saved.id, "query": "SELECT 1", "database": "audit" }),
            ToolCaller::Mcp,
        )
        .await
        .unwrap_err();
    assert!(err.to_string().contains("no access"), "{err}");
}

#[tokio::test]
async fn the_agent_reads_by_default_and_is_denied_writes() {
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
    assert!(msg.contains("The built-in agent has `read`"), "{msg}");
    assert!(msg.contains("needs `write`"), "{msg}");
}

#[tokio::test]
async fn mcp_save_query_does_not_overwrite_another_connection() {
    let app = app();
    let scratch = save_pg(&app, "scratch");
    let prod = save_pg(&app, "prod");
    level(&app, &scratch.id, None, "read");
    level(&app, &prod.id, None, "none");
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
        err.to_string().contains("no access"),
        "the existing row's connection must be visible, got {err}"
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
    level(&app, &scratch.id, None, "read");
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

fn save_sqlite(app: &AppServices, path: &std::path::Path) -> crate::domain::ConnectionProfile {
    save_sqlite_as(app, path, "local", false)
}

fn save_sqlite_as(
    app: &AppServices,
    path: &std::path::Path,
    environment: &str,
    read_only: bool,
) -> crate::domain::ConnectionProfile {
    app.profiles()
        .save(SaveConnectionInput {
            id: None,
            name: "s1-sqlite".into(),
            engine: "sqlite".into(),
            color_id: "teal".into(),
            environment: environment.into(),
            read_only,
            notes: None,
            plugin_config: json!({"path": path.to_string_lossy()}),
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
        .unwrap()
}

#[tokio::test]
async fn cell_paging_never_runs_a_write_again() {
    let path = std::env::temp_dir().join(format!(
        "mystique-s1-paging-{}-{}.sqlite",
        std::process::id(),
        uuid::Uuid::new_v4()
    ));
    let app = app();
    app.tool_access().set_default("agent", "full").unwrap();
    let profile = save_sqlite(&app, &path);
    let tools = app.agent_tools();
    let run = |query: String, offset: u64| {
        let tools = tools.clone();
        let id = profile.id.clone();
        async move {
            tools
                .execute_query(
                    &id,
                    &query,
                    None,
                    None,
                    None,
                    "json",
                    types::AgentCellWindow::from_options(Some(offset), Some(10)),
                    ToolCaller::Agent,
                )
                .await
        }
    };
    let count = || async {
        let rows = tools
            .execute_readonly_query(
                &profile.id,
                "SELECT count(*) FROM s1_page",
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
    };
    run("CREATE TABLE s1_page (v TEXT)".into(), 0)
        .await
        .unwrap();

    let insert = format!(
        "INSERT INTO s1_page (v) VALUES ('{}') RETURNING v",
        "abcdefghij".repeat(3)
    );
    let first = run(insert.clone(), 0).await.unwrap();
    assert_eq!(first.rows.unwrap()[0][0]["next_offset"], json!(10));
    let second = run(insert.clone(), 10).await.unwrap();
    assert_eq!(second.rows.unwrap()[0][0]["text"], json!("abcdefghij"));
    assert_eq!(count().await, 1, "paging inserted again");

    // A write with no cached result is refused, not run.
    let other = "INSERT INTO s1_page (v) VALUES ('zzzzzzzzzzzzzzz') RETURNING v".to_string();
    let err = run(other, 10).await.unwrap_err();
    assert!(err.to_string().contains("cell_char_offset"), "{err}");
    assert_eq!(count().await, 1);

    let _ = app
        .connections()
        .close_session(&format!("agent:{}", profile.id))
        .await;
    let _ = std::fs::remove_file(&path);
}
