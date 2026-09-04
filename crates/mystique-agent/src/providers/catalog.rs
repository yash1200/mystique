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
    AgentProvider::ALL
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
        AgentProvider::Groq => &[
            ("llama-3.3-70b-versatile", "Llama 3.3 70B"),
            ("llama-3.1-8b-instant", "Llama 3.1 8B Instant"),
            ("mixtral-8x7b-32768", "Mixtral 8x7B"),
            ("gemma2-9b-it", "Gemma 2 9B"),
        ],
        AgentProvider::Together => &[
            ("moonshotai/Kimi-K3", "Kimi K3"),
            ("zai-org/GLM-5.3", "GLM 5.3"),
            ("zai-org/GLM-5.3-Flash", "GLM 5.3 Flash"),
            ("deepseek-ai/DeepSeek-V4-Pro-0813", "DeepSeek V4 Pro"),
            ("deepseek-ai/DeepSeek-V4-Flash-0731", "DeepSeek V4 Flash"),
            ("Qwen/Qwen3.8-2.4T-A95B", "Qwen 3.8 2.4T"),
            ("Qwen/Qwen3.8-Flash", "Qwen 3.8 Flash"),
            ("google/gemma-4-31B-it", "Gemma 4 31B"),
            ("meta-models/Muse-Glimmer-30B", "Muse Glimmer 30B"),
            ("MiniMaxAI/MiniMax-M3", "MiniMax M3"),
        ],
        AgentProvider::Mistral => &[
            ("mistral-medium-latest", "Mistral Medium 3.5"),
            ("mistral-small-latest", "Mistral Small 4"),
            ("mistral-large-latest", "Mistral Large 3"),
            ("codestral-latest", "Codestral"),
            ("ministral-14b-2512", "Ministral 3 14B"),
            ("ministral-8b-2512", "Ministral 3 8B"),
        ],
        AgentProvider::Cohere => &[
            ("command-a-plus-05-2026", "Command A+"),
            ("command-a-03-2025", "Command A"),
            ("command-a-reasoning-08-2025", "Command A Reasoning"),
            ("north-mini-code-1-0", "North Mini Code"),
            ("command-r7b-12-2024", "Command R7B"),
            ("command-r-plus-08-2024", "Command R+"),
        ],
        AgentProvider::Huggingface => &[
            ("moonshotai/Kimi-K3", "Kimi K3"),
            ("zai-org/GLM-5.3", "GLM 5.3"),
            ("deepseek-ai/DeepSeek-V4-Pro-0813", "DeepSeek V4 Pro"),
            ("Qwen/Qwen3.8-27B", "Qwen 3.8 27B"),
            ("google/gemma-4-31B-it", "Gemma 4 31B"),
            ("meta-models/Muse-Glimmer-30B", "Muse Glimmer 30B"),
        ],
        AgentProvider::Moonshot => &[
            ("kimi-k3", "Kimi K3"),
            ("kimi-k2.7-code", "Kimi K2.7 Code"),
            ("kimi-k2.7-code-highspeed", "Kimi K2.7 Code Highspeed"),
            ("kimi-k2.6", "Kimi K2.6"),
        ],
        AgentProvider::Deepseek => &[
            ("deepseek-v4-flash", "DeepSeek V4 Flash"),
            ("deepseek-v4-pro", "DeepSeek V4 Pro"),
        ],
        AgentProvider::Openrouter => &[
            ("moonshotai/kimi-k3", "Kimi K3"),
            ("z-ai/glm-5.3", "GLM 5.3"),
            ("z-ai/glm-5.3-flash", "GLM 5.3 Flash"),
            ("deepseek/deepseek-v4-pro", "DeepSeek V4 Pro"),
            ("deepseek/deepseek-v4-flash", "DeepSeek V4 Flash"),
            ("qwen/qwen3.8-flash", "Qwen 3.8 Flash"),
            ("qwen/qwen3.7-plus", "Qwen 3.7 Plus"),
            ("google/gemma-4-31b-it", "Gemma 4 31B"),
            ("meta/muse-glimmer-30b", "Muse Glimmer 30B"),
            ("openai/gpt-5.6-terra", "GPT-5.6 Terra"),
            ("anthropic/claude-sonnet-5", "Claude Sonnet 5"),
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
