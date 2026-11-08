mod catalog;

use mystique_core::{AppError, AppResult};
use serde::{Deserialize, Serialize};

pub use catalog::{default_model, models_for, provider_catalog, AgentModelInfo, AgentProviderInfo};

/// Completion providers exposed in Settings and the query agent.
///
/// Not listed: Azure (needs endpoint + deployment), Perplexity / Hyperbolic /
/// Mira (Rig drops tools), Bedrock / Vertex (IAM + extra crate features),
/// Copilot / ChatGPT (OAuth), VoyageAI (embeddings only), Galadriel (removed
/// in Rig 0.42), EternalAI (separate crate).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "lowercase")]
pub enum AgentProvider {
    Openai,
    Anthropic,
    Google,
    Groq,
    Together,
    Mistral,
    Cohere,
    Huggingface,
    Moonshot,
    Deepseek,
    Openrouter,
    Xai,
    Ollama,
}

impl AgentProvider {
    pub const ALL: [Self; 13] = [
        Self::Openai,
        Self::Anthropic,
        Self::Google,
        Self::Groq,
        Self::Together,
        Self::Mistral,
        Self::Cohere,
        Self::Huggingface,
        Self::Moonshot,
        Self::Deepseek,
        Self::Openrouter,
        Self::Xai,
        Self::Ollama,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Openai => "openai",
            Self::Anthropic => "anthropic",
            Self::Google => "google",
            Self::Groq => "groq",
            Self::Together => "together",
            Self::Mistral => "mistral",
            Self::Cohere => "cohere",
            Self::Huggingface => "huggingface",
            Self::Moonshot => "moonshot",
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
            Self::Groq => "Groq",
            Self::Together => "Together AI",
            Self::Mistral => "Mistral",
            Self::Cohere => "Cohere",
            Self::Huggingface => "Hugging Face",
            Self::Moonshot => "Moonshot",
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
            Self::Google => "gemini-3.8-flash",
            Self::Groq => "llama-3.3-70b-versatile",
            Self::Together => "moonshotai/Kimi-K3",
            Self::Mistral => "mistral-medium-latest",
            Self::Cohere => "command-a-plus-05-2026",
            Self::Huggingface => "Qwen/Qwen3.8-27B",
            Self::Moonshot => "kimi-k3",
            Self::Deepseek => "deepseek-v4-flash",
            Self::Openrouter => "openai/gpt-5.6-terra",
            Self::Xai => "grok-4.6",
            Self::Ollama => "",
        }
    }
}

pub fn parse_provider(raw: &str) -> AppResult<AgentProvider> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "openai" => Ok(AgentProvider::Openai),
        "anthropic" => Ok(AgentProvider::Anthropic),
        "google" | "gemini" => Ok(AgentProvider::Google),
        "groq" => Ok(AgentProvider::Groq),
        "together" | "togetherai" => Ok(AgentProvider::Together),
        "mistral" => Ok(AgentProvider::Mistral),
        "cohere" => Ok(AgentProvider::Cohere),
        "huggingface" | "hf" | "hugging_face" => Ok(AgentProvider::Huggingface),
        "moonshot" | "kimi" => Ok(AgentProvider::Moonshot),
        "deepseek" => Ok(AgentProvider::Deepseek),
        "openrouter" => Ok(AgentProvider::Openrouter),
        "xai" | "grok" => Ok(AgentProvider::Xai),
        "ollama" => Ok(AgentProvider::Ollama),
        other => Err(AppError::Invalid(format!("unknown provider `{other}`"))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_accepts_aliases() {
        assert_eq!(parse_provider("Gemini").unwrap(), AgentProvider::Google);
        assert_eq!(
            parse_provider("togetherai").unwrap(),
            AgentProvider::Together
        );
        assert_eq!(parse_provider("hf").unwrap(), AgentProvider::Huggingface);
        assert_eq!(parse_provider("kimi").unwrap(), AgentProvider::Moonshot);
        assert_eq!(parse_provider("grok").unwrap(), AgentProvider::Xai);
        assert!(parse_provider("azure").is_err());
    }

    #[test]
    fn all_ids_are_unique_and_parse() {
        let mut seen = std::collections::HashSet::new();
        for p in AgentProvider::ALL {
            assert!(seen.insert(p.as_str()), "duplicate id {}", p.as_str());
            assert_eq!(parse_provider(p.as_str()).unwrap(), p);
        }
    }

    #[test]
    fn default_model_is_listed() {
        for p in AgentProvider::ALL {
            if matches!(p, AgentProvider::Ollama) {
                continue;
            }
            let id = p.default_model();
            assert!(
                models_for(p).iter().any(|m| m.id == id),
                "{} default `{id}` is not in the catalog",
                p.as_str()
            );
        }
    }
}
