//! Mongo shell text → [`Statement`] → [`MongoCall`].
//!
//! [`parse`] reads the syntax: which database, which collection, which
//! method, and every argument as a literal. The policy classifies that.
//! [`lower`] turns it into the call the executor runs, and refuses what the
//! executor cannot run. Both read the same [`Statement`], so the policy
//! never judges text the executor reads differently.

use mongodb::bson::{Bson, Document};
use mystique_connector_api::{ConnectorError, ConnectorResult};

use super::lex::{lex, query_err, Tok};
use super::value::{call_args, describe, literal, to_document, Cursor, Lit};

/// `show dbs` / `listDatabases`, `show collections` / `listCollections`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Listing {
    Databases,
    Collections,
}

impl Listing {
    pub(crate) fn command(self) -> &'static str {
        match self {
            Self::Databases => "listDatabases",
            Self::Collections => "listCollections",
        }
    }

    fn of(text: &str) -> Option<Self> {
        let compact: String = text
            .chars()
            .filter(|c| !c.is_whitespace())
            .collect::<String>()
            .to_ascii_lowercase();
        match compact.trim_end_matches(';') {
            "listdatabases" | "showdatabases" | "showdbs" => Some(Self::Databases),
            "listcollections" | "showcollections" => Some(Self::Collections),
            _ => None,
        }
    }
}

/// What the text says, before anything checks the executor can run it.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Statement {
    Listing(Listing),
    /// A bare `{ … }` with no `op` key: a find filter.
    Filter(Lit),
    /// A bare `[ … ]`: an aggregate pipeline.
    Pipeline(Vec<Lit>),
    /// A `{ op: "…", … }` command object.
    Command(Lit),
    Call(ShellCall),
}

/// `db[.getSiblingDB(d)].coll.method(args).chain(args)…`
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ShellCall {
    pub database: Option<String>,
    /// `None` for a database method such as `db.dropDatabase()`.
    pub collection: Option<String>,
    /// As typed.
    pub method: String,
    pub args: Vec<Lit>,
    pub chain: Vec<Chained>,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Chained {
    pub method: String,
    pub args: Vec<Lit>,
}

impl ShellCall {
    pub(crate) fn method_lower(&self) -> String {
        self.method.to_ascii_lowercase()
    }

    /// Every literal in the call, chain included.
    pub(crate) fn literals(&self) -> impl Iterator<Item = &Lit> {
        self.args
            .iter()
            .chain(self.chain.iter().flat_map(|c| c.args.iter()))
    }
}

impl Statement {
    /// Every `op` value at the top of a command object. More than one is an
    /// error at [`lower`]; the policy takes the worst.
    pub(crate) fn command_ops(&self) -> Vec<&Lit> {
        match self {
            Statement::Command(obj) => obj
                .as_object()
                .unwrap_or(&[])
                .iter()
                .filter(|(k, _)| k.eq_ignore_ascii_case("op"))
                .map(|(_, v)| v)
                .collect(),
            _ => Vec::new(),
        }
    }

    /// Every literal in the statement.
    pub(crate) fn literals(&self) -> Vec<&Lit> {
        match self {
            Statement::Listing(_) => Vec::new(),
            Statement::Filter(lit) | Statement::Command(lit) => vec![lit],
            Statement::Pipeline(items) => items.iter().collect(),
            Statement::Call(call) => call.literals().collect(),
        }
    }
}

pub(crate) fn parse(text: &str) -> ConnectorResult<Statement> {
    let text = text.trim();
    if text.is_empty() {
        return Err(query_err("Query is empty."));
    }
    if let Some(listing) = Listing::of(text) {
        return Ok(Statement::Listing(listing));
    }
    let mut toks = lex(text)?;
    while toks.last() == Some(&Tok::Punct(';')) {
        toks.pop();
    }
    if toks.is_empty() {
        return Err(query_err("Query is empty."));
    }
    let mut cur = Cursor::new(&toks);
    let stmt = match cur.peek() {
        Some(Tok::Punct('{')) => {
            let obj = literal(&mut cur)?;
            let has_op = obj
                .as_object()
                .is_some_and(|e| e.iter().any(|(k, _)| k.eq_ignore_ascii_case("op")));
            if has_op {
                Statement::Command(obj)
            } else {
                Statement::Filter(obj)
            }
        }
        Some(Tok::Punct('[')) => match literal(&mut cur)? {
            Lit::Array(items) => Statement::Pipeline(items),
            _ => return Err(query_err("expected a pipeline array")),
        },
        Some(Tok::Ident(_)) => Statement::Call(shell_call(&mut cur)?),
        other => {
            return Err(query_err(&format!(
                "Mongo query must be a JSON filter `{{ … }}`, a pipeline `[ … ]`, or `db.collection.method(…)`; found {}",
                describe(other)
            )))
        }
    };
    if cur.eat(';') {
        return Err(query_err("Run one Mongo statement at a time."));
    }
    if !cur.at_end() {
        return Err(query_err(&format!(
            "unexpected {} after the Mongo call",
            describe(cur.peek())
        )));
    }
    Ok(stmt)
}

/// Where the call is aimed before the method.
enum Scope {
    Db(Option<String>),
    Mongo,
    Coll(Option<String>, String),
}

fn shell_call(cur: &mut Cursor<'_>) -> ConnectorResult<ShellCall> {
    match cur.bump() {
        Some(Tok::Ident(root)) if root == "db" => {}
        other => {
            return Err(query_err(&format!(
                "Mongo shell calls start with `db.`; found {}",
                describe(other)
            )))
        }
    }
    let mut scope = Scope::Db(None);
    let (database, collection, method, args) = loop {
        let (name, called) = member(cur)?;
        let args = if called { Some(call_args(cur)?) } else { None };
        scope = match (scope, args) {
            (Scope::Db(_), Some(args)) if is_name(&name, &["getSiblingDB", "getDB"]) => {
                Scope::Db(Some(string_arg(&name, &args)?))
            }
            (Scope::Db(db), Some(args)) if name == "getCollection" => {
                Scope::Coll(db, string_arg(&name, &args)?)
            }
            (Scope::Db(_), Some(args)) if name == "getMongo" && args.is_empty() => Scope::Mongo,
            (Scope::Mongo, Some(args)) if name == "getDB" => {
                Scope::Db(Some(string_arg(&name, &args)?))
            }
            (Scope::Mongo, _) => {
                return Err(query_err(&format!(
                    "expected `.getDB(…)` after `db.getMongo()`, found `{name}`"
                )))
            }
            (Scope::Db(db), Some(args)) => break (db, None, name, args),
            (Scope::Db(db), None) => Scope::Coll(db, name),
            (Scope::Coll(db, coll), Some(args)) => break (db, Some(coll), name, args),
            (Scope::Coll(db, coll), None) => Scope::Coll(db, format!("{coll}.{name}")),
        };
    };
    let mut chain = Vec::new();
    while !cur.at_end() && cur.peek() != Some(&Tok::Punct(';')) {
        let (name, called) = member(cur)?;
        if !called {
            return Err(query_err(&format!("expected `(` after `.{name}`")));
        }
        chain.push(Chained {
            method: name,
            args: call_args(cur)?,
        });
    }
    Ok(ShellCall {
        database,
        collection,
        method,
        args,
        chain,
    })
}

/// `.name` or `["name"]`, and whether a `(` follows (consumed).
fn member(cur: &mut Cursor<'_>) -> ConnectorResult<(String, bool)> {
    let name = match cur.bump() {
        Some(Tok::Punct('.')) => match cur.bump() {
            Some(Tok::Ident(name)) => name.clone(),
            other => {
                return Err(query_err(&format!(
                    "expected a name after `.`, found {}",
                    describe(other)
                )))
            }
        },
        Some(Tok::Punct('[')) => {
            let name = match cur.bump() {
                Some(Tok::Str(name)) => name.clone(),
                other => {
                    return Err(query_err(&format!(
                        "expected a quoted collection name in `[…]`, found {}",
                        describe(other)
                    )))
                }
            };
            cur.expect(']', "after the collection name")?;
            name
        }
        other => {
            return Err(query_err(&format!(
                "expected `.method(…)`, found {}",
                describe(other)
            )))
        }
    };
    if cur.peek().is_none() {
        return Err(query_err(&format!(
            "expected a method call after `{name}`, e.g. db.collection.find()"
        )));
    }
    Ok((name, cur.eat('(')))
}

fn is_name(name: &str, any: &[&str]) -> bool {
    any.iter().any(|want| want.eq_ignore_ascii_case(name))
}

fn string_arg(method: &str, args: &[Lit]) -> ConnectorResult<String> {
    match args {
        [Lit::Str(s)] if !s.trim().is_empty() => Ok(s.clone()),
        _ => Err(query_err(&format!("{method}(…) takes one name string"))),
    }
}

// ---------------------------------------------------------------------------
// Lowering: the executable call.
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct Target {
    /// From `db.getSiblingDB("…")`. `None` uses the session database.
    pub database: Option<String>,
    /// `None` uses the Collection field.
    pub collection: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct MongoCall {
    pub target: Target,
    pub op: MongoOp,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum MongoOp {
    Find(FindSpec),
    Aggregate(Vec<Document>),
    Count(Document),
    Distinct { field: String, filter: Document },
    Write(ShellWrite),
}

#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct FindSpec {
    pub filter: Document,
    pub projection: Option<Document>,
    pub sort: Option<Document>,
    pub limit: Option<i64>,
    pub skip: Option<u64>,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum ShellWrite {
    InsertOne(Document),
    InsertMany(Vec<Document>),
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
    FindOneAndUpdate {
        filter: Document,
        update: Document,
    },
    Bulk(Vec<BulkModel>),
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum BulkModel {
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

/// What the executor runs.
#[derive(Debug, Clone, PartialEq)]
#[allow(clippy::large_enum_variant)] // built once per query, moved straight into dispatch
pub(crate) enum Executable {
    Listing(Listing),
    Call(MongoCall),
}

pub(crate) fn parse_executable(text: &str) -> ConnectorResult<Executable> {
    lower(&parse(text)?)
}

pub(crate) fn lower(stmt: &Statement) -> ConnectorResult<Executable> {
    let call = match stmt {
        Statement::Listing(listing) => return Ok(Executable::Listing(*listing)),
        Statement::Filter(obj) => MongoCall {
            target: Target::default(),
            op: MongoOp::Find(FindSpec {
                filter: to_document(obj)?,
                ..FindSpec::default()
            }),
        },
        Statement::Pipeline(items) => MongoCall {
            target: Target::default(),
            op: MongoOp::Aggregate(documents(items)?),
        },
        Statement::Command(obj) => MongoCall {
            target: Target::default(),
            op: command_op(stmt, obj)?,
        },
        Statement::Call(call) => lower_call(call)?,
    };
    Ok(Executable::Call(call))
}

fn lower_call(call: &ShellCall) -> ConnectorResult<MongoCall> {
    let method = call.method_lower();
    if call.collection.is_none() {
        return Err(ConnectorError::NotSupported(format!(
            "`db.{}()` is not supported here. Run it from mongosh.",
            call.method
        )));
    }
    let target = Target {
        database: call.database.clone(),
        collection: call.collection.clone(),
    };
    let args = &call.args;
    let op = match method.as_str() {
        "find" | "findone" => {
            let mut spec = FindSpec {
                filter: optional_doc(args.first())?,
                projection: args.get(1).map(to_document).transpose()?,
                ..FindSpec::default()
            };
            for link in &call.chain {
                find_chain(&mut spec, link)?;
            }
            if method == "findone" {
                spec.limit = Some(1);
            }
            return Ok(MongoCall {
                target,
                op: MongoOp::Find(spec),
            });
        }
        "aggregate" => match args.first() {
            Some(Lit::Array(items)) => MongoOp::Aggregate(documents(items)?),
            _ => return Err(query_err("aggregate() takes a pipeline array `[ ... ]`.")),
        },
        "count" | "countdocuments" => MongoOp::Count(optional_doc(args.first())?),
        "distinct" => {
            let field = match args.first() {
                Some(Lit::Str(s) | Lit::Ident(s)) if !s.is_empty() => s.clone(),
                _ => return Err(query_err("distinct needs a field name.")),
            };
            MongoOp::Distinct {
                field,
                filter: optional_doc(args.get(1))?,
            }
        }
        "insertone" => MongoOp::Write(ShellWrite::InsertOne(required_doc(args.first())?)),
        "insertmany" => MongoOp::Write(ShellWrite::InsertMany(doc_array(args.first())?)),
        "updateone" => {
            let (filter, update) = two_docs(args, "updateOne")?;
            MongoOp::Write(ShellWrite::UpdateOne { filter, update })
        }
        "updatemany" => {
            let (filter, update) = two_docs(args, "updateMany")?;
            MongoOp::Write(ShellWrite::UpdateMany { filter, update })
        }
        "replaceone" => {
            let (filter, replacement) = two_docs(args, "replaceOne")?;
            MongoOp::Write(ShellWrite::ReplaceOne {
                filter,
                replacement,
            })
        }
        "deleteone" => MongoOp::Write(ShellWrite::DeleteOne(required_doc(args.first())?)),
        "deletemany" => MongoOp::Write(ShellWrite::DeleteMany(required_doc(args.first())?)),
        "findoneandupdate" => {
            let (filter, update) = two_docs(args, "findOneAndUpdate")?;
            MongoOp::Write(ShellWrite::FindOneAndUpdate { filter, update })
        }
        "bulkwrite" => MongoOp::Write(ShellWrite::Bulk(bulk_models(&doc_array(args.first())?)?)),
        _ => {
            return Err(query_err(&format!(
                "Mongo method `.{}()` is not supported.",
                call.method
            )))
        }
    };
    for link in &call.chain {
        if !is_name(&link.method, &["pretty", "toArray"]) {
            return Err(query_err(&format!(
                "unexpected `.{}()` after .{}(…)",
                link.method, call.method
            )));
        }
    }
    Ok(MongoCall { target, op })
}

fn find_chain(spec: &mut FindSpec, link: &Chained) -> ConnectorResult<()> {
    let first = link.args.first();
    match link.method.to_ascii_lowercase().as_str() {
        "sort" => spec.sort = Some(required_doc(first)?),
        "project" | "projection" => spec.projection = Some(required_doc(first)?),
        "limit" => spec.limit = Some(int_arg(first, "limit")?),
        "skip" => spec.skip = Some(int_arg(first, "skip")?.max(0) as u64),
        "pretty" | "toarray" | "explain" => {}
        _ => {
            return Err(query_err(&format!(
                "unsupported chain `.{}()` — use .sort / .limit / .skip / .project",
                link.method
            )))
        }
    }
    Ok(())
}

fn int_arg(arg: Option<&Lit>, name: &str) -> ConnectorResult<i64> {
    match arg {
        Some(Lit::Num(raw)) => raw
            .parse::<i64>()
            .map_err(|_| query_err(&format!(".{name}() needs an integer, got `{raw}`"))),
        _ => Err(query_err(&format!(".{name}() needs an integer"))),
    }
}

fn optional_doc(arg: Option<&Lit>) -> ConnectorResult<Document> {
    match arg {
        None => Ok(Document::new()),
        Some(lit) => to_document(lit),
    }
}

fn required_doc(arg: Option<&Lit>) -> ConnectorResult<Document> {
    match arg {
        None => Err(query_err("expected an object argument.")),
        Some(lit) => to_document(lit),
    }
}

fn two_docs(args: &[Lit], name: &str) -> ConnectorResult<(Document, Document)> {
    match (args.first(), args.get(1)) {
        (Some(first), Some(second)) => Ok((to_document(first)?, to_document(second)?)),
        _ => Err(query_err(&format!("{name} needs a filter and a document."))),
    }
}

fn doc_array(arg: Option<&Lit>) -> ConnectorResult<Vec<Document>> {
    match arg {
        Some(Lit::Array(items)) => documents(items),
        _ => Err(query_err("expected an array of documents.")),
    }
}

fn documents(items: &[Lit]) -> ConnectorResult<Vec<Document>> {
    items.iter().map(to_document).collect()
}

/// `{ op: "…", … }`. Field names follow the JSON command shape.
fn command_op(stmt: &Statement, obj: &Lit) -> ConnectorResult<MongoOp> {
    let ops = stmt.command_ops();
    let op = match ops.as_slice() {
        [Lit::Str(op)] if obj.get("op").is_some() => op.to_ascii_lowercase(),
        [_] if obj.get("op").is_none() => {
            return Err(query_err("Mongo command key must be `op` (lowercase)."))
        }
        [Lit::Str(_)] | [_] => return Err(query_err("Mongo command `op` must be a string.")),
        [] => return Err(query_err("Mongo command is missing `op`.")),
        _ => return Err(query_err("Mongo command has more than one `op`.")),
    };
    if op.is_empty() {
        return Err(query_err("Mongo command is missing `op`."));
    }
    let field = |key: &str| obj.get(key);
    let required = |key: &str| -> ConnectorResult<Document> {
        let lit =
            field(key).ok_or_else(|| query_err(&format!("Mongo command requires `{key}`.")))?;
        to_document(lit)
    };
    let array = |key: &str| -> ConnectorResult<Vec<Document>> {
        match field(key) {
            Some(Lit::Array(items)) => documents(items),
            Some(_) => Err(query_err("expected a JSON array of documents.")),
            None => Err(query_err(&format!("{op} requires `{key}`."))),
        }
    };
    Ok(match op.as_str() {
        "aggregate" => MongoOp::Aggregate(match field("pipeline") {
            Some(_) => array("pipeline")?,
            None => Vec::new(),
        }),
        "count" | "countdocuments" => MongoOp::Count(optional_doc(field("filter"))?),
        "distinct" => {
            let field_name = match field("field") {
                Some(Lit::Str(s)) if !s.is_empty() => s.clone(),
                _ => return Err(query_err("distinct requires `field`.")),
            };
            MongoOp::Distinct {
                field: field_name,
                filter: optional_doc(field("filter"))?,
            }
        }
        "insertone" => MongoOp::Write(ShellWrite::InsertOne(required("doc")?)),
        "insertmany" => MongoOp::Write(ShellWrite::InsertMany(array("docs")?)),
        "updateone" => MongoOp::Write(ShellWrite::UpdateOne {
            filter: required("filter")?,
            update: required("update")?,
        }),
        "updatemany" => MongoOp::Write(ShellWrite::UpdateMany {
            filter: required("filter")?,
            update: required("update")?,
        }),
        "replaceone" => MongoOp::Write(ShellWrite::ReplaceOne {
            filter: required("filter")?,
            replacement: required("replacement").or_else(|_| required("doc"))?,
        }),
        "deleteone" => MongoOp::Write(ShellWrite::DeleteOne(required("filter")?)),
        "deletemany" | "remove" => MongoOp::Write(ShellWrite::DeleteMany(required("filter")?)),
        "findoneandupdate" => MongoOp::Write(ShellWrite::FindOneAndUpdate {
            filter: required("filter")?,
            update: required("update")?,
        }),
        "bulkwrite" => MongoOp::Write(ShellWrite::Bulk(bulk_models(&array("ops")?)?)),
        other => {
            return Err(ConnectorError::NotSupported(format!(
                "Mongo op `{other}` is not implemented."
            )))
        }
    })
}

fn bulk_models(docs: &[Document]) -> ConnectorResult<Vec<BulkModel>> {
    let models = docs
        .iter()
        .map(bulk_model)
        .collect::<ConnectorResult<Vec<_>>>()?;
    if models.is_empty() {
        return Err(query_err("bulkWrite needs at least one operation."));
    }
    Ok(models)
}

fn bulk_model(doc: &Document) -> ConnectorResult<BulkModel> {
    let mut keys = doc.keys();
    let Some(key) = keys.next() else {
        return Err(query_err("bulkWrite operation is empty."));
    };
    if keys.next().is_some() {
        return Err(query_err("each bulkWrite operation needs one key."));
    }
    let Some(body) = doc.get(key).and_then(Bson::as_document) else {
        return Err(query_err(&format!("bulkWrite `{key}` needs an object.")));
    };
    let field = |name: &str| -> ConnectorResult<Document> {
        body.get(name)
            .and_then(Bson::as_document)
            .cloned()
            .ok_or_else(|| query_err(&format!("bulkWrite needs `{name}`.")))
    };
    match key.to_ascii_lowercase().as_str() {
        "insertone" => Ok(BulkModel::InsertOne(field("document")?)),
        "updateone" => Ok(BulkModel::UpdateOne {
            filter: field("filter")?,
            update: field("update")?,
        }),
        "updatemany" => Ok(BulkModel::UpdateMany {
            filter: field("filter")?,
            update: field("update")?,
        }),
        "replaceone" => Ok(BulkModel::ReplaceOne {
            filter: field("filter")?,
            replacement: field("replacement")?,
        }),
        "deleteone" => Ok(BulkModel::DeleteOne(field("filter")?)),
        "deletemany" => Ok(BulkModel::DeleteMany(field("filter")?)),
        other => Err(query_err(&format!(
            "bulkWrite op `{other}` is not supported."
        ))),
    }
}
