//! Where one shell statement ends and the next begins.
//!
//! The editor's copy is `src/lib/engines/mongodb/statements.ts`; both are
//! held to `tests/fixtures/statements/cases.json`, so run-at-cursor and
//! Run all cut a buffer the same way.
//!
//! A statement ends at a `;` outside brackets, or at a line break outside
//! brackets, as in JavaScript without semicolons. The line break does not end
//! it when the line ends with a character that needs more ([`CONTINUES_LINE`],
//! `db.orders.` or `x,`) or the next line starts with one that cannot begin a
//! statement ([`CONTINUES_NEXT`], `.find()`). Strings and comments are read
//! with the lexer's rules, so a `;` or a line break inside them is text. A
//! range covers the statement's first to last code character: whitespace,
//! comments around it and the `;` are outside it. Empty and comment-only
//! chunks yield nothing.

use std::ops::Range;

use super::lex::{skim, Skim};

/// The last character of a line that keeps the statement going.
pub(crate) const CONTINUES_LINE: &str = ".,([{:?+-*/%&|^!~=<>";

/// The first character of a line that continues the previous statement.
pub(crate) const CONTINUES_NEXT: &str = ".,)]}:?+-*/%&|^=<>";

/// Byte ranges of each statement in `text`, in order.
pub(crate) fn split(text: &str) -> Vec<Range<usize>> {
    let mut out = Vec::new();
    let mut current: Option<Range<usize>> = None;
    let mut depth = 0usize;
    let mut last = '\0';
    let mut line_break = false;
    skim(text, |piece, start, end| {
        let first = match piece {
            Skim::Newline => {
                line_break |= depth == 0 && current.is_some();
                return;
            }
            Skim::Code(';') if depth == 0 => {
                out.extend(current.take());
                line_break = false;
                return;
            }
            Skim::Code(c) => c,
            Skim::Str => '"',
        };
        if std::mem::take(&mut line_break)
            && !CONTINUES_LINE.contains(last)
            && !CONTINUES_NEXT.contains(first)
        {
            out.extend(current.take());
        }
        match &mut current {
            Some(range) => range.end = end,
            None => current = Some(start..end),
        }
        last = first;
        match first {
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' => depth = depth.saturating_sub(1),
            _ => {}
        }
    });
    out.extend(current);
    out
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
