//! SQLite schema text: a tokenizer and a `CREATE TABLE` parser just deep
//! enough to edit a table definition and keep everything else as written.
//!
//! The rebuild (see `sqlite`) starts from the `sqlite_master.sql` text, so
//! constraints, COLLATE, CHECK, DEFAULT, generated columns, AUTOINCREMENT,
//! `WITHOUT ROWID`, and `STRICT` survive unless the draft changes them.
//! Anything the parser does not understand is an error, and the planner
//! refuses the rebuild instead of guessing.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Tok {
    /// Bare or quoted identifier (also keywords).
    Ident,
    /// `'string'` or `x'blob'`.
    Str,
    Num,
    Open,
    Close,
    Comma,
    Dot,
    Semi,
    Other,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct Token {
    pub kind: Tok,
    pub start: usize,
    pub end: usize,
    pub quoted: bool,
}

/// Split SQL into tokens. Whitespace and comments are skipped; the spans
/// keep the original text between tokens available.
pub(crate) fn tokenize(sql: &str) -> Result<Vec<Token>, String> {
    let bytes = sql.as_bytes();
    let mut out = Vec::new();
    let mut at = 0usize;
    let push = |out: &mut Vec<Token>, kind, start, end, quoted| {
        out.push(Token {
            kind,
            start,
            end,
            quoted,
        });
    };
    while at < bytes.len() {
        let ch = bytes[at];
        if ch.is_ascii_whitespace() {
            at += 1;
            continue;
        }
        if ch == b'-' && bytes.get(at + 1) == Some(&b'-') {
            while at < bytes.len() && bytes[at] != b'\n' {
                at += 1;
            }
            continue;
        }
        if ch == b'/' && bytes.get(at + 1) == Some(&b'*') {
            let end = sql[at + 2..]
                .find("*/")
                .map(|offset| at + 2 + offset + 2)
                .unwrap_or(bytes.len());
            at = end;
            continue;
        }
        let start = at;
        match ch {
            b'\'' => {
                at = quoted_end(bytes, at, b'\'')?;
                push(&mut out, Tok::Str, start, at, false);
            }
            b'x' | b'X' if bytes.get(at + 1) == Some(&b'\'') => {
                at = quoted_end(bytes, at + 1, b'\'')?;
                push(&mut out, Tok::Str, start, at, false);
            }
            b'"' | b'`' => {
                at = quoted_end(bytes, at, ch)?;
                push(&mut out, Tok::Ident, start, at, true);
            }
            b'[' => {
                let close = sql[at..]
                    .find(']')
                    .ok_or_else(|| "An identifier in brackets is not closed.".to_string())?;
                at += close + 1;
                push(&mut out, Tok::Ident, start, at, true);
            }
            b'(' => {
                at += 1;
                push(&mut out, Tok::Open, start, at, false);
            }
            b')' => {
                at += 1;
                push(&mut out, Tok::Close, start, at, false);
            }
            b',' => {
                at += 1;
                push(&mut out, Tok::Comma, start, at, false);
            }
            b';' => {
                at += 1;
                push(&mut out, Tok::Semi, start, at, false);
            }
            b'.' if !bytes.get(at + 1).is_some_and(u8::is_ascii_digit) => {
                at += 1;
                push(&mut out, Tok::Dot, start, at, false);
            }
            b'0'..=b'9' | b'.' => {
                at += 1;
                while at < bytes.len() {
                    let next = bytes[at];
                    let exponent_sign = (next == b'+' || next == b'-')
                        && matches!(bytes[at - 1], b'e' | b'E')
                        && !sql[start..at].starts_with("0x")
                        && !sql[start..at].starts_with("0X");
                    if next.is_ascii_alphanumeric() || next == b'.' || next == b'_' || exponent_sign
                    {
                        at += 1;
                    } else {
                        break;
                    }
                }
                push(&mut out, Tok::Num, start, at, false);
            }
            _ if ch == b'_' || ch.is_ascii_alphabetic() || ch >= 0x80 => {
                at += 1;
                while at < bytes.len() {
                    let next = bytes[at];
                    if next == b'_' || next == b'$' || next.is_ascii_alphanumeric() || next >= 0x80
                    {
                        at += 1;
                    } else {
                        break;
                    }
                }
                push(&mut out, Tok::Ident, start, at, false);
            }
            _ => {
                // Operators: one or two characters; the parser never looks inside.
                at += 1;
                while at < bytes.len() && !sql.is_char_boundary(at) {
                    at += 1;
                }
                push(&mut out, Tok::Other, start, at, false);
            }
        }
    }
    Ok(out)
}

fn quoted_end(bytes: &[u8], open: usize, quote: u8) -> Result<usize, String> {
    let mut at = open + 1;
    while at < bytes.len() {
        if bytes[at] == quote {
            if bytes.get(at + 1) == Some(&quote) {
                at += 2;
                continue;
            }
            return Ok(at + 1);
        }
        at += 1;
    }
    Err("A quoted string or identifier is not closed.".into())
}

/// The name an identifier token stands for.
pub(crate) fn ident_text(sql: &str, token: &Token) -> String {
    let raw = &sql[token.start..token.end];
    if !token.quoted {
        return raw.to_string();
    }
    let inner = &raw[1..raw.len() - 1];
    match raw.as_bytes()[0] {
        b'"' => inner.replace("\"\"", "\""),
        b'`' => inner.replace("``", "`"),
        _ => inner.to_string(),
    }
}

/// Index of the matching `)` for every `(`.
fn match_parens(tokens: &[Token]) -> Result<Vec<Option<usize>>, String> {
    let mut matching = vec![None; tokens.len()];
    let mut stack = Vec::new();
    for (index, token) in tokens.iter().enumerate() {
        match token.kind {
            Tok::Open => stack.push(index),
            Tok::Close => {
                let open = stack
                    .pop()
                    .ok_or_else(|| "Unbalanced parentheses.".to_string())?;
                matching[open] = Some(index);
                matching[index] = Some(open);
            }
            _ => {}
        }
    }
    if stack.is_empty() {
        Ok(matching)
    } else {
        Err("Unbalanced parentheses.".into())
    }
}

/// A parsed SQL text: its tokens and paren matches.
pub(crate) struct Text<'a> {
    pub sql: &'a str,
    pub tokens: Vec<Token>,
    matching: Vec<Option<usize>>,
}

impl<'a> Text<'a> {
    pub fn new(sql: &'a str) -> Result<Self, String> {
        let tokens = tokenize(sql)?;
        let matching = match_parens(&tokens)?;
        Ok(Self {
            sql,
            tokens,
            matching,
        })
    }

    pub fn len(&self) -> usize {
        self.tokens.len()
    }

    pub fn kind(&self, at: usize) -> Option<Tok> {
        self.tokens.get(at).map(|token| token.kind)
    }

    /// A bare (unquoted) word, compared without case.
    pub fn is_kw(&self, at: usize, word: &str) -> bool {
        self.tokens.get(at).is_some_and(|token| {
            token.kind == Tok::Ident
                && !token.quoted
                && self.sql[token.start..token.end].eq_ignore_ascii_case(word)
        })
    }

    pub fn ident(&self, at: usize) -> Option<String> {
        let token = self.tokens.get(at)?;
        match token.kind {
            Tok::Ident => Some(ident_text(self.sql, token)),
            // SQLite accepts a string where a name is expected.
            Tok::Str if !self.sql[token.start..].starts_with(['x', 'X']) => {
                let raw = &self.sql[token.start..token.end];
                Some(raw[1..raw.len() - 1].replace("''", "'"))
            }
            _ => None,
        }
    }

    pub fn close_of(&self, open: usize) -> Option<usize> {
        if self.kind(open) == Some(Tok::Open) {
            self.matching.get(open).copied().flatten()
        } else {
            None
        }
    }

    /// Source text of tokens `first..=last`, as written.
    pub fn span(&self, first: usize, last: usize) -> &'a str {
        &self.sql[self.tokens[first].start..self.tokens[last].end]
    }

    /// Source text of tokens `first..=last` with some tokens replaced.
    pub fn render(
        &self,
        first: usize,
        last: usize,
        replace: &dyn Fn(usize) -> Option<String>,
    ) -> String {
        let mut out = String::new();
        for at in first..=last {
            let token = &self.tokens[at];
            match replace(at) {
                Some(text) => out.push_str(&text),
                None => out.push_str(&self.sql[token.start..token.end]),
            }
            if at < last {
                out.push_str(&self.sql[token.end..self.tokens[at + 1].start]);
            }
        }
        out
    }

    /// Identifier tokens in `first..=last` that may name a column: not a
    /// function name, not a collation, not a keyword-only position.
    pub fn column_refs(&self, first: usize, last: usize) -> Vec<(usize, String)> {
        let mut out = Vec::new();
        if first > last {
            return out;
        }
        for at in first..=last {
            if self.kind(at) != Some(Tok::Ident) {
                continue;
            }
            if self.kind(at + 1) == Some(Tok::Open) {
                continue;
            }
            if at > 0 && self.is_kw(at - 1, "COLLATE") {
                continue;
            }
            if let Some(name) = self.ident(at) {
                out.push((at, name));
            }
        }
        out
    }

    pub fn mentions(&self, first: usize, last: usize, column: &str) -> bool {
        self.column_refs(first, last)
            .iter()
            .any(|(_, name)| name.eq_ignore_ascii_case(column))
    }
}

/// One column or table constraint.
#[derive(Debug, Clone)]
pub(crate) struct Constraint {
    /// First token, including `CONSTRAINT name`.
    pub first: usize,
    pub last: usize,
    pub name: Option<String>,
    pub kind: ConstraintKind,
}

#[derive(Debug, Clone)]
pub(crate) enum ConstraintKind {
    PrimaryKey {
        /// Table constraint: the column list.
        columns: Vec<String>,
        autoincrement: Option<usize>,
    },
    NotNull,
    Null,
    Unique {
        columns: Vec<String>,
    },
    Check {
        open: usize,
        close: usize,
    },
    Default,
    Collate,
    ForeignKey(ForeignKeyRef),
    Generated {
        open: usize,
        close: usize,
    },
}

#[derive(Debug, Clone)]
pub(crate) struct ForeignKeyRef {
    /// Local columns (column constraint: the column itself).
    pub columns: Vec<String>,
    /// Token range of the local column list (table constraint only).
    pub column_group: Option<(usize, usize)>,
    pub table: String,
    pub ref_columns: Vec<String>,
    pub ref_group: Option<(usize, usize)>,
    pub on_delete: Option<String>,
    pub on_update: Option<String>,
}

#[derive(Debug, Clone)]
pub(crate) struct ColumnDef {
    pub name: String,
    pub name_token: usize,
    /// Token range of the declared type, if any.
    pub type_range: Option<(usize, usize)>,
    pub constraints: Vec<Constraint>,
}

impl ColumnDef {
    pub fn is_generated(&self) -> bool {
        self.constraints
            .iter()
            .any(|c| matches!(c.kind, ConstraintKind::Generated { .. }))
    }

    pub fn declared_type(&self, text: &Text<'_>) -> String {
        self.type_range
            .map(|(first, last)| text.span(first, last).to_string())
            .unwrap_or_default()
    }
}

#[derive(Debug, Clone)]
pub(crate) enum Item {
    Column(ColumnDef),
    Constraint(Constraint),
}

/// A parsed `CREATE TABLE`.
pub(crate) struct CreateTable<'a> {
    pub text: Text<'a>,
    pub open: usize,
    pub close: usize,
    pub items: Vec<Item>,
}

impl<'a> CreateTable<'a> {
    pub fn parse(sql: &'a str) -> Result<Self, String> {
        let text = Text::new(sql)?;
        let mut at = 0;
        if !text.is_kw(at, "CREATE") {
            return Err("It does not start with CREATE.".into());
        }
        at += 1;
        if text.is_kw(at, "TEMP") || text.is_kw(at, "TEMPORARY") {
            at += 1;
        }
        if text.is_kw(at, "VIRTUAL") {
            return Err("It is a virtual table.".into());
        }
        if !text.is_kw(at, "TABLE") {
            return Err("It is not a CREATE TABLE.".into());
        }
        at += 1;
        if text.is_kw(at, "IF") && text.is_kw(at + 1, "NOT") && text.is_kw(at + 2, "EXISTS") {
            at += 3;
        }
        // Name, maybe schema-qualified.
        text.ident(at).ok_or("The table name is missing.")?;
        at += 1;
        if text.kind(at) == Some(Tok::Dot) {
            text.ident(at + 1).ok_or("The table name is missing.")?;
            at += 2;
        }
        if text.is_kw(at, "AS") {
            return Err("It was created with AS SELECT.".into());
        }
        let open = at;
        let close = text.close_of(open).ok_or("The column list is missing.")?;
        // Everything after `)` must be table options.
        let mut tail = close + 1;
        while tail < text.len() {
            let ok = text.is_kw(tail, "WITHOUT")
                || text.is_kw(tail, "ROWID")
                || text.is_kw(tail, "STRICT")
                || matches!(text.kind(tail), Some(Tok::Comma | Tok::Semi));
            if !ok {
                return Err(format!(
                    "Unexpected text after the column list: {}",
                    text.span(tail, text.len() - 1)
                ));
            }
            tail += 1;
        }
        let mut items = Vec::new();
        let mut first = open + 1;
        let mut at = open + 1;
        while at <= close {
            if at == close || text.kind(at) == Some(Tok::Comma) {
                if first >= at {
                    return Err("An empty column definition.".into());
                }
                items.push(parse_item(&text, first, at - 1)?);
                first = at + 1;
            } else if text.kind(at) == Some(Tok::Open) {
                at = text.close_of(at).ok_or("Unbalanced parentheses.")?;
            }
            at += 1;
        }
        Ok(Self {
            text,
            open,
            close,
            items,
        })
    }

    pub fn columns(&self) -> impl Iterator<Item = &ColumnDef> {
        self.items.iter().filter_map(|item| match item {
            Item::Column(column) => Some(column),
            Item::Constraint(_) => None,
        })
    }

    /// `WITHOUT ROWID` / `STRICT` and anything after the column list.
    pub fn options(&self) -> &'a str {
        &self.text.sql[self.text.tokens[self.close].end..]
    }

    pub fn without_rowid(&self) -> bool {
        (self.close + 1..self.text.len())
            .any(|at| self.text.is_kw(at, "WITHOUT") && self.text.is_kw(at + 1, "ROWID"))
    }

    /// The inside of the parentheses spans more than one line.
    pub fn multiline(&self) -> bool {
        let inner =
            &self.text.sql[self.text.tokens[self.open].end..self.text.tokens[self.close].start];
        inner.contains('\n')
    }
}

const TABLE_CONSTRAINT_START: [&str; 5] = ["CONSTRAINT", "PRIMARY", "UNIQUE", "CHECK", "FOREIGN"];
const COLUMN_CONSTRAINT_START: [&str; 11] = [
    "CONSTRAINT",
    "PRIMARY",
    "NOT",
    "NULL",
    "UNIQUE",
    "CHECK",
    "DEFAULT",
    "COLLATE",
    "REFERENCES",
    "GENERATED",
    "AS",
];

fn parse_item(text: &Text<'_>, first: usize, last: usize) -> Result<Item, String> {
    if TABLE_CONSTRAINT_START
        .iter()
        .any(|word| text.is_kw(first, word))
    {
        let constraint = parse_constraint(text, first, last, None, true)?;
        if constraint.last != last {
            return Err(format!(
                "Unexpected text in a table constraint: {}",
                text.span(constraint.last + 1, last)
            ));
        }
        return Ok(Item::Constraint(constraint));
    }
    let name = text
        .ident(first)
        .ok_or_else(|| format!("Cannot read the column name in: {}", text.span(first, last)))?;
    let mut at = first + 1;
    let type_first = at;
    while at <= last
        && text.kind(at) == Some(Tok::Ident)
        && !COLUMN_CONSTRAINT_START
            .iter()
            .any(|word| text.is_kw(at, word))
    {
        at += 1;
    }
    if at > type_first && at <= last && text.kind(at) == Some(Tok::Open) {
        at = text.close_of(at).ok_or("Unbalanced parentheses.")? + 1;
    }
    let type_range = (at > type_first).then(|| (type_first, at - 1));
    let mut constraints = Vec::new();
    while at <= last {
        let constraint = parse_constraint(text, at, last, Some(&name), false)?;
        at = constraint.last + 1;
        constraints.push(constraint);
    }
    Ok(Item::Column(ColumnDef {
        name,
        name_token: first,
        type_range,
        constraints,
    }))
}

/// Parse one constraint starting at `first`. `column` is set for a column
/// constraint (its local column for REFERENCES).
fn parse_constraint(
    text: &Text<'_>,
    first: usize,
    last: usize,
    column: Option<&str>,
    table_level: bool,
) -> Result<Constraint, String> {
    let unexpected = |at: usize| {
        if at <= last {
            format!("Unsupported column syntax near: {}", text.span(at, last))
        } else {
            "A constraint ends early.".to_string()
        }
    };
    let mut at = first;
    let mut name = None;
    if text.is_kw(at, "CONSTRAINT") {
        name = Some(text.ident(at + 1).ok_or_else(|| unexpected(at))?);
        at += 2;
    }
    let group = |open: usize| -> Result<usize, String> {
        if open > last {
            return Err(unexpected(open));
        }
        text.close_of(open).ok_or_else(|| unexpected(open))
    };
    let conflict = |at: usize| -> usize {
        if text.is_kw(at, "ON") && text.is_kw(at + 1, "CONFLICT") {
            at + 3
        } else {
            at
        }
    };
    let (kind, end) = if text.is_kw(at, "PRIMARY") && text.is_kw(at + 1, "KEY") {
        at += 2;
        let mut columns = Vec::new();
        if table_level {
            let close = group(at)?;
            columns = list_names(text, at, close);
            at = close + 1;
        } else if text.is_kw(at, "ASC") || text.is_kw(at, "DESC") {
            at += 1;
        }
        at = conflict(at);
        let mut autoincrement = None;
        if text.is_kw(at, "AUTOINCREMENT") {
            autoincrement = Some(at);
            at += 1;
        }
        (
            ConstraintKind::PrimaryKey {
                columns,
                autoincrement,
            },
            at,
        )
    } else if !table_level && text.is_kw(at, "NOT") && text.is_kw(at + 1, "NULL") {
        (ConstraintKind::NotNull, conflict(at + 2))
    } else if !table_level && text.is_kw(at, "NULL") {
        (ConstraintKind::Null, conflict(at + 1))
    } else if text.is_kw(at, "UNIQUE") {
        at += 1;
        let mut columns = Vec::new();
        if table_level {
            let close = group(at)?;
            columns = list_names(text, at, close);
            at = close + 1;
        } else if let Some(column) = column {
            columns.push(column.to_string());
        }
        (ConstraintKind::Unique { columns }, conflict(at))
    } else if text.is_kw(at, "CHECK") {
        let open = at + 1;
        let close = group(open)?;
        (ConstraintKind::Check { open, close }, close + 1)
    } else if !table_level && text.is_kw(at, "DEFAULT") {
        let value = at + 1;
        let value_last = if text.kind(value) == Some(Tok::Open) {
            group(value)?
        } else if text.kind(value) == Some(Tok::Other)
            && matches!(text.span(value, value), "+" | "-")
        {
            value + 1
        } else {
            value
        };
        if value_last > last {
            return Err(unexpected(at));
        }
        (ConstraintKind::Default, value_last + 1)
    } else if !table_level && text.is_kw(at, "COLLATE") {
        if at + 1 > last {
            return Err(unexpected(at));
        }
        (ConstraintKind::Collate, at + 2)
    } else if !table_level && text.is_kw(at, "REFERENCES") {
        let (fk, next) = parse_references(text, at, last, Vec::new(), None)?;
        let mut fk = fk;
        if let Some(column) = column {
            fk.columns = vec![column.to_string()];
        }
        (ConstraintKind::ForeignKey(fk), next)
    } else if table_level && text.is_kw(at, "FOREIGN") && text.is_kw(at + 1, "KEY") {
        let open = at + 2;
        let close = group(open)?;
        let columns = list_names(text, open, close);
        if !text.is_kw(close + 1, "REFERENCES") {
            return Err(unexpected(close + 1));
        }
        let (fk, next) = parse_references(text, close + 1, last, columns, Some((open, close)))?;
        (ConstraintKind::ForeignKey(fk), next)
    } else if !table_level
        && (text.is_kw(at, "AS") || (text.is_kw(at, "GENERATED") && text.is_kw(at + 1, "ALWAYS")))
    {
        if text.is_kw(at, "GENERATED") {
            at += 2;
        }
        if !text.is_kw(at, "AS") {
            return Err(unexpected(at));
        }
        let open = at + 1;
        let close = group(open)?;
        at = close + 1;
        if text.is_kw(at, "STORED") || text.is_kw(at, "VIRTUAL") {
            at += 1;
        }
        (ConstraintKind::Generated { open, close }, at)
    } else {
        return Err(unexpected(at));
    };
    if end == 0 || end - 1 > last {
        return Err(unexpected(first));
    }
    Ok(Constraint {
        first,
        last: end - 1,
        name,
        kind,
    })
}

/// `REFERENCES table [(cols)] [ON DELETE|UPDATE action | MATCH name]*
/// [[NOT] DEFERRABLE [INITIALLY DEFERRED|IMMEDIATE]]`. Returns the next token.
fn parse_references(
    text: &Text<'_>,
    references: usize,
    last: usize,
    columns: Vec<String>,
    column_group: Option<(usize, usize)>,
) -> Result<(ForeignKeyRef, usize), String> {
    let unexpected = |at: usize| {
        if at <= last {
            format!(
                "Unsupported REFERENCES clause near: {}",
                text.span(at, last)
            )
        } else {
            "A REFERENCES clause ends early.".to_string()
        }
    };
    let mut at = references + 1;
    let table = text.ident(at).ok_or_else(|| unexpected(at))?;
    at += 1;
    let mut ref_columns = Vec::new();
    let mut ref_group = None;
    if at <= last && text.kind(at) == Some(Tok::Open) {
        let close = text.close_of(at).ok_or_else(|| unexpected(at))?;
        ref_columns = list_names(text, at, close);
        ref_group = Some((at, close));
        at = close + 1;
    }
    let mut on_delete = None;
    let mut on_update = None;
    loop {
        if text.is_kw(at, "ON") && (text.is_kw(at + 1, "DELETE") || text.is_kw(at + 1, "UPDATE")) {
            let delete = text.is_kw(at + 1, "DELETE");
            at += 2;
            let action = if text.is_kw(at, "SET")
                && (text.is_kw(at + 1, "NULL") || text.is_kw(at + 1, "DEFAULT"))
            {
                at += 2;
                text.span(at - 2, at - 1)
            } else if text.is_kw(at, "NO") && text.is_kw(at + 1, "ACTION") {
                at += 2;
                "NO ACTION"
            } else if text.is_kw(at, "CASCADE") || text.is_kw(at, "RESTRICT") {
                at += 1;
                text.span(at - 1, at - 1)
            } else {
                return Err(unexpected(at));
            };
            let action = super::types::clean_action(Some(action));
            if delete {
                on_delete = action;
            } else {
                on_update = action;
            }
        } else if text.is_kw(at, "MATCH") {
            at += 2;
        } else {
            break;
        }
    }
    if text.is_kw(at, "NOT") && text.is_kw(at + 1, "DEFERRABLE") {
        at += 1;
    }
    if text.is_kw(at, "DEFERRABLE") {
        at += 1;
        if text.is_kw(at, "INITIALLY")
            && (text.is_kw(at + 1, "DEFERRED") || text.is_kw(at + 1, "IMMEDIATE"))
        {
            at += 2;
        }
    }
    if at > last + 1 {
        return Err(unexpected(references));
    }
    Ok((
        ForeignKeyRef {
            columns,
            column_group,
            table,
            ref_columns,
            ref_group,
            on_delete,
            on_update,
        },
        at,
    ))
}

/// First name of each comma-separated element inside `open..close`.
fn list_names(text: &Text<'_>, open: usize, close: usize) -> Vec<String> {
    let mut names = Vec::new();
    let mut expect = true;
    let mut at = open + 1;
    while at < close {
        if text.kind(at) == Some(Tok::Comma) {
            expect = true;
        } else if expect {
            if let Some(name) = text.ident(at) {
                names.push(name);
            }
            expect = false;
        }
        if text.kind(at) == Some(Tok::Open) {
            at = text.close_of(at).unwrap_or(at);
        }
        at += 1;
    }
    names
}

/// What the structure editor reads from a SQLite `CREATE TABLE` beyond
/// `PRAGMA table_info`: constraints and AUTOINCREMENT.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SqliteTableExtras {
    /// The `INTEGER PRIMARY KEY AUTOINCREMENT` column.
    pub autoincrement: Option<String>,
    /// `(name, expression)`; name is empty when the CHECK has none.
    pub checks: Vec<(String, String)>,
    /// `(name, columns)`; name is empty when the UNIQUE has none.
    pub uniques: Vec<(String, Vec<String>)>,
    pub foreign_keys: Vec<SqliteForeignKey>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SqliteForeignKey {
    /// `CONSTRAINT name`, when the definition has one.
    pub name: Option<String>,
    pub columns: Vec<String>,
    pub ref_table: String,
    /// Empty when the definition leaves them out (the parent's primary key).
    pub ref_columns: Vec<String>,
    pub on_delete: Option<String>,
    pub on_update: Option<String>,
}

/// Read constraints from a `CREATE TABLE` text. `None` when it cannot be parsed.
pub fn sqlite_table_extras(create_sql: &str) -> Option<SqliteTableExtras> {
    let table = CreateTable::parse(create_sql).ok()?;
    let text = &table.text;
    let mut extras = SqliteTableExtras::default();
    let mut visit = |constraint: &Constraint, column: Option<&str>| match &constraint.kind {
        ConstraintKind::PrimaryKey {
            autoincrement: Some(_),
            ..
        } => {
            extras.autoincrement = column.map(str::to_string);
        }
        ConstraintKind::Check { open, close } => {
            extras.checks.push((
                constraint.name.clone().unwrap_or_default(),
                check_expression(text, *open, *close),
            ));
        }
        ConstraintKind::Unique { columns } => {
            extras
                .uniques
                .push((constraint.name.clone().unwrap_or_default(), columns.clone()));
        }
        ConstraintKind::ForeignKey(fk) => extras.foreign_keys.push(SqliteForeignKey {
            name: constraint.name.clone(),
            columns: fk.columns.clone(),
            ref_table: fk.table.clone(),
            ref_columns: fk.ref_columns.clone(),
            on_delete: fk.on_delete.clone(),
            on_update: fk.on_update.clone(),
        }),
        _ => {}
    };
    for item in &table.items {
        match item {
            Item::Column(column) => {
                for constraint in &column.constraints {
                    visit(constraint, Some(&column.name));
                }
            }
            Item::Constraint(constraint) => visit(constraint, None),
        }
    }
    Some(extras)
}

/// The text inside `CHECK ( … )`, trimmed.
pub(crate) fn check_expression(text: &Text<'_>, open: usize, close: usize) -> String {
    if close <= open + 1 {
        return String::new();
    }
    text.span(open + 1, close - 1).trim().to_string()
}

/// Expression text compared without whitespace differences.
pub(crate) fn normalize_expression(expression: &str) -> String {
    expression.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_tricky_table_parses_and_keeps_its_parts() {
        let sql = "CREATE TABLE \"t x\" (\n  id INTEGER PRIMARY KEY AUTOINCREMENT,\n  a TEXT DEFAULT 'x,y' COLLATE NOCASE CHECK (a <> ''),\n  b, -- untyped\n  c DOUBLE PRECISION NOT NULL ON CONFLICT FAIL DEFAULT -1.5,\n  d INT GENERATED ALWAYS AS (c * 2) STORED,\n  pid INTEGER REFERENCES p(id) ON DELETE SET NULL DEFERRABLE INITIALLY DEFERRED,\n  CONSTRAINT u_ab UNIQUE (a, b),\n  CHECK (c > 0),\n  FOREIGN KEY (b) REFERENCES q ON UPDATE CASCADE\n) WITHOUT ROWID, STRICT";
        let table = match CreateTable::parse(sql) {
            Ok(table) => table,
            Err(err) => panic!("{err}"),
        };
        let names: Vec<_> = table.columns().map(|c| c.name.clone()).collect();
        assert_eq!(names, ["id", "a", "b", "c", "d", "pid"]);
        let b = table.columns().nth(2).map(|c| c.type_range);
        assert_eq!(b, Some(None));
        let c = table.columns().nth(3).map(|c| c.declared_type(&table.text));
        assert_eq!(c.as_deref(), Some("DOUBLE PRECISION"));
        assert!(table.columns().nth(4).is_some_and(ColumnDef::is_generated));
        assert!(table.without_rowid());
        assert_eq!(table.options(), " WITHOUT ROWID, STRICT");

        let extras = match sqlite_table_extras(sql) {
            Some(extras) => extras,
            None => panic!("not parsed"),
        };
        assert_eq!(extras.autoincrement.as_deref(), Some("id"));
        assert_eq!(
            extras.checks,
            vec![
                (String::new(), "a <> ''".to_string()),
                (String::new(), "c > 0".to_string())
            ]
        );
        assert_eq!(
            extras.uniques,
            vec![("u_ab".to_string(), vec!["a".to_string(), "b".to_string()])]
        );
        assert_eq!(extras.foreign_keys.len(), 2);
        assert_eq!(extras.foreign_keys[0].columns, ["pid"]);
        assert_eq!(extras.foreign_keys[1].ref_columns, Vec::<String>::new());
    }

    #[test]
    fn unsupported_tables_are_errors() {
        for sql in [
            "CREATE VIRTUAL TABLE f USING fts5(a)",
            "CREATE TABLE t AS SELECT 1",
            "CREATE TABLE t (a INT PRIMARY)",
        ] {
            assert!(CreateTable::parse(sql).is_err(), "{sql}");
        }
    }
}
