//! Mask secrets in text before it reaches a log line or a diagnostics bundle.
//!
//! What is masked:
//! - the password in a URI's user info: `postgres://u:pass@h` → `postgres://u:***@h`
//!   (any scheme, so `mongodb+srv://`, `mysql://` and `redis://` too);
//! - the value of a secret-looking key in `key=value` text: libpq DSNs
//!   (`password=x host=h`), ADO strings (`Password=x;Server=h`) and URL queries
//!   (`?sslpassword=x&a=b`), quoted or not;
//! - the value of a secret-looking key in JSON text: `"password":"x"`.
//!
//! The user name and host stay. This is a safety net, not a licence: code
//! should still not log SQL, parameter values or connection strings at `info`.
//!
//! [`redact_sql_passwords`] is the SQL pass query history adds: account
//! passwords (`ALTER USER … PASSWORD '…'`, `IDENTIFIED BY '…'`) only.

use serde_json::Value as JsonValue;

mod sql_passwords;

pub use sql_passwords::redact_sql_passwords;

/// What a masked value becomes.
pub const MASK: &str = "***";

/// Fragments that mark a key as secret, matched on the key lower-cased with
/// everything but letters and digits removed (`api_key`, `apiKey` → `apikey`).
const SECRET_KEY_PARTS: &[&str] = &[
    "password",
    "passwd",
    "pwd",
    "passphrase",
    "secret",
    "token",
    "apikey",
    "accesskey",
    "privatekey",
    "credential",
];

/// True when a setting, JSON field or DSN key names a secret.
pub fn is_secret_key(key: &str) -> bool {
    let norm: String = key
        .chars()
        .filter(char::is_ascii_alphanumeric)
        .map(|c| c.to_ascii_lowercase())
        .collect();
    !norm.is_empty() && SECRET_KEY_PARTS.iter().any(|part| norm.contains(part))
}

/// Mask passwords in URIs and secret `key=value` / `"key":"value"` pairs.
pub fn redact_secrets(text: &str) -> String {
    redact_pairs(&redact_uri_passwords(text))
}

/// A copy of `value` with secret fields masked and every string run through
/// [`redact_secrets`]. Only string values are masked: `"passwordSet": true` stays.
pub fn redact_json(value: &JsonValue) -> JsonValue {
    match value {
        JsonValue::Object(map) => JsonValue::Object(
            map.iter()
                .map(|(key, inner)| {
                    let masked = match inner {
                        JsonValue::String(s) if is_secret_key(key) && !s.is_empty() => {
                            JsonValue::String(MASK.into())
                        }
                        other => redact_json(other),
                    };
                    (key.clone(), masked)
                })
                .collect(),
        ),
        JsonValue::Array(items) => JsonValue::Array(items.iter().map(redact_json).collect()),
        JsonValue::String(s) => JsonValue::String(redact_secrets(s)),
        other => other.clone(),
    }
}

/// Ends a bare URI or a bare `key=value` value.
fn ends_token(c: char) -> bool {
    c.is_whitespace() || matches!(c, '"' | '\'' | '<' | '>' | '`')
}

/// Only the URI pass of [`redact_secrets`]: `key=value` text is kept. Query
/// history uses this so `WHERE password = 'x'` in saved SQL stays runnable.
pub fn redact_uri_passwords(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(pos) = rest.find("://") {
        let (head, tail) = rest.split_at(pos + 3);
        out.push_str(head);
        let token_end = tail.find(ends_token).unwrap_or(tail.len());
        let token = &tail[..token_end];
        // User info ends at the last `@` before any query or fragment, so a
        // password may itself hold `@` or `/`.
        let scope_end = token.find(['?', '#']).unwrap_or(token.len());
        match token[..scope_end].rfind('@') {
            Some(at) => {
                let userinfo = &token[..at];
                match userinfo.find(':') {
                    Some(colon) if colon + 1 < userinfo.len() => {
                        out.push_str(&userinfo[..=colon]);
                        out.push_str(MASK);
                        out.push_str(&token[at..]);
                    }
                    _ => out.push_str(token),
                }
            }
            None => out.push_str(token),
        }
        rest = &tail[token_end..];
    }
    out.push_str(rest);
    out
}

/// Characters a bare key may hold (`ssl.password`, `api-key`, `Pwd`).
fn is_key_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.')
}

fn redact_pairs(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = String::with_capacity(text.len());
    let mut copied = 0;
    let mut i = 0;
    while i < bytes.len() {
        let sep = bytes[i];
        let key = match sep {
            b'=' => bare_key_before(text, i),
            // Only a JSON key: `"password": "x"`. A bare `:` is a port or a time.
            b':' if i > 0 && bytes[i - 1] == b'"' => quoted_key_before(text, i - 1),
            _ => None,
        };
        if !key.is_some_and(is_secret_key) {
            i += 1;
            continue;
        }
        let Some((start, end)) = value_span(text, i + 1, sep == b':') else {
            i += 1;
            continue;
        };
        out.push_str(&text[copied..start]);
        out.push_str(MASK);
        copied = end;
        i = end.max(i + 1);
    }
    out.push_str(&text[copied..]);
    out
}

fn bare_key_before(text: &str, eq: usize) -> Option<&str> {
    let head = text[..eq].trim_end_matches(' ');
    let start = head
        .char_indices()
        .rev()
        .find(|&(_, c)| !is_key_char(c))
        .map(|(idx, c)| idx + c.len_utf8())
        .unwrap_or(0);
    let key = &head[start..];
    (!key.is_empty()).then_some(key)
}

fn quoted_key_before(text: &str, close_quote: usize) -> Option<&str> {
    let open = text[..close_quote].rfind('"')?;
    Some(&text[open + 1..close_quote])
}

/// Byte range of the value after a separator, quotes excluded. A JSON value
/// must be a string; numbers, `true` and objects are left alone.
fn value_span(text: &str, from: usize, json: bool) -> Option<(usize, usize)> {
    let after = &text[from..];
    let skipped = after.len() - after.trim_start_matches(' ').len();
    let start = from + skipped;
    let rest = &text[start..];
    let first = rest.chars().next()?;
    if first == '"' || first == '\'' {
        let body = &rest[1..];
        let mut escaped = false;
        for (idx, c) in body.char_indices() {
            if escaped {
                escaped = false;
            } else if c == '\\' {
                escaped = true;
            } else if c == first {
                return (idx > 0).then_some((start + 1, start + 1 + idx));
            }
        }
        // Unterminated: mask to the end rather than leak.
        return (!body.is_empty()).then_some((start + 1, text.len()));
    }
    if json {
        return None;
    }
    let len = rest
        .find(|c: char| ends_token(c) || matches!(c, ';' | '&' | ',' | ')' | '}' | ']'))
        .unwrap_or(rest.len());
    (len > 0 && &rest[..len] != MASK).then_some((start, start + len))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn masks_the_password_in_a_postgres_uri() {
        assert_eq!(
            redact_secrets("postgres://u:pass@db.example.com:5432/app"),
            "postgres://u:***@db.example.com:5432/app"
        );
    }

    #[test]
    fn masks_mongo_srv_and_multi_host_uris() {
        assert_eq!(
            redact_secrets("mongodb+srv://admin:p%40ss@cluster0.x.net/?retryWrites=true"),
            "mongodb+srv://admin:***@cluster0.x.net/?retryWrites=true"
        );
        assert_eq!(
            redact_secrets("mongodb://u:pw@h1:27017,h2:27017/db?authSource=admin"),
            "mongodb://u:***@h1:27017,h2:27017/db?authSource=admin"
        );
    }

    #[test]
    fn a_password_holding_at_or_slash_is_masked_whole() {
        assert_eq!(
            redact_secrets("mysql://root:p@ss/w0rd@localhost:3306/db"),
            "mysql://root:***@localhost:3306/db"
        );
    }

    #[test]
    fn uris_without_a_password_are_unchanged() {
        for text in [
            "postgres://u@host/db",
            "mysql://root:@localhost",
            "sqlite:///Users/me/a@b.db",
            "https://example.com/path?q=1",
        ] {
            assert_eq!(redact_secrets(text), text);
        }
    }

    #[test]
    fn masks_every_uri_in_a_sentence() {
        assert_eq!(
            redact_secrets("from 'postgres://a:b@h1/x' to \"mysql://c:d@h2\" failed"),
            "from 'postgres://a:***@h1/x' to \"mysql://c:***@h2\" failed"
        );
    }

    #[test]
    fn masks_libpq_and_ado_style_dsns() {
        assert_eq!(
            redact_secrets("host=db port=5432 user=app password=hunter2 dbname=x"),
            "host=db port=5432 user=app password=*** dbname=x"
        );
        assert_eq!(
            redact_secrets("Server=db;User Id=sa;Password='a b;c';Database=x"),
            "Server=db;User Id=sa;Password='***';Database=x"
        );
        assert_eq!(
            redact_secrets("host=db password = \"q w\" sslmode=require"),
            "host=db password = \"***\" sslmode=require"
        );
    }

    #[test]
    fn masks_secret_query_parameters() {
        assert_eq!(
            redact_secrets("mysql://h/db?sslpassword=abc&ssl=true"),
            "mysql://h/db?sslpassword=***&ssl=true"
        );
        assert_eq!(
            redact_secrets("http://127.0.0.1:17321/mcp?token=abcdef"),
            "http://127.0.0.1:17321/mcp?token=***"
        );
    }

    #[test]
    fn masks_json_text_but_not_non_string_values() {
        assert_eq!(
            redact_secrets(r#"{"host":"h","password":"x\"y","passwordSet":true}"#),
            r#"{"host":"h","password":"***","passwordSet":true}"#
        );
    }

    #[test]
    fn leaves_ordinary_text_alone() {
        for text in [
            "bypass=1 mode=read",
            "opened session in 12ms at 10:42:01",
            "rows=50 truncated=false",
            "password=",
            "",
        ] {
            assert_eq!(redact_secrets(text), text);
        }
    }

    #[test]
    fn redaction_is_idempotent() {
        let once = redact_secrets("postgres://u:p@h password=x");
        assert_eq!(redact_secrets(&once), once);
    }

    #[test]
    fn secret_keys_ignore_case_and_separators() {
        for key in [
            "password",
            "PWD",
            "mcp.token",
            "ai.openai.api_key",
            "apiKey",
            "sslPassword",
        ] {
            assert!(is_secret_key(key), "{key}");
        }
        for key in ["host", "user", "install_id", "theme", "bypass", "mcp.port"] {
            assert!(!is_secret_key(key), "{key}");
        }
    }

    #[test]
    fn redact_json_masks_secret_fields_and_uris() {
        let value = json!({
            "host": "db",
            "password": "hunter2",
            "passwordSet": true,
            "uri": "postgres://u:p@h/db",
            "nested": [{ "apiKey": "k" }]
        });
        assert_eq!(
            redact_json(&value),
            json!({
                "host": "db",
                "password": "***",
                "passwordSet": true,
                "uri": "postgres://u:***@h/db",
                "nested": [{ "apiKey": "***" }]
            })
        );
    }
}
