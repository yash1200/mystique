//! Shell literals: objects, arrays, strings, numbers and the `ObjectId(…)`
//! style helpers, kept as written until something needs BSON.

use mongodb::bson::oid::ObjectId;
use mongodb::bson::{Bson, DateTime, Document};
use mystique_connector_api::ConnectorResult;
use serde_json::Value as JsonValue;

use super::lex::{query_err, Tok};

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Lit {
    Null,
    Bool(bool),
    /// Number as written.
    Num(String),
    Str(String),
    /// A bare name such as `printjson`. Only some call arguments allow it.
    Ident(String),
    Array(Vec<Lit>),
    /// Keys in source order. A repeated key appears twice.
    Object(Vec<(String, Lit)>),
    /// `ObjectId("…")`, `ISODate("…")`, `new Date(…)`, `NumberLong(…)`, …
    Helper {
        name: String,
        args: Vec<Lit>,
    },
}

impl Lit {
    pub(crate) fn as_object(&self) -> Option<&[(String, Lit)]> {
        match self {
            Lit::Object(entries) => Some(entries),
            _ => None,
        }
    }

    /// Last value for `key`, the same one a JSON parser keeps.
    pub(crate) fn get(&self, key: &str) -> Option<&Lit> {
        self.as_object()?
            .iter()
            .rev()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v)
    }

    /// Every object key at any depth, helpers included.
    pub(crate) fn walk_keys<'a>(&'a self, visit: &mut impl FnMut(&'a str)) {
        match self {
            Lit::Object(entries) => {
                for (key, value) in entries {
                    visit(key);
                    value.walk_keys(visit);
                }
            }
            Lit::Array(items) | Lit::Helper { args: items, .. } => {
                for item in items {
                    item.walk_keys(visit);
                }
            }
            _ => {}
        }
    }
}

/// Reads tokens from a slice. Shared with the statement parser.
pub(crate) struct Cursor<'a> {
    toks: &'a [Tok],
    pos: usize,
}

impl<'a> Cursor<'a> {
    pub(crate) fn new(toks: &'a [Tok]) -> Self {
        Self { toks, pos: 0 }
    }

    pub(crate) fn peek(&self) -> Option<&'a Tok> {
        self.toks.get(self.pos)
    }

    pub(crate) fn bump(&mut self) -> Option<&'a Tok> {
        let tok = self.peek();
        if tok.is_some() {
            self.pos += 1;
        }
        tok
    }

    pub(crate) fn at_end(&self) -> bool {
        self.pos >= self.toks.len()
    }

    pub(crate) fn eat(&mut self, punct: char) -> bool {
        if self.peek() == Some(&Tok::Punct(punct)) {
            self.pos += 1;
            true
        } else {
            false
        }
    }

    pub(crate) fn expect(&mut self, punct: char, what: &str) -> ConnectorResult<()> {
        if self.eat(punct) {
            return Ok(());
        }
        Err(query_err(&format!(
            "expected `{punct}` {what}, found {}",
            describe(self.peek())
        )))
    }
}

pub(crate) fn describe(tok: Option<&Tok>) -> String {
    match tok {
        None => "the end of the query".into(),
        Some(Tok::Punct(c)) => format!("`{c}`"),
        Some(Tok::Ident(name)) => format!("`{name}`"),
        Some(Tok::Num(raw)) => format!("`{raw}`"),
        Some(Tok::Str(s)) => format!("string {s:?}"),
    }
}

pub(crate) fn literal(cur: &mut Cursor<'_>) -> ConnectorResult<Lit> {
    match cur.bump() {
        Some(Tok::Punct('{')) => object(cur),
        Some(Tok::Punct('[')) => {
            let mut items = Vec::new();
            loop {
                if cur.eat(']') {
                    break;
                }
                items.push(literal(cur)?);
                if !cur.eat(',') {
                    cur.expect(']', "to close the array")?;
                    break;
                }
            }
            Ok(Lit::Array(items))
        }
        Some(Tok::Str(s)) => Ok(Lit::Str(s.clone())),
        Some(Tok::Num(raw)) => Ok(Lit::Num(raw.clone())),
        Some(Tok::Ident(name)) => {
            let name = name.as_str();
            match name {
                "true" => return Ok(Lit::Bool(true)),
                "false" => return Ok(Lit::Bool(false)),
                "null" => return Ok(Lit::Null),
                _ => {}
            }
            let (name, is_new) = if name == "new" {
                match cur.bump() {
                    Some(Tok::Ident(helper)) => (helper.as_str(), true),
                    other => {
                        return Err(query_err(&format!(
                            "expected a helper after `new`, found {}",
                            describe(other)
                        )))
                    }
                }
            } else {
                (name, false)
            };
            if cur.eat('(') {
                let args = call_args(cur)?;
                return Ok(Lit::Helper {
                    name: name.to_string(),
                    args,
                });
            }
            if is_new {
                return Err(query_err(&format!("expected `(` after `new {name}`")));
            }
            Ok(Lit::Ident(name.to_string()))
        }
        other => Err(query_err(&format!(
            "expected a value, found {}",
            describe(other)
        ))),
    }
}

/// Arguments after an opening `(`, through the closing `)`.
pub(crate) fn call_args(cur: &mut Cursor<'_>) -> ConnectorResult<Vec<Lit>> {
    let mut args = Vec::new();
    loop {
        if cur.eat(')') {
            break;
        }
        args.push(literal(cur)?);
        if !cur.eat(',') {
            cur.expect(')', "to close the call")?;
            break;
        }
    }
    Ok(args)
}

fn object(cur: &mut Cursor<'_>) -> ConnectorResult<Lit> {
    let mut entries = Vec::new();
    loop {
        if cur.eat('}') {
            break;
        }
        let key = match cur.bump() {
            Some(Tok::Ident(name)) => name.clone(),
            Some(Tok::Str(s)) => s.clone(),
            Some(Tok::Num(raw)) => raw.clone(),
            other => {
                return Err(query_err(&format!(
                    "expected a key, found {}",
                    describe(other)
                )))
            }
        };
        cur.expect(':', &format!("after key `{key}`"))?;
        let value = literal(cur)?;
        entries.push((key, value));
        if !cur.eat(',') {
            cur.expect('}', "to close the object")?;
            break;
        }
    }
    Ok(Lit::Object(entries))
}

/// One object literal, with nothing after it but `;`.
pub(crate) fn parse_document(text: &str) -> ConnectorResult<Document> {
    let toks = super::lex::lex(text)?;
    let mut cur = Cursor::new(&toks);
    if cur.peek() != Some(&Tok::Punct('{')) {
        return Err(query_err("filter must be an object `{ … }`"));
    }
    let lit = literal(&mut cur)?;
    while cur.eat(';') {}
    if !cur.at_end() {
        return Err(query_err(&format!(
            "unexpected {} after the object",
            describe(cur.peek())
        )));
    }
    to_document(&lit)
}

pub(crate) fn to_document(lit: &Lit) -> ConnectorResult<Document> {
    match to_bson(lit)? {
        Bson::Document(doc) => Ok(doc),
        _ => Err(query_err("expected an object `{ … }`")),
    }
}

pub(crate) fn to_bson(lit: &Lit) -> ConnectorResult<Bson> {
    match lit {
        Lit::Null => Ok(Bson::Null),
        Lit::Bool(b) => Ok(Bson::Boolean(*b)),
        Lit::Num(raw) => number(raw),
        Lit::Str(s) => Ok(Bson::String(s.clone())),
        Lit::Ident(name) => Err(query_err(&format!(
            "unknown name `{name}`; quote string values"
        ))),
        Lit::Array(items) => items
            .iter()
            .map(to_bson)
            .collect::<ConnectorResult<Vec<_>>>()
            .map(Bson::Array),
        Lit::Object(entries) => {
            let mut doc = Document::new();
            for (key, value) in entries {
                doc.insert(key.clone(), to_bson(value)?);
            }
            extended_json(doc)
        }
        Lit::Helper { name, args } => helper(name, args),
    }
}

/// JSON → BSON with the same rules as shell text (`$oid`, `$date`, `$numberLong`).
pub(crate) fn json_to_document(value: &JsonValue) -> ConnectorResult<Document> {
    to_document(&from_json(value))
}

fn from_json(value: &JsonValue) -> Lit {
    match value {
        JsonValue::Null => Lit::Null,
        JsonValue::Bool(b) => Lit::Bool(*b),
        JsonValue::Number(n) => Lit::Num(n.to_string()),
        JsonValue::String(s) => Lit::Str(s.clone()),
        JsonValue::Array(items) => Lit::Array(items.iter().map(from_json).collect()),
        JsonValue::Object(map) => {
            Lit::Object(map.iter().map(|(k, v)| (k.clone(), from_json(v))).collect())
        }
    }
}

/// Integers in i32 range are Int32, wider ones Int64, the rest Double.
fn number(raw: &str) -> ConnectorResult<Bson> {
    if let Ok(i) = raw.parse::<i64>() {
        return Ok(match i32::try_from(i) {
            Ok(small) => Bson::Int32(small),
            Err(_) => Bson::Int64(i),
        });
    }
    match raw.parse::<f64>() {
        Ok(f) if f.is_finite() => Ok(Bson::Double(f)),
        _ => Err(query_err(&format!("invalid number `{raw}`"))),
    }
}

/// A one-key `{ $oid }`, `{ $date }` or `{ $numberLong }` object is that type.
fn extended_json(doc: Document) -> ConnectorResult<Bson> {
    if doc.len() == 1 {
        if let Some(Bson::String(hex)) = doc.get("$oid") {
            let oid = ObjectId::parse_str(hex)
                .map_err(|e| query_err(&format!("invalid ObjectId: {e}")))?;
            return Ok(Bson::ObjectId(oid));
        }
        match doc.get("$date") {
            Some(Bson::String(s)) => return Ok(date_from_str(s)),
            Some(Bson::Int32(ms)) => return Ok(Bson::DateTime(DateTime::from_millis(*ms as i64))),
            Some(Bson::Int64(ms)) => return Ok(Bson::DateTime(DateTime::from_millis(*ms))),
            _ => {}
        }
        if let Some(Bson::String(n)) = doc.get("$numberLong") {
            let i: i64 = n
                .parse()
                .map_err(|e| query_err(&format!("invalid NumberLong: {e}")))?;
            return Ok(Bson::Int64(i));
        }
    }
    Ok(Bson::Document(doc))
}

/// RFC 3339 is a date. Anything else stays the string it was.
fn date_from_str(s: &str) -> Bson {
    match DateTime::parse_rfc3339_str(s) {
        Ok(dt) => Bson::DateTime(dt),
        Err(_) => Bson::String(s.to_string()),
    }
}

fn helper(name: &str, args: &[Lit]) -> ConnectorResult<Bson> {
    let one = |what: &str| -> ConnectorResult<Option<&Lit>> {
        if args.len() > 1 {
            return Err(query_err(&format!("{what}(…) takes one argument")));
        }
        Ok(args.first())
    };
    match name.to_ascii_lowercase().as_str() {
        "objectid" => match one(name)? {
            None => Ok(Bson::ObjectId(ObjectId::new())),
            Some(Lit::Str(hex)) => {
                if hex.len() != 24 || !hex.chars().all(|c| c.is_ascii_hexdigit()) {
                    return Err(query_err(&format!(
                        "invalid ObjectId (need 24 hex chars): {hex:?}"
                    )));
                }
                ObjectId::parse_str(hex)
                    .map(Bson::ObjectId)
                    .map_err(|e| query_err(&format!("invalid ObjectId: {e}")))
            }
            Some(_) => Err(query_err("ObjectId(…) takes a hex string")),
        },
        "isodate" | "date" => match one(name)? {
            None => Ok(Bson::DateTime(DateTime::now())),
            Some(Lit::Str(s)) => Ok(date_from_str(s)),
            Some(Lit::Num(raw)) => raw
                .parse::<i64>()
                .map(|ms| Bson::DateTime(DateTime::from_millis(ms)))
                .map_err(|_| query_err(&format!("invalid date `{raw}`"))),
            Some(_) => Err(query_err(&format!("{name}(…) takes a date string"))),
        },
        "numberlong" => match one(name)? {
            Some(Lit::Str(n) | Lit::Num(n)) => n
                .trim()
                .parse::<i64>()
                .map(Bson::Int64)
                .map_err(|e| query_err(&format!("invalid NumberLong: {e}"))),
            _ => Err(query_err("NumberLong(…) takes an integer")),
        },
        "numberint" => match one(name)? {
            Some(Lit::Str(n) | Lit::Num(n)) => n
                .trim()
                .parse::<i32>()
                .map(Bson::Int32)
                .map_err(|e| query_err(&format!("invalid NumberInt: {e}"))),
            _ => Err(query_err("NumberInt(…) takes an integer")),
        },
        _ => Err(query_err(&format!(
            "`{name}(…)` is not supported in a Mongo value"
        ))),
    }
}
