//! Where one shell statement ends and the next begins, for Run all.
//!
//! A port of the editor's statement model,
//! `src/lib/engines/mongodb/statements.ts`, so Run all and run-at-cursor cut
//! a buffer the same way. Both are held to
//! `tests/fixtures/statements/cases.json`.
//!
//! - `;` ends a statement outside brackets.
//! - A line break outside brackets also ends one, unless the statement
//!   clearly goes on: the line ends with one of [`CONTINUES_AFTER`]
//!   (`db.orders.`, `x,`), or the next code starts with one of
//!   [`CONTINUES_BEFORE`] (a chained `.sort()` on its own line, a closer).
//!   A block comment that spans lines is a line break, as in JavaScript.
//! - `( [ {` nest, and a stray closer is ignored. Strings follow the lexer:
//!   backslash escapes, and an unclosed `'` / `"` ends at the line break.
//!   Template literals span lines and nest `${ … }`. Comments are opaque.
//! - Comment-only chunks are not statements. A comment above a statement,
//!   or after it on its line, is part of it, as in SQL.
//!
//! This scans characters instead of [`super::lex`] tokens: the lexer drops
//! offsets and refuses syntax the executor cannot run, and a buffer has to
//! be cut even when one of its statements will then fail to parse.

use std::ops::Range;

/// The line ends with one of these: the statement goes on.
pub(crate) const CONTINUES_AFTER: &str = ".,:=+-*/?&|!<>";

/// The next code starts with one of these: it belongs to this statement.
pub(crate) const CONTINUES_BEFORE: &str = ".)]},?:=+*&|<>";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Frame {
    Paren,
    Bracket,
    Brace,
    Template,
    /// `${` inside a template literal.
    Interpolation,
}

/// Byte ranges of the statements in `text`, in order.
pub(crate) fn split(text: &str) -> Vec<Range<usize>> {
    Splitter {
        src: text,
        out: Vec::new(),
        stack: Vec::new(),
        start: 0,
        last: None,
    }
    .run()
}

struct Splitter<'a> {
    src: &'a str,
    out: Vec<Range<usize>>,
    stack: Vec<Frame>,
    start: usize,
    /// Last code character of the current statement.
    last: Option<char>,
}

impl Splitter<'_> {
    fn run(mut self) -> Vec<Range<usize>> {
        let src = self.src;
        let bytes = src.as_bytes();
        let mut i = 0;
        while i < bytes.len() {
            let c = char_at(src, i);
            let top = self.stack.last().copied();
            if top == Some(Frame::Template) {
                match c {
                    '\\' => i = after_char(src, i + 1),
                    '`' => {
                        self.stack.pop();
                        self.last = Some(c);
                        i += 1;
                    }
                    '$' if bytes.get(i + 1) == Some(&b'{') => {
                        self.stack.push(Frame::Interpolation);
                        i += 2;
                    }
                    _ => i += c.len_utf8(),
                }
                continue;
            }
            match c {
                '/' if bytes.get(i + 1) == Some(&b'/') => i = line_end(src, i),
                '/' if bytes.get(i + 1) == Some(&b'*') => {
                    let end = block_comment_end(src, i);
                    let spans_lines = src.get(i + 2..end).is_some_and(|body| body.contains('\n'));
                    if self.stack.is_empty() && spans_lines {
                        // The comment leads the next statement.
                        self.line_break(i, i, end);
                    }
                    i = end;
                }
                '\'' | '"' => {
                    i = string_end(src, i + 1, c);
                    self.last = Some(c);
                }
                '`' | '(' | '[' | '{' => {
                    self.stack.push(match c {
                        '`' => Frame::Template,
                        '(' => Frame::Paren,
                        '[' => Frame::Bracket,
                        _ => Frame::Brace,
                    });
                    self.last = Some(c);
                    i += 1;
                }
                ')' | ']' | '}' => {
                    if c == '}' && top == Some(Frame::Interpolation) {
                        self.stack.pop();
                    } else {
                        self.close(match c {
                            ')' => Frame::Paren,
                            ']' => Frame::Bracket,
                            _ => Frame::Brace,
                        });
                    }
                    self.last = Some(c);
                    i += 1;
                }
                ';' if self.stack.is_empty() => {
                    self.finish(i);
                    self.start = i + 1;
                    i += 1;
                }
                '\n' if self.stack.is_empty() => {
                    self.line_break(i, i + 1, i + 1);
                    i += 1;
                }
                _ => {
                    if !is_space(c) {
                        self.last = Some(c);
                    }
                    i += c.len_utf8();
                }
            }
        }
        self.finish(bytes.len());
        self.out
    }

    /// Pop to the matching opener. A stray closer is ignored, and one never
    /// reaches past a template literal.
    fn close(&mut self, opener: Frame) {
        let stop = self.stack.iter().rposition(|frame| {
            *frame == opener || matches!(frame, Frame::Template | Frame::Interpolation)
        });
        if let Some(k) = stop {
            if self.stack.get(k) == Some(&opener) {
                self.stack.truncate(k);
            }
        }
    }

    /// A line break outside brackets: the statement ends at `end` and the
    /// next starts at `next`, unless it clearly goes on. The next code is
    /// looked for from `after`.
    fn line_break(&mut self, end: usize, next: usize, after: usize) {
        let Some(last) = self.last else {
            return;
        };
        if CONTINUES_AFTER.contains(last) {
            return;
        }
        if next_code(self.src, after).is_some_and(|c| CONTINUES_BEFORE.contains(c)) {
            return;
        }
        self.finish(end);
        self.start = next;
    }

    fn finish(&mut self, end: usize) {
        if self.last.take().is_some() {
            let chunk = self.src.get(self.start..end).unwrap_or_default();
            let trimmed = chunk.trim_start_matches(is_space);
            let from = self.start + (chunk.len() - trimmed.len());
            let to = from + trimmed.trim_end_matches(is_space).len();
            if from < to {
                self.out.push(from..to);
            }
        }
    }
}

/// The editor's whitespace: ASCII space and controls, NBSP and BOM.
fn is_space(c: char) -> bool {
    matches!(c, ' ' | '\t'..='\r' | '\u{a0}' | '\u{feff}')
}

fn char_at(src: &str, i: usize) -> char {
    src.get(i..)
        .and_then(|rest| rest.chars().next())
        .unwrap_or('\0')
}

/// Just past the character at `i`, or the end.
fn after_char(src: &str, i: usize) -> usize {
    if i >= src.len() {
        return src.len();
    }
    i + char_at(src, i).len_utf8()
}

fn line_end(src: &str, from: usize) -> usize {
    src.get(from..)
        .and_then(|rest| rest.find('\n'))
        .map_or(src.len(), |at| from + at)
}

fn block_comment_end(src: &str, from: usize) -> usize {
    src.get(from + 2..)
        .and_then(|rest| rest.find("*/"))
        .map_or(src.len(), |at| from + 2 + at + 2)
}

/// Just past the closing quote. An unclosed string stops before the newline.
fn string_end(src: &str, from: usize, quote: char) -> usize {
    let mut i = from;
    while i < src.len() {
        let c = char_at(src, i);
        if c == '\\' {
            i = after_char(src, i + 1);
            continue;
        }
        if c == quote {
            return i + 1;
        }
        if c == '\n' {
            return i;
        }
        i += c.len_utf8();
    }
    src.len()
}

/// The first code character at or after `from`, past space and comments.
fn next_code(src: &str, from: usize) -> Option<char> {
    let bytes = src.as_bytes();
    let mut i = from;
    while i < src.len() {
        let c = char_at(src, i);
        if is_space(c) {
            i += c.len_utf8();
        } else if c == '/' && bytes.get(i + 1) == Some(&b'/') {
            i = line_end(src, i);
        } else if c == '/' && bytes.get(i + 1) == Some(&b'*') {
            i = block_comment_end(src, i);
        } else {
            return Some(c);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::split;

    fn texts(text: &str) -> Vec<&str> {
        split(text).into_iter().map(|range| &text[range]).collect()
    }

    /// The corpus the editor's splitter is held to as well.
    #[test]
    fn corpus() {
        let raw = include_str!("../../tests/fixtures/statements/cases.json");
        let cases: Vec<serde_json::Value> = serde_json::from_str(raw).expect("cases.json");
        assert!(cases.len() >= 20, "corpus shrank to {}", cases.len());
        for case in &cases {
            let name = case["name"].as_str().expect("name");
            let text = case["text"].as_str().expect("text");
            let want: Vec<&str> = case["statements"]
                .as_array()
                .expect("statements")
                .iter()
                .map(|s| s.as_str().expect("statement"))
                .collect();
            assert_eq!(texts(text), want, "case `{name}`");
        }
    }

    #[test]
    fn ranges_are_byte_offsets() {
        let text = "db.t.find({ n: \"é\" })\ndb.t.find({})";
        let ranges = split(text);
        assert_eq!(ranges.len(), 2);
        assert_eq!(&text[ranges[1].clone()], "db.t.find({})");
        assert_eq!(ranges[1].start, "db.t.find({ n: \"é\" })\n".len());
    }

    #[test]
    fn nothing_to_run_is_no_statements() {
        assert!(split("").is_empty());
        assert!(split("  \n ;; \n// note\n/* block */").is_empty());
    }
}
