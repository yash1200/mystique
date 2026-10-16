//! Mongo command risk from the parsed op, not from a substring of the text.
//!
//! A filter value that happens to contain "delete" is still a read. An op we
//! do not recognize fails closed as a write so it cannot skip the production
//! unlock.

use serde_json::Value;

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
pub fn mongo_is_safe_dml(text: &str) -> bool {
    if classify_mongo_query(text) != WriteRisk::Write {
        return false;
    }
    named_op(text).is_some_and(|op| is_safe_write_op(&op))
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
    if let Ok(value) = serde_json::from_str::<Value>(trimmed) {
        if value.is_object() || value.is_array() {
            return inspect_json(&value);
        }
    }
    seen_from_facts(scan_shell(trimmed), trimmed)
}

fn inspect_json(value: &Value) -> MongoSeen {
    let has_out = json_has_out_stage(value);
    let has_drop = json_has_drop_stage(value);
    if let Some(op) = value.get("op") {
        return match op.as_str() {
            Some(op) => seen_from_op(op, has_out, has_drop),
            None => MongoSeen::UnknownOp,
        };
    }
    if has_drop {
        MongoSeen::Destructive
    } else if has_out {
        MongoSeen::Write
    } else {
        MongoSeen::Read
    }
}

fn seen_from_op(op: &str, has_out: bool, has_drop: bool) -> MongoSeen {
    let op = op.trim().to_ascii_lowercase();
    if op.is_empty() {
        return MongoSeen::UnknownOp;
    }
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
        | "explain" => MongoSeen::Read,
        "aggregate" => {
            if has_drop {
                MongoSeen::Destructive
            } else if has_out {
                MongoSeen::Write
            } else {
                MongoSeen::Read
            }
        }
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
    let trimmed = text.trim();
    if let Ok(value) = serde_json::from_str::<Value>(trimmed) {
        return value
            .get("op")
            .and_then(|v| v.as_str())
            .map(|s| s.trim().to_ascii_lowercase())
            .filter(|s| !s.is_empty());
    }
    let facts = scan_shell(trimmed);
    facts
        .op_field
        .or(facts.command)
        .map(|s| s.to_ascii_lowercase())
        .filter(|s| !s.is_empty())
}

fn seen_from_facts(facts: ShellFacts, text: &str) -> MongoSeen {
    if let Some(op) = facts.op_field.as_deref() {
        return seen_from_op(op, facts.has_out_stage, facts.has_drop_stage);
    }
    if facts.saw_destructive || facts.has_drop_stage {
        return MongoSeen::Destructive;
    }
    let aggregate = facts
        .command
        .as_deref()
        .is_some_and(|c| c.eq_ignore_ascii_case("aggregate"));
    // `$out` / `$merge` are writes. A quoted `$out` inside a filter is not a stage.
    if facts.has_out_stage && (aggregate || !facts.saw_read) {
        return MongoSeen::Write;
    }
    if facts.saw_write {
        return MongoSeen::Write;
    }
    if facts.saw_unknown_method {
        return MongoSeen::UnknownOp;
    }
    if facts.saw_read || is_mongo_meta(text) || looks_like_filter(text) {
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

fn json_has_out_stage(value: &Value) -> bool {
    match value {
        Value::Object(map) => map
            .iter()
            .any(|(key, child)| is_out_stage_key(key) || json_has_out_stage(child)),
        Value::Array(items) => items.iter().any(json_has_out_stage),
        _ => false,
    }
}

fn is_out_stage_key(key: &str) -> bool {
    let key = key.to_ascii_lowercase();
    key == "$out" || key == "$merge"
}

fn json_has_drop_stage(value: &Value) -> bool {
    match value {
        Value::Object(map) => map
            .iter()
            .any(|(key, child)| is_drop_stage_key(key) || json_has_drop_stage(child)),
        Value::Array(items) => items.iter().any(json_has_drop_stage),
        _ => false,
    }
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
struct ShellFacts {
    saw_read: bool,
    saw_write: bool,
    saw_destructive: bool,
    saw_unknown_method: bool,
    has_out_stage: bool,
    has_drop_stage: bool,
    op_field: Option<String>,
    command: Option<String>,
}

/// Shell text: method calls and a top-level `op` field. Quoted text is ignored,
/// so a filter value cannot change the risk.
fn scan_shell(text: &str) -> ShellFacts {
    let chars: Vec<char> = text.chars().collect();
    let mut facts = ShellFacts::default();
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
            if depth == 1 {
                let (key, next) = read_quoted(&chars, i, c);
                i = next.max(i + 1);
                if facts.op_field.is_none() && key.eq_ignore_ascii_case("op") {
                    if let Some(val) = value_after_colon(&chars, i) {
                        facts.op_field = Some(val);
                    }
                }
                continue;
            }
            i = skip_string(&chars, i, c).max(i + 1);
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
                    if name.eq_ignore_ascii_case("out") || name.eq_ignore_ascii_case("merge") {
                        facts.has_out_stage = true;
                    } else if name.eq_ignore_ascii_case("drop") {
                        facts.has_drop_stage = true;
                    }
                }
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
        if depth == 1 && is_ident_start(c) {
            if let Some(key) = ident_at(&chars, i) {
                i += key.len().max(1);
                if facts.op_field.is_none() && key.eq_ignore_ascii_case("op") {
                    if let Some(val) = value_after_colon(&chars, i) {
                        facts.op_field = Some(val);
                    }
                }
                continue;
            }
        }
        i += 1;
    }
    facts
}

fn note_method(facts: &mut ShellFacts, name: &str) {
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

fn skip_string(chars: &[char], start: usize, quote: char) -> usize {
    let mut i = start + 1;
    while i < chars.len() {
        if chars[i] == '\\' {
            i += 2;
            continue;
        }
        if chars[i] == quote {
            return i + 1;
        }
        i += 1;
    }
    chars.len()
}

fn read_quoted(chars: &[char], start: usize, quote: char) -> (String, usize) {
    let mut out = String::new();
    let mut i = start + 1;
    while i < chars.len() {
        let c = chars[i];
        if c == '\\' {
            if let Some(next) = chars.get(i + 1) {
                out.push(*next);
                i += 2;
                continue;
            }
            break;
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
