//! Masking for everything an external agent sends back.
//!
//! The agent is handed the MCP token (in `session/new`) and the secret
//! environment values. Whatever it echoes — text, tool arguments, tool
//! results, error messages, stderr — passes through [`Scrubber`] before it
//! reaches the window, a log line or a saved row: the literal secrets are
//! replaced first, then the shared redaction (URI passwords, `key=value`
//! secrets, account passwords in SQL) runs.

use std::sync::Arc;

use mystique_agent::kit::{redact_args, redact_text};
use serde_json::Value as JsonValue;

pub(crate) const MASK: &str = "[redacted]";

/// Secrets shorter than this are not matched (too likely to hit real text).
const MIN_SECRET: usize = 6;

#[derive(Clone, Default)]
pub(crate) struct Scrubber {
    secrets: Arc<Vec<String>>,
}

impl Scrubber {
    pub(crate) fn new(secrets: impl IntoIterator<Item = String>) -> Self {
        let mut list: Vec<String> = secrets
            .into_iter()
            .map(|s| s.trim().to_string())
            .filter(|s| s.chars().count() >= MIN_SECRET)
            .collect();
        // Longest first, so a secret that contains another is masked whole.
        list.sort_by_key(|s| std::cmp::Reverse(s.len()));
        list.dedup();
        Self {
            secrets: Arc::new(list),
        }
    }

    /// Bytes at the end of `text` that could be the start of a secret
    /// (the longest proper prefix of any secret that `text` ends with). A
    /// stream holds them back until the next chunk shows what they are.
    pub(crate) fn open_prefix(&self, text: &str) -> usize {
        self.secrets
            .iter()
            .filter_map(|secret| {
                (1..secret.len())
                    .rev()
                    .filter(|n| secret.is_char_boundary(*n))
                    .find(|n| secret.get(..*n).is_some_and(|p| text.ends_with(p)))
            })
            .max()
            .unwrap_or(0)
    }

    fn literal(&self, text: &str) -> String {
        let mut out = text.to_string();
        for secret in self.secrets.iter() {
            if out.contains(secret.as_str()) {
                out = out.replace(secret.as_str(), MASK);
            }
        }
        out
    }

    /// Only the literal secrets: for streamed text, where the shared
    /// redaction would mangle prose (it runs on the saved reply instead).
    pub(crate) fn secrets_only(&self, text: &str) -> String {
        self.literal(text)
    }

    pub(crate) fn text(&self, text: &str) -> String {
        redact_text(&self.literal(text))
    }

    pub(crate) fn json(&self, value: &JsonValue) -> JsonValue {
        redact_args(&self.json_literal(value))
    }

    fn json_literal(&self, value: &JsonValue) -> JsonValue {
        match value {
            JsonValue::String(s) => JsonValue::String(self.literal(s)),
            JsonValue::Array(items) => {
                JsonValue::Array(items.iter().map(|v| self.json_literal(v)).collect())
            }
            JsonValue::Object(map) => JsonValue::Object(
                map.iter()
                    .map(|(k, v)| (self.literal(k), self.json_literal(v)))
                    .collect(),
            ),
            other => other.clone(),
        }
    }
}

/// A text stream that never lets a secret through, even split across chunks.
///
/// Pushed text is masked as a whole; everything is released except a tail
/// that could still grow into a secret ([`Scrubber::open_prefix`]).
/// [`SafeStream::flush`] releases the rest.
pub(crate) struct SafeStream {
    scrub: Scrubber,
    held: String,
}

impl SafeStream {
    pub(crate) fn new(scrub: Scrubber) -> Self {
        Self {
            scrub,
            held: String::new(),
        }
    }

    /// Text that is safe to show now.
    pub(crate) fn push(&mut self, chunk: &str) -> String {
        self.held.push_str(chunk);
        self.held = self.scrub.secrets_only(&self.held);
        let keep = self.scrub.open_prefix(&self.held);
        // A secret's prefix ends on a char boundary of `held` as well.
        let cut = self.held.len() - keep;
        let rest = self.held.split_off(cut);
        std::mem::replace(&mut self.held, rest)
    }

    pub(crate) fn flush(&mut self) -> String {
        let out = self.scrub.secrets_only(&self.held);
        self.held.clear();
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn literal_secrets_and_shared_patterns_are_masked() {
        let s = Scrubber::new(["tok_0123456789abcdef".to_string(), "abc".to_string()]);
        let out = s.text("token tok_0123456789abcdef and postgres://u:pw1234@h/db; abc stays");
        assert!(!out.contains("tok_0123456789abcdef"), "{out}");
        assert!(!out.contains("pw1234"), "{out}");
        assert!(
            out.contains("abc stays"),
            "short values are not secrets: {out}"
        );
        let v = s.json(&json!({ "q": "x tok_0123456789abcdef", "n": [ "tok_0123456789abcdef" ] }));
        assert!(!v.to_string().contains("tok_0123456789abcdef"), "{v}");
    }

    #[test]
    fn a_secret_split_across_chunks_never_leaves_the_stream() {
        let secret = "sk-live-SECRET-0123456789";
        let s = Scrubber::new([secret.to_string()]);
        let mut stream = SafeStream::new(s);
        let mut shown = String::new();
        let text = format!("here is the key: {secret} - done");
        for chunk in text.as_bytes().chunks(3) {
            shown.push_str(&stream.push(std::str::from_utf8(chunk).unwrap_or("")));
        }
        shown.push_str(&stream.flush());
        assert!(!shown.contains(secret), "{shown}");
        assert_eq!(shown, format!("here is the key: {MASK} - done"));
    }

    #[test]
    fn text_that_cannot_start_a_secret_streams_through_at_once() {
        let mut stream = SafeStream::new(Scrubber::default());
        assert_eq!(stream.push("hello"), "hello");
        assert_eq!(stream.flush(), "");
        let mut stream = SafeStream::new(Scrubber::new(["tok_secret_123".to_string()]));
        assert_eq!(stream.push("Working on a"), "Working on a");
        // "to" could start the token: held until the next chunk says.
        assert_eq!(stream.push(" to"), " ");
        assert_eq!(stream.push("day."), "today.");
        assert_eq!(stream.push("tok_sec"), "");
        assert_eq!(stream.flush(), "tok_sec");
    }
}
