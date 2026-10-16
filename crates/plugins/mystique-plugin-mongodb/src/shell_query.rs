//! Parse `db.coll.find(filter, projection).sort(...).limit(...)` shell reads.

use mongodb::bson::Document;
use mystique_connector_api::{ConnectorError, ConnectorResult};

use crate::shell_filter::{json_to_document, parse_filter_object};

#[derive(Debug, Clone, Default)]
pub struct ParsedFind {
    pub collection: Option<String>,
    pub filter: Document,
    pub projection: Option<Document>,
    pub sort: Option<Document>,
    pub limit: Option<i64>,
    pub skip: Option<u64>,
}

/// Bare `{ … }` filter, or a shell `find(...)` with optional projection / sort / limit / skip.
pub fn parse_read_query(text: &str) -> ConnectorResult<ParsedFind> {
    let t = text.trim();
    if t.is_empty() {
        return Err(ConnectorError::Query("Query is empty.".into()));
    }
    if t.starts_with('{') {
        let obj = slice_balanced(t, 0, '{', '}')?;
        let rest = t
            .get(obj.len()..)
            .unwrap_or("")
            .trim()
            .trim_end_matches(';')
            .trim();
        if !rest.is_empty() {
            return Err(ConnectorError::Query(format!(
                "unexpected text after filter: {rest}"
            )));
        }
        return Ok(ParsedFind {
            filter: parse_object_or_empty(obj)?,
            ..ParsedFind::default()
        });
    }

    let lower = t.to_ascii_lowercase();
    let one_at = top_level_method(t, Some("findone"));
    let many_at = top_level_method(t, Some("find"));
    let (find_at, one) = match (many_at, one_at) {
        (Some(many_at), Some(one_at)) if one_at < many_at => (one_at, true),
        (Some(many_at), _) => (many_at, false),
        (None, Some(one_at)) => (one_at, true),
        (None, None) => match find_ident(&lower, "find") {
            Some(at) => (at, false),
            None => {
                return Err(ConnectorError::Query(
                    "Mongo query must be a JSON filter `{ ... }` or `db.collection.find(...)`."
                        .into(),
                ));
            }
        },
    };

    let collection = collection_before_find(t, find_at);
    let name_len = match ascii_ident(&lower, find_at) {
        Some(name) => name.len(),
        None => 4,
    };
    let after_name = skip_ws(t, find_at + name_len);
    if t.as_bytes().get(after_name) != Some(&b'(') {
        return Ok(ParsedFind {
            collection,
            filter: Document::new(),
            limit: one.then_some(1),
            ..ParsedFind::default()
        });
    }

    let call = slice_balanced(t, after_name, '(', ')')?;
    let inner = peel(call);
    let args = split_top_level_objects(inner)?;

    let filter = match args.first() {
        Some(arg) => parse_object_or_empty(arg)?,
        None => Document::new(),
    };
    let projection = match args.get(1) {
        Some(arg) => Some(parse_filter_object(arg)?),
        None => None,
    };

    let mut parsed = ParsedFind {
        collection,
        filter,
        projection,
        ..ParsedFind::default()
    };

    let mut rest_i = after_name + call.len();
    loop {
        rest_i = skip_ws(t, rest_i);
        if rest_i >= t.len() {
            break;
        }
        if t.as_bytes().get(rest_i) == Some(&b';') {
            rest_i += 1;
            continue;
        }
        if t.as_bytes().get(rest_i) != Some(&b'.') {
            let rest = t
                .get(rest_i..)
                .unwrap_or("")
                .trim()
                .trim_end_matches(';')
                .trim();
            if rest.is_empty() {
                break;
            }
            return Err(ConnectorError::Query(format!(
                "unexpected text after find(…): {rest}"
            )));
        }
        rest_i += 1;
        rest_i = skip_ws(t, rest_i);
        let name_end = ident_end(t, rest_i);
        if name_end == rest_i {
            return Err(ConnectorError::Query("expected method after `.`".into()));
        }
        let method = t.get(rest_i..name_end).unwrap_or("").to_ascii_lowercase();
        rest_i = skip_ws(t, name_end);
        if t.as_bytes().get(rest_i) != Some(&b'(') {
            return Err(ConnectorError::Query(format!(
                "expected `(` after .{method}"
            )));
        }
        let args_call = slice_balanced(t, rest_i, '(', ')')?;
        let args_inner = peel(args_call).trim();
        rest_i += args_call.len();

        match method.as_str() {
            "sort" => {
                let obj = first_object_arg(args_inner)?;
                parsed.sort = Some(parse_filter_object(obj)?);
            }
            "project" | "projection" => {
                let obj = first_object_arg(args_inner)?;
                parsed.projection = Some(parse_filter_object(obj)?);
            }
            "limit" => {
                parsed.limit = Some(parse_i64_arg(args_inner, "limit")?);
            }
            "skip" => {
                let n = parse_i64_arg(args_inner, "skip")?;
                parsed.skip = Some(n.max(0) as u64);
            }
            "pretty" | "toarray" | "explain" => {}
            other => {
                return Err(ConnectorError::Query(format!(
                    "unsupported chain `.{other}()` — use .sort / .limit / .skip / .project"
                )));
            }
        }
    }

    if one {
        parsed.limit = Some(1);
    }
    Ok(parsed)
}

fn parse_object_or_empty(slice: &str) -> ConnectorResult<Document> {
    let compact: String = slice.chars().filter(|c| !c.is_whitespace()).collect();
    if compact == "{}" {
        return Ok(Document::new());
    }
    parse_filter_object(slice)
}

fn first_object_arg(inner: &str) -> ConnectorResult<&str> {
    let t = inner.trim();
    if t.is_empty() {
        return Err(ConnectorError::Query("expected object argument".into()));
    }
    if !t.starts_with('{') {
        return Err(ConnectorError::Query(format!(
            "expected object `{{ … }}`, got `{t}`"
        )));
    }
    slice_balanced(t, 0, '{', '}')
}

fn parse_i64_arg(inner: &str, name: &str) -> ConnectorResult<i64> {
    let t = inner.trim().trim_end_matches(',').trim();
    t.parse::<i64>()
        .map_err(|_| ConnectorError::Query(format!(".{name}() needs an integer, got `{t}`")))
}

fn peel(call: &str) -> &str {
    let end = call.len().saturating_sub(1);
    call.get(1..end).unwrap_or("")
}

fn collection_before_find(text: &str, find_at: usize) -> Option<String> {
    let before = text.get(..find_at).unwrap_or("").trim_end();
    let before = before.strip_suffix('.').unwrap_or(before).trim_end();
    if before.is_empty() {
        return None;
    }
    if let Some(rest) = before.strip_prefix("db.") {
        let name = rest.trim();
        if !name.is_empty() && !name.contains('.') {
            return Some(name.to_string());
        }
    }
    if let Some(dot) = before.rfind('.') {
        let name = before.get(dot + 1..).unwrap_or("").trim();
        if !name.is_empty() {
            return Some(name.to_string());
        }
    }
    None
}

fn find_ident(lower: &str, name: &str) -> Option<usize> {
    let hay = lower.as_bytes();
    let needle = name.as_bytes();
    if needle.is_empty() || needle.len() > hay.len() {
        return None;
    }
    let mut i = 0;
    while i + needle.len() <= hay.len() {
        if hay.get(i..i + needle.len()) == Some(needle) {
            let after = i + needle.len();
            let before_ok = i == 0
                || hay
                    .get(i - 1)
                    .is_some_and(|b| !b.is_ascii_alphanumeric() && *b != b'_');
            let after_ok = hay
                .get(after)
                .is_none_or(|b| !b.is_ascii_alphanumeric() && *b != b'_');
            if before_ok && after_ok {
                return Some(i);
            }
        }
        i += 1;
    }
    None
}

fn ident_end(s: &str, start: usize) -> usize {
    let b = s.as_bytes();
    let mut i = start;
    while b
        .get(i)
        .is_some_and(|c| c.is_ascii_alphanumeric() || *c == b'_')
    {
        i += 1;
    }
    i
}

fn skip_ws(s: &str, mut i: usize) -> usize {
    let b = s.as_bytes();
    while b.get(i).is_some_and(|c| c.is_ascii_whitespace()) {
        i += 1;
    }
    i
}

fn split_top_level_objects(inner: &str) -> ConnectorResult<Vec<&str>> {
    let mut out = Vec::new();
    let mut i = 0;
    let n = inner.len();
    while i < n {
        i = skip_ws(inner, i);
        if i >= n {
            break;
        }
        let Some(b) = inner.as_bytes().get(i).copied() else {
            break;
        };
        if b == b',' {
            i += 1;
            continue;
        }
        if b == b'{' {
            let obj = slice_balanced(inner, i, '{', '}')?;
            i += obj.len();
            out.push(obj);
            continue;
        }
        let sample: String = inner.get(i..).unwrap_or("").chars().take(24).collect();
        return Err(ConnectorError::Query(format!(
            "find() arguments must be objects `{{ … }}` (got `{sample}`)"
        )));
    }
    Ok(out)
}

/// Inclusive slice of a balanced `open…close` starting at `start`.
pub fn slice_balanced<'a>(
    s: &'a str,
    start: usize,
    open: char,
    close: char,
) -> ConnectorResult<&'a str> {
    let rest = s
        .get(start..)
        .ok_or_else(|| ConnectorError::Query("expected object or `(`".into()))?;
    let mut chars = rest.char_indices();
    let Some((_, c0)) = chars.next() else {
        return Err(ConnectorError::Query("expected object or `(`".into()));
    };
    if c0 != open {
        return Err(ConnectorError::Query(format!(
            "expected `{open}`, got `{c0}`"
        )));
    }
    let mut depth = 1i32;
    let mut in_str: Option<char> = None;
    let mut esc = false;
    for (rel, c) in chars {
        if let Some(q) = in_str {
            if esc {
                esc = false;
            } else if c == '\\' {
                esc = true;
            } else if c == q {
                in_str = None;
            }
            continue;
        }
        if c == '"' || c == '\'' {
            in_str = Some(c);
            continue;
        }
        if c == open {
            depth += 1;
        } else if c == close {
            depth -= 1;
            if depth == 0 {
                let end = start + rel + c.len_utf8();
                return s.get(start..end).ok_or_else(|| {
                    ConnectorError::Query(format!("unclosed `{open}` in Mongo query"))
                });
            }
        }
    }
    Err(ConnectorError::Query(format!(
        "unclosed `{open}` in Mongo query"
    )))
}

#[derive(Debug, Clone)]
pub struct ParsedAggregate {
    pub collection: Option<String>,
    pub pipeline: Vec<Document>,
}

/// `None` when the text is not an aggregate call.
///
/// The call has to sit outside strings and comments. A `.aggregate(` inside a
/// find filter is not an aggregate.
pub fn parse_aggregate(text: &str) -> ConnectorResult<Option<ParsedAggregate>> {
    let t = text.trim().trim_end_matches(';').trim();
    let Some(at) = top_level_method(t, Some("aggregate")) else {
        return Ok(None);
    };
    Ok(Some(parse_aggregate_at(t, at)?))
}

fn parse_aggregate_at(text: &str, at: usize) -> ConnectorResult<ParsedAggregate> {
    let collection = collection_before_find(text, at);
    let after = skip_ws(text, at + "aggregate".len());
    if text.as_bytes().get(after) != Some(&b'(') {
        return Err(ConnectorError::Query(
            "aggregate needs a pipeline array.".into(),
        ));
    }
    let call = slice_balanced(text, after, '(', ')')?;
    reject_trailing(text, after + call.len())?;
    let inner = peel(call).trim();
    if !inner.starts_with('[') {
        return Err(ConnectorError::Query(
            "aggregate() takes a pipeline array `[ ... ]`.".into(),
        ));
    }
    let array = slice_balanced(inner, 0, '[', ']')?;
    let body = peel(array);
    let stages = split_top_level_objects(body)?;
    let mut pipeline = Vec::new();
    for stage in stages {
        pipeline.push(parse_filter_object(stage)?);
    }
    Ok(ParsedAggregate {
        collection,
        pipeline,
    })
}

/// Index of a depth-0 `.method(`, outside strings and comments.
///
/// `only` restricts the match to that method. Otherwise the first call wins,
/// so a later `insertOne` cannot steal a `find`.
fn top_level_method(text: &str, only: Option<&str>) -> Option<usize> {
    let lower = text.to_ascii_lowercase();
    let bytes = lower.as_bytes();
    let mut i = 0;
    let mut depth = 0i32;
    let mut quote: Option<u8> = None;
    let mut escaped = false;
    let mut line_comment = false;
    let mut block_comment = false;
    while i < bytes.len() {
        let ch = bytes.get(i).copied().unwrap_or(0);
        if line_comment {
            if ch == b'\n' {
                line_comment = false;
            }
            i += 1;
            continue;
        }
        if block_comment {
            if ch == b'*' && bytes.get(i + 1) == Some(&b'/') {
                block_comment = false;
                i += 2;
                continue;
            }
            i += 1;
            continue;
        }
        if let Some(q) = quote {
            if escaped {
                escaped = false;
            } else if ch == b'\\' {
                escaped = true;
            } else if ch == q {
                quote = None;
            }
            i += 1;
            continue;
        }
        if ch == b'/' && bytes.get(i + 1) == Some(&b'/') {
            line_comment = true;
            i += 2;
            continue;
        }
        if ch == b'/' && bytes.get(i + 1) == Some(&b'*') {
            block_comment = true;
            i += 2;
            continue;
        }
        if ch == b'"' || ch == b'\'' || ch == b'`' {
            quote = Some(ch);
            i += 1;
            continue;
        }
        if ch == b'{' || ch == b'[' || ch == b'(' {
            depth += 1;
            i += 1;
            continue;
        }
        if ch == b'}' || ch == b']' || ch == b')' {
            if depth > 0 {
                depth -= 1;
            }
            i += 1;
            continue;
        }
        if depth == 0 && ch == b'.' {
            let name_at = skip_ws(text, i + 1);
            if let Some(name) = ascii_ident(&lower, name_at) {
                let paren = skip_ws(text, name_at + name.len());
                if text.as_bytes().get(paren) == Some(&b'(') && only.is_none_or(|want| name == want)
                {
                    return Some(name_at);
                }
            }
        }
        i += 1;
    }
    None
}

fn ascii_ident(lower: &str, at: usize) -> Option<String> {
    let bytes = lower.as_bytes();
    let start = bytes.get(at).copied()?;
    if !start.is_ascii_alphabetic() && start != b'_' {
        return None;
    }
    let mut end = at + 1;
    while bytes
        .get(end)
        .is_some_and(|c| c.is_ascii_alphanumeric() || *c == b'_')
    {
        end += 1;
    }
    lower.get(at..end).map(str::to_string)
}

fn reject_trailing(text: &str, end: usize) -> ConnectorResult<()> {
    let mut rest = text.get(end..).unwrap_or("").trim();
    if let Some(comment) = rest.strip_prefix("//") {
        rest = match comment.split_once('\n') {
            Some((_, after)) => after.trim(),
            None => "",
        };
    }
    let rest = rest.trim_end_matches(';').trim();
    let compact: String = rest.chars().filter(|c| !c.is_whitespace()).collect();
    let compact = compact.trim_end_matches(';');
    if compact.is_empty()
        || compact.eq_ignore_ascii_case(".pretty()")
        || compact.eq_ignore_ascii_case(".toarray()")
    {
        return Ok(());
    }
    Err(ConnectorError::Query(format!(
        "unexpected text after the Mongo call: {rest}"
    )))
}

#[derive(Debug, Clone)]
pub enum MongoCall {
    Find(ParsedFind),
    Aggregate(ParsedAggregate),
    Count {
        collection: Option<String>,
        filter: Document,
    },
    Distinct {
        collection: Option<String>,
        field: String,
        filter: Document,
    },
    Write(ShellWrite),
}

#[derive(Debug, Clone)]
pub enum ShellWrite {
    InsertOne {
        collection: Option<String>,
        document: Document,
    },
    InsertMany {
        collection: Option<String>,
        documents: Vec<Document>,
    },
    UpdateOne {
        collection: Option<String>,
        filter: Document,
        update: Document,
    },
    UpdateMany {
        collection: Option<String>,
        filter: Document,
        update: Document,
    },
    ReplaceOne {
        collection: Option<String>,
        filter: Document,
        replacement: Document,
    },
    DeleteOne {
        collection: Option<String>,
        filter: Document,
    },
    DeleteMany {
        collection: Option<String>,
        filter: Document,
    },
    FindOneAndUpdate {
        collection: Option<String>,
        filter: Document,
        update: Document,
    },
    Bulk {
        collection: Option<String>,
        models: Vec<BulkModel>,
    },
}

#[derive(Debug, Clone)]
pub enum BulkModel {
    InsertOne(Document),
    UpdateOne {
        filter: Document,
        update: Document,
    },
    UpdateMany {
        filter: Document,
        update: Document,
    },
    ReplaceOne {
        filter: Document,
        replacement: Document,
    },
    DeleteOne(Document),
    DeleteMany(Document),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AggregateEffect {
    Read,
    Write,
    Destructive,
}

pub fn aggregate_effect(pipeline: &[Document]) -> AggregateEffect {
    let mut write = false;
    for stage in pipeline {
        for key in stage.keys() {
            let key = key.to_ascii_lowercase();
            if key == "$drop" {
                return AggregateEffect::Destructive;
            }
            if key == "$out" || key == "$merge" {
                write = true;
            }
        }
    }
    if write {
        AggregateEffect::Write
    } else {
        AggregateEffect::Read
    }
}

/// Shell call, JSON `{ op }`, or a find. Meta commands stay with the caller.
pub fn parse_mongo_call(text: &str) -> ConnectorResult<MongoCall> {
    let t = text.trim().trim_end_matches(';').trim();
    if t.is_empty() {
        return Err(ConnectorError::Query("Query is empty.".into()));
    }
    if let Some(at) = top_level_method(t, None) {
        let method = ascii_ident(&t.to_ascii_lowercase(), at).unwrap_or_default();
        match method.as_str() {
            "aggregate" => return Ok(MongoCall::Aggregate(parse_aggregate_at(t, at)?)),
            "find" | "findone" => return Ok(MongoCall::Find(parse_read_query(t)?)),
            "count" | "countdocuments" | "distinct" | "insertone" | "insertmany" | "updateone"
            | "updatemany" | "replaceone" | "deleteone" | "deletemany" | "findoneandupdate"
            | "bulkwrite" => {
                return call_from_method(t, at, &method);
            }
            _ => {}
        }
    }
    if t.starts_with('{') {
        if let Some(call) = json_object_call(t)? {
            return Ok(call);
        }
    }
    if t.starts_with('[') {
        return Ok(MongoCall::Aggregate(ParsedAggregate {
            collection: None,
            pipeline: documents_in_array(t)?,
        }));
    }
    Ok(MongoCall::Find(parse_read_query(t)?))
}

fn call_from_method(text: &str, at: usize, method: &str) -> ConnectorResult<MongoCall> {
    let collection = collection_before_find(text, at);
    let after = skip_ws(text, at + method.len());
    let call = slice_balanced(text, after, '(', ')')?;
    reject_trailing(text, after + call.len())?;
    method_to_call(collection, method, peel(call))
}

fn method_to_call(
    collection: Option<String>,
    method: &str,
    args: &str,
) -> ConnectorResult<MongoCall> {
    match method {
        "count" | "countdocuments" => Ok(MongoCall::Count {
            collection,
            filter: optional_object(args)?,
        }),
        "distinct" => {
            let parts = split_args(args)?;
            let field_raw = parts
                .first()
                .ok_or_else(|| ConnectorError::Query("distinct needs a field name.".into()))?;
            let field = unquote(field_raw)?;
            if field.is_empty() {
                return Err(ConnectorError::Query("distinct needs a field name.".into()));
            }
            let filter = match parts.get(1) {
                Some(raw) => parse_filter_object(raw)?,
                None => Document::new(),
            };
            Ok(MongoCall::Distinct {
                collection,
                field,
                filter,
            })
        }
        "insertone" => Ok(MongoCall::Write(ShellWrite::InsertOne {
            collection,
            document: required_object(args)?,
        })),
        "insertmany" => Ok(MongoCall::Write(ShellWrite::InsertMany {
            collection,
            documents: documents_in_array(args.trim())?,
        })),
        "updateone" => {
            let (filter, update) = two_objects(args, "updateOne")?;
            Ok(MongoCall::Write(ShellWrite::UpdateOne {
                collection,
                filter,
                update,
            }))
        }
        "updatemany" => {
            let (filter, update) = two_objects(args, "updateMany")?;
            Ok(MongoCall::Write(ShellWrite::UpdateMany {
                collection,
                filter,
                update,
            }))
        }
        "replaceone" => {
            let (filter, replacement) = two_objects(args, "replaceOne")?;
            Ok(MongoCall::Write(ShellWrite::ReplaceOne {
                collection,
                filter,
                replacement,
            }))
        }
        "deleteone" => Ok(MongoCall::Write(ShellWrite::DeleteOne {
            collection,
            filter: required_object(args)?,
        })),
        "deletemany" => Ok(MongoCall::Write(ShellWrite::DeleteMany {
            collection,
            filter: required_object(args)?,
        })),
        "findoneandupdate" => {
            let (filter, update) = two_objects(args, "findOneAndUpdate")?;
            Ok(MongoCall::Write(ShellWrite::FindOneAndUpdate {
                collection,
                filter,
                update,
            }))
        }
        "bulkwrite" => {
            let docs = documents_in_array(args.trim())?;
            let mut models = Vec::with_capacity(docs.len());
            for doc in &docs {
                models.push(bulk_model(doc)?);
            }
            if models.is_empty() {
                return Err(ConnectorError::Query(
                    "bulkWrite needs at least one operation.".into(),
                ));
            }
            Ok(MongoCall::Write(ShellWrite::Bulk { collection, models }))
        }
        other => Err(ConnectorError::Query(format!(
            "Mongo method `.{other}()` is not supported."
        ))),
    }
}

fn json_object_call(text: &str) -> ConnectorResult<Option<MongoCall>> {
    if let Ok(value) = serde_json::from_str::<serde_json::Value>(text) {
        if value.get("op").is_some() {
            return Ok(Some(json_op_call(&value)?));
        }
    }
    if let Ok(doc) = parse_filter_object(text) {
        if doc.get("op").is_some() {
            let value = serde_json::to_value(&doc).map_err(|err| {
                ConnectorError::Query(format!("could not read Mongo command: {err}"))
            })?;
            return Ok(Some(json_op_call(&value)?));
        }
    }
    Ok(None)
}

fn json_op_call(value: &serde_json::Value) -> ConnectorResult<MongoCall> {
    let op = value
        .get("op")
        .and_then(|item| item.as_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    if op.is_empty() {
        return Err(ConnectorError::Query(
            "Mongo command is missing `op`.".into(),
        ));
    }
    match op.as_str() {
        "aggregate" => {
            let pipeline = match value.get("pipeline") {
                Some(pipeline) => json_documents(pipeline)?,
                None => Vec::new(),
            };
            Ok(MongoCall::Aggregate(ParsedAggregate {
                collection: None,
                pipeline,
            }))
        }
        "count" | "countdocuments" => Ok(MongoCall::Count {
            collection: None,
            filter: optional_json_doc(value, "filter")?,
        }),
        "distinct" => {
            let field = value
                .get("field")
                .and_then(|item| item.as_str())
                .unwrap_or("")
                .to_string();
            if field.is_empty() {
                return Err(ConnectorError::Query("distinct requires `field`.".into()));
            }
            Ok(MongoCall::Distinct {
                collection: None,
                field,
                filter: optional_json_doc(value, "filter")?,
            })
        }
        "insertone" => Ok(MongoCall::Write(ShellWrite::InsertOne {
            collection: None,
            document: required_json_doc(value, "doc")?,
        })),
        "insertmany" => Ok(MongoCall::Write(ShellWrite::InsertMany {
            collection: None,
            documents: json_documents(
                value
                    .get("docs")
                    .ok_or_else(|| ConnectorError::Query("insertMany requires `docs`.".into()))?,
            )?,
        })),
        "updateone" => Ok(MongoCall::Write(ShellWrite::UpdateOne {
            collection: None,
            filter: required_json_doc(value, "filter")?,
            update: required_json_doc(value, "update")?,
        })),
        "updatemany" => Ok(MongoCall::Write(ShellWrite::UpdateMany {
            collection: None,
            filter: required_json_doc(value, "filter")?,
            update: required_json_doc(value, "update")?,
        })),
        "replaceone" => Ok(MongoCall::Write(ShellWrite::ReplaceOne {
            collection: None,
            filter: required_json_doc(value, "filter")?,
            replacement: required_json_doc(value, "replacement")
                .or_else(|_| required_json_doc(value, "doc"))?,
        })),
        "deleteone" => Ok(MongoCall::Write(ShellWrite::DeleteOne {
            collection: None,
            filter: required_json_doc(value, "filter")?,
        })),
        "deletemany" | "remove" => Ok(MongoCall::Write(ShellWrite::DeleteMany {
            collection: None,
            filter: required_json_doc(value, "filter")?,
        })),
        "findoneandupdate" => Ok(MongoCall::Write(ShellWrite::FindOneAndUpdate {
            collection: None,
            filter: required_json_doc(value, "filter")?,
            update: required_json_doc(value, "update")?,
        })),
        "bulkwrite" => {
            let docs = json_documents(
                value
                    .get("ops")
                    .ok_or_else(|| ConnectorError::Query("bulkWrite requires `ops`.".into()))?,
            )?;
            let mut models = Vec::with_capacity(docs.len());
            for doc in &docs {
                models.push(bulk_model(doc)?);
            }
            if models.is_empty() {
                return Err(ConnectorError::Query(
                    "bulkWrite needs at least one operation.".into(),
                ));
            }
            Ok(MongoCall::Write(ShellWrite::Bulk {
                collection: None,
                models,
            }))
        }
        other => Err(ConnectorError::NotSupported(format!(
            "Mongo op `{other}` is not implemented."
        ))),
    }
}

fn optional_json_doc(value: &serde_json::Value, key: &str) -> ConnectorResult<Document> {
    match value.get(key) {
        Some(item) => json_to_document(item),
        None => Ok(Document::new()),
    }
}

fn required_json_doc(value: &serde_json::Value, key: &str) -> ConnectorResult<Document> {
    let item = value
        .get(key)
        .ok_or_else(|| ConnectorError::Query(format!("Mongo command requires `{key}`.")))?;
    json_to_document(item)
}

fn json_documents(value: &serde_json::Value) -> ConnectorResult<Vec<Document>> {
    let items = value
        .as_array()
        .ok_or_else(|| ConnectorError::Query("expected a JSON array of documents.".into()))?;
    let mut docs = Vec::with_capacity(items.len());
    for item in items {
        docs.push(json_to_document(item)?);
    }
    Ok(docs)
}

fn optional_object(args: &str) -> ConnectorResult<Document> {
    let t = args.trim();
    if t.is_empty() {
        return Ok(Document::new());
    }
    parse_filter_object(t)
}

fn required_object(args: &str) -> ConnectorResult<Document> {
    let t = args.trim();
    if t.is_empty() {
        return Err(ConnectorError::Query("expected an object argument.".into()));
    }
    let obj = first_object_arg(t)?;
    parse_filter_object(obj)
}

fn two_objects(args: &str, name: &str) -> ConnectorResult<(Document, Document)> {
    let parts = split_args(args)?;
    let first = parts
        .first()
        .ok_or_else(|| ConnectorError::Query(format!("{name} needs a filter and a document.")))?;
    let second = parts
        .get(1)
        .ok_or_else(|| ConnectorError::Query(format!("{name} needs a filter and a document.")))?;
    Ok((parse_filter_object(first)?, parse_filter_object(second)?))
}

fn documents_in_array(raw: &str) -> ConnectorResult<Vec<Document>> {
    let t = raw.trim();
    if !t.starts_with('[') {
        return Err(ConnectorError::Query(
            "expected an array of documents.".into(),
        ));
    }
    let array = slice_balanced(t, 0, '[', ']')?;
    let parts = split_top_level_objects(peel(array))?;
    let mut docs = Vec::with_capacity(parts.len());
    for part in parts {
        docs.push(parse_filter_object(part)?);
    }
    Ok(docs)
}

fn bulk_model(doc: &Document) -> ConnectorResult<BulkModel> {
    let mut keys = doc.keys();
    let Some(key) = keys.next() else {
        return Err(ConnectorError::Query(
            "bulkWrite operation is empty.".into(),
        ));
    };
    if keys.next().is_some() {
        return Err(ConnectorError::Query(
            "each bulkWrite operation needs one key.".into(),
        ));
    }
    let Some(body) = doc.get(key).and_then(mongodb::bson::Bson::as_document) else {
        return Err(ConnectorError::Query(format!(
            "bulkWrite `{key}` needs an object."
        )));
    };
    let filter = |name: &str| object_field(body, name);
    match key.to_ascii_lowercase().as_str() {
        "insertone" => Ok(BulkModel::InsertOne(filter("document")?)),
        "updateone" => Ok(BulkModel::UpdateOne {
            filter: filter("filter")?,
            update: filter("update")?,
        }),
        "updatemany" => Ok(BulkModel::UpdateMany {
            filter: filter("filter")?,
            update: filter("update")?,
        }),
        "replaceone" => Ok(BulkModel::ReplaceOne {
            filter: filter("filter")?,
            replacement: filter("replacement")?,
        }),
        "deleteone" => Ok(BulkModel::DeleteOne(filter("filter")?)),
        "deletemany" => Ok(BulkModel::DeleteMany(filter("filter")?)),
        other => Err(ConnectorError::Query(format!(
            "bulkWrite op `{other}` is not supported."
        ))),
    }
}

fn object_field(doc: &Document, key: &str) -> ConnectorResult<Document> {
    doc.get(key)
        .and_then(mongodb::bson::Bson::as_document)
        .cloned()
        .ok_or_else(|| ConnectorError::Query(format!("bulkWrite needs `{key}`.")))
}

fn split_args(inner: &str) -> ConnectorResult<Vec<&str>> {
    let mut out = Vec::new();
    let bytes = inner.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        i = skip_ws(inner, i);
        if i >= bytes.len() {
            break;
        }
        if bytes.get(i) == Some(&b',') {
            i += 1;
            continue;
        }
        let start = i;
        match bytes.get(i).copied() {
            Some(b'{') => {
                let obj = slice_balanced(inner, i, '{', '}')?;
                i += obj.len();
            }
            Some(b'[') => {
                let arr = slice_balanced(inner, i, '[', ']')?;
                i += arr.len();
            }
            Some(b'"') | Some(b'\'') => {
                i = skip_quoted(inner, i)?;
            }
            _ => {
                while i < bytes.len() && bytes.get(i) != Some(&b',') {
                    i += 1;
                }
            }
        }
        let slice = inner.get(start..i).unwrap_or("").trim();
        if !slice.is_empty() {
            out.push(slice);
        }
    }
    Ok(out)
}

fn skip_quoted(text: &str, start: usize) -> ConnectorResult<usize> {
    let bytes = text.as_bytes();
    let quote = bytes.get(start).copied().unwrap_or(b'"');
    let mut i = start + 1;
    let mut escaped = false;
    while i < bytes.len() {
        let ch = bytes.get(i).copied().unwrap_or(0);
        if escaped {
            escaped = false;
            i += 1;
            continue;
        }
        if ch == b'\\' {
            escaped = true;
            i += 1;
            continue;
        }
        if ch == quote {
            return Ok(i + 1);
        }
        i += 1;
    }
    Err(ConnectorError::Query(
        "unclosed string in Mongo query".into(),
    ))
}

fn unquote(raw: &str) -> ConnectorResult<String> {
    let t = raw.trim();
    let bytes = t.as_bytes();
    let quote = bytes.first().copied();
    if quote != Some(b'"') && quote != Some(b'\'') {
        if !t.is_empty()
            && t.chars()
                .all(|ch| ch.is_ascii_alphanumeric() || ch == '_' || ch == '.' || ch == '$')
        {
            return Ok(t.to_string());
        }
        return Err(ConnectorError::Query(format!(
            "expected a field name, got `{t}`"
        )));
    }
    let mut out = String::new();
    let mut chars = t.chars();
    let _ = chars.next();
    let mut escaped = false;
    for ch in chars {
        if escaped {
            out.push(ch);
            escaped = false;
            continue;
        }
        if ch == '\\' {
            escaped = true;
            continue;
        }
        if Some(ch as u8) == quote {
            break;
        }
        out.push(ch);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn count_distinct_and_writes_parse() {
        let count = match parse_mongo_call(r#"db.orders.countDocuments({ status: "open" })"#) {
            Ok(call) => call,
            Err(err) => panic!("{err}"),
        };
        match count {
            MongoCall::Count { collection, .. } => {
                assert_eq!(collection.as_deref(), Some("orders"));
            }
            other => panic!("count, got {other:?}"),
        }
        let distinct = match parse_mongo_call(r#"db.orders.distinct("city", { a: 1 })"#) {
            Ok(call) => call,
            Err(err) => panic!("{err}"),
        };
        match distinct {
            MongoCall::Distinct {
                field, collection, ..
            } => {
                assert_eq!(field, "city");
                assert_eq!(collection.as_deref(), Some("orders"));
            }
            other => panic!("distinct, got {other:?}"),
        }
        let many = match parse_mongo_call(r#"db.orders.insertMany([{a:1},{a:2}])"#) {
            Ok(call) => call,
            Err(err) => panic!("{err}"),
        };
        match many {
            MongoCall::Write(ShellWrite::InsertMany { documents, .. }) => {
                assert_eq!(documents.len(), 2);
            }
            other => panic!("insertMany, got {other:?}"),
        }
        let bulk = match parse_mongo_call(
            r#"db.orders.bulkWrite([{insertOne:{document:{a:1}}},{deleteMany:{filter:{a:1}}}])"#,
        ) {
            Ok(call) => call,
            Err(err) => panic!("{err}"),
        };
        match bulk {
            MongoCall::Write(ShellWrite::Bulk { models, .. }) => assert_eq!(models.len(), 2),
            other => panic!("bulk, got {other:?}"),
        }
    }

    #[test]
    fn aggregate_with_a_non_ascii_collection_parses() {
        let call = match parse_mongo_call(r#"db.Zürich.aggregate([{$match:{}}])"#) {
            Ok(call) => call,
            Err(err) => panic!("{err}"),
        };
        match call {
            MongoCall::Aggregate(agg) => assert_eq!(agg.collection.as_deref(), Some("Zürich")),
            other => panic!("aggregate, got {other:?}"),
        }
    }

    fn must_call(text: &str) -> MongoCall {
        match parse_mongo_call(text) {
            Ok(call) => call,
            Err(err) => panic!("{text}: {err}"),
        }
    }

    #[test]
    fn find_one_limits_to_one_document() {
        let call = must_call(r#"db.orders.findOne({ a: 1 })"#);
        match call {
            MongoCall::Find(parsed) => {
                assert_eq!(parsed.collection.as_deref(), Some("orders"));
                assert_eq!(parsed.limit, Some(1));
            }
            other => panic!("findOne, got {other:?}"),
        }
        let pretty = must_call(r#"db.orders.aggregate([{ $match: {} }]).pretty()"#);
        match pretty {
            MongoCall::Aggregate(agg) => assert_eq!(agg.pipeline.len(), 1),
            other => panic!("pretty aggregate, got {other:?}"),
        }
    }

    #[test]
    fn aggregate_match_and_limit_parses() {
        let call = must_call(r#"db.coll.aggregate([{ $match: {} }, { $limit: 5 }])"#);
        match call {
            MongoCall::Aggregate(agg) => {
                assert_eq!(agg.collection.as_deref(), Some("coll"));
                assert_eq!(agg.pipeline.len(), 2);
                assert_eq!(aggregate_effect(&agg.pipeline), AggregateEffect::Read);
            }
            other => panic!("aggregate, got {other:?}"),
        }
    }

    #[test]
    fn aggregate_out_is_a_write_and_drop_is_destructive() {
        let out = must_call(r#"db.coll.aggregate([{ $match: {} }, { $out: "archive" }])"#);
        match out {
            MongoCall::Aggregate(agg) => {
                assert_eq!(aggregate_effect(&agg.pipeline), AggregateEffect::Write);
            }
            other => panic!("out, got {other:?}"),
        }
        let merge = must_call(r#"db.coll.aggregate([{ $merge: { into: "t" } }])"#);
        match merge {
            MongoCall::Aggregate(agg) => {
                assert_eq!(aggregate_effect(&agg.pipeline), AggregateEffect::Write);
            }
            other => panic!("merge, got {other:?}"),
        }
        let drop_stage = must_call(r#"db.coll.aggregate([{ $match: {} }, { $drop: "t" }])"#);
        match drop_stage {
            MongoCall::Aggregate(agg) => {
                assert_eq!(
                    aggregate_effect(&agg.pipeline),
                    AggregateEffect::Destructive
                );
            }
            other => panic!("drop, got {other:?}"),
        }
        let quoted = must_call(r#"db.coll.aggregate([{ $match: { note: "$out" } }])"#);
        match quoted {
            MongoCall::Aggregate(agg) => {
                assert_eq!(aggregate_effect(&agg.pipeline), AggregateEffect::Read);
            }
            other => panic!("quoted, got {other:?}"),
        }
    }

    #[test]
    fn quoted_write_method_stays_a_find() {
        let call = must_call(r#"db.orders.find({ note: ".insertOne({a:1})" })"#);
        match call {
            MongoCall::Find(parsed) => {
                assert_eq!(parsed.collection.as_deref(), Some("orders"));
            }
            other => panic!("find, got {other:?}"),
        }
        let trailing = parse_mongo_call(r#"db.orders.find({a:1}); db.orders.insertOne({a:1})"#);
        match trailing {
            Err(err) => {
                let msg = err.to_string().to_ascii_lowercase();
                assert!(msg.contains("unexpected") || msg.contains("find"), "{err}");
            }
            Ok(call) => panic!("trailing insert must not run, got {call:?}"),
        }
    }

    #[test]
    fn update_replace_and_find_one_and_update_parse() {
        let update = must_call(r#"db.orders.updateMany({ a: 1 }, { $set: { b: 2 } })"#);
        match update {
            MongoCall::Write(ShellWrite::UpdateMany { collection, .. }) => {
                assert_eq!(collection.as_deref(), Some("orders"));
            }
            other => panic!("updateMany, got {other:?}"),
        }
        let replace = must_call(r#"db.orders.replaceOne({ a: 1 }, { a: 2 })"#);
        match replace {
            MongoCall::Write(ShellWrite::ReplaceOne { .. }) => {}
            other => panic!("replaceOne, got {other:?}"),
        }
        let found = must_call(r#"db.orders.findOneAndUpdate({ a: 1 }, { $set: { b: 2 } })"#);
        match found {
            MongoCall::Write(ShellWrite::FindOneAndUpdate { collection, .. }) => {
                assert_eq!(collection.as_deref(), Some("orders"));
            }
            other => panic!("findOneAndUpdate, got {other:?}"),
        }
        let json_count = must_call(r#"{"op":"countDocuments","filter":{"a":1}}"#);
        match json_count {
            MongoCall::Count { filter, .. } => assert!(!filter.is_empty()),
            other => panic!("json count, got {other:?}"),
        }
    }

    #[test]
    fn aggregate_pipeline_keeps_the_collection() {
        let parsed = match parse_aggregate(r#"db.orders.aggregate([{"$match":{"a":1}}])"#) {
            Ok(parsed) => parsed,
            Err(err) => panic!("{err}"),
        };
        let Some(parsed) = parsed else {
            panic!("missing aggregate");
        };
        assert_eq!(parsed.collection.as_deref(), Some("orders"));
        assert_eq!(parsed.pipeline.len(), 1);
    }

    #[test]
    fn find_filter_projection_sort() {
        let q = r#"db.mail_notifications.find({ email: "ajaybhatt17@gmail.com" },{ title: 1, content: 1, variables: 1, is_sent: 1, createdAt: 1, sent_at: 1 }).sort({ createdAt: -1 });"#;
        let p = parse_read_query(q).expect("parse");
        assert_eq!(p.collection.as_deref(), Some("mail_notifications"));
        assert_eq!(p.filter.get_str("email").unwrap(), "ajaybhatt17@gmail.com");
        let proj = p.projection.expect("projection");
        assert_eq!(proj.get_i32("title").unwrap(), 1);
        assert_eq!(proj.get_i32("content").unwrap(), 1);
        let sort = p.sort.expect("sort");
        assert_eq!(sort.get_i32("createdAt").unwrap(), -1);
    }

    #[test]
    fn bare_filter_still_works() {
        let p = parse_read_query(r#"{ "status": "ok" }"#).unwrap();
        assert_eq!(p.filter.get_str("status").unwrap(), "ok");
        assert!(p.projection.is_none());
    }

    #[test]
    fn find_empty_and_limit() {
        let p = parse_read_query("db.users.find().limit(10)").unwrap();
        assert_eq!(p.collection.as_deref(), Some("users"));
        assert!(p.filter.is_empty());
        assert_eq!(p.limit, Some(10));
    }

    #[test]
    fn non_ascii_collection_does_not_panic() {
        let p = parse_read_query(r#"db.Zürich.find({ city: "Zürich" })"#).expect("parse");
        assert_eq!(p.collection.as_deref(), Some("Zürich"));
        assert_eq!(p.filter.get_str("city").expect("city"), "Zürich");
    }
}
