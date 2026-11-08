//! Mongo command risk from the parsed call, not from a substring of the text.
//!
//! One scanner reads JSON and shell text alike. A filter value that happens
//! to contain "delete" is still a read, and an `op` key inside a filter is
//! data, not the method. An op we do not recognize fails closed as a write so
//! it cannot skip the production unlock.

use crate::error::{ConnectorError, ConnectorResult};
use crate::query::QueryMode;
use crate::WriteRisk;

use super::EngineQueryPolicy;

/// MongoDB / document policy.
pub struct MongoQueryPolicy;

impl EngineQueryPolicy for MongoQueryPolicy {
    fn engine_id(&self) -> &'static str {
        "mongodb"
    }

    fn validate(&self, text: &str, mode: QueryMode) -> ConnectorResult<()> {
        let t = text.trim();
        if t.is_empty() {
            return Err(ConnectorError::Policy("Query is empty.".into()));
        }
        match inspect_mongo(t) {
            MongoSeen::Unparsed => Err(ConnectorError::Policy(
                "Could not classify this Mongo query. Use a JSON filter, find(...), listDatabases, or listCollections."
                    .into(),
            )),
            MongoSeen::Read => Ok(()),
            MongoSeen::Write | MongoSeen::Destructive | MongoSeen::UnknownOp => {
                if mode == QueryMode::Read {
                    Err(ConnectorError::Policy(
                        "Write operation blocked. This connection is read-only.".into(),
                    ))
                } else {
                    Ok(())
                }
            }
        }
    }

    fn classify(&self, text: &str) -> WriteRisk {
        classify_mongo_query(text)
    }
}

/// INSERT / UPDATE / REPLACE only (P3.2 `safe_write`). Deletes and `$out`/`$merge` are not.
///
/// Every op and every write method in the text must be a safe one, so a
/// filter cannot lend its `op` to the call around it.
pub fn mongo_is_safe_dml(text: &str) -> bool {
    let text = text.trim();
    let facts = scan_mongo(text);
    if seen_from_facts(&facts, text) != MongoSeen::Write || facts.has_out_stage {
        return false;
    }
    let ops = facts.ops.iter().chain(&facts.write_methods);
    facts.ops.len() + facts.write_methods.len() > 0
        && ops.into_iter().all(|op| is_safe_write_op(op))
}

/// True when the text is a write the plugin should not run as a find.
///
/// Unparsed text is false so a typo still gets the find-parser error. A filter
/// value is not enough: the op or the shell method has to be a write.
pub fn mongo_is_write_command(text: &str) -> bool {
    matches!(
        inspect_mongo(text),
        MongoSeen::Write | MongoSeen::Destructive | MongoSeen::UnknownOp
    )
}

fn classify_mongo_query(text: &str) -> WriteRisk {
    match inspect_mongo(text) {
        MongoSeen::Read => WriteRisk::Read,
        MongoSeen::Destructive => WriteRisk::Destructive,
        // Unknown ops and text we cannot name are writes. Callers that only
        // classify (the production guard, MCP) must not treat them as reads.
        MongoSeen::Write | MongoSeen::UnknownOp | MongoSeen::Unparsed => WriteRisk::Write,
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum MongoSeen {
    Read,
    Write,
    Destructive,
    /// Parsed an op or method we do not recognize.
    UnknownOp,
    /// Not a command shape. `validate` rejects this; `classify` fails closed.
    Unparsed,
}

fn inspect_mongo(text: &str) -> MongoSeen {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return MongoSeen::Unparsed;
    }
    seen_from_facts(&scan_mongo(trimmed), trimmed)
}

/// Read, write or destructive for one op name. `$out` / `$merge` stages are
/// counted by the scanner, whatever the op.
fn seen_from_op(op: &str) -> MongoSeen {
    let op = op.trim().to_ascii_lowercase();
    match op.as_str() {
        "find"
        | "findone"
        | "count"
        | "countdocuments"
        | "estimateddocumentcount"
        | "distinct"
        | "listdatabases"
        | "listcollections"
        | "listindexes"
        | "explain"
        | "aggregate" => MongoSeen::Read,
        "drop" | "dropdatabase" | "dropcollection" => MongoSeen::Destructive,
        "insert" | "insertone" | "insertmany" | "update" | "updateone" | "updatemany"
        | "replaceone" | "delete" | "deleteone" | "deletemany" | "remove" | "bulkwrite"
        | "findoneandupdate" | "findoneandreplace" | "findoneanddelete" | "findandmodify" => {
            MongoSeen::Write
        }
        _ => MongoSeen::UnknownOp,
    }
}

fn is_safe_write_op(op: &str) -> bool {
    matches!(
        op,
        "insert"
            | "insertone"
            | "insertmany"
            | "update"
            | "updateone"
            | "updatemany"
            | "replaceone"
    )
}

fn named_op(text: &str) -> Option<String> {
    let facts = scan_mongo(text.trim());
    facts
        .ops
        .last()
        .cloned()
        .or(facts.command)
        .filter(|s| !s.is_empty())
}

/// Worst of everything the text can run: command-object ops, every method
/// call, and `$out` / `$merge` / `$drop` stage keys.
fn seen_from_facts(facts: &MongoFacts, text: &str) -> MongoSeen {
    let op_seen: Vec<MongoSeen> = facts.ops.iter().map(|op| seen_from_op(op)).collect();
    if facts.saw_destructive || facts.has_drop_stage || op_seen.contains(&MongoSeen::Destructive) {
        return MongoSeen::Destructive;
    }
    if facts.saw_write || facts.has_out_stage || op_seen.contains(&MongoSeen::Write) {
        return MongoSeen::Write;
    }
    if facts.saw_unknown_method || facts.bad_op || op_seen.contains(&MongoSeen::UnknownOp) {
        return MongoSeen::UnknownOp;
    }
    if facts.saw_read || !op_seen.is_empty() || is_mongo_meta(text) || looks_like_filter(text) {
        return MongoSeen::Read;
    }
    MongoSeen::Unparsed
}

fn is_mongo_meta(text: &str) -> bool {
    let compact: String = text.chars().filter(|c| !c.is_whitespace()).collect();
    matches!(
        compact.to_ascii_lowercase().as_str(),
        "listdatabases"
            | "showdatabases"
            | "showdbs"
            | "listcollections"
            | "showcollections"
            | "listdatabase"
    )
}

fn looks_like_filter(text: &str) -> bool {
    let t = text.trim_start();
    t.starts_with('{') || t.starts_with('[')
}

fn is_out_stage_key(key: &str) -> bool {
    key.eq_ignore_ascii_case("$out") || key.eq_ignore_ascii_case("$merge")
}

fn is_drop_stage_key(key: &str) -> bool {
    key.eq_ignore_ascii_case("$drop")
}

/// Op name for the production prompt. One bulk write stays one name.
pub fn mongo_op_preview(text: &str) -> String {
    named_op(text).unwrap_or_else(|| {
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
    })
}

#[derive(Default)]
struct MongoFacts {
    saw_read: bool,
    saw_write: bool,
    saw_destructive: bool,
    saw_unknown_method: bool,
    has_out_stage: bool,
    has_drop_stage: bool,
    /// `op` values at the top level of a `{ op: … }` command object.
    /// Duplicates are all kept, so the worst one wins.
    ops: Vec<String>,
    /// An `op` key whose value is not a name.
    bad_op: bool,
    /// Every write method called, lowercased.
    write_methods: Vec<String>,
    command: Option<String>,
}

/// One pass over JSON or shell text. Keys are read with escapes decoded, so
/// `"$out"` is `$out`. Only a text that is itself an object has an `op`;
/// in `db.c.deleteMany({op: "find"})` the `op` is filter data.
fn scan_mongo(text: &str) -> MongoFacts {
    let chars: Vec<char> = text.chars().collect();
    let command_object = text.trim_start().starts_with('{');
    let mut facts = MongoFacts::default();
    let mut i = 0;
    let mut depth = 0i32;
    while i < chars.len() {
        let c = chars[i];
        if c == '/' && chars.get(i + 1) == Some(&'/') {
            i += 2;
            while i < chars.len() && chars[i] != '\n' {
                i += 1;
            }
            continue;
        }
        if c == '/' && chars.get(i + 1) == Some(&'*') {
            i += 2;
            while i + 1 < chars.len() && !(chars[i] == '*' && chars[i + 1] == '/') {
                i += 1;
            }
            i = (i + 2).min(chars.len());
            continue;
        }
        if c == '"' || c == '\'' || c == '`' {
            let (text, next) = read_quoted(&chars, i, c);
            i = next.max(i + 1);
            if colon_after(&chars, i) {
                note_key(&mut facts, &text, &chars, i, command_object && depth == 1);
            }
            continue;
        }
        if c == '{' {
            depth += 1;
            i += 1;
            continue;
        }
        if c == '}' {
            depth -= 1;
            i += 1;
            continue;
        }
        if c == '$' {
            if let Some(name) = ident_at(&chars, i + 1) {
                let after = i + 1 + name.len();
                if colon_after(&chars, after) {
                    note_key(&mut facts, &format!("${name}"), &chars, after, false);
                }
                i = after;
                continue;
            }
            i += 1;
            continue;
        }
        if c == '.' {
            if let Some(name) = ident_at(&chars, i + 1) {
                let after = i + 1 + name.len();
                if paren_after(&chars, after) {
                    note_method(&mut facts, &name);
                    i = after;
                    continue;
                }
            }
            i += 1;
            continue;
        }
        if is_ident_start(c) {
            if let Some(key) = ident_at(&chars, i) {
                i += key.len().max(1);
                if colon_after(&chars, i) {
                    note_key(&mut facts, &key, &chars, i, command_object && depth == 1);
                }
                continue;
            }
        }
        i += 1;
    }
    facts
}

/// `key` is followed by a colon at `after`. `top` is the command object's own level.
fn note_key(facts: &mut MongoFacts, key: &str, chars: &[char], after: usize, top: bool) {
    if is_out_stage_key(key) {
        facts.has_out_stage = true;
    } else if is_drop_stage_key(key) {
        facts.has_drop_stage = true;
    } else if top && key.eq_ignore_ascii_case("op") {
        match value_after_colon(chars, after) {
            Some(op) => facts.ops.push(op.trim().to_ascii_lowercase()),
            None => facts.bad_op = true,
        }
    }
}

fn note_method(facts: &mut MongoFacts, name: &str) {
    let n = name.to_ascii_lowercase();
    match n.as_str() {
        "limit" | "skip" | "sort" | "project" | "projection" | "hint" | "pretty" | "toarray"
        | "foreach" | "map" | "hasnext" | "next" | "close" | "batchsize" | "maxtimems"
        | "collation" | "comment" | "allowdiskuse" | "getsiblingdb" | "getcollection"
        | "getmongo" | "getdb" | "getname" | "readpref" | "max" | "min" => {}
        "find"
        | "findone"
        | "aggregate"
        | "count"
        | "countdocuments"
        | "estimateddocumentcount"
        | "distinct"
        | "explain"
        | "getindexes"
        | "stats"
        | "datasize"
        | "storagesize"
        | "listdatabases"
        | "listcollections" => {
            facts.saw_read = true;
            if facts.command.is_none() {
                facts.command = Some(n);
            }
        }
        "drop" | "dropdatabase" | "dropcollection" => {
            facts.saw_destructive = true;
            facts.command = Some(n);
        }
        "insert" | "insertone" | "insertmany" | "update" | "updateone" | "updatemany"
        | "replaceone" | "delete" | "deleteone" | "deletemany" | "remove" | "bulkwrite"
        | "findoneandupdate" | "findoneandreplace" | "findoneanddelete" | "findandmodify" => {
            facts.saw_write = true;
            facts.write_methods.push(n.clone());
            facts.command = Some(n);
        }
        _ => {
            facts.saw_unknown_method = true;
            if facts.command.is_none() {
                facts.command = Some(n);
            }
        }
    }
}

fn is_ident_start(c: char) -> bool {
    c.is_ascii_alphabetic() || c == '_'
}

fn is_ident_cont(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_'
}

fn ident_at(chars: &[char], start: usize) -> Option<String> {
    let c = *chars.get(start)?;
    if !is_ident_start(c) {
        return None;
    }
    let mut i = start + 1;
    while i < chars.len() && is_ident_cont(chars[i]) {
        i += 1;
    }
    Some(chars[start..i].iter().collect())
}

fn skip_ws(chars: &[char], mut i: usize) -> usize {
    while i < chars.len() && chars[i].is_whitespace() {
        i += 1;
    }
    i
}

fn paren_after(chars: &[char], i: usize) -> bool {
    chars.get(skip_ws(chars, i)) == Some(&'(')
}

fn colon_after(chars: &[char], i: usize) -> bool {
    chars.get(skip_ws(chars, i)) == Some(&':')
}

/// Quoted text with JSON escapes decoded, and the index after the close quote.
fn read_quoted(chars: &[char], start: usize, quote: char) -> (String, usize) {
    let mut out = String::new();
    let mut i = start + 1;
    while i < chars.len() {
        let c = chars[i];
        if c == '\\' {
            let Some(next) = chars.get(i + 1) else {
                break;
            };
            if *next == 'u' {
                let hex: String = chars.iter().skip(i + 2).take(4).collect();
                if let Some(ch) = u32::from_str_radix(&hex, 16).ok().and_then(char::from_u32) {
                    out.push(ch);
                    i += 6;
                    continue;
                }
            }
            out.push(match next {
                'n' => '\n',
                't' => '\t',
                'r' => '\r',
                'b' => '\u{8}',
                'f' => '\u{c}',
                other => *other,
            });
            i += 2;
            continue;
        }
        if c == quote {
            return (out, i + 1);
        }
        out.push(c);
        i += 1;
    }
    (out, chars.len())
}

fn value_after_colon(chars: &[char], i: usize) -> Option<String> {
    let mut i = skip_ws(chars, i);
    if chars.get(i) != Some(&':') {
        return None;
    }
    i += 1;
    i = skip_ws(chars, i);
    let c = *chars.get(i)?;
    if c == '"' || c == '\'' || c == '`' {
        let (val, _) = read_quoted(chars, i, c);
        return Some(val);
    }
    ident_at(chars, i)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn insert_is_safe_dml() {
        assert!(mongo_is_safe_dml("db.t.insertOne({a:1})"));
        assert!(mongo_is_safe_dml("db.t.updateOne({a:1}, {$set:{b:2}})"));
        assert!(mongo_is_safe_dml(
            r#"{"op":"insertOne","doc":{"note":"please delete this"}}"#
        ));
    }

    #[test]
    fn delete_and_drop_are_not_safe_dml() {
        assert!(!mongo_is_safe_dml("db.t.deleteOne({a:1})"));
        assert!(!mongo_is_safe_dml("db.t.drop()"));
        assert!(!mongo_is_safe_dml("db.t.find({})"));
        assert!(!mongo_is_safe_dml(r#"{"op":"bulkWrite","ops":[]}"#));
    }

    #[test]
    fn delete_many_json_is_a_write() {
        let policy = MongoQueryPolicy;
        let text = r#"{"op":"deleteMany","filter":{"note":"do not delete this"}}"#;
        assert_eq!(policy.classify(text), WriteRisk::Write);
        assert!(policy.validate(text, QueryMode::Write).is_ok());
        assert!(policy.validate(text, QueryMode::Read).is_err());
        assert!(mongo_is_write_command(text));
    }

    #[test]
    fn filter_value_delete_is_not_a_write() {
        let policy = MongoQueryPolicy;
        let json = r#"{"name":"please delete this"}"#;
        assert_eq!(policy.classify(json), WriteRisk::Read);
        assert!(policy.validate(json, QueryMode::Read).is_ok());
        assert!(!mongo_is_write_command(json));

        let shell = r#"db.users.find({name: "please delete this"})"#;
        assert_eq!(policy.classify(shell), WriteRisk::Read);
        assert!(!mongo_is_write_command(shell));

        let mentioned = r#"db.users.find({name: ".deleteMany("})"#;
        assert_eq!(policy.classify(mentioned), WriteRisk::Read);
    }

    #[test]
    fn shell_delete_many_is_a_write() {
        let policy = MongoQueryPolicy;
        let text = r#"db.users.deleteMany({status: "keep"})"#;
        assert_eq!(policy.classify(text), WriteRisk::Write);
        assert!(mongo_is_write_command(text));
    }

    #[test]
    fn shell_object_op_ignores_nested_filter_text() {
        let policy = MongoQueryPolicy;
        let write = r#"{ op: "deleteMany", filter: { note: "delete" } }"#;
        assert_eq!(policy.classify(write), WriteRisk::Write);
        let read = r#"{ name: "please delete this" }"#;
        assert_eq!(policy.classify(read), WriteRisk::Read);
    }

    #[test]
    fn unknown_op_fails_closed_as_write() {
        let policy = MongoQueryPolicy;
        let text = r#"{"op":"frobnicate"}"#;
        assert_eq!(policy.classify(text), WriteRisk::Write);
        assert!(policy.validate(text, QueryMode::Read).is_err());
        assert!(mongo_is_write_command(text));
    }

    #[test]
    fn drop_and_out_stage_are_destructive() {
        let policy = MongoQueryPolicy;
        assert_eq!(policy.classify(r#"{"op":"drop"}"#), WriteRisk::Destructive);
        assert_eq!(
            policy.classify(r#"{"op":"dropDatabase"}"#),
            WriteRisk::Destructive
        );
        assert_eq!(policy.classify("db.users.drop()"), WriteRisk::Destructive);
        assert_eq!(policy.classify("db.dropDatabase()"), WriteRisk::Destructive);
        assert_eq!(
            policy.classify(r#"db.c.aggregate([{$out: "t"}])"#),
            WriteRisk::Write
        );
        assert!(mongo_is_write_command(r#"db.c.aggregate([{$out: "t"}])"#));
        assert_eq!(
            policy.classify(r#"[{"$match":{"a":1}},{"$merge":"t"}]"#),
            WriteRisk::Write
        );
        assert_eq!(
            policy.classify(r#"db.c.aggregate([{$match:{}},{$drop: "t"}])"#),
            WriteRisk::Destructive
        );
        assert_eq!(
            policy.classify(r#"db.c.aggregate([{$match:{note:"$out"}}])"#),
            WriteRisk::Read
        );
        assert_eq!(
            mongo_op_preview(r#"db.orders.bulkWrite([{insertOne:{document:{a:1}}}])"#),
            "bulkwrite"
        );
    }

    #[test]
    fn count_and_distinct_are_reads_and_shell_writes_are_not() {
        let policy = MongoQueryPolicy;
        assert_eq!(
            policy.classify(r#"db.orders.countDocuments({ status: "open" })"#),
            WriteRisk::Read
        );
        assert_eq!(
            policy.classify(r#"db.orders.distinct("city", { a: 1 })"#),
            WriteRisk::Read
        );
        assert!(policy
            .validate(
                r#"db.orders.aggregate([{ $match: {} }, { $limit: 5 }])"#,
                QueryMode::Read
            )
            .is_ok());
        for text in [
            r#"db.orders.insertMany([{a:1},{a:2}])"#,
            r#"db.orders.updateMany({a:1}, {$set:{b:2}})"#,
            r#"db.orders.replaceOne({a:1}, {a:2})"#,
            r#"db.orders.findOneAndUpdate({a:1}, {$set:{b:2}})"#,
            r#"db.orders.bulkWrite([{insertOne:{document:{a:1}}}])"#,
        ] {
            assert_eq!(policy.classify(text), WriteRisk::Write, "{text}");
            assert!(policy.validate(text, QueryMode::Read).is_err(), "{text}");
            assert!(policy.validate(text, QueryMode::Write).is_ok(), "{text}");
        }
    }

    #[test]
    fn catalog_meta_stays_a_read() {
        let policy = MongoQueryPolicy;
        assert_eq!(policy.classify("listDatabases"), WriteRisk::Read);
        assert!(policy.validate("show collections", QueryMode::Read).is_ok());
    }
}
