use std::sync::Arc;

use mystique_core::{AgentMessage, AppDatabase, AppServices};
use rig::completion::Message;

use crate::providers::AgentProvider;
use crate::runtime::chat::history_from;
use crate::runtime::draft::extract_draft;
use crate::runtime::prompt::workspace_context;
use crate::secrets::crypto::{decrypt, encrypt};
use crate::secrets::SecretStore;
use crate::tools::{extract_leaked_tool_calls, strip_leaked_tool_calls};

fn prior(role: &str, content: &str) -> AgentMessage {
    AgentMessage {
        id: "m1".into(),
        chat_id: "c1".into(),
        role: role.into(),
        content: content.into(),
        tool_name: None,
        draft_body: None,
        created_at: String::new(),
    }
}

#[test]
fn context_includes_workspace_fields() {
    let c = workspace_context("prod", "postgres", "app", "select 1");
    assert!(c.contains("prod"));
    assert!(c.contains("postgres"));
    assert!(c.contains("app"));
    assert!(c.contains("select 1"));
    assert!(c.contains("```sql"));
}

#[test]
fn context_marks_empty_editor_and_default_catalog() {
    let c = workspace_context("local", "sqlite", "", "");
    assert!(c.contains("(empty)"));
    assert!(c.contains("(default catalog)"));
}

#[test]
fn context_uses_mql_fence_for_mongo() {
    let c = workspace_context("atlas", "mongodb", "shop", "db.x.find()");
    assert!(c.contains("```mql"));
}

#[test]
fn history_puts_workspace_context_before_current_turn() {
    let ctx = workspace_context("prod", "postgres", "app", "select 1");
    let history = history_from(&[prior("user", "hello")], &ctx);
    assert_eq!(history.len(), 2);
    assert_eq!(history[0].rag_text().as_deref(), Some("hello"));
    match history.last() {
        Some(Message::User { .. }) => {
            assert_eq!(history[1].rag_text().as_deref(), Some(ctx.as_str()));
        }
        other => panic!("expected trailing user context, got {other:?}"),
    }
}

#[test]
fn prefers_sql_fence() {
    let text = "Here you go:\n```sql\nselect 1\n```\n";
    assert_eq!(extract_draft(text).as_deref(), Some("select 1"));
}

#[test]
fn accepts_mql_fence() {
    let text = "```mql\ndb.users.find({})\n```";
    assert_eq!(extract_draft(text).as_deref(), Some("db.users.find({})"));
}

#[test]
fn encrypt_decrypt_roundtrip() {
    let blob = encrypt("sk-test-key").unwrap();
    assert!(blob.starts_with("v1:"));
    assert_ne!(blob, "sk-test-key");
    assert_eq!(decrypt(&blob).unwrap(), "sk-test-key");
}

#[test]
fn encrypt_uses_fresh_nonce() {
    let a = encrypt("same").unwrap();
    let b = encrypt("same").unwrap();
    assert_ne!(a, b);
}

#[test]
fn decrypt_rejects_garbage() {
    assert!(decrypt("nope").is_err());
    assert!(decrypt("v1:%%%%").is_err());
}

#[test]
fn sqlite_roundtrip_and_status() {
    let app = Arc::new(AppServices::new(AppDatabase::open_in_memory().unwrap()));
    let store = SecretStore::new(app.clone());
    assert!(
        !store
            .status()
            .iter()
            .find(|s| s.provider == "openai")
            .unwrap()
            .has_key
    );
    store.set(AgentProvider::Openai, " sk-live ").unwrap();
    assert_eq!(
        store.get(AgentProvider::Openai).unwrap().as_deref(),
        Some("sk-live")
    );
    let row = app.ai_secrets().get("openai").unwrap().expect("row");
    assert!(row.ciphertext.starts_with("v1:"));
    assert!(!row.ciphertext.contains("sk-live"));
    store.delete(AgentProvider::Openai).unwrap();
    assert!(store.get(AgentProvider::Openai).unwrap().is_none());
}

#[test]
fn extracts_ollama_style_name_arguments() {
    let text = r#"{"name": "list_objects", "arguments": {"database": "qna_common_db"}}"#;
    let calls = extract_leaked_tool_calls(text);
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].name, "list_objects");
    assert_eq!(calls[0].args["database"], "qna_common_db");
}

#[test]
fn extracts_empty_arguments_object() {
    let text = r#"{"name": "list_saved_queries", "arguments": {}}"#;
    let calls = extract_leaked_tool_calls(text);
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].name, "list_saved_queries");
}

#[test]
fn extracts_tool_json_mixed_with_prose() {
    let text = "Hi\n{\"name\": \"list_objects\", \"arguments\": {\"database\": \"qna_common_db\"}}\nCan you write a query?\n{\"name\": \"list_saved_queries\", \"arguments\": {}}";
    let calls = extract_leaked_tool_calls(text);
    assert_eq!(calls.len(), 2);
    assert_eq!(calls[0].name, "list_objects");
    assert_eq!(calls[1].name, "list_saved_queries");
}

#[test]
fn ignores_unrelated_json() {
    let text = r#"{"name": "users", "columns": ["id"]}"#;
    assert!(extract_leaked_tool_calls(text).is_empty());
}

#[test]
fn strips_leaked_json_leaving_prose() {
    let text = "Hi\n{\"name\": \"list_objects\", \"arguments\": {\"database\": \"qna_common_db\"}}\nCan you write a query?";
    let cleaned = strip_leaked_tool_calls(text);
    assert!(cleaned.contains("Hi"));
    assert!(cleaned.contains("Can you write a query?"));
    assert!(!cleaned.contains("list_objects"));
}
