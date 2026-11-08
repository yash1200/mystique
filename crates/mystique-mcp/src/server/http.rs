use std::collections::HashMap;

use axum::body::Bytes;
use axum::extract::{Query, State};
use axum::http::{header, HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::Router;
use mystique_core::AgentTools;
use serde_json::{json, Value as JsonValue};

use super::rpc::{handle_method, json_rpc_error};
use super::TokenGate;

#[derive(Clone)]
pub(super) struct HttpState {
    pub tools: AgentTools,
    pub token: TokenGate,
}

pub(super) fn router(state: HttpState) -> Router {
    Router::new()
        .route("/mcp", post(mcp_post).get(mcp_get).options(mcp_options))
        .route("/health", get(health))
        .with_state(state)
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
            "server": "mystique",
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
    // Loaded from the vault on the first request, without keychain UI.
    let token = match state.token.get().await {
        Ok(token) => token,
        Err(message) => {
            tracing::warn!("mcp request refused: token unavailable");
            return cors(
                StatusCode::SERVICE_UNAVAILABLE,
                json_rpc_error(None, -32001, &message).to_string(),
            );
        }
    };
    if !authorized(&headers, &query, &token) {
        tracing::warn!("mcp request refused: bad or missing token");
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
    // Method and tool name only: arguments can hold SQL.
    let tool = (method == "tools/call")
        .then(|| {
            params
                .get("name")
                .and_then(|n| n.as_str())
                .map(str::to_owned)
        })
        .flatten();
    let started = std::time::Instant::now();
    let result = handle_method(&state.tools, method, params).await;
    let elapsed_ms = started.elapsed().as_millis() as u64;
    match &result {
        Ok(_) => tracing::info!(method, tool = tool.as_deref(), elapsed_ms, "mcp request"),
        Err((code, _)) => tracing::info!(
            method,
            tool = tool.as_deref(),
            code,
            elapsed_ms,
            "mcp request failed"
        ),
    }
    let body = match result {
        Ok(value) => json!({ "jsonrpc": "2.0", "id": id, "result": value }),
        Err((code, message)) => json_rpc_error(id, code, &message),
    };
    cors(StatusCode::OK, body.to_string())
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
