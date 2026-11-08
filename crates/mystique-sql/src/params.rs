//! Placeholders in user SQL. Values stay out of the text.
//!
//! `:name`, `?`, and `$n` are recognised outside strings and comments.
//! Postgres has no `?` marker (`?`, `?|`, `?&` are jsonb operators), and a
//! `:` inside an array subscript is a slice (`arr[lo:hi]`).
//! The statement sent to the driver uses that dialect's marker. History
//! keeps the text the user typed.

use super::SqlDialect;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParamSlot {
    /// What the prompt shows (`:id`, `?`, `$1`).
    pub label: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    Named,
    Question,
    Dollar,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Use {
    start: usize,
    end: usize,
    slot: usize,
    kind: Kind,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScriptParams {
    slots: Vec<ParamSlot>,
    uses: Vec<Use>,
}

impl ScriptParams {
    pub fn scan(sql: &str, dialect: SqlDialect) -> Result<Self, String> {
        let found = scan_placeholders(sql, dialect);
        let mut slots: Vec<Option<ParamSlot>> = Vec::new();
        let mut names: Vec<(String, usize)> = Vec::new();
        let mut uses = Vec::new();
        for place in found {
            let slot = match place.kind {
                RawKind::Named(ref name) => {
                    if let Some((_, index)) = names.iter().find(|(existing, _)| existing == name) {
                        *index
                    } else {
                        let index = slots.len();
                        slots.push(Some(ParamSlot {
                            label: format!(":{name}"),
                        }));
                        names.push((name.clone(), index));
                        index
                    }
                }
                RawKind::Question => {
                    let index = slots.len();
                    slots.push(Some(ParamSlot { label: "?".into() }));
                    index
                }
                RawKind::Dollar(number) => {
                    let index = usize::try_from(number.saturating_sub(1)).unwrap_or(0);
                    while slots.len() <= index {
                        slots.push(None);
                    }
                    match slots.get(index).and_then(|slot| slot.as_ref()) {
                        Some(slot) if slot.label.starts_with('$') => index,
                        Some(slot) => {
                            return Err(format!(
                                "Cannot mix {} with ${number} in one query.",
                                slot.label
                            ));
                        }
                        None => {
                            slots[index] = Some(ParamSlot {
                                label: format!("${number}"),
                            });
                            index
                        }
                    }
                }
            };
            uses.push(Use {
                start: place.start,
                end: place.end,
                slot,
                kind: match place.kind {
                    RawKind::Named(_) => Kind::Named,
                    RawKind::Question => Kind::Question,
                    RawKind::Dollar(_) => Kind::Dollar,
                },
            });
        }
        let slots = slots
            .into_iter()
            .enumerate()
            .map(|(index, slot)| {
                slot.unwrap_or(ParamSlot {
                    label: format!("${}", index + 1),
                })
            })
            .collect();
        Ok(Self { slots, uses })
    }

    pub fn slots(&self) -> &[ParamSlot] {
        &self.slots
    }

    pub fn check_count(&self, provided: usize) -> Result<(), String> {
        if self.slots.len() == provided {
            return Ok(());
        }
        if self.slots.is_empty() {
            return Ok(());
        }
        let label = self
            .slots
            .first()
            .map(|slot| slot.label.as_str())
            .unwrap_or("?");
        if provided < self.slots.len() {
            let missing = self.slots.get(provided).map(|slot| slot.label.as_str());
            return Err(match missing {
                Some(name) => format!("Missing value for {name}."),
                None => format!("Missing value for {label}."),
            });
        }
        Err(format!(
            "Expected {} parameter{}, got {provided}.",
            self.slots.len(),
            if self.slots.len() == 1 { "" } else { "s" }
        ))
    }

    /// Same-length text so the planner's offsets still point at the user's SQL.
    pub fn mask(sql: &str, dialect: SqlDialect) -> String {
        let Ok(script) = Self::scan(sql, dialect) else {
            return sql.to_string();
        };
        let mut out = String::with_capacity(sql.len());
        let mut cursor = 0usize;
        for use_ in &script.uses {
            if use_.start < cursor || use_.end > sql.len() {
                return sql.to_string();
            }
            out.push_str(&sql[cursor..use_.start]);
            let width = use_.end.saturating_sub(use_.start);
            if width == 0 {
                continue;
            }
            // Spaces would be trimmed off the statement span and the range
            // would end in the middle of the placeholder.
            for _ in 0..width {
                out.push('1');
            }
            cursor = use_.end;
        }
        out.push_str(&sql[cursor..]);
        out
    }

    /// Driver SQL for one statement, plus the values in bind order.
    ///
    /// Postgres reuses `$n` when the same name appears twice. MySQL gets one
    /// `?` per occurrence. SQLite keeps the user's markers; rusqlite binds
    /// them in appearance order.
    /// Driver SQL plus the slot index of each bound value, in bind order.
    pub fn statement(
        &self,
        sql: &str,
        start: usize,
        end: usize,
        dialect: SqlDialect,
    ) -> Result<(String, Vec<usize>), String> {
        self.rewrite(sql, start, end, dialect)
            .map(|rewritten| (rewritten.sql, rewritten.bound))
    }

    /// Map a byte offset in the driver SQL for `start..end` back to the user's
    /// text. Both offsets are relative to `start`. An offset inside a rewritten
    /// marker lands on the start of the marker the user typed.
    pub fn source_offset(
        &self,
        sql: &str,
        start: usize,
        end: usize,
        dialect: SqlDialect,
        offset: usize,
    ) -> usize {
        let Ok(rewritten) = self.rewrite(sql, start, end, dialect) else {
            return offset;
        };
        let (mut out_end, mut source_end) = (0usize, 0usize);
        for mark in &rewritten.marks {
            if offset < mark.out_start {
                break;
            }
            if offset < mark.out_end {
                return mark.source_start.saturating_sub(start);
            }
            out_end = mark.out_end;
            source_end = mark.source_end.saturating_sub(start);
        }
        source_end + (offset - out_end)
    }

    fn rewrite(
        &self,
        sql: &str,
        start: usize,
        end: usize,
        dialect: SqlDialect,
    ) -> Result<Rewritten, String> {
        let uses: Vec<&Use> = self
            .uses
            .iter()
            .filter(|use_| use_.start >= start && use_.end <= end)
            .collect();
        if uses.is_empty() {
            return Ok(Rewritten::unchanged(slice_sql(sql, start, end), Vec::new()));
        }
        match dialect {
            SqlDialect::Postgres => rewrite_dollar(sql, start, end, &uses),
            SqlDialect::Mysql | SqlDialect::Generic => rewrite_question(sql, start, end, &uses),
            SqlDialect::Sqlite => {
                let slice = slice_sql(sql, start, end);
                let mut bound = Vec::new();
                for use_ in &uses {
                    if bound.contains(&use_.slot) && use_.kind != Kind::Question {
                        continue;
                    }
                    bound.push(use_.slot);
                }
                Ok(Rewritten::unchanged(slice, bound))
            }
        }
    }
}

/// Driver SQL, bind order, and where each marker went.
struct Rewritten {
    sql: String,
    bound: Vec<usize>,
    marks: Vec<Mark>,
}

impl Rewritten {
    fn unchanged(sql: String, bound: Vec<usize>) -> Self {
        Self {
            sql,
            bound,
            marks: Vec::new(),
        }
    }
}

/// One rewritten marker. Source offsets are absolute, output offsets are
/// relative to the statement start.
struct Mark {
    source_start: usize,
    source_end: usize,
    out_start: usize,
    out_end: usize,
}

fn rewrite_dollar(sql: &str, start: usize, end: usize, uses: &[&Use]) -> Result<Rewritten, String> {
    let mut local_of: Vec<Option<usize>> = Vec::new();
    let mut bound = Vec::new();
    let mut marks = Vec::with_capacity(uses.len());
    let mut out = String::new();
    let mut cursor = start;
    for use_ in uses {
        if use_.start < cursor || use_.end > end {
            return Err("Parameter marker is outside the statement.".into());
        }
        out.push_str(&sql[cursor..use_.start]);
        let out_start = out.len();
        let local = if let Some(index) = local_of.get(use_.slot).copied().flatten() {
            index
        } else {
            bound.push(use_.slot);
            let index = bound.len();
            if local_of.len() <= use_.slot {
                local_of.resize(use_.slot + 1, None);
            }
            if let Some(slot) = local_of.get_mut(use_.slot) {
                *slot = Some(index);
            }
            index
        };
        out.push('$');
        out.push_str(&local.to_string());
        marks.push(Mark {
            source_start: use_.start,
            source_end: use_.end,
            out_start,
            out_end: out.len(),
        });
        cursor = use_.end;
    }
    if cursor < end {
        out.push_str(&sql[cursor..end]);
    }
    Ok(Rewritten {
        sql: out,
        bound,
        marks,
    })
}

fn rewrite_question(
    sql: &str,
    start: usize,
    end: usize,
    uses: &[&Use],
) -> Result<Rewritten, String> {
    let mut out = String::new();
    let mut bound = Vec::new();
    let mut marks = Vec::with_capacity(uses.len());
    let mut cursor = start;
    for use_ in uses {
        if use_.start < cursor || use_.end > end {
            return Err("Parameter marker is outside the statement.".into());
        }
        out.push_str(&sql[cursor..use_.start]);
        marks.push(Mark {
            source_start: use_.start,
            source_end: use_.end,
            out_start: out.len(),
            out_end: out.len() + 1,
        });
        out.push('?');
        bound.push(use_.slot);
        cursor = use_.end;
    }
    if cursor < end {
        out.push_str(&sql[cursor..end]);
    }
    Ok(Rewritten {
        sql: out,
        bound,
        marks,
    })
}

fn slice_sql(sql: &str, start: usize, end: usize) -> String {
    if start > end || end > sql.len() || !sql.is_char_boundary(start) || !sql.is_char_boundary(end)
    {
        return String::new();
    }
    sql[start..end].to_string()
}

struct RawPlace {
    start: usize,
    end: usize,
    kind: RawKind,
}

enum RawKind {
    Named(String),
    Question,
    Dollar(u32),
}

/// An open `(` or `[` on Postgres.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Nest {
    Paren,
    /// `expr[…]`: a `:` here separates slice bounds.
    Subscript,
    /// `ARRAY[…]`, or a type's `[]`.
    Bracket,
}

fn scan_placeholders(sql: &str, dialect: SqlDialect) -> Vec<RawPlace> {
    let bytes = sql.as_bytes();
    let postgres = dialect == SqlDialect::Postgres;
    let mut nests: Vec<Nest> = Vec::new();
    let mut index = 0usize;
    let mut found = Vec::new();
    while index < bytes.len() {
        if starts_with(bytes, index, b"--") {
            index = skip_line(bytes, index + 2);
            continue;
        }
        if dialect == SqlDialect::Mysql && bytes[index] == b'#' {
            index = skip_line(bytes, index + 1);
            continue;
        }
        if starts_with(bytes, index, b"/*") {
            index = skip_block_comment(bytes, index + 2);
            continue;
        }
        if bytes[index] == b'\'' {
            index = skip_string(bytes, index + 1, b'\'', dialect == SqlDialect::Mysql);
            continue;
        }
        if bytes[index] == b'"' {
            index = skip_string(bytes, index + 1, b'"', false);
            continue;
        }
        if dialect == SqlDialect::Mysql && bytes[index] == b'`' {
            index = skip_string(bytes, index + 1, b'`', false);
            continue;
        }
        if bytes[index] == b'$' {
            if let Some(end) = dollar_quote_end(bytes, index) {
                index = end;
                continue;
            }
            if let Some((end, number)) = dollar_number(bytes, index) {
                found.push(RawPlace {
                    start: index,
                    end,
                    kind: RawKind::Dollar(number),
                });
                index = end;
                continue;
            }
        }
        if postgres {
            match bytes[index] {
                b'(' => nests.push(Nest::Paren),
                b'[' if opens_subscript(bytes, index) => nests.push(Nest::Subscript),
                b'[' => nests.push(Nest::Bracket),
                b')' | b']' => {
                    nests.pop();
                }
                b':' if bytes.get(index + 1) != Some(&b':')
                    && nests.last() == Some(&Nest::Subscript) =>
                {
                    index += 1;
                    continue;
                }
                _ => {}
            }
        }
        if bytes[index] == b'?' && !postgres && !question_operator(bytes, index) {
            found.push(RawPlace {
                start: index,
                end: index + 1,
                kind: RawKind::Question,
            });
            index += 1;
            continue;
        }
        if bytes[index] == b':' {
            if bytes.get(index + 1) == Some(&b':') || bytes.get(index + 1) == Some(&b'=') {
                index += 2;
                continue;
            }
            if let Some((end, name)) = named_ident(sql, index + 1) {
                found.push(RawPlace {
                    start: index,
                    end,
                    kind: RawKind::Named(name),
                });
                index = end;
                continue;
            }
        }
        index += 1;
    }
    found
}

fn starts_with(bytes: &[u8], index: usize, needle: &[u8]) -> bool {
    bytes.get(index..index + needle.len()) == Some(needle)
}

fn skip_line(bytes: &[u8], mut index: usize) -> usize {
    while index < bytes.len() && bytes[index] != b'\n' {
        index += 1;
    }
    index
}

fn skip_block_comment(bytes: &[u8], mut index: usize) -> usize {
    while index + 1 < bytes.len() {
        if bytes[index] == b'*' && bytes[index + 1] == b'/' {
            return index + 2;
        }
        index += 1;
    }
    bytes.len()
}

fn skip_string(bytes: &[u8], mut index: usize, quote: u8, backslash: bool) -> usize {
    while index < bytes.len() {
        if backslash && bytes[index] == b'\\' && index + 1 < bytes.len() {
            index += 2;
            continue;
        }
        if bytes[index] == quote {
            if bytes.get(index + 1) == Some(&quote) {
                index += 2;
                continue;
            }
            return index + 1;
        }
        index += 1;
    }
    bytes.len()
}

fn dollar_quote_end(bytes: &[u8], start: usize) -> Option<usize> {
    if bytes.get(start) != Some(&b'$') {
        return None;
    }
    let mut tag_end = start + 1;
    if bytes.get(tag_end) == Some(&b'$') {
        return find_closer(bytes, tag_end + 1, b"$$");
    }
    while tag_end < bytes.len() && is_ident_byte(bytes[tag_end]) && !bytes[tag_end].is_ascii_digit()
    {
        tag_end += 1;
    }
    if tag_end == start + 1 || bytes.get(tag_end) != Some(&b'$') {
        return None;
    }
    let tag = bytes.get(start..=tag_end)?;
    find_closer(bytes, tag_end + 1, tag)
}

fn find_closer(bytes: &[u8], mut index: usize, tag: &[u8]) -> Option<usize> {
    while index + tag.len() <= bytes.len() {
        if bytes.get(index..index + tag.len()) == Some(tag) {
            return Some(index + tag.len());
        }
        index += 1;
    }
    Some(bytes.len())
}

fn dollar_number(bytes: &[u8], start: usize) -> Option<(usize, u32)> {
    let mut index = start + 1;
    if index >= bytes.len() || !bytes[index].is_ascii_digit() {
        return None;
    }
    let mut number: u32 = 0;
    while index < bytes.len() && bytes[index].is_ascii_digit() {
        number = number
            .saturating_mul(10)
            .saturating_add(u32::from(bytes[index] - b'0'));
        index += 1;
    }
    if number == 0 {
        return None;
    }
    Some((index, number))
}

/// `[` right after an expression (`arr[`, `f(x)[`, `"col"[`), not `ARRAY[`.
fn opens_subscript(bytes: &[u8], index: usize) -> bool {
    let mut end = index;
    while end > 0 && bytes[end - 1].is_ascii_whitespace() {
        end -= 1;
    }
    let Some(&prev) = end.checked_sub(1).and_then(|at| bytes.get(at)) else {
        return false;
    };
    if matches!(prev, b')' | b']' | b'"') {
        return true;
    }
    if !is_ident_byte(prev) {
        return false;
    }
    let mut word = end - 1;
    while word > 0 && is_ident_byte(bytes[word - 1]) {
        word -= 1;
    }
    !bytes[word..end].eq_ignore_ascii_case(b"array")
}

fn question_operator(bytes: &[u8], index: usize) -> bool {
    matches!(bytes.get(index + 1), Some(b'|' | b'&'))
}

fn named_ident(sql: &str, start: usize) -> Option<(usize, String)> {
    let bytes = sql.as_bytes();
    let first = *bytes.get(start)?;
    if !is_ident_start(first) {
        return None;
    }
    let mut index = start + 1;
    while index < bytes.len() && is_ident_byte(bytes[index]) {
        index += 1;
    }
    Some((index, sql[start..index].to_string()))
}

fn is_ident_start(byte: u8) -> bool {
    byte.is_ascii_alphabetic() || byte == b'_'
}

fn is_ident_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_'
}

pub fn mask_placeholders(sql: &str, dialect: SqlDialect) -> String {
    ScriptParams::mask(sql, dialect)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::classify;
    use crate::StatementRisk;

    #[test]
    fn string_literal_is_not_a_parameter() {
        let script = ScriptParams::scan("SELECT ':id'", SqlDialect::Postgres).expect("scan");
        assert!(script.slots().is_empty());
    }

    #[test]
    fn named_placeholder_becomes_a_postgres_bind() {
        let script = ScriptParams::scan("SELECT :id", SqlDialect::Postgres).expect("scan");
        assert_eq!(script.slots().len(), 1);
        assert_eq!(script.slots()[0].label, ":id");
        let (sql, bound) = script
            .statement("SELECT :id", 0, "SELECT :id".len(), SqlDialect::Postgres)
            .expect("rewrite");
        assert_eq!(sql, "SELECT $1");
        assert_eq!(bound, vec![0]);
        assert!(!sql.contains("secret-value-zz"));
        let masked = ScriptParams::mask("SELECT :id", SqlDialect::Postgres);
        assert_eq!(masked, "SELECT 111");
        assert_eq!(classify(&masked, SqlDialect::Postgres), StatementRisk::Read);
        let planned = crate::plan_batch(&masked, SqlDialect::Sqlite);
        let stmt = planned.first().expect("statement");
        assert_eq!(stmt.end as usize, "SELECT :id".len());
    }

    #[test]
    fn repeated_name_is_one_postgres_slot() {
        let script = ScriptParams::scan("SELECT :id, :id", SqlDialect::Postgres).expect("scan");
        assert_eq!(script.slots().len(), 1);
        let (sql, bound) = script
            .statement(
                "SELECT :id, :id",
                0,
                "SELECT :id, :id".len(),
                SqlDialect::Postgres,
            )
            .expect("rewrite");
        assert_eq!(sql, "SELECT $1, $1");
        assert_eq!(bound, vec![0]);
    }

    #[test]
    fn mysql_question_mark_stays_a_question_mark() {
        let script = ScriptParams::scan("SELECT ?", SqlDialect::Mysql).expect("scan");
        let (sql, bound) = script
            .statement("SELECT ?", 0, "SELECT ?".len(), SqlDialect::Mysql)
            .expect("rewrite");
        assert_eq!(sql, "SELECT ?");
        assert_eq!(bound, vec![0]);
    }

    #[test]
    fn cast_and_assignment_are_not_parameters() {
        let script =
            ScriptParams::scan("SELECT 1::int, : = 1", SqlDialect::Postgres).expect("scan");
        assert!(script.slots().is_empty());
    }

    #[test]
    fn missing_value_is_an_error() {
        let script = ScriptParams::scan("SELECT :id", SqlDialect::Postgres).expect("scan");
        let err = script.check_count(0).expect_err("missing");
        assert!(err.contains(":id"), "{err}");
    }

    #[test]
    fn a_bound_value_cannot_add_a_statement() {
        let original = "SELECT :id";
        let masked = ScriptParams::mask(original, SqlDialect::Postgres);
        assert_eq!(classify(&masked, SqlDialect::Postgres), StatementRisk::Read);
        let script = ScriptParams::scan(original, SqlDialect::Postgres).expect("scan");
        let (sql, _) = script
            .statement(original, 0, original.len(), SqlDialect::Postgres)
            .expect("rewrite");
        assert!(!sql.contains("DROP"));
        assert_eq!(sql, "SELECT $1");
    }

    fn labels(sql: &str, dialect: SqlDialect) -> Vec<String> {
        ScriptParams::scan(sql, dialect)
            .expect("scan")
            .slots()
            .iter()
            .map(|slot| slot.label.clone())
            .collect()
    }

    #[test]
    fn postgres_question_marks_are_jsonb_operators() {
        let sql = "SELECT d ? 'a', d ?| array['a'], d ?& array['b'], d @? '$.a' FROM t";
        assert!(labels(sql, SqlDialect::Postgres).is_empty());
        assert_eq!(ScriptParams::mask(sql, SqlDialect::Postgres), sql);
        // No params at all: the agent and MCP send none.
        let script = ScriptParams::scan(sql, SqlDialect::Postgres).expect("scan");
        assert!(script.check_count(0).is_ok());
    }

    #[test]
    fn mysql_sqlite_and_generic_keep_question_mark_placeholders() {
        for dialect in [SqlDialect::Mysql, SqlDialect::Sqlite, SqlDialect::Generic] {
            assert_eq!(
                labels("SELECT ? , :name", dialect),
                vec!["?".to_string(), ":name".to_string()],
                "{dialect:?}"
            );
        }
        let script = ScriptParams::scan("SELECT ?", SqlDialect::Sqlite).expect("scan");
        let (sql, bound) = script
            .statement("SELECT ?", 0, "SELECT ?".len(), SqlDialect::Sqlite)
            .expect("rewrite");
        assert_eq!((sql.as_str(), bound), ("SELECT ?", vec![0]));
    }

    #[test]
    fn postgres_slice_colons_are_not_parameters() {
        let sql = "SELECT arr[lo:hi], arr[:hi], arr[lo:], (f(x))[a:b], \"c\"[1:n], \
                   arr [ lo : hi ], m[1:2][lo:hi], arr[lo::int:hi] FROM t";
        assert!(labels(sql, SqlDialect::Postgres).is_empty());
        assert_eq!(ScriptParams::mask(sql, SqlDialect::Postgres), sql);
    }

    #[test]
    fn postgres_binds_inside_array_constructors_and_subscript_subqueries() {
        assert_eq!(
            labels(
                "SELECT ARRAY[:a, :b]::int[], x[(SELECT :i)], :after",
                SqlDialect::Postgres
            ),
            vec![":a", ":b", ":i", ":after"]
        );
        assert_eq!(
            labels("SELECT arr[$1:$2]", SqlDialect::Postgres),
            vec!["$1", "$2"]
        );
    }

    /// The user's text at the offset the driver reported for `needle`.
    fn mapped<'a>(text: &'a str, dialect: SqlDialect, needle: &str) -> &'a str {
        let script = ScriptParams::scan(text, dialect).expect("scan");
        let start = text.rfind("SELECT").expect("statement");
        let (sql, _) = script
            .statement(text, start, text.len(), dialect)
            .expect("rewrite");
        let local = sql.find(needle).expect("needle in driver sql");
        let offset = start + script.source_offset(text, start, text.len(), dialect, local);
        text.get(offset..).unwrap_or("")
    }

    #[test]
    fn driver_error_offsets_map_back_to_the_typed_text() {
        let pg = "SELECT 1;\nSELECT :long_name, :b, :long_name, nosuchcol";
        assert!(mapped(pg, SqlDialect::Postgres, "nosuchcol").starts_with("nosuchcol"));
        assert!(mapped(pg, SqlDialect::Postgres, "$2").starts_with(":b"));
        let my = "SELECT 1;\nSELECT :long_name, :b, nosuchcol";
        assert!(mapped(my, SqlDialect::Mysql, "nosuchcol").starts_with("nosuchcol"));
        let plain = "SELECT nosuchcol";
        assert!(mapped(plain, SqlDialect::Postgres, "nosuchcol").starts_with("nosuchcol"));
    }
}
