mod catalog;

use mystique_core::{AppError, AppResult};
use serde::{Deserialize, Serialize};

pub use catalog::{default_model, models_for, provider_catalog, AgentModelInfo, AgentProviderInfo};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AgentProvider {
    Openai,
    Anthropic,
    Google,
    Deepseek,
    Openrouter,
    /// xAI Grok (console.x.ai API key).
    Xai,
    Ollama,
}

impl AgentProvider {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Openai => "openai",
            Self::Anthropic => "anthropic",
            Self::Google => "google",
            Self::Deepseek => "deepseek",
            Self::Openrouter => "openrouter",
            Self::Xai => "xai",
            Self::Ollama => "ollama",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Openai => "OpenAI",
            Self::Anthropic => "Anthropic",
            Self::Google => "Gemini",
            Self::Deepseek => "DeepSeek",
            Self::Openrouter => "OpenRouter",
            Self::Xai => "Grok",
            Self::Ollama => "Ollama",
        }
    }

    pub fn needs_key(self) -> bool {
        !matches!(self, Self::Ollama)
    }

    pub fn default_model(self) -> &'static str {
        match self {
            Self::Openai => "gpt-5.6-terra",
            Self::Anthropic => "claude-sonnet-5",
            Self::Google => "gemini-3.7-flash",
            Self::Deepseek => "deepseek-v4-flash",
            Self::Openrouter => "openai/gpt-5.6-terra",
            Self::Xai => "grok-4.6",
            // Live list from the local daemon — never invent a pulled model.
            Self::Ollama => "",
        }
    }
}

pub fn parse_provider(raw: &str) -> AppResult<AgentProvider> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "openai" => Ok(AgentProvider::Openai),
        "anthropic" => Ok(AgentProvider::Anthropic),
        "google" | "gemini" => Ok(AgentProvider::Google),
        "deepseek" => Ok(AgentProvider::Deepseek),
        "openrouter" => Ok(AgentProvider::Openrouter),
        "xai" | "grok" => Ok(AgentProvider::Xai),
        "ollama" => Ok(AgentProvider::Ollama),
        other => Err(AppError::Invalid(format!("unknown provider `{other}`"))),
    }
}
