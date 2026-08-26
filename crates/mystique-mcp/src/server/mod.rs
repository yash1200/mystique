//! Minimal MCP Streamable HTTP (JSON-RPC 2.0) on 127.0.0.1.

mod http;
mod rpc;
mod token;

use std::net::SocketAddr;
use std::sync::Arc;

use mystique_core::{AgentTools, AppError, AppResult};
use serde::{Deserialize, Serialize};
use tokio::net::TcpListener;
use tokio::sync::{oneshot, Mutex};

use crate::settings::DEFAULT_MCP_PORT;

use self::http::{router, HttpState};

pub use token::ensure_token;

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

        let app = router(HttpState {
            tools,
            token: token.clone(),
        });

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
