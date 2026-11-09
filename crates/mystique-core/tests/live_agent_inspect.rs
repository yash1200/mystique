//! Live checks for `sample_rows` and `explain_query` (#144).
//!
//! `cargo test -p mystique-core --test live_agent_inspect -- --ignored`
//!
//! Postgres: local `postgres` database as `$USER`. MySQL: `admin`/`123`,
//! database `mystique`. Scratch tables start with `mq_gb_` and are dropped.

use std::sync::Arc;

use mystique_core::{
    AppDatabase, AppServices, ConnectionManager, ConnectionProfile, SaveConnectionInput,
    SecretStore, SshConfig, ToolCaller,
};
use serde_json::{json, Value as JsonValue};

fn app() -> Arc<AppServices> {
    Arc::new(AppServices::with_secrets(
        AppDatabase::open_in_memory().unwrap(),
        ConnectionManager::bootstrap(),
        SecretStore::memory(),
    ))
}

fn save(app: &AppServices, engine: &str) -> ConnectionProfile {
    let (config, password) = match engine {
        "mysql" => (
            json!({ "host": "127.0.0.1", "port": 3306, "database": "mystique", "username": "admin" }),
            Some("123".to_string()),
        ),
        _ => (
            json!({
                "host": "127.0.0.1",
                "port": 5432,
                "database": "postgres",
                "username": std::env::var("USER").unwrap_or_else(|_| "postgres".into())
            }),
            None,
        ),
    };
    app.profiles()
        .save(SaveConnectionInput {
            id: None,
            name: format!("mq_gb_{engine}"),
            engine: engine.into(),
            color_id: "teal".into(),
            environment: "production".into(),
            read_only: false,
            notes: None,
            plugin_config: config,
            password,
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
}

struct Live {
    app: Arc<AppServices>,
    id: String,
}

impl Live {
    async fn tool(
        &self,
        caller: ToolCaller,
        name: &str,
        mut args: JsonValue,
    ) -> Result<JsonValue, String> {
        args["connection_id"] = json!(self.id);
        self.app
            .agent_tools()
            .call_tool(name, &args, caller)
            .await
            .map_err(|e| e.to_string())
    }

    async fn write(&self, query: &str) {
        self.tool(ToolCaller::Mcp, "execute_query", json!({ "query": query }))
            .await
            .unwrap_or_else(|e| panic!("{query}: {e}"));
    }

    fn level(&self, level: &str) {
        self.app
            .tool_access()
            .set_level(&self.id, None, Some(level))
            .unwrap();
    }

    async fn count(&self) -> i64 {
        let out = self
            .tool(
                ToolCaller::Mcp,
                "execute_readonly_query",
                json!({ "query": "SELECT count(*) FROM mq_gb_sample" }),
            )
            .await
            .unwrap();
        let cell = &out["rows"][0][0];
        cell.as_i64()
            .or_else(|| cell.as_str().and_then(|s| s.parse().ok()))
            .unwrap_or_else(|| panic!("count: {out}"))
    }
}

async fn inspect_on(engine: &str, object: &str, placeholder: &str) {
    let app = app();
    let p = save(&app, engine);
    let live = Live {
        app: Arc::clone(&app),
        id: p.id.clone(),
    };
    live.level("full");
    live.write("DROP TABLE IF EXISTS mq_gb_sample").await;
    live.write("CREATE TABLE mq_gb_sample (id INT PRIMARY KEY, name VARCHAR(20), body TEXT)")
        .await;
    let values: Vec<String> = (1..=30)
        .map(|i| format!("({i}, 'row{i}', 'short')"))
        .collect();
    live.write(&format!(
        "INSERT INTO mq_gb_sample VALUES {}",
        values.join(", ")
    ))
    .await;
    live.write("UPDATE mq_gb_sample SET body = REPEAT('a', 5000) WHERE id = 1")
        .await;
    live.level("read");

    // At most 20 rows, cells windowed.
    let out = live
        .tool(
            ToolCaller::Mcp,
            "sample_rows",
            json!({ "object": object, "limit": 100 }),
        )
        .await
        .unwrap();
    assert_eq!(out["rows"].as_array().unwrap().len(), 20, "{out}");
    let body = out["rows"]
        .as_array()
        .unwrap()
        .iter()
        .find_map(|row| row[2].get("next_offset").map(|_| row[2].clone()))
        .unwrap_or_else(|| panic!("no windowed cell: {out}"));
    assert_eq!(body["text"].as_str().unwrap().chars().count(), 200);
    assert_eq!(body["next_offset"], json!(200));

    // A SELECT's plan, not its rows. (A primary-key equality is avoided:
    // MySQL reads such a const row while planning and prints its value in
    // the plan's query text.)
    let plan = live
        .tool(
            ToolCaller::Mcp,
            "explain_query",
            json!({
                "sql": format!("SELECT name, body FROM mq_gb_sample WHERE id > {placeholder}"),
                "params": [25]
            }),
        )
        .await
        .unwrap();
    assert_eq!(plan["analyzed"], json!(false));
    let text = plan.to_string();
    assert!(!text.contains("row26") && !text.contains("short"), "{plan}");
    assert!(plan.to_string().contains("mq_gb_sample"), "{plan}");
    let analyzed = live
        .tool(
            ToolCaller::Mcp,
            "explain_query",
            json!({ "sql": "SELECT count(*) FROM mq_gb_sample", "analyze": true }),
        )
        .await
        .unwrap();
    assert_eq!(analyzed["analyzed"], json!(true), "{analyzed}");

    // A delete is planned, never run; ANALYZE of it is refused.
    let plain = live
        .tool(
            ToolCaller::Mcp,
            "explain_query",
            json!({ "sql": "DELETE FROM mq_gb_sample" }),
        )
        .await
        .unwrap();
    assert_eq!(plain["analyzed"], json!(false), "{plain}");
    assert_eq!(live.count().await, 30);
    for args in [
        json!({ "sql": "DELETE FROM mq_gb_sample", "analyze": true }),
        json!({ "sql": "EXPLAIN ANALYZE DELETE FROM mq_gb_sample" }),
    ] {
        let err = live
            .tool(ToolCaller::Mcp, "explain_query", args.clone())
            .await
            .unwrap_err();
        assert!(err.contains("Nothing ran"), "{args}: {err}");
        assert_eq!(live.count().await, 30, "{args}");
    }

    // The agent reads the same way (its default level is read).
    let agent = live
        .tool(
            ToolCaller::Agent,
            "sample_rows",
            json!({ "object": object, "limit": 3 }),
        )
        .await
        .unwrap();
    assert_eq!(agent["rowCount"], json!(3), "{agent}");

    live.level("full");
    live.write("DROP TABLE mq_gb_sample").await;
    for prefix in ["mcp:", "agent:"] {
        let _ = app
            .connections()
            .close_session(&format!("{prefix}{}", p.id))
            .await;
    }
}

#[tokio::test]
#[ignore = "needs local Postgres"]
async fn postgres_sample_and_explain() {
    inspect_on("postgres", "public.mq_gb_sample", "$1").await;
}

#[tokio::test]
#[ignore = "needs local MySQL"]
async fn mysql_sample_and_explain() {
    inspect_on("mysql", "mq_gb_sample", "?").await;
}
