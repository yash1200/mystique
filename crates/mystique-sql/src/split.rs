//! Split a SQL batch on `;` without breaking strings, comments, or dollar quotes.

use super::SqlDialect;

/// Split `sql` into statements. Original text is preserved (comments inside
/// a statement stay). Empty / comment-only chunks are dropped.
pub fn split_statements(sql: &str, dialect: SqlDialect) -> Vec<String> {
    split_via_scan(sql, dialect)
}

fn split_via_scan(sql: &str, dialect: SqlDialect) -> Vec<String> {
    let bytes = sql.as_bytes();
    let mut out = Vec::new();
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

fn push_stmt(out: &mut Vec<String>, sql: &str, start: usize, end: usize) {
    let stmt = sql[start..end].trim();
    if stmt.is_empty() || is_comment_only(stmt) {
        return;
    }
    out.push(stmt.to_string());
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
}
