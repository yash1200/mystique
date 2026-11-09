//! Split a SQL batch into statements without breaking strings, comments,
//! dollar quotes, or routine bodies.
//!
//! The editor has a TypeScript copy (`src/lib/engines/sql/split.ts`). Both
//! run the fixture corpus in `tests/fixtures/split`, so they cannot drift.
//!
//! Dialect rules:
//! - `--` and `/* … */` comments everywhere. Postgres nests block comments.
//!   MySQL needs whitespace after `--` and also has `#` line comments.
//! - MySQL `/*! … */`, `/*M! … */` and `/*T! … */` are run by the server, so
//!   a chunk holding only one of them is a statement, not a comment.
//! - `'…'` doubles its quote. MySQL also takes backslash escapes in `'…'` and
//!   `"…"`; Postgres only in `E'…'`.
//! - Postgres and generic SQL have dollar quotes (`$$ … $$`, `$tag$ … $tag$`).
//!   SQLite has `[name]` identifiers.
//! - Postgres (like psql) does not end a statement inside parentheses.
//! - The `BEGIN … END` body of `CREATE PROCEDURE / FUNCTION / TRIGGER / EVENT`
//!   (and MariaDB's `BEGIN NOT ATOMIC … END`) stays one statement. A leading
//!   `BEGIN;` is still a transaction. Inside a body `BEGIN` and `CASE` nest;
//!   `END IF / LOOP / WHILE / REPEAT / FOR` closes nothing that was counted.
//! - MySQL client `DELIMITER xx` lines are dropped. Later statements end at
//!   `xx` (and not at `;`) until `DELIMITER ;`.

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
    let mut scan = Scanner {
        sql,
        b: sql.as_bytes(),
        dialect,
        out: Vec::new(),
        start: 0,
        delim: b";".to_vec(),
        stmt: Stmt::default(),
    };
    scan.run();
    scan.out
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

/// What the current statement has shown so far.
#[derive(Default)]
struct Stmt {
    paren: u32,
    /// Open `BEGIN` / `CASE` inside a routine body.
    block: u32,
    first: First,
    /// `PROCEDURE`, `FUNCTION`, `TRIGGER`, `EVENT` or `PACKAGE` seen.
    routine: bool,
}

#[derive(Default, PartialEq, Eq)]
enum First {
    #[default]
    None,
    /// `CREATE` or `ALTER`: may define a routine with a body.
    Ddl,
    Other,
}

struct Scanner<'a> {
    sql: &'a str,
    b: &'a [u8],
    dialect: SqlDialect,
    out: Vec<StatementSpan>,
    start: usize,
    delim: Vec<u8>,
    stmt: Stmt,
}

impl Scanner<'_> {
    fn custom_delimiter(&self) -> bool {
        self.delim.as_slice() != b";"
    }

    fn run(&mut self) {
        let b = self.b;
        let mysql = self.dialect == SqlDialect::Mysql;
        let mut i = 0usize;
        while i < b.len() {
            let custom = self.custom_delimiter();
            if custom && b[i..].starts_with(&self.delim) {
                self.finish(i);
                i += self.delim.len();
                self.start = i;
                continue;
            }
            let c = b[i];
            i = match c {
                b'\'' | b'"' => skip_quoted(b, i + 1, c, mysql),
                b'`' => skip_quoted(b, i + 1, c, false),
                b'[' if self.dialect == SqlDialect::Sqlite => skip_until(b, i + 1, b']'),
                b'-' if line_comment_at(b, i, self.dialect) => skip_line(b, i),
                b'#' if mysql => skip_line(b, i),
                b'/' if b.get(i + 1) == Some(&b'*') => skip_block_comment(b, i, self.dialect),
                b'$' if dollar_quotes(self.dialect) => match dollar_tag_end(b, i) {
                    Some(tag_end) => skip_dollar(b, i, tag_end),
                    None => i + 1,
                },
                b'(' => {
                    self.stmt.paren += 1;
                    i + 1
                }
                b')' => {
                    self.stmt.paren = self.stmt.paren.saturating_sub(1);
                    i + 1
                }
                b';' if !custom && self.ends_here() => {
                    self.finish(i);
                    self.start = i + 1;
                    i + 1
                }
                _ if is_word_byte(c) => self.word(i),
                _ => i + 1,
            };
        }
        self.finish(b.len());
    }

    /// A `;` here ends the statement.
    fn ends_here(&self) -> bool {
        self.stmt.block == 0 && (self.stmt.paren == 0 || self.dialect != SqlDialect::Postgres)
    }

    /// Read the word at `i` and act on keywords. Returns the next index.
    fn word(&mut self, i: usize) -> usize {
        let b = self.b;
        let mut end = word_end(b, i);
        if self.custom_delimiter() {
            // `END$$` with `DELIMITER $$`: the word stops at the delimiter.
            if let Some(at) = (i + 1..end).find(|&k| b[k..].starts_with(&self.delim)) {
                end = at;
            }
        }
        let word = &b[i..end];
        if b[i].is_ascii_digit() || (i > 0 && b[i - 1] == b'.') {
            return end;
        }
        if self.dialect == SqlDialect::Postgres
            && word.eq_ignore_ascii_case(b"e")
            && b.get(end) == Some(&b'\'')
        {
            return skip_quoted(b, end + 1, b'\'', true);
        }
        if self.dialect == SqlDialect::Mysql && word.eq_ignore_ascii_case(b"delimiter") {
            if let Some(next) = self.delimiter_command(i, end) {
                return next;
            }
        }
        let custom = self.custom_delimiter();
        if self.stmt.first == First::None {
            if word.eq_ignore_ascii_case(b"create") || word.eq_ignore_ascii_case(b"alter") {
                self.stmt.first = First::Ddl;
                return end;
            }
            self.stmt.first = First::Other;
            if !custom && word.eq_ignore_ascii_case(b"begin") {
                // MariaDB anonymous block: `BEGIN NOT ATOMIC … END`.
                let (not, after) = next_word(b, end);
                let (atomic, _) = next_word(b, after);
                if not.eq_ignore_ascii_case(b"not") && atomic.eq_ignore_ascii_case(b"atomic") {
                    self.stmt.block = 1;
                }
            }
            return end;
        }
        if custom {
            return end;
        }
        if self.stmt.block == 0 {
            if self.stmt.paren != 0 || self.stmt.first != First::Ddl {
                return end;
            }
            if ROUTINES.iter().any(|kw| word.eq_ignore_ascii_case(kw)) {
                self.stmt.routine = true;
            } else if self.stmt.routine && word.eq_ignore_ascii_case(b"begin") {
                self.stmt.block = 1;
            }
            return end;
        }
        if word.eq_ignore_ascii_case(b"begin") || word.eq_ignore_ascii_case(b"case") {
            self.stmt.block += 1;
            return end;
        }
        if word.eq_ignore_ascii_case(b"end") {
            let (next, after) = next_word(b, end);
            if UNCOUNTED_ENDS
                .iter()
                .any(|kw| next.eq_ignore_ascii_case(kw))
            {
                return after;
            }
            self.stmt.block -= 1;
            if next.eq_ignore_ascii_case(b"case") {
                return after;
            }
        }
        end
    }

    /// `DELIMITER xx` at the start of a line. Returns the index after the
    /// line, or `None` when this is not the client command.
    fn delimiter_command(&mut self, i: usize, end: usize) -> Option<usize> {
        let b = self.b;
        if self.stmt.block != 0 || !at_line_start(b, i) {
            return None;
        }
        if !matches!(b.get(end), Some(b' ' | b'\t')) {
            return None;
        }
        let mut k = end;
        while matches!(b.get(k), Some(b' ' | b'\t')) {
            k += 1;
        }
        let token_start = k;
        while k < b.len() && !matches!(b[k], b' ' | b'\t' | b'\r' | b'\n') {
            k += 1;
        }
        if k == token_start {
            return None;
        }
        let token = b[token_start..k].to_vec();
        while k < b.len() && b[k] != b'\n' {
            k += 1;
        }
        self.finish(i);
        self.delim = token;
        self.start = k;
        Some(k)
    }

    fn finish(&mut self, end: usize) {
        push_stmt(&mut self.out, self.sql, self.start, end, self.dialect);
        self.stmt = Stmt::default();
    }
}

const ROUTINES: [&[u8]; 5] = [b"procedure", b"function", b"trigger", b"event", b"package"];
const UNCOUNTED_ENDS: [&[u8]; 5] = [b"if", b"loop", b"while", b"repeat", b"for"];

fn dollar_quotes(dialect: SqlDialect) -> bool {
    matches!(dialect, SqlDialect::Postgres | SqlDialect::Generic)
}

/// Identifier byte. Every byte of a non-ASCII character counts, so a word
/// never ends inside one.
fn is_word_byte(c: u8) -> bool {
    c.is_ascii_alphanumeric() || c == b'_' || c == b'$' || c >= 0x80
}

fn word_end(b: &[u8], mut i: usize) -> usize {
    while i < b.len() && is_word_byte(b[i]) {
        i += 1;
    }
    i
}

/// The next word after ASCII whitespace, and the index after it.
fn next_word(b: &[u8], mut i: usize) -> (&[u8], usize) {
    while i < b.len() && b[i].is_ascii_whitespace() {
        i += 1;
    }
    let end = if i < b.len() && is_word_byte(b[i]) && b[i] != b'$' {
        word_end(b, i)
    } else {
        i
    };
    (&b[i..end], end)
}

fn at_line_start(b: &[u8], i: usize) -> bool {
    let mut k = i;
    while k > 0 && matches!(b[k - 1], b' ' | b'\t') {
        k -= 1;
    }
    k == 0 || b[k - 1] == b'\n'
}

/// `--` starts a comment. MySQL wants whitespace (or the end) after it.
fn line_comment_at(b: &[u8], i: usize, dialect: SqlDialect) -> bool {
    if b.get(i) != Some(&b'-') || b.get(i + 1) != Some(&b'-') {
        return false;
    }
    dialect != SqlDialect::Mysql || b.get(i + 2).is_none_or(|&c| c <= b' ')
}

fn skip_line(b: &[u8], mut i: usize) -> usize {
    while i < b.len() && b[i] != b'\n' {
        i += 1;
    }
    i
}

fn skip_until(b: &[u8], mut i: usize, close: u8) -> usize {
    while i < b.len() && b[i] != close {
        i += 1;
    }
    (i + 1).min(b.len())
}

/// From just after the opening quote to just after the closing one.
fn skip_quoted(b: &[u8], mut i: usize, quote: u8, backslash: bool) -> usize {
    while i < b.len() {
        let c = b[i];
        if backslash && c == b'\\' {
            i += 2;
            continue;
        }
        if c == quote {
            if b.get(i + 1) == Some(&quote) {
                i += 2;
                continue;
            }
            return i + 1;
        }
        i += 1;
    }
    b.len()
}

/// `i` is at `/*`. Postgres nests block comments.
fn skip_block_comment(b: &[u8], mut i: usize, dialect: SqlDialect) -> usize {
    let nests = dialect == SqlDialect::Postgres;
    let mut depth = 1u32;
    i += 2;
    while i < b.len() {
        if nests && b[i] == b'/' && b.get(i + 1) == Some(&b'*') {
            depth += 1;
            i += 2;
            continue;
        }
        if b[i] == b'*' && b.get(i + 1) == Some(&b'/') {
            depth -= 1;
            i += 2;
            if depth == 0 {
                return i;
            }
            continue;
        }
        i += 1;
    }
    b.len()
}

/// `$` + optional tag + `$`. The tag cannot start with a digit (`$1` is a
/// parameter).
fn dollar_tag_end(b: &[u8], i: usize) -> Option<usize> {
    let mut j = i + 1;
    if b.get(j).is_some_and(u8::is_ascii_digit) {
        return None;
    }
    while j < b.len() && is_word_byte(b[j]) && b[j] != b'$' {
        j += 1;
    }
    (b.get(j) == Some(&b'$')).then_some(j + 1)
}

fn skip_dollar(b: &[u8], i: usize, tag_end: usize) -> usize {
    let tag = &b[i..tag_end];
    let mut k = tag_end;
    while k < b.len() {
        if b[k..].starts_with(tag) {
            return k + tag.len();
        }
        k += 1;
    }
    b.len()
}

/// Whitespace for trimming. Same set as the TypeScript copy.
fn is_space(c: char) -> bool {
    c.is_whitespace() || c == '\u{feff}'
}

fn push_stmt(
    out: &mut Vec<StatementSpan>,
    sql: &str,
    start: usize,
    end: usize,
    dialect: SqlDialect,
) {
    if start >= end {
        return;
    }
    let raw = &sql[start..end];
    let trimmed_start = raw.trim_start_matches(is_space);
    let lead = raw.len() - trimmed_start.len();
    let trimmed = trimmed_start.trim_end_matches(is_space);
    let trail = trimmed_start.len() - trimmed.len();
    if !has_code(trimmed, dialect) {
        return;
    }
    out.push(StatementSpan {
        start: start + lead,
        end: end - trail,
        text: trimmed.to_string(),
    });
}

/// Anything other than whitespace and comments. A MySQL versioned comment
/// (`/*!`, `/*M!`, `/*T!`) counts: the server runs it.
fn has_code(text: &str, dialect: SqlDialect) -> bool {
    let b = text.as_bytes();
    let mysql = dialect == SqlDialect::Mysql;
    let mut i = 0usize;
    while i < b.len() {
        let c = b[i];
        if line_comment_at(b, i, dialect) || (mysql && c == b'#') {
            i = skip_line(b, i);
        } else if c == b'/' && b.get(i + 1) == Some(&b'*') {
            if mysql && versioned_comment(&b[i + 2..]) {
                return true;
            }
            i = skip_block_comment(b, i, dialect);
        } else {
            // Comments end on ASCII bytes, so `i` is on a char boundary.
            match text[i..].chars().next() {
                Some(ch) if is_space(ch) => i += ch.len_utf8(),
                _ => return true,
            }
        }
    }
    false
}

fn versioned_comment(rest: &[u8]) -> bool {
    match rest {
        [b'!', ..] => true,
        [m, b'!', ..] => m.eq_ignore_ascii_case(&b'm') || m.eq_ignore_ascii_case(&b't'),
        _ => false,
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
        let versioned = split_statement_spans("/*!50000 SELECT 1 */", SqlDialect::Mysql);
        assert_eq!(versioned.len(), 1, "{versioned:?}");
        assert!(versioned[0].text.contains("SELECT 1"));

        assert_eq!(hashed.len(), 2, "{hashed:?}");
        assert_eq!(hashed[0].text, "SELECT 1");
        assert!(
            hashed[1].text.contains("SELECT 2") && hashed[1].text.starts_with('#'),
            "hash comment stays with the statement and does not split: {hashed:?}"
        );
    }

    #[test]
    fn transaction_begin_is_not_a_body() {
        let parts = split_statements("BEGIN; UPDATE t SET a = 1; END;", SqlDialect::Postgres);
        assert_eq!(parts, vec!["BEGIN", "UPDATE t SET a = 1", "END"]);
    }

    #[test]
    fn delimiter_lines_are_not_statements() {
        let sql =
            "DELIMITER //\nCREATE PROCEDURE p() BEGIN SELECT 1; END//\nDELIMITER ;\nCALL p();";
        let spans = split_statement_spans(sql, SqlDialect::Mysql);
        let texts: Vec<_> = spans.iter().map(|s| s.text.as_str()).collect();
        assert_eq!(
            texts,
            vec!["CREATE PROCEDURE p() BEGIN SELECT 1; END", "CALL p()"]
        );
        for span in &spans {
            assert_eq!(&sql[span.start..span.end], span.text);
        }
    }
}
