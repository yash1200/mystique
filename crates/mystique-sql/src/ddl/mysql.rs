//! MySQL-only ALTER pieces: MODIFY COLUMN rebuilt from the full definition.

use super::statements::quote;
use super::types::{ColumnDraft, ColumnOriginal};
use super::DdlTarget;

/// `MODIFY COLUMN` with the new type and NULL-ness. `Err` is the warning shown
/// when the original definition is not known.
pub(super) fn modify_column(
    dialect: DdlTarget,
    table: &str,
    original: &ColumnOriginal,
    column: &ColumnDraft,
) -> Result<String, String> {
    let definition = mysql_modify_definition(original, column)?;
    Ok(format!(
        "ALTER TABLE {table} MODIFY COLUMN {} {definition}",
        quote(dialect, &column.name),
    ))
}

/// The full MySQL column definition for MODIFY: the new type and NULL-ness,
/// with every other attribute copied from the original definition.
pub(super) fn mysql_modify_definition(
    original: &ColumnOriginal,
    column: &ColumnDraft,
) -> Result<String, String> {
    let reload = || {
        format!(
            "Reload the table before changing column {}. MODIFY needs its full definition, or it drops DEFAULT, AUTO_INCREMENT, COMMENT, and charset.",
            original.name
        )
    };
    let definition = original.definition.as_deref().ok_or_else(reload)?.trim();
    let old_type = original.data_type.trim();
    let tail = definition
        .get(..old_type.len())
        .filter(|head| head.eq_ignore_ascii_case(old_type))
        .and_then(|_| definition.get(old_type.len()..))
        .filter(|rest| rest.is_empty() || rest.starts_with(' '))
        .ok_or_else(reload)?;
    let mut words = top_level_words(tail);
    let is = |words: &[String], at: usize, want: &str| {
        words
            .get(at)
            .is_some_and(|word| word.eq_ignore_ascii_case(want))
    };

    let new_type = column.data_type.trim();
    if !new_type.eq_ignore_ascii_case(old_type) && !is_text_type(new_type) {
        // CHARACTER SET / COLLATE only apply to text types.
        let mut at = 0;
        while at < words.len() {
            let span = if is(&words, at, "CHARACTER") && is(&words, at + 1, "SET") {
                3
            } else if is(&words, at, "CHARSET") || is(&words, at, "COLLATE") {
                2
            } else {
                at += 1;
                continue;
            };
            words.drain(at..(at + span).min(words.len()));
        }
    }

    // Take out the old NULL / NOT NULL; remember where it sat.
    let mut slot = None;
    let mut at = 0;
    while at < words.len() {
        if is(&words, at, "NOT") && is(&words, at + 1, "NULL") {
            words.drain(at..at + 2);
            slot.get_or_insert(at);
        } else if is(&words, at, "NULL") && !(at > 0 && is(&words, at - 1, "DEFAULT")) {
            words.remove(at);
            slot.get_or_insert(at);
        } else if !column.nullable && is(&words, at, "DEFAULT") && is(&words, at + 1, "NULL") {
            words.drain(at..at + 2);
        } else {
            at += 1;
        }
    }
    let slot = slot.unwrap_or_else(|| {
        words
            .iter()
            .position(|word| {
                ["DEFAULT", "ON", "AUTO_INCREMENT", "COMMENT"]
                    .iter()
                    .any(|key| word.eq_ignore_ascii_case(key))
                    || word.starts_with("/*")
            })
            .unwrap_or(words.len())
    });
    let null_sql = if column.nullable { "NULL" } else { "NOT NULL" };
    words.insert(slot.min(words.len()), null_sql.to_string());
    Ok(format!("{new_type} {}", words.join(" ")))
}

fn is_text_type(data_type: &str) -> bool {
    let lower = data_type.trim().to_ascii_lowercase();
    [
        "char",
        "varchar",
        "tinytext",
        "text",
        "mediumtext",
        "longtext",
        "enum",
        "set",
    ]
    .iter()
    .any(|name| {
        lower
            .strip_prefix(name)
            .is_some_and(|rest| rest.is_empty() || rest.starts_with(['(', ' ']))
    })
}

/// Split on spaces outside quotes, parentheses, and comments.
fn top_level_words(text: &str) -> Vec<String> {
    let mut words = Vec::new();
    let mut word = String::new();
    let mut depth = 0usize;
    let mut quote: Option<char> = None;
    let mut chars = text.chars().peekable();
    while let Some(ch) = chars.next() {
        if let Some(open) = quote {
            word.push(ch);
            if ch == '\\' && open != '`' {
                if let Some(next) = chars.next() {
                    word.push(next);
                }
            } else if open == '*' {
                if ch == '*' && chars.peek() == Some(&'/') {
                    word.push('/');
                    chars.next();
                    quote = None;
                }
            } else if ch == open {
                // A doubled quote stays inside the string.
                if chars.peek() == Some(&open) {
                    word.push(open);
                    chars.next();
                } else {
                    quote = None;
                }
            }
            continue;
        }
        match ch {
            '\'' | '"' | '`' => {
                quote = Some(ch);
                word.push(ch);
            }
            '/' if chars.peek() == Some(&'*') => {
                quote = Some('*');
                word.push(ch);
                word.push('*');
                chars.next();
            }
            '(' => {
                depth += 1;
                word.push(ch);
            }
            ')' => {
                depth = depth.saturating_sub(1);
                word.push(ch);
            }
            ch if ch.is_whitespace() && depth == 0 => {
                if !word.is_empty() {
                    words.push(std::mem::take(&mut word));
                }
            }
            _ => word.push(ch),
        }
    }
    if !word.is_empty() {
        words.push(word);
    }
    words
}
