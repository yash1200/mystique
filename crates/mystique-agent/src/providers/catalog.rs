use serde::{Deserialize, Serialize};

use super::AgentProvider;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentProviderInfo {
    pub id: String,
    pub label: String,
    pub needs_key: bool,
    pub default_model: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentModelInfo {
    pub id: String,
    pub label: String,
}

pub fn provider_catalog() -> Vec<AgentProviderInfo> {
    [
        AgentProvider::Openai,
        AgentProvider::Anthropic,
        AgentProvider::Google,
        AgentProvider::Deepseek,
        AgentProvider::Openrouter,
        AgentProvider::Xai,
        AgentProvider::Ollama,
    ]
    .into_iter()
    .map(|p| AgentProviderInfo {
        id: p.as_str().into(),
        label: p.label().into(),
        needs_key: p.needs_key(),
        default_model: p.default_model().into(),
    })
    .collect()
}

pub fn models_for(provider: AgentProvider) -> Vec<AgentModelInfo> {
    let pairs: &[(&str, &str)] = match provider {
        AgentProvider::Openai => &[
            ("gpt-5.6-sol", "GPT-5.6 Sol"),
            ("gpt-5.6-terra", "GPT-5.6 Terra"),
            ("gpt-5.6-luna", "GPT-5.6 Luna"),
        ],
        AgentProvider::Anthropic => &[
            ("claude-sonnet-5", "Claude Sonnet 5"),
            ("claude-opus-5", "Claude Opus 5"),
            ("claude-fable-5", "Claude Fable 5"),
            ("claude-haiku-4-5", "Claude Haiku 4.5"),
        ],
        AgentProvider::Google => &[
            ("gemini-3.7-flash", "Gemini 3.7 Flash"),
            ("gemini-3.5-flash-lite", "Gemini 3.5 Flash-Lite"),
            ("gemini-3.1-pro-preview", "Gemini 3.1 Pro"),
        ],
        AgentProvider::Deepseek => &[
            ("deepseek-v4-flash", "DeepSeek V4 Flash"),
            ("deepseek-v4-pro", "DeepSeek V4 Pro"),
        ],
        AgentProvider::Openrouter => &[
            ("openai/gpt-5.6-terra", "GPT-5.6 Terra"),
            ("openai/gpt-5.6-luna", "GPT-5.6 Luna"),
            ("anthropic/claude-sonnet-5", "Claude Sonnet 5"),
            ("anthropic/claude-opus-5", "Claude Opus 5"),
            ("google/gemini-3.7-flash", "Gemini 3.7 Flash"),
            ("deepseek/deepseek-v4-flash", "DeepSeek V4 Flash"),
        ],
        AgentProvider::Xai => &[("grok-4.6", "Grok 4.6")],
        AgentProvider::Ollama => &[],
    };
    pairs
        .iter()
        .map(|(id, label)| AgentModelInfo {
            id: (*id).into(),
            label: (*label).into(),
        })
        .collect()
}

pub fn default_model(provider: AgentProvider) -> String {
    provider.default_model().into()
}
