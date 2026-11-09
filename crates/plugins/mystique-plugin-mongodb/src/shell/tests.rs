//! Differential corpus for Mongo shell text.
//!
//! Each row is one input. `old` is what the two parsers did before this
//! module existed: the connector-api write policy (`classify`, `validate`,
//! `mongo_is_safe_dml`, `mongo_op_preview`) and the executor
//! (`shell_query::parse_mongo_call`). `new` is set only where the shared
//! parser does something else, with the reason. `exec` is a summary of the
//! call the executor runs (`err` when it refuses the text; `db/coll` when the
//! text names a database).

use mongodb::bson::{Bson, Document};
use mystique_connector_api::{EngineQueryPolicy, QueryMode, WriteRisk};

use super::parse::{parse_executable, BulkModel, Executable, Listing, MongoOp, ShellWrite, Target};
use super::policy::MongoQueryPolicy;

use WriteRisk::{Destructive as X, Read as R, Write as W};

/// What the policy and the executor did with one text.
struct Seen {
    /// `classify`.
    risk: WriteRisk,
    /// `validate(text, Read)` passed.
    read: bool,
    /// `validate(text, Write)` passed.
    write: bool,
    /// `safe_write` tier (INSERT / UPDATE).
    safe: bool,
    /// Write-guard preview.
    preview: &'static str,
    /// Executor parse, summarised.
    exec: &'static str,
}

/// A row whose behaviour changed, and why.
struct Change {
    seen: Seen,
    why: &'static str,
}

struct Row {
    text: &'static str,
    old: Seen,
    new: Option<Change>,
}

#[rustfmt::skip]
const CORPUS: &[Row] = &[
    Row { text: r#"db.orders.find({status: "open"})"#, old: Seen { risk: R, read: true, write: true, safe: false, preview: "find", exec: r#"find orders {"status":"open"}"# }, new: None },
    Row { text: "db.orders.find()", old: Seen { risk: R, read: true, write: true, safe: false, preview: "find", exec: "find orders {}" }, new: None },
    Row { text: "db.orders.find({}).sort({a: -1}).limit(5)", old: Seen { risk: R, read: true, write: true, safe: false, preview: "find", exec: r#"find orders {} s={"a":-1} l=5"# }, new: None },
    Row { text: "db.orders.find({}, {a: 1}).skip(10).limit(5).pretty()", old: Seen { risk: R, read: true, write: true, safe: false, preview: "find", exec: r#"find orders {} p={"a":1} l=5 k=10"# }, new: None },
    Row { text: "db.orders.findOne({a: 1})", old: Seen { risk: R, read: true, write: true, safe: false, preview: "findone", exec: r#"find orders {"a":1} l=1"# }, new: None },
    Row { text: "db.orders.find({a: 1}).count()", old: Seen { risk: R, read: true, write: true, safe: false, preview: "find", exec: "err" }, new: Some(Change { seen: Seen { risk: R, read: false, write: false, safe: false, preview: "find", exec: "err" }, why: "validate now refuses what the executor cannot run, before the write guard asks to confirm it (`.count()` chain)" }) },
    Row { text: "db.orders.find({a: 1}).explain()", old: Seen { risk: R, read: true, write: true, safe: false, preview: "find", exec: r#"find orders {"a":1}"# }, new: None },
    Row { text: "db.orders.find({}).forEach(printjson)", old: Seen { risk: R, read: true, write: true, safe: false, preview: "find", exec: "err" }, new: Some(Change { seen: Seen { risk: R, read: false, write: false, safe: false, preview: "find", exec: "err" }, why: "validate now refuses what the executor cannot run, before the write guard asks to confirm it (`.forEach()` chain)" }) },
    Row { text: "db.orders.find", old: Seen { risk: W, read: false, write: false, safe: false, preview: "db.orders.find", exec: "find orders {}" }, new: Some(Change { seen: Seen { risk: W, read: false, write: false, safe: false, preview: "db.orders.find", exec: "err" }, why: "a method needs `(…)`; the policy already refused it" }) },
    Row { text: "find({a: 1})", old: Seen { risk: W, read: false, write: false, safe: false, preview: "find({a:1})", exec: r#"find - {"a":1}"# }, new: Some(Change { seen: Seen { risk: W, read: false, write: false, safe: false, preview: "find({a:1})", exec: "err" }, why: "a call needs `db.` in front; the policy already refused it" }) },
    Row { text: "orders.find({a: 1})", old: Seen { risk: R, read: true, write: true, safe: false, preview: "find", exec: r#"find - {"a":1}"# }, new: Some(Change { seen: Seen { risk: W, read: false, write: false, safe: false, preview: "orders.find({a:1})", exec: "err" }, why: "a call needs `db.` in front; the executor read it as a find on the Collection field, not `orders`" }) },
    Row { text: "db.system.profile.find({})", old: Seen { risk: R, read: true, write: true, safe: false, preview: "find", exec: "find profile {}" }, new: Some(Change { seen: Seen { risk: R, read: true, write: true, safe: false, preview: "find", exec: "find system.profile {}" }, why: "dotted collection names are kept whole, as in mongosh" }) },
    Row { text: "db.orders.aggregate([{$match: {}}, {$limit: 5}])", old: Seen { risk: R, read: true, write: true, safe: false, preview: "aggregate", exec: r#"aggregate orders [{"$match":{}},{"$limit":5}]"# }, new: None },
    Row { text: r#"db.orders.aggregate([{$match: {}}, {$out: "t"}])"#, old: Seen { risk: W, read: false, write: true, safe: false, preview: "aggregate", exec: r#"aggregate orders [{"$match":{}},{"$out":"t"}]"# }, new: None },
    Row { text: r#"db.orders.aggregate([{"$out": "t"}])"#, old: Seen { risk: W, read: false, write: true, safe: false, preview: "aggregate", exec: r#"aggregate orders [{"$out":"t"}]"# }, new: None },
    Row { text: r#"db.orders.aggregate([{'$merge': {into: "t"}}])"#, old: Seen { risk: W, read: false, write: true, safe: false, preview: "aggregate", exec: "err" }, new: Some(Change { seen: Seen { risk: W, read: false, write: true, safe: false, preview: "aggregate", exec: r#"aggregate orders [{"$merge":{"into":"t"}}]"# }, why: "R-17: single-quoted stage keys parse; the write is classed and run as one" }) },
    Row { text: r#"db.orders.aggregate([{`$out`: "t"}])"#, old: Seen { risk: W, read: false, write: true, safe: false, preview: "aggregate", exec: "err" }, new: Some(Change { seen: Seen { risk: W, read: false, write: true, safe: false, preview: "aggregate", exec: r#"aggregate orders [{"$out":"t"}]"# }, why: "R-17: backtick stage keys parse; the write is classed and run as one" }) },
    Row { text: r#"db.orders.aggregate([{"$out": "t"}])"#, old: Seen { risk: W, read: false, write: true, safe: false, preview: "aggregate", exec: r#"aggregate orders [{"$out":"t"}]"# }, new: None },
    Row { text: r#"db.orders.aggregate([{$match: {note: "$out"}}])"#, old: Seen { risk: R, read: true, write: true, safe: false, preview: "aggregate", exec: r#"aggregate orders [{"$match":{"note":"$out"}}]"# }, new: None },
    Row { text: r#"db.orders.aggregate([{$lookup: {from: "x", pipeline: [{$out: "y"}], as: "z"}}])"#, old: Seen { risk: W, read: false, write: true, safe: false, preview: "aggregate", exec: r#"aggregate orders [{"$lookup":{"from":"x","pipeline":[{"$out":"y"}],"as":"z"}}]"# }, new: None },
    Row { text: r#"db.orders.aggregate([{$match: {}}, {$drop: "t"}])"#, old: Seen { risk: X, read: false, write: true, safe: false, preview: "aggregate", exec: r#"aggregate orders [{"$match":{}},{"$drop":"t"}]"# }, new: None },
    Row { text: "db.orders.aggregate([{$match: {}}]).pretty()", old: Seen { risk: R, read: true, write: true, safe: false, preview: "aggregate", exec: r#"aggregate orders [{"$match":{}}]"# }, new: None },
    Row { text: "db.orders.aggregate([{$match: {}}], {allowDiskUse: true})", old: Seen { risk: R, read: true, write: true, safe: false, preview: "aggregate", exec: r#"aggregate orders [{"$match":{}}]"# }, new: None },
    Row { text: "db.orders.insertOne({a: 1})", old: Seen { risk: W, read: false, write: true, safe: true, preview: "insertone", exec: r#"insertOne orders {"a":1}"# }, new: None },
    Row { text: "db.orders.insertMany([{a: 1}, {a: 2}])", old: Seen { risk: W, read: false, write: true, safe: true, preview: "insertmany", exec: r#"insertMany orders [{"a":1},{"a":2}]"# }, new: None },
    Row { text: "db.orders.updateOne({a: 1}, {$set: {b: 2}})", old: Seen { risk: W, read: false, write: true, safe: true, preview: "updateone", exec: r#"updateOne orders {"a":1} {"$set":{"b":2}}"# }, new: None },
    Row { text: "db.orders.updateMany({a: 1}, {$set: {b: 2}}, {upsert: true})", old: Seen { risk: W, read: false, write: true, safe: true, preview: "updatemany", exec: r#"updateMany orders {"a":1} {"$set":{"b":2}}"# }, new: None },
    Row { text: "db.orders.updateMany({}, [{$set: {b: 2}}])", old: Seen { risk: W, read: false, write: true, safe: true, preview: "updatemany", exec: "err" }, new: Some(Change { seen: Seen { risk: W, read: false, write: false, safe: true, preview: "updatemany", exec: "err" }, why: "validate now refuses what the executor cannot run, before the write guard asks to confirm it (update pipelines)" }) },
    Row { text: r#"db.orders.deleteOne({_id: ObjectId("64f123456789abcdef123456")})"#, old: Seen { risk: W, read: false, write: true, safe: false, preview: "deleteone", exec: r#"deleteOne orders {"_id":{"$oid":"64f123456789abcdef123456"}}"# }, new: None },
    Row { text: "db.orders.deleteMany({})", old: Seen { risk: W, read: false, write: true, safe: false, preview: "deletemany", exec: "deleteMany orders {}" }, new: None },
    Row { text: "db.orders.deleteMany()", old: Seen { risk: W, read: false, write: true, safe: false, preview: "deletemany", exec: "err" }, new: Some(Change { seen: Seen { risk: W, read: false, write: false, safe: false, preview: "deletemany", exec: "err" }, why: "validate now refuses what the executor cannot run, before the write guard asks to confirm it (no filter)" }) },
    Row { text: "db.orders.replaceOne({a: 1}, {a: 2})", old: Seen { risk: W, read: false, write: true, safe: true, preview: "replaceone", exec: r#"replaceOne orders {"a":1} {"a":2}"# }, new: None },
    Row { text: r#"db.orders.findOneAndUpdate({a: 1}, {$set: {b: 2}}, {returnDocument: "after"})"#, old: Seen { risk: W, read: false, write: true, safe: false, preview: "findoneandupdate", exec: r#"findOneAndUpdate orders {"a":1} {"$set":{"b":2}}"# }, new: None },
    Row { text: "db.orders.findOneAndDelete({a: 1})", old: Seen { risk: W, read: false, write: true, safe: false, preview: "findoneanddelete", exec: "err" }, new: Some(Change { seen: Seen { risk: W, read: false, write: false, safe: false, preview: "findoneanddelete", exec: "err" }, why: "validate now refuses what the executor cannot run, before the write guard asks to confirm it" }) },
    Row { text: "db.orders.findOneAndReplace({a: 1}, {a: 2})", old: Seen { risk: W, read: false, write: true, safe: false, preview: "findoneandreplace", exec: "err" }, new: Some(Change { seen: Seen { risk: W, read: false, write: false, safe: false, preview: "findoneandreplace", exec: "err" }, why: "validate now refuses what the executor cannot run, before the write guard asks to confirm it" }) },
    Row { text: "db.orders.bulkWrite([{insertOne: {document: {a: 1}}}, {deleteOne: {filter: {a: 1}}}])", old: Seen { risk: W, read: false, write: true, safe: false, preview: "bulkwrite", exec: r#"bulkWrite orders [insertOne {"a":1},deleteOne {"a":1}]"# }, new: None },
    Row { text: "db.orders.insert({a: 1})", old: Seen { risk: W, read: false, write: true, safe: true, preview: "insert", exec: "err" }, new: Some(Change { seen: Seen { risk: W, read: false, write: false, safe: true, preview: "insert", exec: "err" }, why: "validate now refuses what the executor cannot run, before the write guard asks to confirm it (legacy insert)" }) },
    Row { text: "db.orders.update({a: 1}, {$set: {b: 1}})", old: Seen { risk: W, read: false, write: true, safe: true, preview: "update", exec: "err" }, new: Some(Change { seen: Seen { risk: W, read: false, write: false, safe: true, preview: "update", exec: "err" }, why: "validate now refuses what the executor cannot run, before the write guard asks to confirm it (legacy update)" }) },
    Row { text: "db.orders.remove({a: 1})", old: Seen { risk: W, read: false, write: true, safe: false, preview: "remove", exec: "err" }, new: Some(Change { seen: Seen { risk: W, read: false, write: false, safe: false, preview: "remove", exec: "err" }, why: "validate now refuses what the executor cannot run, before the write guard asks to confirm it (legacy remove)" }) },
    Row { text: "db.orders.insertOne({a: 1}).pretty()", old: Seen { risk: W, read: false, write: true, safe: true, preview: "insertone", exec: r#"insertOne orders {"a":1}"# }, new: None },
    Row { text: r#"db.orders.deleteMany({op: "find"})"#, old: Seen { risk: W, read: false, write: true, safe: false, preview: "deletemany", exec: r#"deleteMany orders {"op":"find"}"# }, new: None },
    Row { text: r#"db.orders.deleteMany({"op": "insertOne"})"#, old: Seen { risk: W, read: false, write: true, safe: false, preview: "deletemany", exec: r#"deleteMany orders {"op":"insertOne"}"# }, new: None },
    Row { text: r#"db.orders.find({op: "deleteMany"})"#, old: Seen { risk: R, read: true, write: true, safe: false, preview: "find", exec: r#"find orders {"op":"deleteMany"}"# }, new: None },
    Row { text: r#"db.orders.insertOne({op: "find"})"#, old: Seen { risk: W, read: false, write: true, safe: true, preview: "insertone", exec: r#"insertOne orders {"op":"find"}"# }, new: None },
    Row { text: "db.orders.drop()", old: Seen { risk: X, read: false, write: true, safe: false, preview: "drop", exec: "err" }, new: Some(Change { seen: Seen { risk: X, read: false, write: false, safe: false, preview: "drop", exec: "err" }, why: "validate now refuses what the executor cannot run, before the write guard asks to confirm it" }) },
    Row { text: r#"db.orders.drop({op: "find"})"#, old: Seen { risk: X, read: false, write: true, safe: false, preview: "drop", exec: r#"find drop({op: " {}"# }, new: Some(Change { seen: Seen { risk: X, read: false, write: false, safe: false, preview: "drop", exec: "err" }, why: r#"validate now refuses what the executor cannot run, before the write guard asks to confirm it; the old executor ran a find on a collection named `drop({op: "`"# }) },
    Row { text: "db.dropDatabase()", old: Seen { risk: X, read: false, write: true, safe: false, preview: "dropdatabase", exec: "err" }, new: Some(Change { seen: Seen { risk: X, read: false, write: false, safe: false, preview: "dropdatabase", exec: "err" }, why: "validate now refuses what the executor cannot run, before the write guard asks to confirm it" }) },
    Row { text: "db.orders.createIndex({a: 1})", old: Seen { risk: W, read: false, write: true, safe: false, preview: "createindex", exec: "err" }, new: Some(Change { seen: Seen { risk: W, read: false, write: false, safe: false, preview: "createindex", exec: "err" }, why: "validate now refuses what the executor cannot run, before the write guard asks to confirm it" }) },
    Row { text: r#"db.orders.dropIndex("a_1")"#, old: Seen { risk: W, read: false, write: true, safe: false, preview: "dropindex", exec: "err" }, new: Some(Change { seen: Seen { risk: X, read: false, write: false, safe: false, preview: "dropindex", exec: "err" }, why: "dropping an index is destructive; validate now refuses what the executor cannot run, before the write guard asks to confirm it" }) },
    Row { text: "db.orders.dropIndexes()", old: Seen { risk: W, read: false, write: true, safe: false, preview: "dropindexes", exec: "err" }, new: Some(Change { seen: Seen { risk: X, read: false, write: false, safe: false, preview: "dropindexes", exec: "err" }, why: "dropping indexes is destructive; validate now refuses what the executor cannot run, before the write guard asks to confirm it" }) },
    Row { text: r#"db.orders.renameCollection("orders2")"#, old: Seen { risk: W, read: false, write: true, safe: false, preview: "renamecollection", exec: "err" }, new: Some(Change { seen: Seen { risk: W, read: false, write: false, safe: false, preview: "renamecollection", exec: "err" }, why: "validate now refuses what the executor cannot run, before the write guard asks to confirm it" }) },
    Row { text: r#"db.createCollection("x")"#, old: Seen { risk: W, read: false, write: true, safe: false, preview: "createcollection", exec: "err" }, new: Some(Change { seen: Seen { risk: W, read: false, write: false, safe: false, preview: "createcollection", exec: "err" }, why: "validate now refuses what the executor cannot run, before the write guard asks to confirm it" }) },
    Row { text: r#"db.runCommand({drop: "orders"})"#, old: Seen { risk: W, read: false, write: true, safe: false, preview: "runcommand", exec: "err" }, new: Some(Change { seen: Seen { risk: X, read: false, write: false, safe: false, preview: "runcommand", exec: "err" }, why: "runCommand is classed by its command name; `drop` is destructive; validate now refuses what the executor cannot run, before the write guard asks to confirm it" }) },
    Row { text: "db.runCommand({ping: 1})", old: Seen { risk: W, read: false, write: true, safe: false, preview: "runcommand", exec: "err" }, new: Some(Change { seen: Seen { risk: W, read: false, write: false, safe: false, preview: "runcommand", exec: "err" }, why: "validate now refuses what the executor cannot run, before the write guard asks to confirm it" }) },
    Row { text: "db.adminCommand({shutdown: 1})", old: Seen { risk: W, read: false, write: true, safe: false, preview: "admincommand", exec: "err" }, new: Some(Change { seen: Seen { risk: X, read: false, write: false, safe: false, preview: "admincommand", exec: "err" }, why: "`shutdown` is destructive; validate now refuses what the executor cannot run, before the write guard asks to confirm it" }) },
    Row { text: "db.getCollectionNames()", old: Seen { risk: W, read: false, write: true, safe: false, preview: "getcollectionnames", exec: "err" }, new: Some(Change { seen: Seen { risk: W, read: false, write: false, safe: false, preview: "getcollectionnames", exec: "err" }, why: "validate now refuses what the executor cannot run, before the write guard asks to confirm it" }) },
    Row { text: "db.orders.getIndexes()", old: Seen { risk: R, read: true, write: true, safe: false, preview: "getindexes", exec: "err" }, new: Some(Change { seen: Seen { risk: R, read: false, write: false, safe: false, preview: "getindexes", exec: "err" }, why: "validate now refuses what the executor cannot run, before the write guard asks to confirm it" }) },
    Row { text: "db.orders.stats()", old: Seen { risk: R, read: true, write: true, safe: false, preview: "stats", exec: "err" }, new: Some(Change { seen: Seen { risk: R, read: false, write: false, safe: false, preview: "stats", exec: "err" }, why: "validate now refuses what the executor cannot run, before the write guard asks to confirm it" }) },
    Row { text: r#"db.getSiblingDB("other").orders.find({})"#, old: Seen { risk: R, read: true, write: true, safe: false, preview: "find", exec: "find orders {}" }, new: Some(Change { seen: Seen { risk: R, read: true, write: true, safe: false, preview: "find", exec: "find other/orders {}" }, why: "getSiblingDB is honoured; the old executor read the session database" }) },
    Row { text: r#"db.getSiblingDB("other").orders.deleteMany({})"#, old: Seen { risk: W, read: false, write: true, safe: false, preview: "deletemany", exec: "err" }, new: Some(Change { seen: Seen { risk: W, read: false, write: true, safe: false, preview: "deletemany", exec: "deleteMany other/orders {}" }, why: "getSiblingDB is honoured; the write runs on `other`" }) },
    Row { text: r#"db.getSiblingDB("other").dropDatabase()"#, old: Seen { risk: X, read: false, write: true, safe: false, preview: "dropdatabase", exec: "err" }, new: Some(Change { seen: Seen { risk: X, read: false, write: false, safe: false, preview: "dropdatabase", exec: "err" }, why: "validate now refuses what the executor cannot run, before the write guard asks to confirm it" }) },
    Row { text: r#"db.getCollection("orders").find({a: 1})"#, old: Seen { risk: R, read: true, write: true, safe: false, preview: "find", exec: r#"find getCollection("orders") {"a":1}"# }, new: Some(Change { seen: Seen { risk: R, read: true, write: true, safe: false, preview: "find", exec: r#"find orders {"a":1}"# }, why: r#"getCollection names the collection; the old executor used the text `getCollection("orders")`"# }) },
    Row { text: r#"db.getCollection("orders").deleteMany({a: 1})"#, old: Seen { risk: W, read: false, write: true, safe: false, preview: "deletemany", exec: "err" }, new: Some(Change { seen: Seen { risk: W, read: false, write: true, safe: false, preview: "deletemany", exec: r#"deleteMany orders {"a":1}"# }, why: "getCollection names the collection" }) },
    Row { text: r#"db["order-items"].find({})"#, old: Seen { risk: R, read: true, write: true, safe: false, preview: "find", exec: "find - {}" }, new: Some(Change { seen: Seen { risk: R, read: true, write: true, safe: false, preview: "find", exec: "find order-items {}" }, why: r#"`db["…"]` names the collection; the old executor used the Collection field"# }) },
    Row { text: "db['order-items'].deleteMany({a: 1})", old: Seen { risk: W, read: false, write: true, safe: false, preview: "deletemany", exec: r#"deleteMany - {"a":1}"# }, new: Some(Change { seen: Seen { risk: W, read: false, write: true, safe: false, preview: "deletemany", exec: r#"deleteMany order-items {"a":1}"# }, why: "`db['…']` names the collection; the old executor deleted from the Collection field" }) },
    Row { text: r#"db.getMongo().getDB("other").orders.find({})"#, old: Seen { risk: R, read: true, write: true, safe: false, preview: "find", exec: "find orders {}" }, new: Some(Change { seen: Seen { risk: R, read: true, write: true, safe: false, preview: "find", exec: "find other/orders {}" }, why: "getMongo().getDB is honoured" }) },
    Row { text: r#"db.Zürich.find({city: "Zürich"})"#, old: Seen { risk: R, read: true, write: true, safe: false, preview: "find", exec: r#"find Zürich {"city":"Zürich"}"# }, new: None },
    Row { text: "// comment\ndb.orders.find({})", old: Seen { risk: R, read: true, write: true, safe: false, preview: "find", exec: "find orders {}" }, new: None },
    Row { text: "db.orders.find({}) // trailing comment", old: Seen { risk: R, read: true, write: true, safe: false, preview: "find", exec: "err" }, new: Some(Change { seen: Seen { risk: R, read: true, write: true, safe: false, preview: "find", exec: "find orders {}" }, why: "comments are dropped by the lexer" }) },
    Row { text: "/* block */ db.orders.deleteMany({})", old: Seen { risk: W, read: false, write: true, safe: false, preview: "deletemany", exec: "deleteMany orders {}" }, new: None },
    Row { text: "db.orders.find({a: 1 /* inline */})", old: Seen { risk: R, read: true, write: true, safe: false, preview: "find", exec: "err" }, new: Some(Change { seen: Seen { risk: R, read: true, write: true, safe: false, preview: "find", exec: r#"find orders {"a":1}"# }, why: "comments are dropped by the lexer" }) },
    Row { text: "db.orders.find({});", old: Seen { risk: R, read: true, write: true, safe: false, preview: "find", exec: "find orders {}" }, new: None },
    Row { text: "db.orders.find({});;  ", old: Seen { risk: R, read: true, write: true, safe: false, preview: "find", exec: "find orders {}" }, new: None },
    Row { text: "db.orders.find({}); db.orders.deleteMany({})", old: Seen { risk: W, read: false, write: true, safe: false, preview: "deletemany", exec: "err" }, new: Some(Change { seen: Seen { risk: W, read: false, write: false, safe: false, preview: "db.orders.find({});db.orders.deleteMany({})", exec: "err" }, why: "more than one statement does not parse: still a write, now refused by validate, preview shows the text" }) },
    Row { text: "db.orders.insertOne({a: 1}); db.orders.deleteMany({})", old: Seen { risk: W, read: false, write: true, safe: false, preview: "deletemany", exec: "err" }, new: Some(Change { seen: Seen { risk: W, read: false, write: false, safe: false, preview: "db.orders.insertOne({a:1});db.orders.deleteMany({})", exec: "err" }, why: "more than one statement does not parse: still a write, now refused by validate, preview shows the text" }) },
    Row { text: "  db . orders . find ( { a : 1 } )  ", old: Seen { risk: W, read: false, write: false, safe: false, preview: "db.orders.find({a:1})", exec: r#"find orders {"a":1}"# }, new: Some(Change { seen: Seen { risk: R, read: true, write: true, safe: false, preview: "find", exec: r#"find orders {"a":1}"# }, why: "the policy reads the same tokens as the executor; this is the find it runs" }) },
    Row { text: "db.orders\n  .find({a: 1})\n  .limit(2)", old: Seen { risk: R, read: true, write: true, safe: false, preview: "find", exec: r#"find orders {"a":1} l=2"# }, new: None },
    Row { text: r#"db.orders.find({note: "a) b; c"})"#, old: Seen { risk: R, read: true, write: true, safe: false, preview: "find", exec: r#"find orders {"note":"a) b; c"}"# }, new: None },
    Row { text: r#"db.orders.find({note: 'it\'s ) here'})"#, old: Seen { risk: R, read: true, write: true, safe: false, preview: "find", exec: "err" }, new: Some(Change { seen: Seen { risk: R, read: true, write: true, safe: false, preview: "find", exec: r#"find orders {"note":"it's ) here"}"# }, why: "single-quoted strings parse" }) },
    Row { text: r#"db.orders.find({note: ".deleteMany("})"#, old: Seen { risk: R, read: true, write: true, safe: false, preview: "find", exec: r#"find orders {"note":".deleteMany("}"# }, new: None },
    Row { text: r#"db.orders.find({note: "// not a comment"})"#, old: Seen { risk: R, read: true, write: true, safe: false, preview: "find", exec: r#"find orders {"note":"// not a comment"}"# }, new: None },
    Row { text: r#"db.orders.find({_id: ObjectId("64f123456789abcdef123456"), n: NumberLong(5), i: NumberInt("7")})"#, old: Seen { risk: R, read: true, write: true, safe: false, preview: "find", exec: r#"find orders {"_id":{"$oid":"64f123456789abcdef123456"},"n":5,"i":7}"# }, new: None },
    Row { text: r#"db.orders.find({at: ISODate("2024-01-01T00:00:00Z")})"#, old: Seen { risk: R, read: true, write: true, safe: false, preview: "find", exec: r#"find orders {"at":{"$date":"2024-01-01T00:00:00Z"}}"# }, new: None },
    Row { text: r#"db.orders.find({at: {$gt: new Date("2024-01-01T00:00:00Z")}})"#, old: Seen { risk: R, read: true, write: true, safe: false, preview: "find", exec: "err" }, new: Some(Change { seen: Seen { risk: R, read: true, write: true, safe: false, preview: "find", exec: r#"find orders {"at":{"$gt":{"$date":"2024-01-01T00:00:00Z"}}}"# }, why: "`new Date(…)` parses" }) },
    Row { text: r#"db.orders.find({_id: ObjectId("nothex")})"#, old: Seen { risk: R, read: true, write: true, safe: false, preview: "find", exec: "err" }, new: Some(Change { seen: Seen { risk: R, read: false, write: false, safe: false, preview: "find", exec: "err" }, why: "values are converted at validate, so a bad ObjectId fails there" }) },
    Row { text: "db.orders.find({a: 1,})", old: Seen { risk: R, read: true, write: true, safe: false, preview: "find", exec: r#"find orders {"a":1}"# }, new: None },
    Row { text: r#"{"status": "open"}"#, old: Seen { risk: R, read: true, write: true, safe: false, preview: r#"{"status":"open"}"#, exec: r#"find - {"status":"open"}"# }, new: None },
    Row { text: r#"{status: "open"}"#, old: Seen { risk: R, read: true, write: true, safe: false, preview: r#"{status:"open"}"#, exec: r#"find - {"status":"open"}"# }, new: None },
    Row { text: "{}", old: Seen { risk: R, read: true, write: true, safe: false, preview: "{}", exec: "find - {}" }, new: None },
    Row { text: r#"{"status": "open"};"#, old: Seen { risk: R, read: true, write: true, safe: false, preview: r#"{"status":"open"};"#, exec: r#"find - {"status":"open"}"# }, new: None },
    Row { text: r#"[{"$match": {"a": 1}}, {"$merge": "t"}]"#, old: Seen { risk: W, read: false, write: true, safe: false, preview: r#"[{"$match":{"a":1}},{"$merge":"t"}]"#, exec: r#"aggregate - [{"$match":{"a":1}},{"$merge":"t"}]"# }, new: None },
    Row { text: r#"[{"$match": {"a": 1}}]"#, old: Seen { risk: R, read: true, write: true, safe: false, preview: r#"[{"$match":{"a":1}}]"#, exec: r#"aggregate - [{"$match":{"a":1}}]"# }, new: None },
    Row { text: r#"{"op": "deleteMany", "filter": {"a": 1}}"#, old: Seen { risk: W, read: false, write: true, safe: false, preview: "deletemany", exec: r#"deleteMany - {"a":1}"# }, new: None },
    Row { text: r#"{"op": "insertOne", "doc": {"a": 1}}"#, old: Seen { risk: W, read: false, write: true, safe: true, preview: "insertone", exec: r#"insertOne - {"a":1}"# }, new: None },
    Row { text: r#"{"op": "countDocuments", "filter": {"a": 1}}"#, old: Seen { risk: R, read: true, write: true, safe: false, preview: "countdocuments", exec: r#"count - {"a":1}"# }, new: None },
    Row { text: r#"{"op": "find", "filter": {"op": "deleteMany"}}"#, old: Seen { risk: R, read: true, write: true, safe: false, preview: "find", exec: "err" }, new: Some(Change { seen: Seen { risk: R, read: false, write: false, safe: false, preview: "find", exec: "err" }, why: "validate now refuses what the executor cannot run, before the write guard asks to confirm it (JSON `find` op)" }) },
    Row { text: r#"{"op": "deleteMany", "filter": {"op": "find"}}"#, old: Seen { risk: W, read: false, write: true, safe: false, preview: "deletemany", exec: r#"deleteMany - {"op":"find"}"# }, new: None },
    Row { text: r#"{"op": "find", "op": "deleteMany"}"#, old: Seen { risk: W, read: false, write: true, safe: false, preview: "deletemany", exec: "err" }, new: Some(Change { seen: Seen { risk: W, read: false, write: false, safe: false, preview: "deletemany", exec: "err" }, why: "more than one `op` is refused; the risk is still the worst one" }) },
    Row { text: r#"{"op": "deleteMany", "op": "insertOne"}"#, old: Seen { risk: W, read: false, write: true, safe: false, preview: "insertone", exec: "err" }, new: Some(Change { seen: Seen { risk: W, read: false, write: false, safe: false, preview: "insertone", exec: "err" }, why: "more than one `op` is refused; the old executor ran the last one" }) },
    Row { text: r#"{"op": "drop"}"#, old: Seen { risk: X, read: false, write: true, safe: false, preview: "drop", exec: "err" }, new: Some(Change { seen: Seen { risk: X, read: false, write: false, safe: false, preview: "drop", exec: "err" }, why: "validate now refuses what the executor cannot run, before the write guard asks to confirm it" }) },
    Row { text: r#"{"op": "frobnicate"}"#, old: Seen { risk: W, read: false, write: true, safe: false, preview: "frobnicate", exec: "err" }, new: Some(Change { seen: Seen { risk: W, read: false, write: false, safe: false, preview: "frobnicate", exec: "err" }, why: "validate now refuses what the executor cannot run, before the write guard asks to confirm it" }) },
    Row { text: r#"{"op": 5}"#, old: Seen { risk: W, read: false, write: true, safe: false, preview: r#"{"op":5}"#, exec: "err" }, new: Some(Change { seen: Seen { risk: W, read: false, write: false, safe: false, preview: r#"{"op":5}"#, exec: "err" }, why: "validate now refuses what the executor cannot run, before the write guard asks to confirm it" }) },
    Row { text: r#"{op: "deleteMany", filter: {note: "delete"}}"#, old: Seen { risk: W, read: false, write: true, safe: false, preview: "deletemany", exec: r#"deleteMany - {"note":"delete"}"# }, new: None },
    Row { text: r#"{"op": "aggregate", "pipeline": [{"$out": "t"}]}"#, old: Seen { risk: W, read: false, write: true, safe: false, preview: "aggregate", exec: r#"aggregate - [{"$out":"t"}]"# }, new: None },
    Row { text: r#"{"op": "find", "pipeline": [{"$merge": "t"}]}"#, old: Seen { risk: W, read: false, write: true, safe: false, preview: "find", exec: "err" }, new: Some(Change { seen: Seen { risk: W, read: false, write: false, safe: false, preview: "find", exec: "err" }, why: "validate now refuses what the executor cannot run, before the write guard asks to confirm it (JSON `find` op)" }) },
    Row { text: r#"{"op": "bulkWrite", "ops": [{"insertOne": {"document": {"a": 1}}}]}"#, old: Seen { risk: W, read: false, write: true, safe: false, preview: "bulkwrite", exec: r#"bulkWrite - [insertOne {"a":1}]"# }, new: None },
    Row { text: r#"{"op": "insertOne", "doc": {"note": "please delete this"}}"#, old: Seen { risk: W, read: false, write: true, safe: true, preview: "insertone", exec: r#"insertOne - {"note":"please delete this"}"# }, new: None },
    Row { text: "listDatabases", old: Seen { risk: R, read: true, write: true, safe: false, preview: "listDatabases", exec: "list dbs" }, new: None },
    Row { text: "show dbs", old: Seen { risk: R, read: true, write: true, safe: false, preview: "showdbs", exec: "list dbs" }, new: None },
    Row { text: "show  collections", old: Seen { risk: R, read: true, write: true, safe: false, preview: "showcollections", exec: "list colls" }, new: None },
    Row { text: "listCollections", old: Seen { risk: R, read: true, write: true, safe: false, preview: "listCollections", exec: "list colls" }, new: None },
    Row { text: "listdatabase", old: Seen { risk: R, read: true, write: true, safe: false, preview: "listdatabase", exec: "err" }, new: Some(Change { seen: Seen { risk: W, read: false, write: false, safe: false, preview: "listdatabase", exec: "err" }, why: "not a listing the executor knows; text that does not parse is a write" }) },
    Row { text: "", old: Seen { risk: W, read: false, write: false, safe: false, preview: "write", exec: "err" }, new: None },
    Row { text: "hello", old: Seen { risk: W, read: false, write: false, safe: false, preview: "hello", exec: "err" }, new: None },
    Row { text: "db.orders.find({a: 1}", old: Seen { risk: R, read: true, write: true, safe: false, preview: "find", exec: "err" }, new: Some(Change { seen: Seen { risk: W, read: false, write: false, safe: false, preview: "db.orders.find({a:1}", exec: "err" }, why: "text that does not parse is a write" }) },
    Row { text: r#"db.orders.find({a: "unterminated})"#, old: Seen { risk: R, read: true, write: true, safe: false, preview: "find", exec: "err" }, new: Some(Change { seen: Seen { risk: W, read: false, write: false, safe: false, preview: r#"db.orders.find({a:"unterminated})"#, exec: "err" }, why: "text that does not parse is a write" }) },
];

fn json(doc: &Document) -> String {
    Bson::Document(doc.clone())
        .into_relaxed_extjson()
        .to_string()
}

fn docs(items: &[Document]) -> String {
    format!("[{}]", items.iter().map(json).collect::<Vec<_>>().join(","))
}

fn target(t: &Target) -> String {
    let coll = t.collection.as_deref().unwrap_or("-");
    match t.database.as_deref() {
        Some(db) => format!("{db}/{coll}"),
        None => coll.to_string(),
    }
}

/// The executor's parse, summarised. `err` when it refuses the text.
fn exec(text: &str) -> String {
    let call = match parse_executable(text) {
        Err(_) => return "err".into(),
        Ok(Executable::Listing(Listing::Databases)) => return "list dbs".into(),
        Ok(Executable::Listing(Listing::Collections)) => return "list colls".into(),
        Ok(Executable::Call(call)) => call,
    };
    let at = target(&call.target);
    match call.op {
        MongoOp::Find(p) => {
            let mut s = format!("find {at} {}", json(&p.filter));
            if let Some(d) = &p.projection {
                s.push_str(&format!(" p={}", json(d)));
            }
            if let Some(d) = &p.sort {
                s.push_str(&format!(" s={}", json(d)));
            }
            if let Some(n) = p.limit {
                s.push_str(&format!(" l={n}"));
            }
            if let Some(n) = p.skip {
                s.push_str(&format!(" k={n}"));
            }
            s
        }
        MongoOp::Aggregate(pipeline) => format!("aggregate {at} {}", docs(&pipeline)),
        MongoOp::Count(filter) => format!("count {at} {}", json(&filter)),
        MongoOp::Distinct { field, filter } => format!("distinct {at} {field} {}", json(&filter)),
        MongoOp::Write(w) => match w {
            ShellWrite::InsertOne(d) => format!("insertOne {at} {}", json(&d)),
            ShellWrite::InsertMany(d) => format!("insertMany {at} {}", docs(&d)),
            ShellWrite::UpdateOne { filter, update } => {
                format!("updateOne {at} {} {}", json(&filter), json(&update))
            }
            ShellWrite::UpdateMany { filter, update } => {
                format!("updateMany {at} {} {}", json(&filter), json(&update))
            }
            ShellWrite::ReplaceOne {
                filter,
                replacement,
            } => format!("replaceOne {at} {} {}", json(&filter), json(&replacement)),
            ShellWrite::DeleteOne(d) => format!("deleteOne {at} {}", json(&d)),
            ShellWrite::DeleteMany(d) => format!("deleteMany {at} {}", json(&d)),
            ShellWrite::FindOneAndUpdate { filter, update } => {
                format!("findOneAndUpdate {at} {} {}", json(&filter), json(&update))
            }
            ShellWrite::Bulk(models) => format!(
                "bulkWrite {at} [{}]",
                models
                    .iter()
                    .map(|m| match m {
                        BulkModel::InsertOne(d) => format!("insertOne {}", json(d)),
                        BulkModel::UpdateOne { filter, update } => {
                            format!("updateOne {} {}", json(filter), json(update))
                        }
                        BulkModel::UpdateMany { filter, update } => {
                            format!("updateMany {} {}", json(filter), json(update))
                        }
                        BulkModel::ReplaceOne {
                            filter,
                            replacement,
                        } => format!("replaceOne {} {}", json(filter), json(replacement)),
                        BulkModel::DeleteOne(d) => format!("deleteOne {}", json(d)),
                        BulkModel::DeleteMany(d) => format!("deleteMany {}", json(d)),
                    })
                    .collect::<Vec<_>>()
                    .join(",")
            ),
        },
    }
}

#[derive(Debug, PartialEq)]
struct Got {
    risk: WriteRisk,
    read: bool,
    write: bool,
    safe: bool,
    preview: String,
    exec: String,
}

fn run(text: &str) -> Got {
    let policy = MongoQueryPolicy;
    Got {
        risk: policy.classify(text),
        read: policy.validate(text, QueryMode::Read).is_ok(),
        write: policy.validate(text, QueryMode::Write).is_ok(),
        safe: policy.is_safe_dml(text),
        preview: policy.op_preview(text),
        exec: exec(text),
    }
}

fn want(seen: &Seen) -> Got {
    Got {
        risk: seen.risk,
        read: seen.read,
        write: seen.write,
        safe: seen.safe,
        preview: seen.preview.to_string(),
        exec: seen.exec.to_string(),
    }
}

#[test]
fn corpus_matches_the_shared_parser() {
    let mut failures = Vec::new();
    for row in CORPUS {
        let expected = want(row.new.as_ref().map_or(&row.old, |c| &c.seen));
        let got = run(row.text);
        if got != expected {
            failures.push(format!(
                "{:?}\n  got  {got:?}\n  want {expected:?}",
                row.text
            ));
        }
    }
    assert!(failures.is_empty(), "\n{}", failures.join("\n"));
}

/// A changed row says why, and differs from the old row in some way.
#[test]
fn every_change_is_explained() {
    for row in CORPUS {
        if let Some(change) = &row.new {
            assert!(!change.why.trim().is_empty(), "{:?}", row.text);
            assert_ne!(want(&change.seen), want(&row.old), "{:?}", row.text);
        }
    }
}

/// Risk only drops where the executor runs the text as a read.
#[test]
fn no_row_loses_risk_unless_it_runs_as_a_read() {
    for row in CORPUS {
        let new = row.new.as_ref().map_or(&row.old, |c| &c.seen);
        if new.risk < row.old.risk {
            let read_call = ["find ", "aggregate ", "count ", "distinct ", "list "]
                .iter()
                .any(|p| new.exec.starts_with(p));
            assert!(
                read_call,
                "{:?} lost risk but runs as {}",
                row.text, new.exec
            );
        }
    }
}

/// Whatever the executor runs as a write is classed as at least a write, and
/// Read mode refuses it.
#[test]
fn executed_writes_are_never_reads() {
    let writes = [
        "insertOne ",
        "insertMany ",
        "updateOne ",
        "updateMany ",
        "replaceOne ",
        "deleteOne ",
        "deleteMany ",
        "findOneAndUpdate ",
        "bulkWrite ",
    ];
    for row in CORPUS {
        let new = row.new.as_ref().map_or(&row.old, |c| &c.seen);
        // A stage key, not a string value that says `$out`.
        let stage = new.exec.contains("\"$out\":") || new.exec.contains("\"$merge\":");
        if writes.iter().any(|w| new.exec.starts_with(w)) || stage {
            assert!(new.risk >= W, "{:?}", row.text);
            assert!(!new.read, "{:?}", row.text);
        }
    }
}

const ALLOW: bool = true;
const DENY: bool = false;
const SAFE: bool = true;
const UNSAFE: bool = false;

/// Bypass probes moved from connector-api `tests/classifier_corpus.rs`.
/// (text, risk, Read mode, `safe_write`). See R-15 and R-17.
#[rustfmt::skip]
const PROBES: &[(&str, WriteRisk, bool, bool)] = &[
    // R-15: an `op` key inside a filter never overrides the method.
    (r#"db.t.deleteMany({op: "find"})"#, W, DENY, UNSAFE),
    (r#"db.t.deleteMany({"op": "find"})"#, W, DENY, UNSAFE),
    (r#"db.t.deleteMany({op: "insertOne"})"#, W, DENY, UNSAFE),
    (r#"db.t.deleteOne({op: "updateOne"})"#, W, DENY, UNSAFE),
    (r#"db.t.drop({op: "find"})"#, X, DENY, UNSAFE),
    (r#"db.t.find({op: "deleteMany"})"#, R, ALLOW, UNSAFE),
    (r#"db.t.insertOne({op: "find"})"#, W, DENY, SAFE),
    (r#"{"op":"find","op":"deleteMany"}"#, W, DENY, UNSAFE),
    (r#"{"op":"insertOne","op":"deleteMany"}"#, W, DENY, UNSAFE),
    (r#"{"OP":"deleteMany","filter":{}}"#, W, DENY, UNSAFE),
    (r#"{"op":5}"#, W, DENY, UNSAFE),
    // A JSON `find` op is a read, but the executor has no JSON find, so
    // validate refuses it in every mode.
    (r#"{"op":"find","filter":{"op":"deleteMany"}}"#, R, DENY, UNSAFE),
    (r#"{"op":"deleteMany","filter":{"op":"find"}}"#, W, DENY, UNSAFE),
    (r#"{"op":"insertOne","doc":{"op":"deleteMany"}}"#, W, DENY, SAFE),
    (r#"{"op":"deleteMany"}"#, W, DENY, UNSAFE),
    (r#"{ op: "deleteMany", filter: { note: "delete" } }"#, W, DENY, UNSAFE),

    // R-17: `$out` / `$merge` stage keys, quoted or not, are writes.
    (r#"db.c.aggregate([{"$out": "t"}])"#, W, DENY, UNSAFE),
    (r#"db.c.aggregate([{'$merge': {into: "t"}}])"#, W, DENY, UNSAFE),
    (r#"db.c.aggregate([{`$out`: "t"}])"#, W, DENY, UNSAFE),
    (r#"db.c.aggregate([{"$merge": "t"}])"#, W, DENY, UNSAFE),
    (r#"db.c.aggregate([{$match: {}}, {"$merge": "t"}])"#, W, DENY, UNSAFE),
    (r#"db.c.aggregate([{$out: "t"}])"#, W, DENY, UNSAFE),
    (r#"db.c.aggregate([{$facet: {a: [{$merge: "t"}]}}])"#, W, DENY, UNSAFE),
    (r#"{"op":"aggregate","pipeline":[{"$out":"t"}]}"#, W, DENY, UNSAFE),
    (r#"{"op":"find","pipeline":[{"$merge":"t"}]}"#, W, DENY, UNSAFE),
    (r#"[{"$match":{"a":1}},{"$merge":"t"}]"#, W, DENY, UNSAFE),
    (r#"db.c.aggregate([{$match: {note: "$out"}}])"#, R, ALLOW, UNSAFE),
    (r#"db.c.aggregate([{$match: {"note": "$merge"}}])"#, R, ALLOW, UNSAFE),
    (r#"db.c.insertOne({"$out": 1})"#, W, DENY, UNSAFE),

    // Ordinary calls.
    (r#"db.t.find({name: "please delete this"})"#, R, ALLOW, UNSAFE),
    (r#"db.t.find({name: ".deleteMany("})"#, R, ALLOW, UNSAFE),
    (r#"db.orders.aggregate([{ $match: {} }, { $limit: 5 }])"#, R, ALLOW, UNSAFE),
    (r#"{"name":"please delete this"}"#, R, ALLOW, UNSAFE),
    (r#"{ name: "please delete this" }"#, R, ALLOW, UNSAFE),
    (r#"{"op":"countDocuments","filter":{"a":1}}"#, R, ALLOW, UNSAFE),
    (r#"db.orders.countDocuments({ status: "open" })"#, R, ALLOW, UNSAFE),
    (r#"db.orders.distinct("city", { a: 1 })"#, R, ALLOW, UNSAFE),
    ("listDatabases", R, ALLOW, UNSAFE),
    ("show collections", R, ALLOW, UNSAFE),
    (r#"db.t.insertOne({a: 1})"#, W, DENY, SAFE),
    (r#"db.t.updateOne({a: 1}, {$set: {b: 2}})"#, W, DENY, SAFE),
    (r#"db.orders.insertMany([{a:1},{a:2}])"#, W, DENY, SAFE),
    (r#"db.orders.updateMany({a:1}, {$set:{b:2}})"#, W, DENY, SAFE),
    (r#"db.orders.replaceOne({a:1}, {a:2})"#, W, DENY, SAFE),
    (r#"{"op":"insertOne","doc":{"note":"please delete this"}}"#, W, DENY, SAFE),
    (r#"db.orders.findOneAndUpdate({a:1}, {$set:{b:2}})"#, W, DENY, UNSAFE),
    (r#"db.t.deleteOne({a: 1})"#, W, DENY, UNSAFE),
    (r#"db.t.bulkWrite([{insertOne: {document: {a: 1}}}])"#, W, DENY, UNSAFE),
    (r#"{"op":"bulkWrite","ops":[{"insertOne":{"document":{"a":1}}}]}"#, W, DENY, UNSAFE),
    (r#"{"op":"deleteMany","filter":{"note":"do not delete this"}}"#, W, DENY, UNSAFE),
    ("db.t.drop()", X, DENY, UNSAFE),
    ("db.dropDatabase()", X, DENY, UNSAFE),
    (r#"{"op":"drop"}"#, X, DENY, UNSAFE),
    (r#"{"op":"dropDatabase"}"#, X, DENY, UNSAFE),
    (r#"db.c.aggregate([{$match:{}},{$drop: "t"}])"#, X, DENY, UNSAFE),
    (r#"db.orders.renameCollection("o2", true)"#, X, DENY, UNSAFE),
    (r#"db.adminCommand({renameCollection: "a.b", to: "a.c", dropTarget: true})"#, X, DENY, UNSAFE),
    (r#"db.orders.find({}).deleteMany({})"#, W, DENY, UNSAFE),
    (r#"db.orders.find({}).frobnicate()"#, W, DENY, UNSAFE),
    (r#"{"op":"frobnicate"}"#, W, DENY, UNSAFE),
    // More than one statement: the worst one, and safe only when all are.
    ("db.orders.find({})\ndb.orders.drop()", X, DENY, UNSAFE),
    ("db.orders.find({});\n{\"op\":\"dropDatabase\"}", X, DENY, UNSAFE),
    ("db.a.find({})\ndb.a.insertOne({a: 1})", W, DENY, UNSAFE),
    (
        "db.a.insertOne({a: 1}); db.a.updateOne({}, {$set: {b: 1}})",
        W,
        DENY,
        SAFE,
    ),
    ("db.a.insertOne({a: 1})\ndb.a.frobnicate()", W, DENY, UNSAFE),
];

#[test]
fn bypass_probes() {
    let policy = MongoQueryPolicy;
    let mut failures = Vec::new();
    for &(text, risk, read_ok, safe) in PROBES {
        let got = policy.classify(text);
        if got != risk {
            failures.push(format!("{text}: risk {got:?}, want {risk:?}"));
        }
        let read = policy.validate(text, QueryMode::Read);
        if read.is_ok() != read_ok {
            failures.push(format!(
                "{text}: Read mode ok={}, want {read_ok}",
                read.is_ok()
            ));
        }
        if policy.is_safe_dml(text) != safe {
            failures.push(format!("{text}: safe_write {}, want {safe}", !safe));
        }
    }
    assert!(failures.is_empty(), "\n{}", failures.join("\n"));
}

#[test]
fn read_mode_refuses_writes_with_the_policy_message() {
    let policy = MongoQueryPolicy;
    let err = match policy.validate("db.t.deleteMany({})", QueryMode::Read) {
        Err(err) => err.to_string(),
        Ok(()) => panic!("Read mode ran a delete"),
    };
    assert!(err.contains("read-only"), "{err}");
    assert!(policy
        .validate(r#"{"op":"deleteOne","filter":{"_id":1}}"#, QueryMode::Write)
        .is_ok());
}

#[test]
fn unsupported_methods_fail_at_validate_with_the_executor_message() {
    let policy = MongoQueryPolicy;
    let err = match policy.validate("db.orders.drop()", QueryMode::Write) {
        Err(err) => err.to_string(),
        Ok(()) => panic!("drop is not run by the executor"),
    };
    assert!(err.contains("drop"), "{err}");
    assert_eq!(policy.classify("db.orders.drop()"), X);
}

#[test]
fn preview_names_the_operation() {
    let policy = MongoQueryPolicy;
    assert_eq!(
        policy.op_preview(r#"db.orders.bulkWrite([{insertOne:{document:{a:1}}}])"#),
        "bulkwrite"
    );
    assert_eq!(
        policy.op_preview(r#"{"op":"deleteMany","filter":{}}"#),
        "deletemany"
    );
    assert_eq!(policy.op_preview(""), "write");
}

mod parse {
    use mongodb::bson::Bson;

    use super::super::parse::{
        parse, parse_executable, Executable, MongoCall, MongoOp, ShellWrite, Statement,
    };
    use super::super::value::parse_document;

    fn call(text: &str) -> MongoCall {
        match parse_executable(text) {
            Ok(Executable::Call(call)) => call,
            other => panic!("{text}: {other:?}"),
        }
    }

    #[test]
    fn find_filter_projection_sort() {
        let q = r#"db.mail_notifications.find({ email: "ajaybhatt17@gmail.com" },{ title: 1, content: 1, variables: 1, is_sent: 1, createdAt: 1, sent_at: 1 }).sort({ createdAt: -1 });"#;
        let c = call(q);
        assert_eq!(c.target.collection.as_deref(), Some("mail_notifications"));
        let MongoOp::Find(p) = c.op else {
            panic!("find");
        };
        assert_eq!(
            p.filter.get_str("email").ok(),
            Some("ajaybhatt17@gmail.com")
        );
        let proj = p.projection.unwrap_or_default();
        assert_eq!(proj.get_i32("title").ok(), Some(1));
        let sort = p.sort.unwrap_or_default();
        assert_eq!(sort.get_i32("createdAt").ok(), Some(-1));
    }

    #[test]
    fn sort_keeps_key_order() {
        let MongoOp::Find(p) = call("db.t.find().sort({b: 1, a: -1})").op else {
            panic!("find");
        };
        let keys: Vec<_> = p.sort.unwrap_or_default().keys().cloned().collect();
        assert_eq!(keys, ["b", "a"]);
    }

    #[test]
    fn non_ascii_collection_and_values() {
        let c = call(r#"db.Zürich.aggregate([{$match:{city: "Zürich"}}])"#);
        assert_eq!(c.target.collection.as_deref(), Some("Zürich"));
    }

    #[test]
    fn helpers_become_bson_types() {
        let doc = match parse_document(
            r#"{ _id: ObjectId("64f123456789abcdef123456"), n: NumberLong("5"), i: NumberInt(7), at: ISODate("2024-01-01T00:00:00Z"), big: 3000000000, f: 1.5, s: 'x' }"#,
        ) {
            Ok(doc) => doc,
            Err(err) => panic!("{err}"),
        };
        assert!(matches!(doc.get("_id"), Some(Bson::ObjectId(_))));
        assert_eq!(doc.get("n"), Some(&Bson::Int64(5)));
        assert_eq!(doc.get("i"), Some(&Bson::Int32(7)));
        assert!(matches!(doc.get("at"), Some(Bson::DateTime(_))));
        assert_eq!(doc.get("big"), Some(&Bson::Int64(3_000_000_000)));
        assert_eq!(doc.get("f"), Some(&Bson::Double(1.5)));
        assert_eq!(doc.get("s"), Some(&Bson::String("x".into())));
    }

    #[test]
    fn extended_json_still_converts() {
        let doc = match parse_document(
            r#"{"_id": {"$oid": "64f123456789abcdef123456"}, "n": {"$numberLong": "9"}}"#,
        ) {
            Ok(doc) => doc,
            Err(err) => panic!("{err}"),
        };
        assert!(matches!(doc.get("_id"), Some(Bson::ObjectId(_))));
        assert_eq!(doc.get("n"), Some(&Bson::Int64(9)));
    }

    #[test]
    fn strings_keep_escapes_and_syntax_characters() {
        let MongoOp::Find(p) = call(r#"db.t.find({a: "x\"); db.t.drop(", b: "é"})"#).op else {
            panic!("find");
        };
        assert_eq!(p.filter.get_str("a").ok(), Some(r#"x"); db.t.drop("#));
        assert_eq!(p.filter.get_str("b").ok(), Some("é"));
    }

    #[test]
    fn bulk_write_models() {
        let c = call(
            r#"db.orders.bulkWrite([{insertOne:{document:{a:1}}},{deleteMany:{filter:{a:1}}}])"#,
        );
        let MongoOp::Write(ShellWrite::Bulk(models)) = c.op else {
            panic!("bulk");
        };
        assert_eq!(models.len(), 2);
    }

    #[test]
    fn a_filter_op_key_is_data() {
        match parse(r#"db.t.deleteMany({op: "find"})"#) {
            Ok(Statement::Call(c)) => assert_eq!(c.method, "deleteMany"),
            other => panic!("{other:?}"),
        }
        assert!(matches!(
            parse(r#"{"note": "op"}"#),
            Ok(Statement::Filter(_))
        ));
    }

    #[test]
    fn the_command_key_must_be_lowercase_op() {
        assert!(parse_executable(r#"{"OP":"deleteMany","filter":{}}"#).is_err());
    }

    #[test]
    fn errors_name_the_problem() {
        let msg = |text: &str| match parse_executable(text) {
            Err(err) => err.to_string(),
            Ok(ok) => panic!("{text}: {ok:?}"),
        };
        assert!(msg("db.t.find({}); db.t.find({})").contains("one Mongo statement"));
        assert!(msg("db.t.find({a: 1}").contains("expected"));
        assert!(msg(r#"db.t.find({a: "x})"#).contains("unclosed string"));
        assert!(msg("db.t.find({}).forEach(printjson)").contains("forEach"));
        assert!(msg("users.find({})").contains("db."));
        assert!(msg("db.t.find({a: /x/})").contains("$regex"));
    }
}
