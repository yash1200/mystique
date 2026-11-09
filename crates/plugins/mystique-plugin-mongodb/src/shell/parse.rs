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
use super::options::{self, Options};
use super::value::{call_args, describe, literal, to_bson, to_document, Cursor, Lit};

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
#[allow(clippy::large_enum_variant)] // built once per query, moved straight into dispatch
pub(crate) enum MongoOp {
    Find(FindSpec),
    Aggregate(AggregateSpec),
    Count(CountSpec),
    EstimatedCount(ReadOptions),
    Distinct {
        field: String,
        filter: Document,
        read: ReadOptions,
    },
    Write(ShellWrite),
}

/// Options any read takes.
#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct ReadOptions {
    /// An index name (`Bson::String`) or key pattern (`Bson::Document`).
    pub hint: Option<Bson>,
    pub collation: Option<Document>,
    /// The `maxTimeMS` the user typed. The profile timeout still caps it.
    pub max_time_ms: Option<u64>,
    pub comment: Option<Bson>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct FindSpec {
    pub filter: Document,
    pub projection: Option<Document>,
    pub sort: Option<Document>,
    /// An explicit limit. `None` reads up to the row cap.
    pub limit: Option<i64>,
    pub skip: Option<u64>,
    pub batch_size: Option<u32>,
    pub allow_disk_use: Option<bool>,
    pub read: ReadOptions,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct AggregateSpec {
    pub pipeline: Vec<Document>,
    pub allow_disk_use: Option<bool>,
    pub batch_size: Option<u32>,
    pub bypass_document_validation: Option<bool>,
    pub let_vars: Option<Document>,
    pub read: ReadOptions,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct CountSpec {
    pub filter: Document,
    pub limit: Option<u64>,
    pub skip: Option<u64>,
    pub read: ReadOptions,
}

/// The writes that run as `insert` / `update` / `delete` commands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum WriteMethod {
    InsertOne,
    InsertMany,
    UpdateOne,
    UpdateMany,
    ReplaceOne,
    DeleteOne,
    DeleteMany,
    BulkWrite,
}

impl WriteMethod {
    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::InsertOne => "insertOne",
            Self::InsertMany => "insertMany",
            Self::UpdateOne => "updateOne",
            Self::UpdateMany => "updateMany",
            Self::ReplaceOne => "replaceOne",
            Self::DeleteOne => "deleteOne",
            Self::DeleteMany => "deleteMany",
            Self::BulkWrite => "bulkWrite",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
#[allow(clippy::large_enum_variant)] // built once per query, moved straight into dispatch
pub(crate) enum ShellWrite {
    /// insertOne … deleteMany are one model; insertMany and bulkWrite many.
    Models {
        method: WriteMethod,
        models: Vec<WriteModel>,
        options: WriteOptions,
    },
    FindAndModify(FindAndModify),
}

/// Options on the write command as a whole.
#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct WriteOptions {
    /// `None` is ordered, as on the server.
    pub ordered: Option<bool>,
    pub bypass_document_validation: Option<bool>,
    pub comment: Option<Bson>,
    pub let_vars: Option<Document>,
    pub write_concern: Option<Document>,
    /// The `maxTimeMS` the user typed. The profile timeout still caps it.
    pub max_time_ms: Option<u64>,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum WriteModel {
    InsertOne(Document),
    UpdateOne(UpdateModel),
    UpdateMany(UpdateModel),
    ReplaceOne(UpdateModel),
    DeleteOne(DeleteModel),
    DeleteMany(DeleteModel),
}

/// One update statement. For `replaceOne`, `update` is the replacement.
#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct UpdateModel {
    pub filter: Document,
    pub update: Document,
    pub upsert: Option<bool>,
    pub array_filters: Option<Vec<Document>>,
    pub collation: Option<Document>,
    pub hint: Option<Bson>,
    pub sort: Option<Document>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct DeleteModel {
    pub filter: Document,
    pub collation: Option<Document>,
    pub hint: Option<Bson>,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Modify {
    Update(Document),
    Replace(Document),
    Delete,
}

/// `findOneAndUpdate` / `findOneAndReplace` / `findOneAndDelete`.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct FindAndModify {
    pub filter: Document,
    pub modify: Modify,
    pub upsert: Option<bool>,
    /// `Some(true)` returns the document after the change.
    pub return_after: Option<bool>,
    pub projection: Option<Document>,
    pub sort: Option<Document>,
    pub array_filters: Option<Vec<Document>>,
    pub collation: Option<Document>,
    pub hint: Option<Bson>,
    /// `ordered` is never set here.
    pub options: WriteOptions,
}

impl FindAndModify {
    pub(crate) fn method(&self) -> &'static str {
        match self.modify {
            Modify::Update(_) => "findOneAndUpdate",
            Modify::Replace(_) => "findOneAndReplace",
            Modify::Delete => "findOneAndDelete",
        }
    }
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
            op: MongoOp::Aggregate(AggregateSpec {
                pipeline: documents(items)?,
                ..AggregateSpec::default()
            }),
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
    let name = call.method.as_str();
    // The options argument at `at`, read against `allowed`. An argument after
    // it is refused.
    let options = |at: usize, allowed: &[&str]| -> ConnectorResult<Options> {
        if args.len() > at + 1 {
            return Err(query_err(&format!(
                "{name}() takes at most {} arguments.",
                at + 1
            )));
        }
        Options::from_lit(name, args.get(at), allowed)
    };
    let op = match method.as_str() {
        "find" | "findone" => {
            let opts = options(2, options::FIND)?;
            let mut spec = find_spec(
                optional_doc(args.first())?,
                projection_arg(args.get(1))?,
                &opts,
            )?;
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
            Some(Lit::Array(items)) => {
                aggregate_op(documents(items)?, &options(1, options::AGGREGATE)?)?
            }
            _ => return Err(query_err("aggregate() takes a pipeline array `[ ... ]`.")),
        },
        "count" | "countdocuments" => {
            count_op(optional_doc(args.first())?, &options(1, options::COUNT)?)?
        }
        "estimateddocumentcount" => {
            MongoOp::EstimatedCount(read_options(&options(0, options::ESTIMATED_COUNT)?)?)
        }
        "distinct" => {
            let field = match args.first() {
                Some(Lit::Str(s) | Lit::Ident(s)) if !s.is_empty() => s.clone(),
                _ => return Err(query_err("distinct needs a field name.")),
            };
            distinct_op(
                field,
                optional_doc(args.get(1))?,
                &options(2, options::DISTINCT)?,
            )?
        }
        "insertone" => insert_op(
            WriteMethod::InsertOne,
            vec![required_doc(args.first())?],
            &options(1, options::INSERT_ONE)?,
        )?,
        "insertmany" => insert_op(
            WriteMethod::InsertMany,
            doc_array(args.first())?,
            &options(1, options::INSERT_MANY)?,
        )?,
        "updateone" => {
            let (filter, update) = two_docs(args, "updateOne")?;
            update_op(
                WriteMethod::UpdateOne,
                filter,
                update,
                &options(2, options::UPDATE_ONE)?,
            )?
        }
        "updatemany" => {
            let (filter, update) = two_docs(args, "updateMany")?;
            update_op(
                WriteMethod::UpdateMany,
                filter,
                update,
                &options(2, options::UPDATE_MANY)?,
            )?
        }
        "replaceone" => {
            let (filter, replacement) = two_docs(args, "replaceOne")?;
            update_op(
                WriteMethod::ReplaceOne,
                filter,
                replacement,
                &options(2, options::REPLACE_ONE)?,
            )?
        }
        "deleteone" => delete_op(
            WriteMethod::DeleteOne,
            required_doc(args.first())?,
            &options(1, options::DELETE)?,
        )?,
        "deletemany" => delete_op(
            WriteMethod::DeleteMany,
            required_doc(args.first())?,
            &options(1, options::DELETE)?,
        )?,
        "findoneandupdate" => {
            let (filter, update) = two_docs(args, "findOneAndUpdate")?;
            find_and_modify(
                filter,
                Modify::Update(update),
                &options(2, options::FIND_ONE_AND_UPDATE)?,
            )?
        }
        "findoneandreplace" => {
            let (filter, replacement) = two_docs(args, "findOneAndReplace")?;
            find_and_modify(
                filter,
                Modify::Replace(replacement),
                &options(2, options::FIND_ONE_AND_REPLACE)?,
            )?
        }
        "findoneanddelete" => find_and_modify(
            required_doc(args.first())?,
            Modify::Delete,
            &options(1, options::FIND_ONE_AND_DELETE)?,
        )?,
        "bulkwrite" => bulk_op(&doc_array(args.first())?, &options(1, options::BULK_WRITE)?)?,
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

fn read_options(opts: &Options) -> ConnectorResult<ReadOptions> {
    Ok(ReadOptions {
        hint: opts.hint()?,
        collation: opts.collation()?,
        max_time_ms: opts.max_time_ms()?,
        comment: opts.any("comment"),
    })
}

fn write_options(opts: &Options) -> ConnectorResult<WriteOptions> {
    Ok(WriteOptions {
        ordered: opts.bool("ordered")?,
        bypass_document_validation: opts.bool("bypassDocumentValidation")?,
        comment: opts.any("comment"),
        let_vars: opts.doc("let")?,
        write_concern: opts.write_concern()?,
        max_time_ms: opts.max_time_ms()?,
    })
}

/// `find(filter, projection, options)`. The projection may come from either
/// place, not both.
fn find_spec(
    filter: Document,
    projection: Option<Document>,
    opts: &Options,
) -> ConnectorResult<FindSpec> {
    let in_options = opts.doc("projection")?;
    if projection.is_some() && in_options.is_some() {
        return Err(query_err(
            "find() takes the projection as the second argument or as option `projection`, not both.",
        ));
    }
    Ok(FindSpec {
        filter,
        projection: projection.or(in_options),
        sort: opts.doc("sort")?,
        // `limit: 0` is no limit, as on the server.
        limit: opts.int("limit")?.filter(|n| *n != 0),
        skip: opts.uint("skip")?,
        batch_size: opts.u32("batchSize")?,
        allow_disk_use: opts.bool("allowDiskUse")?,
        read: read_options(opts)?,
    })
}

fn aggregate_op(pipeline: Vec<Document>, opts: &Options) -> ConnectorResult<MongoOp> {
    Ok(MongoOp::Aggregate(AggregateSpec {
        pipeline,
        allow_disk_use: opts.bool("allowDiskUse")?,
        batch_size: opts.u32("batchSize")?,
        bypass_document_validation: opts.bool("bypassDocumentValidation")?,
        let_vars: opts.doc("let")?,
        read: read_options(opts)?,
    }))
}

fn count_op(filter: Document, opts: &Options) -> ConnectorResult<MongoOp> {
    Ok(MongoOp::Count(CountSpec {
        filter,
        limit: opts.uint("limit")?.filter(|n| *n != 0),
        skip: opts.uint("skip")?,
        read: read_options(opts)?,
    }))
}

fn distinct_op(field: String, filter: Document, opts: &Options) -> ConnectorResult<MongoOp> {
    Ok(MongoOp::Distinct {
        field,
        filter,
        read: read_options(opts)?,
    })
}

fn insert_op(method: WriteMethod, docs: Vec<Document>, opts: &Options) -> ConnectorResult<MongoOp> {
    if docs.is_empty() {
        return Err(query_err(&format!(
            "{}() needs at least one document.",
            method.name()
        )));
    }
    Ok(MongoOp::Write(ShellWrite::Models {
        method,
        models: docs.into_iter().map(WriteModel::InsertOne).collect(),
        options: write_options(opts)?,
    }))
}

fn update_model(
    filter: Document,
    update: Document,
    opts: &Options,
) -> ConnectorResult<UpdateModel> {
    Ok(UpdateModel {
        filter,
        update,
        upsert: opts.bool("upsert")?,
        array_filters: opts.docs("arrayFilters")?,
        collation: opts.collation()?,
        hint: opts.hint()?,
        sort: opts.doc("sort")?,
    })
}

fn delete_model(filter: Document, opts: &Options) -> ConnectorResult<DeleteModel> {
    Ok(DeleteModel {
        filter,
        collation: opts.collation()?,
        hint: opts.hint()?,
    })
}

fn update_op(
    method: WriteMethod,
    filter: Document,
    update: Document,
    opts: &Options,
) -> ConnectorResult<MongoOp> {
    let model = update_model(filter, update, opts)?;
    let model = match method {
        WriteMethod::UpdateMany => WriteModel::UpdateMany(model),
        WriteMethod::ReplaceOne => WriteModel::ReplaceOne(model),
        _ => WriteModel::UpdateOne(model),
    };
    Ok(MongoOp::Write(ShellWrite::Models {
        method,
        models: vec![model],
        options: write_options(opts)?,
    }))
}

fn delete_op(method: WriteMethod, filter: Document, opts: &Options) -> ConnectorResult<MongoOp> {
    let model = delete_model(filter, opts)?;
    let model = match method {
        WriteMethod::DeleteMany => WriteModel::DeleteMany(model),
        _ => WriteModel::DeleteOne(model),
    };
    Ok(MongoOp::Write(ShellWrite::Models {
        method,
        models: vec![model],
        options: write_options(opts)?,
    }))
}

fn find_and_modify(filter: Document, modify: Modify, opts: &Options) -> ConnectorResult<MongoOp> {
    Ok(MongoOp::Write(ShellWrite::FindAndModify(FindAndModify {
        filter,
        modify,
        upsert: opts.bool("upsert")?,
        return_after: opts.return_after()?,
        projection: opts.doc("projection")?,
        sort: opts.doc("sort")?,
        array_filters: opts.docs("arrayFilters")?,
        collation: opts.collation()?,
        hint: opts.hint()?,
        options: write_options(opts)?,
    })))
}

fn bulk_op(docs: &[Document], opts: &Options) -> ConnectorResult<MongoOp> {
    Ok(MongoOp::Write(ShellWrite::Models {
        method: WriteMethod::BulkWrite,
        models: bulk_models(docs)?,
        options: write_options(opts)?,
    }))
}

fn projection_arg(arg: Option<&Lit>) -> ConnectorResult<Option<Document>> {
    match arg {
        None | Some(Lit::Null) => Ok(None),
        Some(lit) => to_document(lit).map(Some),
    }
}

fn find_chain(spec: &mut FindSpec, link: &Chained) -> ConnectorResult<()> {
    let first = link.args.first();
    match link.method.to_ascii_lowercase().as_str() {
        "sort" => spec.sort = Some(required_doc(first)?),
        "project" | "projection" => spec.projection = Some(required_doc(first)?),
        // `.limit(0)` is no limit, as on the server.
        "limit" => spec.limit = Some(int_arg(first, "limit")?).filter(|n| *n != 0),
        "skip" => spec.skip = Some(int_arg(first, "skip")?.max(0) as u64),
        "hint" => {
            spec.read.hint = Some(match first {
                Some(Lit::Str(name)) => Bson::String(name.clone()),
                Some(lit @ Lit::Object(_)) => Bson::Document(to_document(lit)?),
                _ => return Err(query_err(".hint() needs an index name or a key pattern.")),
            })
        }
        "collation" => {
            let collation = required_doc(first)?;
            options::check_collation(".collation()", &collation)?;
            spec.read.collation = Some(collation);
        }
        "maxtimems" => {
            let ms = int_arg(first, "maxTimeMS")?;
            spec.read.max_time_ms = u64::try_from(ms).ok().filter(|ms| *ms > 0);
        }
        "batchsize" => {
            let size = int_arg(first, "batchSize")?;
            spec.batch_size = Some(
                u32::try_from(size)
                    .map_err(|_| query_err(".batchSize() needs a positive integer"))?,
            );
        }
        "allowdiskuse" => {
            spec.allow_disk_use = Some(match first {
                None => true,
                Some(Lit::Bool(b)) => *b,
                _ => return Err(query_err(".allowDiskUse() takes true or false")),
            })
        }
        "comment" => {
            spec.read.comment = match first {
                Some(lit) => Some(to_bson(lit)?),
                None => return Err(query_err(".comment() needs a value")),
            }
        }
        "pretty" | "toarray" | "explain" => {}
        _ => {
            return Err(query_err(&format!(
                "unsupported chain `.{}()` — use .sort / .limit / .skip / .project / .hint / .collation / .maxTimeMS / .batchSize / .allowDiskUse / .comment",
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

/// The shell spelling of a command `op`, for messages.
fn command_name(op: &str) -> &str {
    const NAMES: &[&str] = &[
        "aggregate",
        "count",
        "countDocuments",
        "estimatedDocumentCount",
        "distinct",
        "insertOne",
        "insertMany",
        "updateOne",
        "updateMany",
        "replaceOne",
        "deleteOne",
        "deleteMany",
        "remove",
        "findOneAndUpdate",
        "findOneAndReplace",
        "findOneAndDelete",
        "bulkWrite",
    ];
    NAMES
        .iter()
        .find(|name| name.eq_ignore_ascii_case(op))
        .copied()
        .unwrap_or(op)
}

/// `{ op: "…", … }`. Field names follow the JSON command shape. Options go
/// in `options: { … }`, with the keys the shell method takes.
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
    let name = command_name(&op);
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
    let options = |allowed: &[&str]| Options::from_lit(name, field("options"), allowed);
    Ok(match op.as_str() {
        "aggregate" => aggregate_op(
            match field("pipeline") {
                Some(_) => array("pipeline")?,
                None => Vec::new(),
            },
            &options(options::AGGREGATE)?,
        )?,
        "count" | "countdocuments" => {
            count_op(optional_doc(field("filter"))?, &options(options::COUNT)?)?
        }
        "estimateddocumentcount" => {
            MongoOp::EstimatedCount(read_options(&options(options::ESTIMATED_COUNT)?)?)
        }
        "distinct" => {
            let field_name = match field("field") {
                Some(Lit::Str(s)) if !s.is_empty() => s.clone(),
                _ => return Err(query_err("distinct requires `field`.")),
            };
            distinct_op(
                field_name,
                optional_doc(field("filter"))?,
                &options(options::DISTINCT)?,
            )?
        }
        "insertone" => insert_op(
            WriteMethod::InsertOne,
            vec![required("doc")?],
            &options(options::INSERT_ONE)?,
        )?,
        "insertmany" => insert_op(
            WriteMethod::InsertMany,
            array("docs")?,
            &options(options::INSERT_MANY)?,
        )?,
        "updateone" => update_op(
            WriteMethod::UpdateOne,
            required("filter")?,
            required("update")?,
            &options(options::UPDATE_ONE)?,
        )?,
        "updatemany" => update_op(
            WriteMethod::UpdateMany,
            required("filter")?,
            required("update")?,
            &options(options::UPDATE_MANY)?,
        )?,
        "replaceone" => update_op(
            WriteMethod::ReplaceOne,
            required("filter")?,
            required("replacement").or_else(|_| required("doc"))?,
            &options(options::REPLACE_ONE)?,
        )?,
        "deleteone" => delete_op(
            WriteMethod::DeleteOne,
            required("filter")?,
            &options(options::DELETE)?,
        )?,
        "deletemany" | "remove" => delete_op(
            WriteMethod::DeleteMany,
            required("filter")?,
            &options(options::DELETE)?,
        )?,
        "findoneandupdate" => find_and_modify(
            required("filter")?,
            Modify::Update(required("update")?),
            &options(options::FIND_ONE_AND_UPDATE)?,
        )?,
        "findoneandreplace" => find_and_modify(
            required("filter")?,
            Modify::Replace(required("replacement").or_else(|_| required("doc"))?),
            &options(options::FIND_ONE_AND_REPLACE)?,
        )?,
        "findoneanddelete" => find_and_modify(
            required("filter")?,
            Modify::Delete,
            &options(options::FIND_ONE_AND_DELETE)?,
        )?,
        "bulkwrite" => bulk_op(&array("ops")?, &options(options::BULK_WRITE)?)?,
        other => {
            return Err(ConnectorError::NotSupported(format!(
                "Mongo op `{other}` is not implemented."
            )))
        }
    })
}

fn bulk_models(docs: &[Document]) -> ConnectorResult<Vec<WriteModel>> {
    let models = docs
        .iter()
        .map(bulk_model)
        .collect::<ConnectorResult<Vec<_>>>()?;
    if models.is_empty() {
        return Err(query_err("bulkWrite needs at least one operation."));
    }
    Ok(models)
}

/// `{ updateOne: { filter, update, upsert, … } }`. A body takes the same
/// per-statement options as the single-statement method.
fn bulk_model(doc: &Document) -> ConnectorResult<WriteModel> {
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
    let label = format!("bulkWrite {key}");
    let body = |allowed: &[&str]| Options::new(&label, body.clone(), allowed);
    match key.to_ascii_lowercase().as_str() {
        "insertone" => Ok(WriteModel::InsertOne(
            body(options::BULK_INSERT)?.required_doc("document")?,
        )),
        "updateone" => {
            let opts = body(options::BULK_UPDATE_ONE)?;
            let (filter, update) = (opts.required_doc("filter")?, opts.required_doc("update")?);
            Ok(WriteModel::UpdateOne(update_model(filter, update, &opts)?))
        }
        "updatemany" => {
            let opts = body(options::BULK_UPDATE_MANY)?;
            let (filter, update) = (opts.required_doc("filter")?, opts.required_doc("update")?);
            Ok(WriteModel::UpdateMany(update_model(filter, update, &opts)?))
        }
        "replaceone" => {
            let opts = body(options::BULK_REPLACE)?;
            let (filter, replacement) = (
                opts.required_doc("filter")?,
                opts.required_doc("replacement")?,
            );
            Ok(WriteModel::ReplaceOne(update_model(
                filter,
                replacement,
                &opts,
            )?))
        }
        "deleteone" => {
            let opts = body(options::BULK_DELETE)?;
            Ok(WriteModel::DeleteOne(delete_model(
                opts.required_doc("filter")?,
                &opts,
            )?))
        }
        "deletemany" => {
            let opts = body(options::BULK_DELETE)?;
            Ok(WriteModel::DeleteMany(delete_model(
                opts.required_doc("filter")?,
                &opts,
            )?))
        }
        other => Err(query_err(&format!(
            "bulkWrite op `{other}` is not supported."
        ))),
    }
}
