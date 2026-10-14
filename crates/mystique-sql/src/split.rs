//! Split a SQL batch on `;` without breaking strings, comments, or dollar quotes.

use super::SqlDialect;

/// One statement inside the original batch.
///
/// `start` and `end` are byte offsets of [`Self::text`] in `sql`
/// (`sql[start..end] == text`). Leading and trailing whitespace is not
/// part of the range. Cursor lookup uses the gaps between ranges so a
/// caret in that whitespace still belongs to a statement.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StatementSpan {
    pub start: usize,
    pub end: usize,
    pub text: String,
}

/// Split `sql` into statements. Original text is preserved (comments inside
/// a statement stay). Empty / comment-only chunks are dropped.
pub fn split_statements(sql: &str, dialect: SqlDialect) -> Vec<String> {
    split_statement_spans(sql, dialect)
        .into_iter()
        .map(|span| span.text)
        .collect()
}

/// Same split as [`split_statements`], with byte ranges into `sql`.
pub fn split_statement_spans(sql: &str, dialect: SqlDialect) -> Vec<StatementSpan> {
    split_via_scan(sql, dialect)
}

/// Statement whose region contains `cursor`.
///
/// The region runs from the previous statement's end (or 0) up to the next
/// statement's start. A caret inside `SELECT 2` of `SELECT 1; SELECT 2;`
/// selects the second span.
pub fn statement_at(spans: &[StatementSpan], cursor: usize) -> Option<&StatementSpan> {
    if spans.is_empty() {
        return None;
    }
    for (i, span) in spans.iter().enumerate() {
        let region_start = if i == 0 {
            0
        } else {
            spans.get(i - 1).map(|prev| prev.end).unwrap_or(0)
        };
        let region_end = spans
            .get(i + 1)
            .map(|next| next.start)
            .unwrap_or(usize::MAX);
        if cursor >= region_start && cursor < region_end {
            return Some(span);
        }
    }
    spans.last()
}

fn split_via_scan(sql: &str, dialect: SqlDialect) -> Vec<StatementSpan> {
    let bytes = sql.as_bytes();
    let mut out: Vec<StatementSpan> = Vec::new();
    let mut start = 0usize;
    let mut i = 0usize;
    let mut in_single = false;
    let mut in_double = false;
    let mut in_backtick = false;
    let mut dollar: Option<&[u8]> = None;

    while i < bytes.len() {
        let b = bytes[i];
        if let Some(tag) = dollar {
            if bytes[i..].starts_with(tag) {
                i += tag.len();
                dollar = None;
                continue;
            }
            i += 1;
            continue;
        }
        if in_single {
            if b == b'\'' {
                if i + 1 < bytes.len() && bytes[i + 1] == b'\'' {
                    i += 2;
                    continue;
                }
                in_single = false;
            }
            i += 1;
            continue;
        }
        if in_double {
            if b == b'"' {
                if i + 1 < bytes.len() && bytes[i + 1] == b'"' {
                    i += 2;
                    continue;
                }
                in_double = false;
            }
            i += 1;
            continue;
        }
        if in_backtick {
            if b == b'`' {
                if i + 1 < bytes.len() && bytes[i + 1] == b'`' {
                    i += 2;
                    continue;
                }
                in_backtick = false;
            }
            i += 1;
            continue;
        }
        if b == b'-' && i + 1 < bytes.len() && bytes[i + 1] == b'-' {
            i += 2;
            while i < bytes.len() && bytes[i] != b'\n' {
                i += 1;
            }
            continue;
        }
        if b == b'#' && dialect == SqlDialect::Mysql {
            while i < bytes.len() && bytes[i] != b'\n' {
                i += 1;
            }
            continue;
        }
        if b == b'/' && i + 1 < bytes.len() && bytes[i + 1] == b'*' {
            i += 2;
            while i + 1 < bytes.len() && !(bytes[i] == b'*' && bytes[i + 1] == b'/') {
                i += 1;
            }
            i = (i + 2).min(bytes.len());
            continue;
        }
        if b == b'$' {
            if let Some(tag_end) = dollar_tag_end(bytes, i) {
                dollar = Some(&bytes[i..tag_end]);
                i = tag_end;
                continue;
            }
        }
        match b {
            b'\'' => in_single = true,
            b'"' => in_double = true,
            b'`' => in_backtick = true,
            b';' => {
                push_stmt(&mut out, sql, start, i);
                start = i + 1;
            }
            _ => {}
        }
        i += 1;
    }
    push_stmt(&mut out, sql, start, sql.len());
    out
}

fn push_stmt(out: &mut Vec<StatementSpan>, sql: &str, start: usize, end: usize) {
    let raw = &sql[start..end];
    let trimmed_start = raw.trim_start();
    let lead = raw.len() - trimmed_start.len();
    let trimmed = trimmed_start.trim_end();
    let trail = trimmed_start.len() - trimmed.len();
    if trimmed.is_empty() || is_comment_only(trimmed) {
        return;
    }
    let span_start = start + lead;
    let span_end = end - trail;
    out.push(StatementSpan {
        start: span_start,
        end: span_end,
        text: trimmed.to_string(),
    });
}

fn is_comment_only(sql: &str) -> bool {
    let t = sql.trim();
    if t.is_empty() {
        return true;
    }
    if t.starts_with("--") && !t.contains('\n') {
        return true;
    }
    t.starts_with("/*") && t.ends_with("*/") && !t[2..t.len() - 2].contains("*/")
}

fn dollar_tag_end(bytes: &[u8], i: usize) -> Option<usize> {
    let mut j = i + 1;
    while j < bytes.len() && (bytes[j].is_ascii_alphanumeric() || bytes[j] == b'_') {
        j += 1;
    }
    if j < bytes.len() && bytes[j] == b'$' {
        Some(j + 1)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_on_semicolon() {
        let parts = split_statements("SELECT 1; DELETE FROM t;", SqlDialect::Postgres);
        assert_eq!(parts, vec!["SELECT 1", "DELETE FROM t"]);
    }

    #[test]
    fn keeps_semicolon_inside_string() {
        let parts = split_statements("SELECT ';'; SELECT 2", SqlDialect::Postgres);
        assert_eq!(parts, vec!["SELECT ';'", "SELECT 2"]);
    }

    #[test]
    fn keeps_dollar_quoted_body() {
        let sql = "SELECT $tag$ a; b $tag$; SELECT 2";
        let parts = split_statements(sql, SqlDialect::Postgres);
        assert_eq!(parts.len(), 2);
        assert!(parts[0].contains("a; b"), "{parts:?}");
    }

    #[test]
    fn drops_comment_only_chunk() {
        let parts = split_statements(
            "SELECT 1; -- just a comment\n; SELECT 2",
            SqlDialect::Generic,
        );
        assert_eq!(parts, vec!["SELECT 1", "SELECT 2"]);
    }

    #[test]
    fn dollar_quote_range_keeps_the_inner_semicolon() {
        let sql = "SELECT $$ a; b $$; SELECT 2";
        let spans = split_statement_spans(sql, SqlDialect::Postgres);
        assert_eq!(spans.len(), 2);
        assert_eq!(&sql[spans[0].start..spans[0].end], spans[0].text.as_str());
        assert!(spans[0].text.contains("a; b"), "{spans:?}");
        assert!(!spans[0].text.contains("SELECT 2"));
        assert_eq!(spans[1].text, "SELECT 2");
        assert_eq!(&sql[spans[1].start..spans[1].end], "SELECT 2");
    }

    #[test]
    fn cursor_in_the_second_statement_selects_only_that_text() {
        let sql = "SELECT 1; SELECT 2;";
        let spans = split_statement_spans(sql, SqlDialect::Postgres);
        let cursor = sql.find("SELECT 2").expect("second") + 3;
        let hit = statement_at(&spans, cursor).expect("span");
        assert_eq!(hit.text, "SELECT 2");
    }

    #[test]
    fn block_comment_and_mysql_hash_do_not_split() {
        let block = split_statement_spans("SELECT 1 /* ; */ ; SELECT 2", SqlDialect::Postgres);
        assert_eq!(
            block.iter().map(|s| s.text.as_str()).collect::<Vec<_>>(),
            vec!["SELECT 1 /* ; */", "SELECT 2"]
        );
        let hashed = split_statement_spans("SELECT 1; # ;\nSELECT 2", SqlDialect::Mysql);
        assert_eq!(hashed.len(), 2, "{hashed:?}");
        assert_eq!(hashed[0].text, "SELECT 1");
        assert!(
            hashed[1].text.contains("SELECT 2") && hashed[1].text.starts_with('#'),
            "hash comment stays with the statement and does not split: {hashed:?}"
        );
    }
}
