//! Provider failures in words a person can act on.
//!
//! Rig reports an HTTP failure as its error chain plus the raw response body
//! (`HttpError: Invalid status code 400 Bad Request with message: {json}`).
//! The chat panel shows one sentence instead: what went wrong and what to do.
//! The raw text is logged by the caller.

use serde_json::Value;

/// Wrappers from Rig's and Mystique's error chains. They say nothing to a user.
const PREFIXES: &[&str] = &[
    "internal error:",
    "invalid input:",
    "completionerror:",
    "prompterror:",
    "httperror:",
    "http error:",
    "http client error:",
    "providererror:",
    "providerresponseerror:",
    "responseerror:",
    "requesterror:",
    "jsonerror:",
];

/// Longest provider detail kept in a sentence.
const DETAIL_MAX_CHARS: usize = 240;

/// One sentence for a failed model call to `provider` (a display name such as
/// "Gemini", or "The agent" for an external agent).
pub fn friendly_error(provider: &str, raw: &str) -> String {
    provider_error_summary(provider, raw).unwrap_or_else(|| {
        let text = strip_prefixes(raw);
        if text.is_empty() {
            format!("{provider} request failed.")
        } else {
            sentence(&clip(text))
        }
    })
}

/// [`friendly_error`] when `raw` is a recognised HTTP or API error, else `None`.
pub fn provider_error_summary(provider: &str, raw: &str) -> Option<String> {
    let text = strip_prefixes(raw);
    let status = status_code(text);
    let body = json_body(text);
    let detail = body.as_ref().and_then(detail_message);
    let tokens = body.as_ref().map(reason_tokens).unwrap_or_default();
    let hay = format!("{tokens} {}", text.to_ascii_lowercase());

    let has = |needles: &[&str]| needles.iter().any(|n| hay.contains(n));

    let category = if status == Some(401)
        || has(&[
            "api_key_invalid",
            "invalid_api_key",
            "invalid x-api-key",
            "api key not valid",
            "incorrect api key",
            "invalid api key",
            "authentication_error",
            "unauthenticated",
            "unauthorized",
        ]) {
        Some(format!(
            "{provider} rejected the API key. Check it in Settings → AI."
        ))
    } else if has(&[
        "insufficient_quota",
        "billing",
        "credit balance",
        "out of credits",
    ]) {
        Some(format!(
            "Your {provider} account is out of credits or quota. Check your plan with {provider}."
        ))
    } else if status == Some(429)
        || has(&["rate limit", "rate_limit", "resource_exhausted", "quota"])
    {
        Some(format!(
            "{provider} is rate-limiting requests. Wait a moment and try again."
        ))
    } else if has(&[
        "context_length_exceeded",
        "maximum context length",
        "context window",
        "too many tokens",
        "prompt is too long",
    ]) {
        Some(
            "The conversation is too long for this model. Start a new chat or pick a model with a larger context."
                .to_string(),
        )
    } else if status == Some(404) || has(&["model_not_found", "model not found"]) {
        Some(format!(
            "{provider} could not find this model, or your key cannot use it. Pick another model."
        ))
    } else if matches!(status, Some(500..=599)) || has(&["overloaded"]) {
        Some(format!(
            "{provider} is having trouble right now. Try again in a moment."
        ))
    } else if has(&["content_filter", "safety", "blocked"]) && body.is_some() {
        Some(format!(
            "{provider} blocked this request with its content filter."
        ))
    } else if has(&[
        "error sending request",
        "connection refused",
        "tcp connect",
        "dns error",
        "failed to lookup address",
        "connection reset",
        "network is unreachable",
    ]) {
        Some(if provider == "Ollama" {
            "Could not reach Ollama. Make sure it is running.".to_string()
        } else {
            format!("Could not reach {provider}. Check your internet connection.")
        })
    } else {
        None
    };
    if category.is_some() {
        return category;
    }
    match (detail, status) {
        (Some(detail), _) => Some(format!(
            "{provider} returned an error: {}",
            sentence(&clip(&detail))
        )),
        (None, Some(code)) => Some(format!("{provider} returned HTTP {code}.")),
        (None, None) => None,
    }
}

fn strip_prefixes(raw: &str) -> &str {
    let mut text = raw.trim();
    loop {
        let lower = text.to_ascii_lowercase();
        let Some(prefix) = PREFIXES.iter().find(|p| lower.starts_with(**p)) else {
            return text;
        };
        text = text[prefix.len()..].trim_start();
    }
}

/// The number after "status code", as Rig and reqwest print it.
fn status_code(text: &str) -> Option<u16> {
    let lower = text.to_ascii_lowercase();
    let at = lower.find("status code")? + "status code".len();
    let digits: String = lower[at..]
        .trim_start_matches([':', ' '])
        .chars()
        .take_while(char::is_ascii_digit)
        .collect();
    digits.parse().ok()
}

/// The JSON response body inside the error text, if any.
fn json_body(text: &str) -> Option<Value> {
    let start = text.find(['{', '['])?;
    let end = text.rfind(['}', ']'])?;
    if end <= start {
        return None;
    }
    let value: Value = serde_json::from_str(&text[start..=end]).ok()?;
    // Gemini can answer with a one-element array.
    match value {
        Value::Array(items) => items.into_iter().next(),
        other => Some(other),
    }
}

/// The provider's own message: `error.message`, `error` as text, `message`,
/// or `detail`.
fn detail_message(body: &Value) -> Option<String> {
    let text = |v: &Value| {
        v.as_str()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(String::from)
    };
    let error = body.get("error");
    error
        .and_then(|e| e.get("message"))
        .and_then(text)
        .or_else(|| {
            error
                .and_then(|e| e.get("error"))
                .and_then(|e| e.get("message"))
                .and_then(text)
        })
        .or_else(|| error.and_then(text))
        .or_else(|| body.get("message").and_then(text))
        .or_else(|| body.get("detail").and_then(text))
}

/// Machine-readable codes (`status`, `type`, `code`, `reason`), lowercased.
fn reason_tokens(body: &Value) -> String {
    let mut out = Vec::new();
    let mut push = |v: Option<&Value>| {
        if let Some(s) = v.and_then(Value::as_str) {
            out.push(s.to_ascii_lowercase());
        }
    };
    for node in [Some(body), body.get("error")].into_iter().flatten() {
        push(node.get("status"));
        push(node.get("type"));
        push(node.get("code"));
        if let Some(details) = node.get("details").and_then(Value::as_array) {
            for d in details {
                push(d.get("reason"));
            }
        }
    }
    out.join(" ")
}

fn clip(text: &str) -> String {
    let text = text.trim();
    if text.chars().count() <= DETAIL_MAX_CHARS {
        return text.to_string();
    }
    let cut: String = text.chars().take(DETAIL_MAX_CHARS).collect();
    format!("{}…", cut.trim_end())
}

fn sentence(text: &str) -> String {
    let text = text.trim();
    if text.ends_with(['.', '!', '?', '…']) {
        text.to_string()
    } else {
        format!("{text}.")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const GEMINI_BAD_KEY: &str = r#"internal error: CompletionError: HttpError: Invalid status code 400 Bad Request with message: { "error": { "code": 400, "message": "API key not valid. Please pass a valid API key.", "status": "INVALID_ARGUMENT", "details": [ { "@type": "type.googleapis.com/google.rpc.ErrorInfo", "reason": "API_KEY_INVALID", "domain": "googleapis.com", "metadata": { "service": "generativelanguage.googleapis.com" } }, { "@type": "type.googleapis.com/google.rpc.LocalizedMessage", "locale": "en-US", "message": "API key not valid. Please pass a valid API key." } ] } }"#;

    #[test]
    fn gemini_bad_key_is_one_sentence() {
        assert_eq!(
            friendly_error("Gemini", GEMINI_BAD_KEY),
            "Gemini rejected the API key. Check it in Settings → AI."
        );
    }

    #[test]
    fn anthropic_bad_key() {
        let raw = r#"HttpError: Invalid status code 401 Unauthorized with message: {"type":"error","error":{"type":"authentication_error","message":"invalid x-api-key"}}"#;
        assert_eq!(
            friendly_error("Anthropic", raw),
            "Anthropic rejected the API key. Check it in Settings → AI."
        );
    }

    #[test]
    fn openai_quota_and_rate_limit() {
        let quota = r#"HttpError: Invalid status code 429 Too Many Requests with message: {"error":{"message":"You exceeded your current quota.","type":"insufficient_quota","code":"insufficient_quota"}}"#;
        assert!(
            friendly_error("OpenAI", quota).starts_with("Your OpenAI account is out of credits")
        );
        let rate = r#"HttpError: Invalid status code 429 Too Many Requests with message: {"error":{"message":"Slow down.","type":"requests"}}"#;
        assert!(friendly_error("OpenAI", rate).contains("rate-limiting"));
    }

    #[test]
    fn overloaded_and_missing_model() {
        let busy = r#"HttpError: Invalid status code 529 with message: {"type":"error","error":{"type":"overloaded_error","message":"Overloaded"}}"#;
        assert!(friendly_error("Anthropic", busy).contains("having trouble"));
        let missing = r#"HttpError: Invalid status code 404 Not Found with message: {"error":{"message":"The model `gpt-9` does not exist","code":"model_not_found"}}"#;
        assert!(friendly_error("OpenAI", missing).contains("could not find this model"));
    }

    #[test]
    fn unknown_api_error_keeps_the_provider_message() {
        let raw = r#"HttpError: Invalid status code 400 Bad Request with message: {"error":{"message":"Invalid value at 'contents[0]'","status":"INVALID_ARGUMENT"}}"#;
        assert_eq!(
            friendly_error("Gemini", raw),
            "Gemini returned an error: Invalid value at 'contents[0]'."
        );
    }

    #[test]
    fn ollama_down() {
        let raw = "HttpError: Http client error: error sending request for url (http://localhost:11434/api/chat): tcp connect error: Connection refused";
        assert_eq!(
            friendly_error("Ollama", raw),
            "Could not reach Ollama. Make sure it is running."
        );
    }

    #[test]
    fn plain_text_passes_through_without_wrappers() {
        assert_eq!(
            friendly_error("OpenAI", "ProviderError: Something odd happened"),
            "Something odd happened."
        );
        assert_eq!(
            provider_error_summary("OpenAI", "Something odd happened"),
            None
        );
    }
}
