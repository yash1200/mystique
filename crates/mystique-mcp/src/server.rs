//! Minimal MCP Streamable HTTP (JSON-RPC 2.0) on 127.0.0.1.

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;

use axum::body::Bytes;
use axum::extract::{Query, State};
use axum::http::{header, HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::Router;
use mystique_core::{tool_specs, AgentTools, AppError, AppResult};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value as JsonValue};
use tokio::net::TcpListener;
use tokio::sync::{oneshot, Mutex};
use uuid::Uuid;

pub const DEFAULT_MCP_PORT: u16 = 17321;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpStatus {
    pub enabled: bool,
    pub running: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub port: Option<u16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub token: Option<String>,
}

impl McpStatus {
    pub fn stopped(enabled: bool) -> Self {
        Self {
            enabled,
            running: false,
            port: None,
            url: None,
            token: None,
        }
    }
}

struct Running {
    port: u16,
    token: String,
    shutdown: Option<oneshot::Sender<()>>,
}

#[derive(Clone, Default)]
pub struct McpController {
    inner: Arc<Mutex<Option<Running>>>,
}

impl McpController {
    pub fn new() -> Self {
        Self::default()
    }

    pub async fn status(&self, enabled: bool) -> McpStatus {
        let guard = self.inner.lock().await;
        match guard.as_ref() {
            Some(r) => McpStatus {
                enabled,
                running: true,
                port: Some(r.port),
                url: Some(format!("http://127.0.0.1:{}/mcp", r.port)),
                token: Some(r.token.clone()),
            },
            None => McpStatus::stopped(enabled),
        }
    }

    pub async fn start(&self, tools: AgentTools, port: u16, token: String) -> AppResult<McpStatus> {
        self.stop().await;
        let bind_port = if port == 0 { DEFAULT_MCP_PORT } else { port };
        let listener = match TcpListener::bind(SocketAddr::from(([127, 0, 0, 1], bind_port))).await
        {
            Ok(l) => l,
            Err(_) if bind_port != 0 => TcpListener::bind(SocketAddr::from(([127, 0, 0, 1], 0)))
                .await
                .map_err(|e| AppError::Internal(format!("MCP bind failed: {e}")))?,
            Err(e) => {
                return Err(AppError::Internal(format!("MCP bind failed: {e}")));
            }
        };
        let actual = listener
            .local_addr()
            .map_err(|e| AppError::Internal(e.to_string()))?
            .port();

        let state = HttpState {
            tools,
            token: token.clone(),
        };
        let app = Router::new()
            .route("/mcp", post(mcp_post).get(mcp_get).options(mcp_options))
            .route("/health", get(health))
            .with_state(state);

        let (tx, rx) = oneshot::channel::<()>();
        tokio::spawn(async move {
            let server = axum::serve(listener, app).with_graceful_shutdown(async {
                let _ = rx.await;
            });
            let _ = server.await;
        });

        let mut guard = self.inner.lock().await;
        *guard = Some(Running {
            port: actual,
            token: token.clone(),
            shutdown: Some(tx),
        });
        Ok(McpStatus {
            enabled: true,
            running: true,
            port: Some(actual),
            url: Some(format!("http://127.0.0.1:{actual}/mcp")),
            token: Some(token),
        })
    }

    pub async fn stop(&self) {
        let mut guard = self.inner.lock().await;
        if let Some(mut running) = guard.take() {
            if let Some(tx) = running.shutdown.take() {
                let _ = tx.send(());
            }
        }
    }
}

#[derive(Clone)]
struct HttpState {
    tools: AgentTools,
    token: String,
}

async fn health() -> impl IntoResponse {
    cors(StatusCode::OK, "ok")
}

async fn mcp_options() -> impl IntoResponse {
    cors(StatusCode::NO_CONTENT, "")
}

async fn mcp_get() -> impl IntoResponse {
    cors(
        StatusCode::OK,
        json!({
            "ok": true,
            "server": "mystique-db",
            "transport": "streamable-http",
            "hint": "POST JSON-RPC to this URL"
        })
        .to_string(),
    )
}

fn authorized(headers: &HeaderMap, query: &HashMap<String, String>, token: &str) -> bool {
    if token.is_empty() {
        return false;
    }
    if query.get("token").map(|t| t.as_str()) == Some(token) {
        return true;
    }
    let auth = headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    let bearer = auth
        .strip_prefix("Bearer ")
        .or_else(|| auth.strip_prefix("bearer "))
        .unwrap_or("");
    bearer == token
}

async fn mcp_post(
    State(state): State<HttpState>,
    headers: HeaderMap,
    Query(query): Query<HashMap<String, String>>,
    body: Bytes,
) -> Response {
    if !authorized(&headers, &query, &state.token) {
        return cors(StatusCode::UNAUTHORIZED, "unauthorized");
    }
    let text = String::from_utf8_lossy(&body);
    let msg: JsonValue = match serde_json::from_str(text.trim()) {
        Ok(v) => v,
        Err(e) => {
            return cors(
                StatusCode::OK,
                json_rpc_error(None, -32700, &format!("parse error: {e}")).to_string(),
            );
        }
    };
    let id = msg.get("id").cloned();
    let method = msg.get("method").and_then(|m| m.as_str()).unwrap_or("");
    if method.starts_with("notifications/") || id.is_none() {
        return cors(StatusCode::ACCEPTED, "");
    }
    let params = msg.get("params").cloned().unwrap_or(JsonValue::Null);
    let result = handle_method(&state.tools, method, params).await;
    let body = match result {
        Ok(value) => json!({ "jsonrpc": "2.0", "id": id, "result": value }),
        Err((code, message)) => json_rpc_error(id, code, &message),
    };
    cors(StatusCode::OK, body.to_string())
}

async fn handle_method(
    tools: &AgentTools,
    method: &str,
    params: JsonValue,
) -> Result<JsonValue, (i64, String)> {
    match method {
        "initialize" => {
            let proto = params
                .get("protocolVersion")
                .and_then(|v| v.as_str())
                .unwrap_or("2024-11-05");
            let version = if proto.starts_with("2025") {
                proto
            } else {
                "2024-11-05"
            };
            Ok(json!({
                "protocolVersion": version,
                "capabilities": {
                    "tools": { "listChanged": false },
                    "resources": { "subscribe": false, "listChanged": false }
                },
                "serverInfo": {
                    "name": "mystique-db",
                    "version": env!("CARGO_PKG_VERSION")
                },
                "instructions": "Mystique DB is a desktop database studio. Tools are read-only: schema (DBML), list/describe tables or collections, run SELECT/find, save and list queries. Use list_connections first. Never ask the user to paste schema."
            }))
        }
        "ping" => Ok(json!({})),
        "tools/list" => {
            let tools: Vec<JsonValue> = tool_specs()
                .into_iter()
                .map(|t| {
                    json!({
                        "name": t.name,
                        "description": t.description,
                        "inputSchema": t.input_schema
                    })
                })
                .collect();
            Ok(json!({ "tools": tools }))
        }
        "tools/call" => {
            let name = params
                .get("name")
                .and_then(|v| v.as_str())
                .ok_or((-32602, "tools/call requires name".into()))?;
            let args = params.get("arguments").cloned().unwrap_or(json!({}));
            match tools.call_tool(name, &args).await {
                Ok(value) => Ok(json!({
                    "content": [{
                        "type": "text",
                        "text": value.to_string()
                    }],
                    "structuredContent": value,
                    "isError": false
                })),
                Err(e) => Ok(json!({
                    "content": [{
                        "type": "text",
                        "text": e.to_string()
                    }],
                    "isError": true
                })),
            }
        }
        "resources/list" => Ok(json!({
            "resources": [
                {
                    "uri": "mystique://connections",
                    "name": "Connections",
                    "description": "Saved connections (name, engine, database)",
                    "mimeType": "application/json"
                }
            ]
        })),
        "resources/read" => {
            let uri = params.get("uri").and_then(|v| v.as_str()).unwrap_or("");
            if uri == "mystique://connections" {
                let list = tools
                    .list_connections()
                    .await
                    .map_err(|e| (-32000, e.to_string()))?;
                let text = serde_json::to_string_pretty(&list).unwrap_or_default();
                return Ok(json!({
                    "contents": [{
                        "uri": uri,
                        "mimeType": "application/json",
                        "text": text
                    }]
                }));
            }
            if let Some(rest) = uri.strip_prefix("mystique://connections/") {
                if let Some(id) = rest.strip_suffix("/schema.dbml") {
                    let dbml = tools
                        .get_schema_dbml(id, None)
                        .await
                        .map_err(|e| (-32000, e.to_string()))?;
                    return Ok(json!({
                        "contents": [{
                            "uri": uri,
                            "mimeType": "text/plain",
                            "text": dbml
                        }]
                    }));
                }
            }
            Err((-32602, format!("unknown resource `{uri}`")))
        }
        "prompts/list" => Ok(json!({ "prompts": [] })),
        other => Err((-32601, format!("method not found: {other}"))),
    }
}

fn json_rpc_error(id: Option<JsonValue>, code: i64, message: &str) -> JsonValue {
    json!({
        "jsonrpc": "2.0",
        "id": id.unwrap_or(JsonValue::Null),
        "error": { "code": code, "message": message }
    })
}

fn cors(status: StatusCode, body: impl Into<String>) -> Response {
    let body = body.into();
    (
        status,
        [
            (header::CONTENT_TYPE, "application/json"),
            (header::ACCESS_CONTROL_ALLOW_ORIGIN, "*"),
            (
                header::ACCESS_CONTROL_ALLOW_HEADERS,
                "Content-Type, Authorization, Mcp-Session-Id",
            ),
            (header::ACCESS_CONTROL_ALLOW_METHODS, "GET, POST, OPTIONS"),
        ],
        body,
    )
        .into_response()
}

pub fn ensure_token(existing: Option<&str>) -> String {
    let t = existing.map(str::trim).unwrap_or("");
    if t.len() >= 16 {
        t.to_string()
    } else {
        Uuid::new_v4().simple().to_string()
    }
}
