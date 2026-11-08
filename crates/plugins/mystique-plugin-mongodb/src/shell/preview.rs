//! The short name the write-guard prompt shows for a Mongo statement.

use super::parse::{parse, Statement};
use super::value::Lit;

/// The method or command `op`, lowercased (`deletemany`, `bulkwrite`).
/// Text without one, or text that does not parse, shows its first 80
/// non-space characters.
pub(crate) fn op_preview(text: &str) -> String {
    let named = match parse(text) {
        Ok(Statement::Call(call)) => Some(call.method_lower()),
        Ok(stmt @ Statement::Command(_)) => match stmt.command_ops().last() {
            Some(Lit::Str(op)) if !op.is_empty() => Some(op.to_ascii_lowercase()),
            _ => None,
        },
        _ => None,
    };
    named.unwrap_or_else(|| compact(text))
}

fn compact(text: &str) -> String {
    let compact: String = text
        .chars()
        .filter(|c| !c.is_whitespace())
        .take(80)
        .collect();
    if compact.is_empty() {
        "write".into()
    } else {
        compact
    }
}
