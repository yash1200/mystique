//! The options argument of a shell method: `find(filter, projection, options)`,
//! `updateOne(filter, update, options)`, `aggregate(pipeline, options)`, …
//!
//! Each method names the keys it takes. A key outside that list is refused
//! with the list, so an option is never parsed and then dropped. A key set to
//! `null` counts as absent, as in mongosh.
//!
//! | Option | Methods | How it runs |
//! |---|---|---|
//! | `projection`, `sort` | find, findOne, findOneAnd* | driver `FindOptions` / `FindOneAnd*Options` |
//! | `limit`, `skip` | find, countDocuments | driver options |
//! | `batchSize`, `allowDiskUse` | find, aggregate | driver options |
//! | `hint`, `collation` | reads, writes, bulk models | driver options; `hint` / `collation` per statement in write commands |
//! | `maxTimeMS` | every method | capped by the profile timeout |
//! | `comment` | every method | driver option or command field |
//! | `let` | aggregate, writes | driver option or command field |
//! | `bypassDocumentValidation` | aggregate, inserts, updates | driver option or command field |
//! | `upsert` | updateOne, updateMany, replaceOne, findOneAndUpdate/Replace, bulk models | statement `upsert` / driver option |
//! | `arrayFilters` | updateOne, updateMany, findOneAndUpdate, bulk models | statement `arrayFilters` / driver option |
//! | `returnDocument` (`"before"` / `"after"`), `returnNewDocument` | findOneAndUpdate, findOneAndReplace | driver `ReturnDocument` |
//! | `ordered` | insertMany, bulkWrite | write command `ordered` |
//! | `writeConcern` | writes | write command `writeConcern` / driver option |

use mongodb::bson::{self, Bson, Document};
use mongodb::options::{Collation, WriteConcern};
use mystique_connector_api::ConnectorResult;

use super::lex::query_err;
use super::value::{to_document, Lit};

pub(super) const FIND: &[&str] = &[
    "projection",
    "sort",
    "limit",
    "skip",
    "hint",
    "collation",
    "maxTimeMS",
    "batchSize",
    "allowDiskUse",
    "comment",
];
pub(super) const AGGREGATE: &[&str] = &[
    "allowDiskUse",
    "maxTimeMS",
    "collation",
    "hint",
    "batchSize",
    "comment",
    "let",
    "bypassDocumentValidation",
];
pub(super) const COUNT: &[&str] = &["limit", "skip", "hint", "collation", "maxTimeMS", "comment"];
pub(super) const ESTIMATED_COUNT: &[&str] = &["maxTimeMS", "comment"];
pub(super) const DISTINCT: &[&str] = &["collation", "maxTimeMS", "comment"];
pub(super) const INSERT_ONE: &[&str] = &[
    "bypassDocumentValidation",
    "comment",
    "maxTimeMS",
    "writeConcern",
];
pub(super) const INSERT_MANY: &[&str] = &[
    "ordered",
    "bypassDocumentValidation",
    "comment",
    "maxTimeMS",
    "writeConcern",
];
pub(super) const UPDATE_ONE: &[&str] = &[
    "upsert",
    "arrayFilters",
    "collation",
    "hint",
    "sort",
    "let",
    "bypassDocumentValidation",
    "comment",
    "maxTimeMS",
    "writeConcern",
];
pub(super) const UPDATE_MANY: &[&str] = &[
    "upsert",
    "arrayFilters",
    "collation",
    "hint",
    "let",
    "bypassDocumentValidation",
    "comment",
    "maxTimeMS",
    "writeConcern",
];
pub(super) const REPLACE_ONE: &[&str] = &[
    "upsert",
    "collation",
    "hint",
    "sort",
    "let",
    "bypassDocumentValidation",
    "comment",
    "maxTimeMS",
    "writeConcern",
];
pub(super) const DELETE: &[&str] = &[
    "collation",
    "hint",
    "let",
    "comment",
    "maxTimeMS",
    "writeConcern",
];
pub(super) const FIND_ONE_AND_UPDATE: &[&str] = &[
    "upsert",
    "returnDocument",
    "returnNewDocument",
    "projection",
    "sort",
    "arrayFilters",
    "collation",
    "hint",
    "maxTimeMS",
    "let",
    "bypassDocumentValidation",
    "comment",
    "writeConcern",
];
pub(super) const FIND_ONE_AND_REPLACE: &[&str] = &[
    "upsert",
    "returnDocument",
    "returnNewDocument",
    "projection",
    "sort",
    "collation",
    "hint",
    "maxTimeMS",
    "let",
    "bypassDocumentValidation",
    "comment",
    "writeConcern",
];
pub(super) const FIND_ONE_AND_DELETE: &[&str] = &[
    "projection",
    "sort",
    "collation",
    "hint",
    "maxTimeMS",
    "let",
    "comment",
    "writeConcern",
];
pub(super) const BULK_WRITE: &[&str] = &[
    "ordered",
    "bypassDocumentValidation",
    "comment",
    "let",
    "maxTimeMS",
    "writeConcern",
];

/// Keys of each `bulkWrite` model body.
pub(super) const BULK_INSERT: &[&str] = &["document"];
pub(super) const BULK_UPDATE_ONE: &[&str] = &[
    "filter",
    "update",
    "upsert",
    "arrayFilters",
    "collation",
    "hint",
    "sort",
];
pub(super) const BULK_UPDATE_MANY: &[&str] = &[
    "filter",
    "update",
    "upsert",
    "arrayFilters",
    "collation",
    "hint",
];
pub(super) const BULK_REPLACE: &[&str] = &[
    "filter",
    "replacement",
    "upsert",
    "collation",
    "hint",
    "sort",
];
pub(super) const BULK_DELETE: &[&str] = &["filter", "collation", "hint"];

/// One options object, checked against the keys its method takes.
pub(super) struct Options {
    /// `updateOne()`, or `bulkWrite updateOne` for a model body.
    label: String,
    doc: Document,
}

impl Options {
    pub(super) fn empty(method: &str) -> Self {
        Self {
            label: format!("{method}()"),
            doc: Document::new(),
        }
    }

    /// An options argument. `None` (not given) and `null` are empty.
    pub(super) fn from_lit(
        method: &str,
        lit: Option<&Lit>,
        allowed: &[&str],
    ) -> ConnectorResult<Self> {
        match lit {
            None | Some(Lit::Null) => Ok(Self::empty(method)),
            Some(lit @ Lit::Object(_)) => {
                Self::new(&format!("{method}()"), to_document(lit)?, allowed)
            }
            Some(_) => Err(query_err(&format!(
                "{method}() options must be an object `{{ … }}`."
            ))),
        }
    }

    /// `label` names the caller in errors: `updateOne()`, `bulkWrite updateOne`.
    pub(super) fn new(label: &str, doc: Document, allowed: &[&str]) -> ConnectorResult<Self> {
        if let Some(key) = doc.keys().find(|key| !allowed.contains(&key.as_str())) {
            return Err(query_err(&format!(
                "{label} does not take option `{key}`. It takes: {}.",
                allowed.join(", ")
            )));
        }
        Ok(Self {
            label: label.to_string(),
            doc,
        })
    }

    fn get(&self, key: &str) -> Option<&Bson> {
        self.doc
            .get(key)
            .filter(|value| !matches!(value, Bson::Null))
    }

    fn wrong(&self, key: &str, what: &str) -> mystique_connector_api::ConnectorError {
        query_err(&format!("{} option `{key}` must be {what}.", self.label))
    }

    pub(super) fn bool(&self, key: &str) -> ConnectorResult<Option<bool>> {
        match self.get(key) {
            None => Ok(None),
            Some(Bson::Boolean(b)) => Ok(Some(*b)),
            Some(_) => Err(self.wrong(key, "true or false")),
        }
    }

    pub(super) fn int(&self, key: &str) -> ConnectorResult<Option<i64>> {
        match self.get(key) {
            None => Ok(None),
            Some(Bson::Int32(n)) => Ok(Some(i64::from(*n))),
            Some(Bson::Int64(n)) => Ok(Some(*n)),
            Some(Bson::Double(f)) if f.fract() == 0.0 && f.abs() < 9.0e15 => Ok(Some(*f as i64)),
            Some(_) => Err(self.wrong(key, "an integer")),
        }
    }

    pub(super) fn uint(&self, key: &str) -> ConnectorResult<Option<u64>> {
        match self.int(key)? {
            None => Ok(None),
            Some(n) => u64::try_from(n)
                .map(Some)
                .map_err(|_| self.wrong(key, "zero or more")),
        }
    }

    pub(super) fn u32(&self, key: &str) -> ConnectorResult<Option<u32>> {
        match self.uint(key)? {
            None => Ok(None),
            Some(n) => u32::try_from(n)
                .map(Some)
                .map_err(|_| self.wrong(key, "at most 4294967295")),
        }
    }

    pub(super) fn doc(&self, key: &str) -> ConnectorResult<Option<Document>> {
        match self.get(key) {
            None => Ok(None),
            Some(Bson::Document(doc)) => Ok(Some(doc.clone())),
            Some(_) => Err(self.wrong(key, "an object `{ … }`")),
        }
    }

    pub(super) fn required_doc(&self, key: &str) -> ConnectorResult<Document> {
        self.doc(key)?
            .ok_or_else(|| query_err(&format!("{} needs `{key}`.", self.label)))
    }

    pub(super) fn docs(&self, key: &str) -> ConnectorResult<Option<Vec<Document>>> {
        match self.get(key) {
            None => Ok(None),
            Some(Bson::Array(items)) => items
                .iter()
                .map(|item| match item {
                    Bson::Document(doc) => Ok(doc.clone()),
                    _ => Err(self.wrong(key, "an array of objects")),
                })
                .collect::<ConnectorResult<Vec<_>>>()
                .map(Some),
            Some(_) => Err(self.wrong(key, "an array of objects")),
        }
    }

    /// An index name or a key pattern.
    pub(super) fn hint(&self) -> ConnectorResult<Option<Bson>> {
        match self.get("hint") {
            None => Ok(None),
            Some(hint @ (Bson::String(_) | Bson::Document(_))) => Ok(Some(hint.clone())),
            Some(_) => Err(self.wrong("hint", "an index name or a key pattern `{ … }`")),
        }
    }

    /// Checked against the driver's collation shape (`locale` is required).
    pub(super) fn collation(&self) -> ConnectorResult<Option<Document>> {
        let Some(doc) = self.doc("collation")? else {
            return Ok(None);
        };
        check_collation(&self.label, &doc)?;
        Ok(Some(doc))
    }

    pub(super) fn write_concern(&self) -> ConnectorResult<Option<Document>> {
        let Some(doc) = self.doc("writeConcern")? else {
            return Ok(None);
        };
        bson::from_document::<WriteConcern>(doc.clone()).map_err(|err| {
            query_err(&format!(
                "{} option `writeConcern` is not valid: {err}",
                self.label
            ))
        })?;
        Ok(Some(doc))
    }

    /// `maxTimeMS: 0` means no limit, as on the server.
    pub(super) fn max_time_ms(&self) -> ConnectorResult<Option<u64>> {
        Ok(self.uint("maxTimeMS")?.filter(|ms| *ms > 0))
    }

    pub(super) fn any(&self, key: &str) -> Option<Bson> {
        self.get(key).cloned()
    }

    /// `returnDocument: "after"` or `returnNewDocument: true` return the new
    /// document. Both given and in conflict is refused.
    pub(super) fn return_after(&self) -> ConnectorResult<Option<bool>> {
        let by_name = match self.get("returnDocument") {
            None => None,
            Some(Bson::String(s)) if s.eq_ignore_ascii_case("after") => Some(true),
            Some(Bson::String(s)) if s.eq_ignore_ascii_case("before") => Some(false),
            Some(_) => return Err(self.wrong("returnDocument", "\"before\" or \"after\"")),
        };
        let legacy = self.bool("returnNewDocument")?;
        match (by_name, legacy) {
            (Some(a), Some(b)) if a != b => Err(query_err(&format!(
                "{} options `returnDocument` and `returnNewDocument` disagree.",
                self.label
            ))),
            (a, b) => Ok(a.or(b)),
        }
    }
}

pub(super) fn check_collation(label: &str, doc: &Document) -> ConnectorResult<()> {
    bson::from_document::<Collation>(doc.clone())
        .map(|_| ())
        .map_err(|err| query_err(&format!("{label} option `collation` is not valid: {err}")))
}
