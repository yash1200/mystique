//! Tokens for Mongo shell text. Comments and whitespace are dropped here, so
//! nothing after this sees a `)` or `;` inside a string or a comment.

use mystique_connector_api::{ConnectorError, ConnectorResult};

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Tok {
    /// Identifier. `$` counts as a letter, as in JavaScript.
    Ident(String),
    /// String literal with escapes decoded. Quote style is not kept.
    Str(String),
    /// Number as written, sign included.
    Num(String),
    /// One of `{ } [ ] ( ) . , : ;`.
    Punct(char),
}

pub(crate) fn lex(text: &str) -> ConnectorResult<Vec<Tok>> {
    let mut out = Vec::new();
    let mut chars = text.char_indices().peekable();
    while let Some((_, c)) = chars.next() {
        if c.is_whitespace() {
            continue;
        }
        if c == '/' {
            match chars.peek().map(|&(_, n)| n) {
                Some('/') => {
                    for (_, n) in chars.by_ref() {
                        if n == '\n' {
                            break;
                        }
                    }
                    continue;
                }
                Some('*') => {
                    chars.next();
                    let mut prev = '\0';
                    let mut closed = false;
                    for (_, n) in chars.by_ref() {
                        if prev == '*' && n == '/' {
                            closed = true;
                            break;
                        }
                        prev = n;
                    }
                    if !closed {
                        return Err(query_err("unclosed `/*` comment in Mongo query"));
                    }
                    continue;
                }
                _ => {
                    return Err(query_err(
                        "regular expression literals are not supported; use { $regex: \"…\" }",
                    ))
                }
            }
        }
        if c == '"' || c == '\'' || c == '`' {
            let value = read_string(&mut chars, c)?;
            out.push(Tok::Str(value));
            continue;
        }
        if c.is_ascii_digit()
            || (c == '-' && chars.peek().is_some_and(|&(_, n)| n.is_ascii_digit()))
        {
            let mut raw = String::from(c);
            while let Some(&(_, n)) = chars.peek() {
                let sign_after_exp = (n == '+' || n == '-') && raw.ends_with(['e', 'E']);
                if n.is_ascii_digit() || n == '.' || n == 'e' || n == 'E' || sign_after_exp {
                    raw.push(n);
                    chars.next();
                } else {
                    break;
                }
            }
            out.push(Tok::Num(raw));
            continue;
        }
        if is_ident_start(c) {
            let mut name = String::from(c);
            while let Some(&(_, n)) = chars.peek() {
                if is_ident_continue(n) {
                    name.push(n);
                    chars.next();
                } else {
                    break;
                }
            }
            out.push(Tok::Ident(name));
            continue;
        }
        if matches!(c, '{' | '}' | '[' | ']' | '(' | ')' | '.' | ',' | ':' | ';') {
            out.push(Tok::Punct(c));
            continue;
        }
        return Err(query_err(&format!("unexpected `{c}` in Mongo query")));
    }
    Ok(out)
}

fn is_ident_start(c: char) -> bool {
    c.is_alphabetic() || c == '_' || c == '$'
}

fn is_ident_continue(c: char) -> bool {
    c.is_alphanumeric() || c == '_' || c == '$'
}

fn read_string(
    chars: &mut std::iter::Peekable<std::str::CharIndices<'_>>,
    quote: char,
) -> ConnectorResult<String> {
    let mut out = String::new();
    while let Some((_, c)) = chars.next() {
        if c == quote {
            return Ok(out);
        }
        if c == '\n' && quote != '`' {
            break;
        }
        if quote == '`' && c == '$' && chars.peek().is_some_and(|&(_, n)| n == '{') {
            return Err(query_err(
                "template literals with `${…}` are not supported in Mongo queries",
            ));
        }
        if c != '\\' {
            out.push(c);
            continue;
        }
        let Some((_, esc)) = chars.next() else {
            break;
        };
        match esc {
            'n' => out.push('\n'),
            't' => out.push('\t'),
            'r' => out.push('\r'),
            'b' => out.push('\u{8}'),
            'f' => out.push('\u{c}'),
            'v' => out.push('\u{b}'),
            '0' => out.push('\0'),
            '\n' => {}
            'u' => out.push(read_unicode(chars)?),
            'x' => {
                let hex: String = (0..2)
                    .filter_map(|_| chars.next().map(|(_, h)| h))
                    .collect();
                let ch = u32::from_str_radix(&hex, 16)
                    .ok()
                    .and_then(char::from_u32)
                    .ok_or_else(|| query_err("invalid `\\x` escape in Mongo query"))?;
                out.push(ch);
            }
            other => out.push(other),
        }
    }
    Err(query_err("unclosed string in Mongo query"))
}

/// `\uXXXX`, a surrogate pair of them, or `\u{…}`.
fn read_unicode(
    chars: &mut std::iter::Peekable<std::str::CharIndices<'_>>,
) -> ConnectorResult<char> {
    let bad = || query_err("invalid `\\u` escape in Mongo query");
    if chars.peek().is_some_and(|&(_, c)| c == '{') {
        chars.next();
        let mut hex = String::new();
        for (_, c) in chars.by_ref() {
            if c == '}' {
                break;
            }
            hex.push(c);
        }
        return u32::from_str_radix(&hex, 16)
            .ok()
            .and_then(char::from_u32)
            .ok_or_else(bad);
    }
    let first = hex4(chars).ok_or_else(bad)?;
    if (0xD800..0xDC00).contains(&first) {
        let mut ahead = chars.clone();
        if ahead.next().map(|(_, c)| c) == Some('\\') && ahead.next().map(|(_, c)| c) == Some('u') {
            if let Some(low) = hex4(&mut ahead).filter(|n| (0xDC00..0xE000).contains(n)) {
                *chars = ahead;
                let code = 0x10000 + ((first - 0xD800) << 10) + (low - 0xDC00);
                return char::from_u32(code).ok_or_else(bad);
            }
        }
        return Err(bad());
    }
    char::from_u32(first).ok_or_else(bad)
}

fn hex4(chars: &mut std::iter::Peekable<std::str::CharIndices<'_>>) -> Option<u32> {
    let hex: String = (0..4)
        .filter_map(|_| chars.next().map(|(_, h)| h))
        .collect();
    if hex.len() != 4 {
        return None;
    }
    u32::from_str_radix(&hex, 16).ok()
}

pub(crate) fn query_err(msg: &str) -> mystique_connector_api::ConnectorError {
    ConnectorError::Query(msg.to_string())
}
