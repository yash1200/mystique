//! Read and upsert Mystique into each client's MCP config format.

use std::fs;
use std::path::Path;

use mystique_core::{AppError, AppResult};
use serde_json::{json, Map, Value};
use toml_edit::{value, DocumentMut, Item, Table};

use super::{McpEndpoint, SERVER_NAME};

#[derive(Debug, Clone, Copy)]
pub enum FileKind {
    /// `{ mcpServers.name: { url, headers } }` — Cursor, Windsurf.
    CursorJson,
    /// `{ mcpServers.name: { type: "http", url, headers } }` — Claude Code / Desktop.
    ClaudeHttp,
    /// `{ mcpServers.name: { serverUrl, headers } }` — Antigravity / agy.
    AgyJson,
    /// `{ mcpServers.name: { httpUrl, headers } }` — Gemini CLI.
    GeminiJson,
    /// `{ mcp: { name: { type: "remote", url, headers, enabled } } }` — OpenCode.
    OpenCodeJson,
    /// `{ servers.name: { type: "http", url, headers } }` — VS Code.
    VsCodeJson,
    /// `[mcp_servers.name]` + `http_headers`.
    CodexToml,
    /// `[mcp_servers.name]` + `[mcp_servers.name.headers]`.
    GrokToml,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EntryState {
    pub installed: bool,
    pub stale: bool,
}

impl EntryState {
    pub const MISSING: Self = Self {
        installed: false,
        stale: false,
    };
}

pub fn read_state(path: &Path, kind: FileKind, endpoint: &McpEndpoint) -> EntryState {
    let Ok(raw) = fs::read_to_string(path) else {
        return EntryState::MISSING;
    };
    match kind {
        FileKind::CodexToml | FileKind::GrokToml => toml_state(&raw, kind, endpoint),
        _ => json_state(&raw, kind, endpoint),
    }
}

pub fn write_entry(path: &Path, kind: FileKind, endpoint: &McpEndpoint) -> AppResult<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    match kind {
        FileKind::CodexToml | FileKind::GrokToml => write_toml(path, kind, endpoint),
        _ => write_json(path, kind, endpoint),
    }
}

fn json_state(raw: &str, kind: FileKind, endpoint: &McpEndpoint) -> EntryState {
    let Ok(value) = serde_json::from_str::<Value>(raw) else {
        return EntryState::MISSING;
    };
    let Some(entry) = json_entry(&value, kind) else {
        return EntryState::MISSING;
    };
    let url = json_url(entry, kind);
    let token = json_bearer(entry);
    EntryState {
        installed: true,
        stale: !urls_match(url.as_deref(), &endpoint.url)
            || !tokens_match(token.as_deref(), &endpoint.token),
    }
}

fn json_entry(root: &Value, kind: FileKind) -> Option<&Value> {
    let group = match kind {
        FileKind::OpenCodeJson => "mcp",
        FileKind::VsCodeJson => "servers",
        _ => "mcpServers",
    };
    root.get(group)?.get(SERVER_NAME)
}

fn json_url(entry: &Value, kind: FileKind) -> Option<String> {
    let keys: &[&str] = match kind {
        FileKind::AgyJson => &["serverUrl", "url"],
        FileKind::GeminiJson => &["httpUrl", "url"],
        _ => &["url", "httpUrl", "serverUrl"],
    };
    keys.iter()
        .find_map(|k| entry.get(*k).and_then(Value::as_str))
        .map(str::to_string)
}

fn json_bearer(entry: &Value) -> Option<String> {
    entry
        .get("headers")
        .and_then(Value::as_object)
        .and_then(|h| {
            h.iter().find_map(|(k, v)| {
                if k.eq_ignore_ascii_case("authorization") {
                    v.as_str().map(strip_bearer)
                } else {
                    None
                }
            })
        })
}

fn write_json(path: &Path, kind: FileKind, endpoint: &McpEndpoint) -> AppResult<()> {
    let mut root = read_json_object(path)?;
    let group = match kind {
        FileKind::OpenCodeJson => "mcp",
        FileKind::VsCodeJson => "servers",
        _ => "mcpServers",
    };
    let group_map = object_field(&mut root, group)?;
    group_map.insert(SERVER_NAME.to_string(), json_payload(kind, endpoint));
    write_atomic(path, &pretty_json(&root)?)
}

fn json_payload(kind: FileKind, endpoint: &McpEndpoint) -> Value {
    let headers = json!({ "Authorization": format!("Bearer {}", endpoint.token) });
    match kind {
        FileKind::CursorJson => json!({
            "url": endpoint.url,
            "headers": headers,
        }),
        FileKind::ClaudeHttp => json!({
            "type": "http",
            "url": endpoint.url,
            "headers": headers,
        }),
        FileKind::AgyJson => json!({
            "serverUrl": endpoint.url,
            "headers": headers,
        }),
        FileKind::GeminiJson => json!({
            "httpUrl": endpoint.url,
            "headers": headers,
        }),
        FileKind::OpenCodeJson => json!({
            "type": "remote",
            "url": endpoint.url,
            "enabled": true,
            "headers": headers,
        }),
        FileKind::VsCodeJson => json!({
            "type": "http",
            "url": endpoint.url,
            "headers": headers,
        }),
        FileKind::CodexToml | FileKind::GrokToml => unreachable!("toml kinds use write_toml"),
    }
}

fn read_json_object(path: &Path) -> AppResult<Map<String, Value>> {
    if !path.exists() {
        return Ok(Map::new());
    }
    let raw = fs::read_to_string(path)?;
    if raw.trim().is_empty() {
        return Ok(Map::new());
    }
    let value: Value = serde_json::from_str(&raw)
        .map_err(|e| AppError::Invalid(format!("Invalid JSON in {}: {e}", path.display())))?;
    match value {
        Value::Object(map) => Ok(map),
        _ => Err(AppError::Invalid(format!(
            "{} must be a JSON object",
            path.display()
        ))),
    }
}

fn object_field<'a>(
    root: &'a mut Map<String, Value>,
    key: &str,
) -> AppResult<&'a mut Map<String, Value>> {
    let needs_object = !root.get(key).is_some_and(Value::is_object);
    if needs_object {
        root.insert(key.to_string(), Value::Object(Map::new()));
    }
    root.get_mut(key)
        .and_then(Value::as_object_mut)
        .ok_or_else(|| AppError::Internal(format!("missing JSON object `{key}`")))
}

fn pretty_json(map: &Map<String, Value>) -> AppResult<String> {
    let mut out = serde_json::to_string_pretty(&Value::Object(map.clone()))?;
    if !out.ends_with('\n') {
        out.push('\n');
    }
    Ok(out)
}

fn toml_state(raw: &str, kind: FileKind, endpoint: &McpEndpoint) -> EntryState {
    let Ok(doc) = raw.parse::<DocumentMut>() else {
        return EntryState::MISSING;
    };
    let Some(server) = doc
        .get("mcp_servers")
        .and_then(Item::as_table)
        .and_then(|t| t.get(SERVER_NAME))
        .and_then(Item::as_table)
    else {
        return EntryState::MISSING;
    };
    let url = server.get("url").and_then(Item::as_str);
    let header_key = match kind {
        FileKind::CodexToml => "http_headers",
        _ => "headers",
    };
    let token = table_header(server, header_key)
        .as_deref()
        .map(strip_bearer);
    EntryState {
        installed: true,
        stale: !urls_match(url, &endpoint.url) || !tokens_match(token.as_deref(), &endpoint.token),
    }
}

fn write_toml(path: &Path, kind: FileKind, endpoint: &McpEndpoint) -> AppResult<()> {
    let mut doc = read_toml_doc(path)?;
    let servers = doc["mcp_servers"].or_insert(Item::Table(Table::new()));
    let server = match servers.as_table_mut() {
        Some(table) => table[SERVER_NAME].or_insert(Item::Table(Table::new())),
        None => {
            return Err(AppError::Invalid(format!(
                "{}: mcp_servers must be a table",
                path.display()
            )));
        }
    };
    let Some(table) = server.as_table_mut() else {
        return Err(AppError::Invalid(format!(
            "{}: mcp_servers.{SERVER_NAME} must be a table",
            path.display()
        )));
    };

    // Codex rejects mixed stdio + HTTP keys on one server.
    for key in ["command", "args", "env", "cwd"] {
        table.remove(key);
    }
    table["url"] = value(endpoint.url.as_str());
    let header_key = match kind {
        FileKind::CodexToml => "http_headers",
        _ => "headers",
    };
    table[header_key]["Authorization"] = value(format!("Bearer {}", endpoint.token));
    write_atomic(path, &doc.to_string())
}

fn table_header(table: &Table, key: &str) -> Option<String> {
    let headers = table.get(key)?;
    if let Some(inner) = headers.as_table() {
        return inner
            .get("Authorization")
            .or_else(|| inner.get("authorization"))
            .and_then(Item::as_str)
            .map(str::to_string);
    }
    if let Some(inner) = headers.as_inline_table() {
        return inner
            .get("Authorization")
            .or_else(|| inner.get("authorization"))
            .and_then(|v| v.as_str())
            .map(str::to_string);
    }
    None
}

fn read_toml_doc(path: &Path) -> AppResult<DocumentMut> {
    if !path.exists() {
        return Ok(DocumentMut::new());
    }
    let raw = fs::read_to_string(path)?;
    if raw.trim().is_empty() {
        return Ok(DocumentMut::new());
    }
    raw.parse::<DocumentMut>()
        .map_err(|e| AppError::Invalid(format!("Invalid TOML in {}: {e}", path.display())))
}

fn write_atomic(path: &Path, contents: &str) -> AppResult<()> {
    let tmp = path.with_extension("tmp");
    fs::write(&tmp, contents)?;
    restrict_owner_rw(path);
    restrict_owner_rw(&tmp);
    fs::rename(&tmp, path)?;
    restrict_owner_rw(path);
    Ok(())
}

fn restrict_owner_rw(path: &Path) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(path, fs::Permissions::from_mode(0o600));
    }
    #[cfg(not(unix))]
    {
        let _ = path;
    }
}

fn urls_match(found: Option<&str>, expected: &str) -> bool {
    match found {
        Some(url) => normalize_url(url) == normalize_url(expected),
        None => false,
    }
}

fn tokens_match(found: Option<&str>, expected: &str) -> bool {
    match found {
        Some(token) => token == expected,
        None => false,
    }
}

fn normalize_url(url: &str) -> String {
    url.trim().trim_end_matches('/').to_string()
}

fn strip_bearer(value: &str) -> String {
    let trimmed = value.trim();
    trimmed
        .strip_prefix("Bearer ")
        .or_else(|| trimmed.strip_prefix("bearer "))
        .unwrap_or(trimmed)
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ep() -> McpEndpoint {
        McpEndpoint {
            url: "http://127.0.0.1:17321/mcp".into(),
            token: "tok-1".into(),
        }
    }

    #[test]
    fn write_claude_http_then_read_matches() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("claude.json");
        fs::write(&path, r#"{"numStartups":1}"#).unwrap();
        write_entry(&path, FileKind::ClaudeHttp, &ep()).unwrap();
        let state = read_state(&path, FileKind::ClaudeHttp, &ep());
        assert!(state.installed);
        assert!(!state.stale);
        let parsed: Value = serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(parsed["numStartups"], 1);
        assert_eq!(parsed["mcpServers"]["mystique"]["type"], "http");
    }

    #[test]
    fn stale_when_token_differs() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("mcp.json");
        write_entry(&path, FileKind::CursorJson, &ep()).unwrap();
        let other = McpEndpoint {
            token: "other".into(),
            ..ep()
        };
        let state = read_state(&path, FileKind::CursorJson, &other);
        assert!(state.installed);
        assert!(state.stale);
    }

    #[test]
    fn grok_toml_preserves_other_tables() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        fs::write(&path, "[ui]\ntheme = \"dark\"\n").unwrap();
        write_entry(&path, FileKind::GrokToml, &ep()).unwrap();
        let raw = fs::read_to_string(&path).unwrap();
        assert!(raw.contains("[ui]"));
        assert!(raw.contains("[mcp_servers.mystique]"));
        assert!(raw.contains("Bearer tok-1"));
        let state = read_state(&path, FileKind::GrokToml, &ep());
        assert!(state.installed && !state.stale);
    }

    #[test]
    fn codex_toml_replaces_stdio_keys() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        fs::write(
            &path,
            "[mcp_servers.mystique]\ncommand = \"npx\"\nargs = [\"-y\", \"x\"]\n",
        )
        .unwrap();
        write_entry(&path, FileKind::CodexToml, &ep()).unwrap();
        let raw = fs::read_to_string(&path).unwrap();
        assert!(!raw.contains("command"));
        assert!(raw.contains("http_headers"));
        assert!(read_state(&path, FileKind::CodexToml, &ep()).installed);
    }

    #[test]
    fn agy_uses_server_url() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("mcp_config.json");
        write_entry(&path, FileKind::AgyJson, &ep()).unwrap();
        let parsed: Value = serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(
            parsed["mcpServers"]["mystique"]["serverUrl"],
            "http://127.0.0.1:17321/mcp"
        );
    }

    #[test]
    fn opencode_uses_remote_type() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("opencode.json");
        write_entry(&path, FileKind::OpenCodeJson, &ep()).unwrap();
        let parsed: Value = serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(parsed["mcp"]["mystique"]["type"], "remote");
        assert_eq!(parsed["mcp"]["mystique"]["enabled"], true);
    }
}
