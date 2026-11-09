//! insertOne … deleteMany, insertMany and bulkWrite as `insert` / `update` /
//! `delete` commands.
//!
//! The driver's insert/update/delete helpers take no `maxTimeMS`, so the
//! profile timeout never reached the server for a write: a client timeout
//! dropped the call while the server kept going. The commands here carry
//! `maxTimeMS` (see [`super::opts::max_time`]) and every option the shell
//! method took.
//!
//! Consecutive models of one kind share a command (at most
//! [`MAX_OPS_PER_COMMAND`] statements or about [`MAX_BYTES_PER_COMMAND`]).
//! Each reply adds to one [`WriteOutcome`]: counts, upserted ids and write
//! errors. An ordered write stops at its first error; an unordered one runs
//! every command. On failure the error names what was applied before it, so
//! partial `bulkWrite` / `insertMany` counts are not lost.
//!
//! Unlike the driver helpers these commands are not retried on a transient
//! network error.

use mongodb::bson::{self, doc, oid::ObjectId, Bson, Document};
use mongodb::{ClientSession, Database};
use mystique_connector_api::{prettify_sql_error, ConnectorError, ConnectorResult, QueryControl};

use super::error::TXN_ABORTED;
use super::opts::max_time_field;
use super::query::{boxed, drive};
use crate::shell::{DeleteModel, UpdateModel, WriteMethod, WriteModel, WriteOptions};

pub(super) const MAX_OPS_PER_COMMAND: usize = 1_000;
pub(super) const MAX_BYTES_PER_COMMAND: usize = 8 * 1024 * 1024;
/// Write errors listed in a failure message.
const LISTED_ERRORS: usize = 5;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum BatchKind {
    Insert,
    Update,
    Delete,
}

/// One command and where its statements sit in the user's list.
#[derive(Debug, Clone, PartialEq)]
pub(super) struct WriteBatch {
    pub kind: BatchKind,
    /// Index of the first statement in the user's list.
    pub start: usize,
    pub len: usize,
    pub ordered: bool,
    /// `_id` of each inserted document, generated where the user gave none.
    pub ids: Vec<Bson>,
    pub command: Document,
}

/// Fields every command of one write gets.
#[derive(Debug, Clone, Default, PartialEq)]
pub(super) struct CommandFields {
    pub max_time_ms: Option<i64>,
    /// The user's `writeConcern`, else the client default outside a
    /// transaction. A transaction takes none.
    pub write_concern: Option<Document>,
}

impl CommandFields {
    pub(super) fn new(
        options: &WriteOptions,
        timeout_ms: u64,
        in_transaction: bool,
        default_concern: Option<Document>,
    ) -> Self {
        let write_concern = match &options.write_concern {
            Some(concern) => Some(concern.clone()),
            None if in_transaction => None,
            None => default_concern.filter(|concern| !concern.is_empty()),
        };
        Self {
            max_time_ms: max_time_field(timeout_ms, options.max_time_ms),
            write_concern,
        }
    }
}

fn kind_of(model: &WriteModel) -> BatchKind {
    match model {
        WriteModel::InsertOne(_) => BatchKind::Insert,
        WriteModel::UpdateOne(_) | WriteModel::UpdateMany(_) | WriteModel::ReplaceOne(_) => {
            BatchKind::Update
        }
        WriteModel::DeleteOne(_) | WriteModel::DeleteMany(_) => BatchKind::Delete,
    }
}

/// `_id` first, generated when missing, so the reply can name it.
fn with_id(document: Document) -> (Bson, Document) {
    if let Some(id) = document.get("_id") {
        return (id.clone(), document);
    }
    let id = Bson::ObjectId(ObjectId::new());
    let mut out = Document::new();
    out.insert("_id", id.clone());
    out.extend(document);
    (id, out)
}

fn update_statement(model: UpdateModel, multi: bool) -> Document {
    let mut statement = doc! { "q": model.filter, "u": model.update, "multi": multi };
    if let Some(upsert) = model.upsert {
        statement.insert("upsert", upsert);
    }
    if let Some(filters) = model.array_filters {
        statement.insert("arrayFilters", filters);
    }
    if let Some(collation) = model.collation {
        statement.insert("collation", collation);
    }
    if let Some(hint) = model.hint {
        statement.insert("hint", hint);
    }
    if let Some(sort) = model.sort {
        statement.insert("sort", sort);
    }
    statement
}

fn delete_statement(model: DeleteModel, many: bool) -> Document {
    let mut statement = doc! { "q": model.filter, "limit": if many { 0 } else { 1 } };
    if let Some(collation) = model.collation {
        statement.insert("collation", collation);
    }
    if let Some(hint) = model.hint {
        statement.insert("hint", hint);
    }
    statement
}

fn size_of(document: &Document) -> usize {
    bson::to_vec(document).map(|bytes| bytes.len()).unwrap_or(0)
}

struct Pending {
    kind: BatchKind,
    start: usize,
    ids: Vec<Bson>,
    statements: Vec<Document>,
    bytes: usize,
}

/// The commands for `models`, in order.
pub(super) fn write_batches(
    coll: &str,
    models: Vec<WriteModel>,
    options: &WriteOptions,
    fields: &CommandFields,
) -> Vec<WriteBatch> {
    let ordered = options.ordered.unwrap_or(true);
    let mut batches = Vec::new();
    let mut pending: Option<Pending> = None;
    for (index, model) in models.into_iter().enumerate() {
        let kind = kind_of(&model);
        let (id, statement) = match model {
            WriteModel::InsertOne(document) => {
                let (id, document) = with_id(document);
                (Some(id), document)
            }
            WriteModel::UpdateOne(m) | WriteModel::ReplaceOne(m) => {
                (None, update_statement(m, false))
            }
            WriteModel::UpdateMany(m) => (None, update_statement(m, true)),
            WriteModel::DeleteOne(m) => (None, delete_statement(m, false)),
            WriteModel::DeleteMany(m) => (None, delete_statement(m, true)),
        };
        let bytes = size_of(&statement);
        let full = pending.as_ref().is_some_and(|p| {
            p.kind != kind
                || p.statements.len() >= MAX_OPS_PER_COMMAND
                || p.bytes + bytes > MAX_BYTES_PER_COMMAND
        });
        if full {
            if let Some(done) = pending.take() {
                batches.push(finish(coll, done, ordered, options, fields));
            }
        }
        let p = pending.get_or_insert_with(|| Pending {
            kind,
            start: index,
            ids: Vec::new(),
            statements: Vec::new(),
            bytes: 0,
        });
        p.ids.extend(id);
        p.statements.push(statement);
        p.bytes += bytes;
    }
    if let Some(done) = pending {
        batches.push(finish(coll, done, ordered, options, fields));
    }
    batches
}

fn finish(
    coll: &str,
    pending: Pending,
    ordered: bool,
    options: &WriteOptions,
    fields: &CommandFields,
) -> WriteBatch {
    let len = pending.statements.len();
    let mut command = match pending.kind {
        BatchKind::Insert => doc! { "insert": coll, "documents": pending.statements },
        BatchKind::Update => doc! { "update": coll, "updates": pending.statements },
        BatchKind::Delete => doc! { "delete": coll, "deletes": pending.statements },
    };
    command.insert("ordered", ordered);
    if pending.kind != BatchKind::Delete {
        if let Some(bypass) = options.bypass_document_validation {
            command.insert("bypassDocumentValidation", bypass);
        }
    }
    if pending.kind != BatchKind::Insert {
        if let Some(vars) = &options.let_vars {
            command.insert("let", vars.clone());
        }
    }
    if let Some(comment) = &options.comment {
        command.insert("comment", comment.clone());
    }
    if let Some(concern) = &fields.write_concern {
        command.insert("writeConcern", concern.clone());
    }
    if let Some(ms) = fields.max_time_ms {
        command.insert("maxTimeMS", ms);
    }
    WriteBatch {
        kind: pending.kind,
        start: pending.start,
        len,
        ordered,
        ids: pending.ids,
        command,
    }
}

/// One write error, at its index in the user's list.
#[derive(Debug, Clone, PartialEq)]
pub(super) struct WriteError {
    pub index: usize,
    pub code: i64,
    pub message: String,
}

/// What a write did, across its commands. Kept when a command fails, so the
/// error can say what was applied before it.
#[derive(Debug, Clone, Default, PartialEq)]
pub(super) struct WriteOutcome {
    pub inserted: u64,
    pub matched: u64,
    pub modified: u64,
    pub deleted: u64,
    /// `(index, _id)` of each inserted document.
    pub inserted_ids: Vec<(usize, Bson)>,
    /// `(index, _id)` of each upserted document.
    pub upserted: Vec<(usize, Bson)>,
    pub errors: Vec<WriteError>,
    pub concern_error: Option<String>,
}

fn count(value: Option<&Bson>) -> u64 {
    match value {
        Some(Bson::Int32(n)) => u64::try_from(*n).unwrap_or(0),
        Some(Bson::Int64(n)) => u64::try_from(*n).unwrap_or(0),
        Some(Bson::Double(f)) if *f >= 0.0 => *f as u64,
        _ => 0,
    }
}

fn int(value: Option<&Bson>) -> i64 {
    match value {
        Some(Bson::Int32(n)) => i64::from(*n),
        Some(Bson::Int64(n)) => *n,
        Some(Bson::Double(f)) => *f as i64,
        _ => 0,
    }
}

fn message_of(entry: &Document) -> String {
    entry
        .get_str("errmsg")
        .map(str::to_string)
        .unwrap_or_else(|_| format!("MongoDB write error {}", int(entry.get("code"))))
}

impl WriteOutcome {
    /// Add one command reply (`n`, `nModified`, `upserted`, `writeErrors`,
    /// `writeConcernError`).
    pub(super) fn absorb(&mut self, batch: &WriteBatch, reply: &Document) {
        let n = count(reply.get("n"));
        let errors: Vec<WriteError> = reply
            .get_array("writeErrors")
            .map(|items| {
                items
                    .iter()
                    .filter_map(Bson::as_document)
                    .map(|entry| WriteError {
                        index: batch.start + usize::try_from(int(entry.get("index"))).unwrap_or(0),
                        code: int(entry.get("code")),
                        message: message_of(entry),
                    })
                    .collect()
            })
            .unwrap_or_default();
        match batch.kind {
            BatchKind::Insert => {
                self.inserted += n;
                let first_error = errors.iter().map(|e| e.index).min();
                for (offset, id) in batch.ids.iter().enumerate() {
                    let index = batch.start + offset;
                    let failed = errors.iter().any(|e| e.index == index);
                    let stopped = batch.ordered && first_error.is_some_and(|first| index > first);
                    if !failed && !stopped {
                        self.inserted_ids.push((index, id.clone()));
                    }
                }
            }
            BatchKind::Update => {
                let upserted: Vec<(usize, Bson)> = reply
                    .get_array("upserted")
                    .map(|items| {
                        items
                            .iter()
                            .filter_map(Bson::as_document)
                            .map(|entry| {
                                let index = batch.start
                                    + usize::try_from(int(entry.get("index"))).unwrap_or(0);
                                (index, entry.get("_id").cloned().unwrap_or(Bson::Null))
                            })
                            .collect()
                    })
                    .unwrap_or_default();
                self.matched += n.saturating_sub(upserted.len() as u64);
                self.modified += count(reply.get("nModified"));
                self.upserted.extend(upserted);
            }
            BatchKind::Delete => self.deleted += n,
        }
        self.errors.extend(errors);
        if let Ok(concern) = reply.get_document("writeConcernError") {
            self.concern_error = Some(message_of(concern));
        }
    }

    pub(super) fn failed(&self) -> bool {
        !self.errors.is_empty() || self.concern_error.is_some()
    }

    fn applied(&self) -> bool {
        self.inserted + self.matched + self.modified + self.deleted > 0 || !self.upserted.is_empty()
    }

    /// The counts this method reports.
    pub(super) fn counts(&self, method: WriteMethod) -> String {
        let all = format!(
            "inserted={} matched={} modified={} deleted={} upserted={}",
            self.inserted,
            self.matched,
            self.modified,
            self.deleted,
            self.upserted.len()
        );
        match method {
            WriteMethod::InsertOne | WriteMethod::InsertMany => {
                format!("inserted={}", self.inserted)
            }
            WriteMethod::UpdateOne | WriteMethod::UpdateMany | WriteMethod::ReplaceOne => {
                let mut out = format!("matched={} modified={}", self.matched, self.modified);
                if let Some((_, id)) = self.upserted.first() {
                    out.push_str(&format!(" upsertedId={id:?}"));
                }
                out
            }
            WriteMethod::DeleteOne | WriteMethod::DeleteMany => {
                format!("deleted={}", self.deleted)
            }
            WriteMethod::BulkWrite => all,
        }
    }

    /// The result line of a write that succeeded.
    pub(super) fn summary(&self, method: WriteMethod, at: &str) -> String {
        match (method, self.inserted_ids.first()) {
            (WriteMethod::InsertOne, Some((_, id))) => {
                format!("insertOne on {at} id={id:?}")
            }
            _ => format!("{} on {at} {}", method.name(), self.counts(method)),
        }
    }

    /// The error of a write that failed: the write errors (each with its
    /// index for a many-statement write), the write concern error, then what
    /// was applied. Inside a transaction nothing is applied until commit, so
    /// no counts are given.
    pub(super) fn failure(
        &self,
        method: WriteMethod,
        at: &str,
        ordered: bool,
        in_transaction: bool,
        cause: Option<String>,
    ) -> String {
        let many = matches!(method, WriteMethod::InsertMany | WriteMethod::BulkWrite);
        let mut parts = Vec::new();
        if let Some(cause) = cause {
            parts.push(cause);
        }
        if many && !self.errors.is_empty() {
            let listed: Vec<String> = self
                .errors
                .iter()
                .take(LISTED_ERRORS)
                .map(|e| format!("op {}: {}", e.index, prettify_sql_error(&e.message)))
                .collect();
            let more = self.errors.len().saturating_sub(LISTED_ERRORS);
            let noun = if self.errors.len() == 1 {
                "write error"
            } else {
                "write errors"
            };
            let mut line = format!("{} {noun}: {}", self.errors.len(), listed.join("; "));
            if more > 0 {
                line.push_str(&format!("; and {more} more"));
            }
            if !line.ends_with(['.', '!', '?']) {
                line.push('.');
            }
            parts.push(line);
        } else if let Some(first) = self.errors.first() {
            parts.push(prettify_sql_error(&first.message));
        }
        if let Some(concern) = &self.concern_error {
            parts.push(format!("Write concern error: {concern}"));
        }
        if !in_transaction && (self.applied() || many) {
            let how = if ordered || !many {
                "before it stopped"
            } else {
                "for the other operations"
            };
            parts.push(format!(
                "{} on {at} applied {} {how}.",
                method.name(),
                self.counts(WriteMethod::BulkWrite)
            ));
        }
        parts.join(" ")
    }
}

/// Run `models` as write commands on `db`. Returns the result line.
#[allow(clippy::too_many_arguments)] // one call site, values already resolved
pub(super) async fn run_models(
    db: &Database,
    at: &str,
    coll: &str,
    method: WriteMethod,
    models: Vec<WriteModel>,
    options: &WriteOptions,
    timeout_ms: u64,
    control: &QueryControl,
    mut session: Option<&mut ClientSession>,
    in_transaction: bool,
) -> ConnectorResult<String> {
    let default_concern = db
        .write_concern()
        .and_then(|concern| bson::to_document(concern).ok());
    let fields = CommandFields::new(options, timeout_ms, in_transaction, default_concern);
    let ordered = options.ordered.unwrap_or(true);
    let batches = write_batches(coll, models, options, &fields);
    let mut outcome = WriteOutcome::default();
    for batch in &batches {
        let command = batch.command.clone();
        let reply = match session.as_mut() {
            Some(session) => {
                drive(
                    control,
                    boxed(db.run_command(command).session(&mut **session)),
                )
                .await
            }
            None => drive(control, db.run_command(command)).await,
        };
        let reply = match reply {
            Ok(reply) => reply,
            Err(ConnectorError::Query(msg)) => {
                // The server dropped the transaction: nothing in it stands.
                if msg.ends_with(TXN_ABORTED) || (!outcome.applied() && outcome.errors.is_empty()) {
                    return Err(ConnectorError::Query(msg));
                }
                return Err(ConnectorError::Query(outcome.failure(
                    method,
                    at,
                    ordered,
                    in_transaction,
                    Some(msg),
                )));
            }
            Err(other) => return Err(other),
        };
        outcome.absorb(batch, &reply);
        if ordered && !outcome.errors.is_empty() {
            break;
        }
    }
    if outcome.failed() {
        return Err(ConnectorError::Query(outcome.failure(
            method,
            at,
            ordered,
            in_transaction,
            None,
        )));
    }
    Ok(outcome.summary(method, at))
}

#[cfg(test)]
mod tests {
    use mongodb::bson::{doc, Bson, Document};
    use mystique_connector_api::NO_TIMEOUT;

    use super::*;
    use crate::shell::{parse_executable, Executable, MongoOp, ShellWrite};

    fn models(text: &str) -> (WriteMethod, Vec<WriteModel>, WriteOptions) {
        match parse_executable(text) {
            Ok(Executable::Call(call)) => match call.op {
                MongoOp::Write(ShellWrite::Models {
                    method,
                    models,
                    options,
                }) => (method, models, options),
                other => panic!("{text}: {other:?}"),
            },
            other => panic!("{text}: {other:?}"),
        }
    }

    fn batches(text: &str, timeout_ms: u64, in_transaction: bool) -> Vec<WriteBatch> {
        let (_, models, options) = models(text);
        let fields = CommandFields::new(
            &options,
            timeout_ms,
            in_transaction,
            Some(doc! { "w": "majority" }),
        );
        write_batches("t", models, &options, &fields)
    }

    fn only(text: &str, timeout_ms: u64, in_transaction: bool) -> Document {
        let mut all = batches(text, timeout_ms, in_transaction);
        assert_eq!(all.len(), 1, "{text}");
        all.remove(0).command
    }

    #[test]
    fn update_one_sends_every_option_and_the_time_limit() {
        let command = only(
            r#"db.t.updateOne({a: 1}, {$set: {"x.$[e]": 1}}, {upsert: true, arrayFilters: [{e: 1}], collation: {locale: "fr"}, hint: "a_1", let: {v: 1}, comment: "c", bypassDocumentValidation: true, maxTimeMS: 500})"#,
            30_000,
            false,
        );
        assert_eq!(
            command,
            doc! {
                "update": "t",
                "updates": [{
                    "q": { "a": 1 },
                    "u": { "$set": { "x.$[e]": 1 } },
                    "multi": false,
                    "upsert": true,
                    "arrayFilters": [{ "e": 1 }],
                    "collation": { "locale": "fr" },
                    "hint": "a_1",
                }],
                "ordered": true,
                "bypassDocumentValidation": true,
                "let": { "v": 1 },
                "comment": "c",
                "writeConcern": { "w": "majority" },
                "maxTimeMS": 500_i64,
            }
        );
    }

    #[test]
    fn the_profile_timeout_reaches_a_write_without_options() {
        let command = only("db.t.updateMany({a: 1}, {$set: {b: 1}})", 120_000, false);
        assert_eq!(command.get("maxTimeMS"), Some(&Bson::Int64(120_000)));
        let command = only("db.t.deleteOne({a: 1})", NO_TIMEOUT, false);
        assert_eq!(command.get("maxTimeMS"), None, "0 sends no maxTimeMS");
        // The user's limit lowers the profile timeout, never raises it.
        let command = only("db.t.deleteOne({a: 1}, {maxTimeMS: 999999})", 5_000, false);
        assert_eq!(command.get("maxTimeMS"), Some(&Bson::Int64(5_000)));
    }

    #[test]
    fn delete_many_and_replace_one_statements() {
        let command = only(
            r#"db.t.deleteMany({a: 1}, {hint: {a: 1}, collation: {locale: "en"}})"#,
            1_000,
            true,
        );
        assert_eq!(
            command,
            doc! {
                "delete": "t",
                "deletes": [{ "q": { "a": 1 }, "limit": 0, "collation": { "locale": "en" }, "hint": { "a": 1 } }],
                "ordered": true,
                "maxTimeMS": 1_000_i64,
            },
            "a transaction takes no writeConcern"
        );
        let command = only(
            r#"db.t.replaceOne({a: 1}, {a: 2, b: 3}, {upsert: true, sort: {a: 1}})"#,
            1_000,
            false,
        );
        let updates = command.get_array("updates").cloned().unwrap_or_default();
        assert_eq!(
            updates,
            vec![Bson::Document(doc! {
                "q": { "a": 1 },
                "u": { "a": 2, "b": 3 },
                "multi": false,
                "upsert": true,
                "sort": { "a": 1 },
            })]
        );
    }

    #[test]
    fn insert_puts_an_id_first_and_keeps_a_given_one() {
        let batch = batches(
            r#"db.t.insertMany([{a: 1}, {_id: 7, a: 2}], {ordered: false, bypassDocumentValidation: false, writeConcern: {w: 1}})"#,
            NO_TIMEOUT,
            false,
        )
        .remove(0);
        assert_eq!(batch.kind, BatchKind::Insert);
        assert!(!batch.ordered);
        let documents = batch
            .command
            .get_array("documents")
            .cloned()
            .unwrap_or_default();
        let Some(Bson::Document(first)) = documents.first() else {
            panic!("{documents:?}");
        };
        assert_eq!(first.keys().next().map(String::as_str), Some("_id"));
        assert!(matches!(first.get("_id"), Some(Bson::ObjectId(_))));
        assert_eq!(batch.ids.first(), first.get("_id"));
        assert_eq!(batch.ids.get(1), Some(&Bson::Int32(7)));
        assert_eq!(batch.command.get_bool("ordered").ok(), Some(false));
        assert_eq!(
            batch.command.get_bool("bypassDocumentValidation").ok(),
            Some(false)
        );
        assert_eq!(
            batch.command.get_document("writeConcern").ok(),
            Some(&doc! { "w": 1 }),
            "the user's writeConcern wins over the default"
        );
        assert!(batch.command.get("let").is_none());
    }

    #[test]
    fn bulk_write_groups_runs_of_one_kind() {
        let all = batches(
            r#"db.t.bulkWrite([{insertOne: {document: {_id: 1}}}, {insertOne: {document: {_id: 2}}}, {updateOne: {filter: {_id: 1}, update: {$set: {a: 1}}}}, {deleteOne: {filter: {_id: 2}}}, {insertOne: {document: {_id: 3}}}], {ordered: false})"#,
            1_000,
            false,
        );
        let shape: Vec<(BatchKind, usize, usize, bool)> = all
            .iter()
            .map(|b| (b.kind, b.start, b.len, b.ordered))
            .collect();
        assert_eq!(
            shape,
            vec![
                (BatchKind::Insert, 0, 2, false),
                (BatchKind::Update, 2, 1, false),
                (BatchKind::Delete, 3, 1, false),
                (BatchKind::Insert, 4, 1, false),
            ]
        );
    }

    #[test]
    fn a_large_insert_splits_into_commands() {
        let docs: Vec<WriteModel> = (0..2_500)
            .map(|i| WriteModel::InsertOne(doc! { "_id": i }))
            .collect();
        let options = WriteOptions::default();
        let all = write_batches("t", docs, &options, &CommandFields::default());
        let shape: Vec<(usize, usize)> = all.iter().map(|b| (b.start, b.len)).collect();
        assert_eq!(shape, vec![(0, 1_000), (1_000, 1_000), (2_000, 500)]);
    }

    fn batch(kind: BatchKind, start: usize, len: usize, ordered: bool) -> WriteBatch {
        WriteBatch {
            kind,
            start,
            len,
            ordered,
            ids: (0..len).map(|i| Bson::Int64((start + i) as i64)).collect(),
            command: Document::new(),
        }
    }

    #[test]
    fn update_replies_count_matches_apart_from_upserts() {
        let mut outcome = WriteOutcome::default();
        outcome.absorb(
            &batch(BatchKind::Update, 2, 3, true),
            &doc! { "ok": 1, "n": 3, "nModified": 1, "upserted": [{ "index": 1, "_id": 7 }] },
        );
        assert_eq!(outcome.matched, 2);
        assert_eq!(outcome.modified, 1);
        assert_eq!(outcome.upserted, vec![(3, Bson::Int32(7))]);
        assert!(!outcome.failed());
        assert_eq!(
            outcome.summary(WriteMethod::UpdateOne, "db.t"),
            "updateOne on db.t matched=2 modified=1 upsertedId=Int32(7)"
        );
    }

    #[test]
    fn an_ordered_bulk_write_keeps_what_ran_before_the_error() {
        let mut outcome = WriteOutcome::default();
        outcome.absorb(
            &batch(BatchKind::Delete, 0, 1, true),
            &doc! { "ok": 1, "n": 1 },
        );
        outcome.absorb(
            &batch(BatchKind::Insert, 1, 3, true),
            &doc! {
                "ok": 1,
                "n": 1,
                "writeErrors": [{ "index": 1, "code": 11000, "errmsg": "E11000 duplicate key error collection: app.t index: _id_ dup key: { _id: 2 }" }],
            },
        );
        assert_eq!(outcome.deleted, 1);
        assert_eq!(outcome.inserted, 1);
        assert_eq!(outcome.inserted_ids, vec![(1, Bson::Int64(1))]);
        assert_eq!(
            outcome.errors,
            vec![WriteError {
                index: 2,
                code: 11000,
                message:
                    "E11000 duplicate key error collection: app.t index: _id_ dup key: { _id: 2 }"
                        .into(),
            }]
        );
        assert!(outcome.failed());
        let message = outcome.failure(WriteMethod::BulkWrite, "app.t", true, false, None);
        assert_eq!(
            message,
            "1 write error: op 2: A row with this value already exists. bulkWrite on app.t applied inserted=1 matched=0 modified=0 deleted=1 upserted=0 before it stopped."
        );
        // In a transaction nothing stands until commit: no counts.
        let message = outcome.failure(WriteMethod::BulkWrite, "app.t", true, true, None);
        assert!(!message.contains("applied"), "{message}");
    }

    #[test]
    fn an_unordered_write_lists_its_errors() {
        let mut outcome = WriteOutcome::default();
        let errors: Vec<Bson> = (0..7)
            .map(|i| {
                Bson::Document(doc! { "index": i * 2, "code": 121, "errmsg": format!("bad {i}") })
            })
            .collect();
        outcome.absorb(
            &batch(BatchKind::Insert, 0, 14, false),
            &doc! { "ok": 1, "n": 7, "writeErrors": errors },
        );
        assert_eq!(outcome.inserted, 7);
        assert_eq!(
            outcome.inserted_ids.len(),
            7,
            "unordered: every other one ran"
        );
        let message = outcome.failure(WriteMethod::InsertMany, "db.t", false, false, None);
        assert!(
            message.starts_with("7 write errors: op 0: bad 0; op 2: bad 1;"),
            "{message}"
        );
        assert!(message.contains("; and 2 more."), "{message}");
        assert!(
            message.ends_with("insertMany on db.t applied inserted=7 matched=0 modified=0 deleted=0 upserted=0 for the other operations."),
            "{message}"
        );
    }

    #[test]
    fn single_writes_fail_with_the_server_message() {
        let mut outcome = WriteOutcome::default();
        outcome.absorb(
            &batch(BatchKind::Update, 0, 1, true),
            &doc! { "ok": 1, "n": 0, "nModified": 0, "writeErrors": [{ "index": 0, "code": 50, "errmsg": "operation exceeded time limit" }] },
        );
        assert_eq!(
            outcome.failure(WriteMethod::UpdateMany, "db.t", true, false, None),
            "operation exceeded time limit"
        );
        let mut concern = WriteOutcome::default();
        concern.absorb(
            &batch(BatchKind::Delete, 0, 1, true),
            &doc! { "ok": 1, "n": 4, "writeConcernError": { "code": 64, "errmsg": "waiting for replication timed out" } },
        );
        assert!(concern.failed());
        assert_eq!(
            concern.failure(WriteMethod::DeleteMany, "db.t", true, false, None),
            "Write concern error: waiting for replication timed out deleteMany on db.t applied inserted=0 matched=0 modified=0 deleted=4 upserted=0 before it stopped."
        );
    }

    #[test]
    fn success_lines_keep_their_shape() {
        let mut outcome = WriteOutcome::default();
        outcome.absorb(
            &batch(BatchKind::Insert, 0, 1, true),
            &doc! { "ok": 1, "n": 1 },
        );
        assert_eq!(
            outcome.summary(WriteMethod::InsertOne, "db.t"),
            "insertOne on db.t id=Int64(0)"
        );
        outcome.absorb(
            &batch(BatchKind::Delete, 1, 1, true),
            &doc! { "ok": 1, "n": 2 },
        );
        assert_eq!(
            outcome.summary(WriteMethod::BulkWrite, "db.t"),
            "bulkWrite on db.t inserted=1 matched=0 modified=0 deleted=2 upserted=0"
        );
        assert_eq!(
            outcome.summary(WriteMethod::DeleteMany, "db.t"),
            "deleteMany on db.t deleted=2"
        );
    }
}
