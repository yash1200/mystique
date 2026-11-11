//! #144: `sample_rows`, `explain_query` and `infer_fields`, run against
//! SQLite and a MongoDB double. Levels and hiding apply as for every tool.

use std::sync::{Arc, Mutex};

use mystique_connector_api::{
    ConnectorResult, ConnectorSession, EngineCapabilities, FieldHistogram, FieldInference,
    InferFieldsInput, PoolStatus, INFER_MAX_SAMPLE,
};
use serde_json::{json, Value as JsonValue};

use super::*;
use crate::connectors::{ConnectionManager, SessionOrigin};
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

fn app() -> Arc<AppServices> {
    Arc::new(AppServices::with_secrets(
        AppDatabase::open_in_memory().unwrap(),
        ConnectionManager::bootstrap(),
        SecretStore::memory(),
    ))
}

fn save(
    app: &AppServices,
    engine: &str,
    environment: &str,
    config: JsonValue,
) -> ConnectionProfile {
    app.profiles()
        .save(SaveConnectionInput {
            id: None,
            name: format!("{engine}-inspect"),
            engine: engine.into(),
            color_id: "teal".into(),
            environment: environment.into(),
            read_only: false,
            notes: None,
            plugin_config: config,
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

/// SQLite file with `t`: 50 rows, row 1 holding a 5 000-character value.
fn sqlite(environment: &str) -> Fixture {
    let path = std::env::temp_dir().join(format!(
        "mystique-g144-{}-{}.sqlite",
        std::process::id(),
        uuid::Uuid::new_v4()
    ));
    let conn = rusqlite::Connection::open(&path).unwrap();
    conn.execute_batch(
        "CREATE TABLE t (id INTEGER PRIMARY KEY, name TEXT, body TEXT);
         WITH RECURSIVE n(i) AS (SELECT 1 UNION ALL SELECT i + 1 FROM n WHERE i < 50)
         INSERT INTO t SELECT i, 'row' || i, 'short' FROM n;",
    )
    .unwrap();
    conn.execute(
        "UPDATE t SET body = ?1 WHERE id = 1",
        [format!("{}SECRET_TAIL", "a".repeat(5_000))],
    )
    .unwrap();
    drop(conn);
    let app = app();
    let profile = save(
        &app,
        "sqlite",
        environment,
        json!({ "path": path.to_string_lossy() }),
    );
    Fixture { app, profile, path }
}

impl Fixture {
    fn level(&self, level: &str) {
        self.app
            .tool_access()
            .set_level(&self.profile.id, None, Some(level))
            .unwrap();
    }

    async fn call(
        &self,
        caller: ToolCaller,
        tool: &str,
        mut args: JsonValue,
    ) -> AppResult<JsonValue> {
        args["connection_id"] = json!(self.profile.id);
        self.app.agent_tools().call_tool(tool, &args, caller).await
    }

    fn count(&self) -> i64 {
        let conn = rusqlite::Connection::open(&self.path).unwrap();
        conn.query_row("SELECT count(*) FROM t", [], |r| r.get(0))
            .unwrap()
    }

    async fn close(&self) {
        for prefix in ["agent:", "mcp:"] {
            let _ = self
                .app
                .connections()
                .close_session(&format!("{prefix}{}", self.profile.id))
                .await;
        }
    }
}

#[test]
fn the_inspection_tools_are_listed_read_only_for_every_caller() {
    for may_write in [false, true] {
        let specs = tool_specs_for(may_write);
        for name in ["sample_rows", "explain_query", "infer_fields"] {
            let spec = specs
                .iter()
                .find(|s| s.name == name)
                .unwrap_or_else(|| panic!("{name} not listed"));
            let a = spec.annotations.to_json();
            assert_eq!(a["readOnlyHint"], json!(true), "{name}");
            assert_eq!(a["destructiveHint"], json!(false), "{name}");
            assert_eq!(
                spec.input_schema["required"][0],
                json!("connection_id"),
                "{name}"
            );
        }
    }
    let sample = tool_specs()
        .into_iter()
        .find(|s| s.name == "sample_rows")
        .unwrap();
    assert_eq!(
        sample.input_schema["properties"]["limit"]["maximum"],
        json!(AGENT_SAMPLE_MAX_ROWS)
    );
    let infer = tool_specs()
        .into_iter()
        .find(|s| s.name == "infer_fields")
        .unwrap();
    assert_eq!(
        infer.input_schema["properties"]["sample"]["maximum"],
        json!(INFER_MAX_SAMPLE)
    );
}

#[tokio::test]
async fn sample_rows_caps_rows_at_20_and_windows_cells() {
    let fx = sqlite("local");
    let out = fx
        .call(
            ToolCaller::Agent,
            "sample_rows",
            json!({ "object": "t", "limit": 500 }),
        )
        .await
        .unwrap();
    assert_eq!(out["rows"].as_array().unwrap().len(), 20);
    assert_eq!(out["rowCount"], json!(20));
    assert_eq!(out["truncated"], json!(true));
    let names: Vec<&str> = out["columns"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|c| c["name"].as_str())
        .collect();
    assert_eq!(names, ["id", "name", "body"]);
    // Row 1 (preview order is the primary key): its body is windowed.
    let body = &out["rows"][0][2];
    assert_eq!(body["text"].as_str().unwrap().chars().count(), 200);
    assert_eq!(body["next_offset"], json!(200));
    assert!(!out.to_string().contains("SECRET_TAIL"));
    assert_eq!(out["rows"][1][2], json!("short"));

    // Default is 10 rows; the next slice of a cell by offset.
    let out = fx
        .call(
            ToolCaller::Agent,
            "sample_rows",
            json!({ "object": "main.t", "cell_char_offset": 4_990, "cell_char_limit": 4_000 }),
        )
        .await
        .unwrap();
    assert_eq!(
        out["rows"].as_array().unwrap().len(),
        AGENT_SAMPLE_DEFAULT_ROWS as usize
    );
    assert_eq!(out["schema"], json!("main"));
    let tail = &out["rows"][0][2];
    assert_eq!(tail["text"], json!("aaaaaaaaaaSECRET_TAIL"));
    assert_eq!(tail["end"], json!(true));
    fx.close().await;
}

#[tokio::test]
async fn explain_of_a_select_returns_a_plan_not_rows() {
    let fx = sqlite("local");
    let out = fx
        .call(
            ToolCaller::Agent,
            "explain_query",
            json!({ "sql": "SELECT name FROM t WHERE id = ?", "params": [7] }),
        )
        .await
        .unwrap();
    assert_eq!(out["analyzed"], json!(false));
    assert!(out.get("rows").is_none(), "{out}");
    let text = out.to_string();
    assert!(!text.contains("row7"), "{text}");
    assert!(
        !out["plan"]["name"].as_str().unwrap_or("").is_empty(),
        "{out}"
    );
    let upper = text.to_ascii_uppercase();
    assert!(upper.contains("SEARCH") || upper.contains("SCAN"), "{text}");
    assert!(
        out["raw"].is_string() || out["raw"]["text"].is_string(),
        "{out}"
    );

    // ANALYZE of a proven read is allowed.
    let analyzed = fx
        .call(
            ToolCaller::Agent,
            "explain_query",
            json!({ "sql": "SELECT count(*) FROM t", "analyze": true }),
        )
        .await
        .unwrap();
    assert_eq!(analyzed["analyzed"], json!(true));
    fx.close().await;
}

#[tokio::test]
async fn explain_of_a_delete_never_deletes() {
    let fx = sqlite("production");
    assert_eq!(fx.count(), 50);
    // A plain explain plans the delete without running it. SQLite has no
    // plan for a bare `DELETE FROM t` (it truncates), so that one answers
    // with an error; neither deletes.
    let _ = fx
        .call(
            ToolCaller::Agent,
            "explain_query",
            json!({ "sql": "DELETE FROM t" }),
        )
        .await;
    assert_eq!(fx.count(), 50);
    let out = fx
        .call(
            ToolCaller::Agent,
            "explain_query",
            json!({ "sql": "DELETE FROM t WHERE id > 0" }),
        )
        .await
        .unwrap();
    assert_eq!(out["analyzed"], json!(false));
    assert_eq!(fx.count(), 50);

    // ANALYZE of a write is refused before anything reaches the database,
    // asked for by the flag or by the text.
    for args in [
        json!({ "sql": "DELETE FROM t", "analyze": true }),
        json!({ "sql": "EXPLAIN ANALYZE DELETE FROM t" }),
        json!({ "sql": "EXPLAIN QUERY PLAN DELETE FROM t", "analyze": true }),
    ] {
        let err = fx
            .call(ToolCaller::Agent, "explain_query", args.clone())
            .await
            .unwrap_err();
        assert!(matches!(err, AppError::AccessDenied(_)), "{args}: {err:?}");
        assert!(err.to_string().contains("Nothing ran"), "{err}");
        assert_eq!(fx.count(), 50, "{args}");
    }
    // Even with `full`, the tool is read-only.
    fx.level("full");
    let err = fx
        .call(
            ToolCaller::Agent,
            "explain_query",
            json!({ "sql": "DELETE FROM t", "analyze": true }),
        )
        .await
        .unwrap_err();
    assert!(err.to_string().contains("ANALYZE"), "{err}");
    // A second statement is not smuggled in behind the first.
    let _ = fx
        .call(
            ToolCaller::Agent,
            "explain_query",
            json!({ "sql": "SELECT 1; DELETE FROM t" }),
        )
        .await
        .unwrap_err();
    assert_eq!(fx.count(), 50);
    fx.close().await;
}

#[tokio::test]
async fn every_inspection_tool_is_denied_on_a_hidden_connection() {
    let fx = sqlite("local");
    let calls = [
        ("sample_rows", json!({ "object": "t" })),
        ("explain_query", json!({ "sql": "SELECT 1" })),
        ("infer_fields", json!({ "collection": "t" })),
    ];
    // MCP sees nothing by default.
    for (tool, args) in &calls {
        let err = fx
            .call(ToolCaller::Mcp, tool, args.clone())
            .await
            .unwrap_err();
        assert!(matches!(err, AppError::AccessDenied(_)), "{tool}: {err:?}");
        assert!(!err.to_string().contains("sqlite-inspect"), "{tool}: {err}");
    }
    // `none` on the connection hides it from the agent too.
    fx.level("none");
    for (tool, args) in &calls {
        let err = fx
            .call(ToolCaller::Agent, tool, args.clone())
            .await
            .unwrap_err();
        assert!(matches!(err, AppError::AccessDenied(_)), "{tool}: {err:?}");
    }
    // `read` lets MCP sample; a database set to `none` stays closed.
    fx.level("read");
    fx.call(
        ToolCaller::Mcp,
        "sample_rows",
        json!({ "object": "t", "limit": 1 }),
    )
    .await
    .unwrap();
    fx.app
        .tool_access()
        .set_level(&fx.profile.id, Some("audit"), Some("none"))
        .unwrap();
    let err = fx
        .call(
            ToolCaller::Mcp,
            "sample_rows",
            json!({ "object": "t", "database": "audit" }),
        )
        .await
        .unwrap_err();
    assert!(matches!(err, AppError::AccessDenied(_)), "{err:?}");
    // infer_fields on a SQL connection says what to use instead.
    let err = fx
        .call(
            ToolCaller::Mcp,
            "infer_fields",
            json!({ "collection": "t" }),
        )
        .await
        .unwrap_err();
    assert!(err.to_string().contains("describe_object"), "{err}");
    fx.close().await;
}

/// Collection, database and sample size of one `infer_fields` call.
type Asked = (String, Option<String>, u32);

/// A MongoDB session double: answers `infer_fields` from a fixed histogram
/// and records what it was asked.
#[derive(Clone, Default)]
struct InferProbe {
    asked: Arc<Mutex<Vec<Asked>>>,
}

#[async_trait::async_trait]
impl ConnectorSession for InferProbe {
    fn engine_id(&self) -> &str {
        "mongodb"
    }

    fn capabilities(&self) -> EngineCapabilities {
        mystique_plugin_mongodb::default_capabilities()
    }

    async fn ping(&self) -> ConnectorResult<()> {
        Ok(())
    }

    fn pool_status(&self) -> Option<PoolStatus> {
        None
    }

    async fn close(&self) -> ConnectorResult<()> {
        Ok(())
    }

    async fn infer_fields(&self, input: &InferFieldsInput) -> ConnectorResult<FieldInference> {
        self.asked.lock().unwrap().push((
            input.collection.clone(),
            input.database.clone(),
            input.sample,
        ));
        let mut h = FieldHistogram::default();
        h.begin_document();
        h.observe("_id", "ObjectId");
        h.observe("address", "object");
        h.observe("address.city", "string");
        h.observe("tags", "array");
        h.observe("tags[]", "string");
        h.begin_document();
        h.observe("_id", "ObjectId");
        h.observe("tags", "array");
        h.observe("tags[]", "int32");
        Ok(h.finish("mongodb", &input.collection, false, 1))
    }
}

#[tokio::test]
async fn infer_fields_returns_the_histogram_and_clamps_the_sample() {
    let app = app();
    for engine in ["mongodb", "documentdb"] {
        let profile = save(
            &app,
            engine,
            "local",
            json!({ "uri": "mongodb://127.0.0.1:1/app" }),
        );
        let probe = InferProbe::default();
        app.connections()
            .insert_test_session(
                &format!("agent:{}", profile.id),
                Arc::new(probe.clone()),
                "local",
                SessionOrigin::Agent,
            )
            .await;
        let out = app
            .agent_tools()
            .call_tool(
                "infer_fields",
                &json!({ "connection_id": profile.id, "collection": "people", "sample": 100_000 }),
                ToolCaller::Agent,
            )
            .await
            .unwrap();
        assert_eq!(out["sampled"], json!(2), "{engine}");
        assert_eq!(out["collection"], json!("people"));
        let paths: Vec<&str> = out["fields"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|f| f["path"].as_str())
            .collect();
        assert_eq!(paths, ["_id", "address", "address.city", "tags", "tags[]"]);
        let tags = &out["fields"][4];
        assert_eq!(tags["present"], json!(2));
        assert_eq!(
            tags["types"],
            json!([{ "type": "string", "count": 1 }, { "type": "int32", "count": 1 }])
        );
        assert_eq!(
            probe.asked.lock().unwrap().as_slice(),
            [("people".to_string(), None, INFER_MAX_SAMPLE)]
        );

        // MCP at `none` never reaches the session.
        let err = app
            .agent_tools()
            .call_tool(
                "infer_fields",
                &json!({ "connection_id": profile.id, "collection": "people" }),
                ToolCaller::Mcp,
            )
            .await
            .unwrap_err();
        assert!(matches!(err, AppError::AccessDenied(_)), "{err:?}");
        assert_eq!(probe.asked.lock().unwrap().len(), 1);
    }
}
