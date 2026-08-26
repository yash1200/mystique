//! List models from a local Ollama daemon.

use mystique_core::{AppError, AppResult, AppServices};
use rig::client::Nothing;
use rig::prelude::*;
use rig::providers::ollama;

use crate::providers::AgentModelInfo;

use super::AgentRuntime;

pub(super) async fn list_ollama_models(app: &AppServices) -> AppResult<Vec<AgentModelInfo>> {
    let url = AgentRuntime::ollama_url(app);
    let client = ollama::Client::builder()
        .api_key(Nothing)
        .base_url(&url)
        .build()
        .map_err(|e| AppError::Internal(e.to_string()))?;
    let listed = client.list_models().await.map_err(|e| {
        AppError::Invalid(format!(
            "Could not list Ollama models at {url}. Is Ollama running? ({e})"
        ))
    })?;

    let mut out: Vec<AgentModelInfo> = listed
        .iter()
        .filter(|m| !is_likely_embedding(&m.id) && !is_likely_embedding(m.display_name()))
        .map(|m| AgentModelInfo {
            id: m.id.clone(),
            label: m.display_name().to_string(),
        })
        .collect();
    if out.is_empty() {
        out = listed
            .iter()
            .map(|m| AgentModelInfo {
                id: m.id.clone(),
                label: m.display_name().to_string(),
            })
            .collect();
    }
    out.sort_by(|a, b| {
        a.label
            .to_ascii_lowercase()
            .cmp(&b.label.to_ascii_lowercase())
    });
    out.dedup_by(|a, b| a.id == b.id);
    Ok(out)
}

fn is_likely_embedding(id: &str) -> bool {
    let l = id.to_ascii_lowercase();
    l.contains("embed")
        || l.contains("minilm")
        || l.contains("nomic-bert")
        || l.contains("bge-")
        || l.contains("e5-")
}
