//! Differential corpus for Mongo shell text.
//!
//! Each row is one input. `old` is what the two parsers did before this
//! module existed: the connector-api write policy (`classify`, `validate`,
//! `mongo_is_safe_dml`, `mongo_op_preview`) and the executor
//! (`shell_query::parse_mongo_call`). `new` is set only where the shared
//! parser does something else, with the reason. `exec` is a summary of the
//! call the executor runs (`err` when it refuses the text; `db/coll` when the
//! text names a database; ` o={…}` lists the options the executor applies).
//!
//! Rows added with E-09 (options arguments, #175) record the Phase D parser
//! as `old`: it read options arguments and then dropped them.

use mongodb::bson::{Bson, Document};
use mystique_connector_api::{EngineQueryPolicy, QueryMode, WriteRisk};

use super::parse::{
    parse_executable, DeleteModel, Executable, FindAndModify, Listing, Modify, MongoOp,
    ReadOptions, ShellWrite, Target, UpdateModel, WriteMethod, WriteModel, WriteOptions,
};
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
    Row { text: "db.orders.aggregate([{$match: {}}], {allowDiskUse: true})", old: Seen { risk: R, read: true, write: true, safe: false, preview: "aggregate", exec: r#"aggregate orders [{"$match":{}}]"# }, new: Some(Change { seen: Seen { risk: R, read: true, write: true, safe: false, preview: "aggregate", exec: r#"aggregate orders [{"$match":{}}] o={"allowDiskUse":true}"# }, why: "E-09: aggregate options are applied" }) },
    Row { text: "db.orders.insertOne({a: 1})", old: Seen { risk: W, read: false, write: true, safe: true, preview: "insertone", exec: r#"insertOne orders {"a":1}"# }, new: None },
    Row { text: "db.orders.insertMany([{a: 1}, {a: 2}])", old: Seen { risk: W, read: false, write: true, safe: true, preview: "insertmany", exec: r#"insertMany orders [{"a":1},{"a":2}]"# }, new: None },
    Row { text: "db.orders.updateOne({a: 1}, {$set: {b: 2}})", old: Seen { risk: W, read: false, write: true, safe: true, preview: "updateone", exec: r#"updateOne orders {"a":1} {"$set":{"b":2}}"# }, new: None },
    Row { text: "db.orders.updateMany({a: 1}, {$set: {b: 2}}, {upsert: true})", old: Seen { risk: W, read: false, write: true, safe: true, preview: "updatemany", exec: r#"updateMany orders {"a":1} {"$set":{"b":2}}"# }, new: Some(Change { seen: Seen { risk: W, read: false, write: true, safe: true, preview: "updatemany", exec: r#"updateMany orders {"a":1} {"$set":{"b":2}} o={"upsert":true}"# }, why: "E-09: upsert is applied; still a write and still the safe_write tier" }) },
    Row { text: "db.orders.updateMany({}, [{$set: {b: 2}}])", old: Seen { risk: W, read: false, write: true, safe: true, preview: "updatemany", exec: "err" }, new: Some(Change { seen: Seen { risk: W, read: false, write: false, safe: true, preview: "updatemany", exec: "err" }, why: "validate now refuses what the executor cannot run, before the write guard asks to confirm it (update pipelines)" }) },
    Row { text: r#"db.orders.deleteOne({_id: ObjectId("64f123456789abcdef123456")})"#, old: Seen { risk: W, read: false, write: true, safe: false, preview: "deleteone", exec: r#"deleteOne orders {"_id":{"$oid":"64f123456789abcdef123456"}}"# }, new: None },
    Row { text: "db.orders.deleteMany({})", old: Seen { risk: W, read: false, write: true, safe: false, preview: "deletemany", exec: "deleteMany orders {}" }, new: None },
    Row { text: "db.orders.deleteMany()", old: Seen { risk: W, read: false, write: true, safe: false, preview: "deletemany", exec: "err" }, new: Some(Change { seen: Seen { risk: W, read: false, write: false, safe: false, preview: "deletemany", exec: "err" }, why: "validate now refuses what the executor cannot run, before the write guard asks to confirm it (no filter)" }) },
    Row { text: "db.orders.replaceOne({a: 1}, {a: 2})", old: Seen { risk: W, read: false, write: true, safe: true, preview: "replaceone", exec: r#"replaceOne orders {"a":1} {"a":2}"# }, new: None },
    Row { text: r#"db.orders.findOneAndUpdate({a: 1}, {$set: {b: 2}}, {returnDocument: "after"})"#, old: Seen { risk: W, read: false, write: true, safe: false, preview: "findoneandupdate", exec: r#"findOneAndUpdate orders {"a":1} {"$set":{"b":2}}"# }, new: Some(Change { seen: Seen { risk: W, read: false, write: true, safe: false, preview: "findoneandupdate", exec: r#"findOneAndUpdate orders {"a":1} {"$set":{"b":2}} o={"returnDocument":"after"}"# }, why: "E-09: returnDocument is applied" }) },
    Row { text: "db.orders.findOneAndDelete({a: 1})", old: Seen { risk: W, read: false, write: true, safe: false, preview: "findoneanddelete", exec: "err" }, new: Some(Change { seen: Seen { risk: W, read: false, write: true, safe: false, preview: "findoneanddelete", exec: r#"findOneAndDelete orders {"a":1}"# }, why: "E-09: the executor runs findOneAndDelete (Phase D refused it at validate)" }) },
    Row { text: "db.orders.findOneAndReplace({a: 1}, {a: 2})", old: Seen { risk: W, read: false, write: true, safe: false, preview: "findoneandreplace", exec: "err" }, new: Some(Change { seen: Seen { risk: W, read: false, write: true, safe: false, preview: "findoneandreplace", exec: r#"findOneAndReplace orders {"a":1} {"a":2}"# }, why: "E-09: the executor runs findOneAndReplace (Phase D refused it at validate)" }) },
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
    // E-09 (#175): options arguments. `old` is the Phase D parser.
    Row { text: "db.orders.find({a: 1}, {b: 1}, {sort: {b: -1}, limit: 3, skip: 2})", old: Seen { risk: R, read: true, write: true, safe: false, preview: "find", exec: r#"find orders {"a":1} p={"b":1}"# }, new: Some(Change { seen: Seen { risk: R, read: true, write: true, safe: false, preview: "find", exec: r#"find orders {"a":1} p={"b":1} s={"b":-1} l=3 k=2"# }, why: "E-09: the third find argument (options) is applied" }) },
    Row { text: r#"db.orders.find({}, null, {projection: {b: 1}, hint: "a_1", maxTimeMS: 500, batchSize: 10, allowDiskUse: true, comment: "x"})"#, old: Seen { risk: R, read: true, write: true, safe: false, preview: "find", exec: "err" }, new: Some(Change { seen: Seen { risk: R, read: true, write: true, safe: false, preview: "find", exec: r#"find orders {} p={"b":1} o={"batchSize":10,"allowDiskUse":true,"hint":"a_1","maxTimeMS":500,"comment":"x"}"# }, why: "E-09: a null projection is none; find options are applied" }) },
    Row { text: "db.orders.find({}, {a: 1}, {projection: {b: 1}})", old: Seen { risk: R, read: true, write: true, safe: false, preview: "find", exec: r#"find orders {} p={"a":1}"# }, new: Some(Change { seen: Seen { risk: R, read: false, write: false, safe: false, preview: "find", exec: "err" }, why: "E-09: a projection in both places is refused instead of dropping one" }) },
    Row { text: "db.orders.find({}, {}, {}, {})", old: Seen { risk: R, read: true, write: true, safe: false, preview: "find", exec: "find orders {} p={}" }, new: Some(Change { seen: Seen { risk: R, read: false, write: false, safe: false, preview: "find", exec: "err" }, why: "E-09: arguments after the options are refused, not dropped" }) },
    Row { text: "db.orders.find({}, {}, {frobnicate: 1})", old: Seen { risk: R, read: true, write: true, safe: false, preview: "find", exec: "find orders {} p={}" }, new: Some(Change { seen: Seen { risk: R, read: false, write: false, safe: false, preview: "find", exec: "err" }, why: "E-09: an unknown option is refused, not dropped" }) },
    Row { text: r#"db.orders.find({}).hint({a: 1}).maxTimeMS(250).collation({locale: "fr"}).batchSize(5).allowDiskUse().comment("c")"#, old: Seen { risk: R, read: false, write: false, safe: false, preview: "find", exec: "err" }, new: Some(Change { seen: Seen { risk: R, read: true, write: true, safe: false, preview: "find", exec: r#"find orders {} o={"batchSize":5,"allowDiskUse":true,"hint":{"a":1},"collation":{"locale":"fr"},"maxTimeMS":250,"comment":"c"}"# }, why: "E-09: cursor option chains run" }) },
    Row { text: "db.orders.find({}).limit(0)", old: Seen { risk: R, read: true, write: true, safe: false, preview: "find", exec: "find orders {} l=0" }, new: Some(Change { seen: Seen { risk: R, read: true, write: true, safe: false, preview: "find", exec: "find orders {}" }, why: "E-09: limit 0 is no limit, as on the server (Phase D read one row)" }) },
    Row { text: r#"db.orders.findOne({a: 1}, null, {sort: {a: 1}})"#, old: Seen { risk: R, read: false, write: false, safe: false, preview: "findone", exec: "err" }, new: Some(Change { seen: Seen { risk: R, read: true, write: true, safe: false, preview: "findone", exec: r#"find orders {"a":1} s={"a":1} l=1"# }, why: "E-09: findOne takes the find options" }) },
    Row { text: r#"db.orders.aggregate([{$match: {}}], {maxTimeMS: 100, collation: {locale: "en"}, hint: "a_1", batchSize: 5, let: {v: 1}})"#, old: Seen { risk: R, read: true, write: true, safe: false, preview: "aggregate", exec: r#"aggregate orders [{"$match":{}}]"# }, new: Some(Change { seen: Seen { risk: R, read: true, write: true, safe: false, preview: "aggregate", exec: r#"aggregate orders [{"$match":{}}] o={"batchSize":5,"let":{"v":1},"hint":"a_1","collation":{"locale":"en"},"maxTimeMS":100}"# }, why: "E-09: aggregate options are applied" }) },
    Row { text: r#"db.orders.aggregate([{$out: "t"}], {allowDiskUse: true, bypassDocumentValidation: true})"#, old: Seen { risk: W, read: false, write: true, safe: false, preview: "aggregate", exec: r#"aggregate orders [{"$out":"t"}]"# }, new: Some(Change { seen: Seen { risk: W, read: false, write: true, safe: false, preview: "aggregate", exec: r#"aggregate orders [{"$out":"t"}] o={"allowDiskUse":true,"bypassDocumentValidation":true}"# }, why: "E-09: options are applied; $out keeps it a write" }) },
    Row { text: "db.orders.aggregate([{$match: {}}], {out: 1})", old: Seen { risk: R, read: true, write: true, safe: false, preview: "aggregate", exec: r#"aggregate orders [{"$match":{}}]"# }, new: Some(Change { seen: Seen { risk: R, read: false, write: false, safe: false, preview: "aggregate", exec: "err" }, why: "E-09: an unknown aggregate option is refused, not dropped" }) },
    Row { text: "db.orders.countDocuments({a: 1}, {limit: 5, skip: 1, hint: {a: 1}})", old: Seen { risk: R, read: true, write: true, safe: false, preview: "countdocuments", exec: r#"count orders {"a":1}"# }, new: Some(Change { seen: Seen { risk: R, read: true, write: true, safe: false, preview: "countdocuments", exec: r#"count orders {"a":1} o={"limit":5,"skip":1,"hint":{"a":1}}"# }, why: "E-09: count options are applied" }) },
    Row { text: "db.orders.estimatedDocumentCount()", old: Seen { risk: R, read: false, write: false, safe: false, preview: "estimateddocumentcount", exec: "err" }, new: Some(Change { seen: Seen { risk: R, read: true, write: true, safe: false, preview: "estimateddocumentcount", exec: "estimatedCount orders" }, why: "E-09: estimatedDocumentCount runs, as a read" }) },
    Row { text: "db.orders.estimatedDocumentCount({maxTimeMS: 50})", old: Seen { risk: R, read: false, write: false, safe: false, preview: "estimateddocumentcount", exec: "err" }, new: Some(Change { seen: Seen { risk: R, read: true, write: true, safe: false, preview: "estimateddocumentcount", exec: r#"estimatedCount orders o={"maxTimeMS":50}"# }, why: "E-09: estimatedDocumentCount runs, with its options" }) },
    Row { text: r#"db.orders.distinct("city", {}, {collation: {locale: "en", strength: 2}})"#, old: Seen { risk: R, read: true, write: true, safe: false, preview: "distinct", exec: "distinct orders city {}" }, new: Some(Change { seen: Seen { risk: R, read: true, write: true, safe: false, preview: "distinct", exec: r#"distinct orders city {} o={"collation":{"locale":"en","strength":2}}"# }, why: "E-09: distinct options are applied" }) },
    Row { text: "db.orders.insertMany([{a: 1}, {a: 2}], {ordered: false})", old: Seen { risk: W, read: false, write: true, safe: true, preview: "insertmany", exec: r#"insertMany orders [{"a":1},{"a":2}]"# }, new: Some(Change { seen: Seen { risk: W, read: false, write: true, safe: true, preview: "insertmany", exec: r#"insertMany orders [{"a":1},{"a":2}] o={"ordered":false}"# }, why: "E-09: ordered is applied" }) },
    Row { text: r#"db.orders.insertOne({a: 1}, {writeConcern: {w: "majority"}, comment: "c"})"#, old: Seen { risk: W, read: false, write: true, safe: true, preview: "insertone", exec: r#"insertOne orders {"a":1}"# }, new: Some(Change { seen: Seen { risk: W, read: false, write: true, safe: true, preview: "insertone", exec: r#"insertOne orders {"a":1} o={"comment":"c","writeConcern":{"w":"majority"}}"# }, why: "E-09: insert options are applied" }) },
    Row { text: r#"db.orders.updateOne({a: 1}, {$set: {"x.$[e].v": 1}}, {arrayFilters: [{"e.k": 2}], upsert: true, maxTimeMS: 900})"#, old: Seen { risk: W, read: false, write: true, safe: true, preview: "updateone", exec: r#"updateOne orders {"a":1} {"$set":{"x.$[e].v":1}}"# }, new: Some(Change { seen: Seen { risk: W, read: false, write: true, safe: true, preview: "updateone", exec: r#"updateOne orders {"a":1} {"$set":{"x.$[e].v":1}} o={"upsert":true,"arrayFilters":[{"e.k":2}],"maxTimeMS":900}"# }, why: "E-09: upsert, arrayFilters and maxTimeMS are applied" }) },
    Row { text: "db.orders.updateOne({a: 1}, {$set: {b: 1}}, {upsret: true})", old: Seen { risk: W, read: false, write: true, safe: true, preview: "updateone", exec: r#"updateOne orders {"a":1} {"$set":{"b":1}}"# }, new: Some(Change { seen: Seen { risk: W, read: false, write: false, safe: true, preview: "updateone", exec: "err" }, why: "E-09: a misspelt option is refused instead of running without it" }) },
    Row { text: r#"db.orders.replaceOne({a: 1}, {a: 2}, {upsert: true, hint: "a_1", collation: {locale: "fr"}})"#, old: Seen { risk: W, read: false, write: true, safe: true, preview: "replaceone", exec: r#"replaceOne orders {"a":1} {"a":2}"# }, new: Some(Change { seen: Seen { risk: W, read: false, write: true, safe: true, preview: "replaceone", exec: r#"replaceOne orders {"a":1} {"a":2} o={"upsert":true,"collation":{"locale":"fr"},"hint":"a_1"}"# }, why: "E-09: replace options are applied" }) },
    Row { text: r#"db.orders.deleteMany({a: 1}, {collation: {locale: "fr"}, hint: {a: 1}})"#, old: Seen { risk: W, read: false, write: true, safe: false, preview: "deletemany", exec: r#"deleteMany orders {"a":1}"# }, new: Some(Change { seen: Seen { risk: W, read: false, write: true, safe: false, preview: "deletemany", exec: r#"deleteMany orders {"a":1} o={"collation":{"locale":"fr"},"hint":{"a":1}}"# }, why: "E-09: delete options are applied" }) },
    Row { text: r#"db.orders.findOneAndUpdate({a: 1}, {$inc: {n: 1}}, {upsert: true, returnDocument: "before", projection: {n: 1}, sort: {n: -1}})"#, old: Seen { risk: W, read: false, write: true, safe: false, preview: "findoneandupdate", exec: r#"findOneAndUpdate orders {"a":1} {"$inc":{"n":1}}"# }, new: Some(Change { seen: Seen { risk: W, read: false, write: true, safe: false, preview: "findoneandupdate", exec: r#"findOneAndUpdate orders {"a":1} {"$inc":{"n":1}} o={"upsert":true,"returnDocument":"before","projection":{"n":1},"sort":{"n":-1}}"# }, why: "E-09: findOneAndUpdate options are applied; upsert stays a write" }) },
    Row { text: "db.orders.findOneAndReplace({a: 1}, {a: 2}, {returnNewDocument: true, upsert: true})", old: Seen { risk: W, read: false, write: false, safe: false, preview: "findoneandreplace", exec: "err" }, new: Some(Change { seen: Seen { risk: W, read: false, write: true, safe: false, preview: "findoneandreplace", exec: r#"findOneAndReplace orders {"a":1} {"a":2} o={"upsert":true,"returnDocument":"after"}"# }, why: "E-09: findOneAndReplace runs with its options" }) },
    Row { text: "db.orders.findOneAndDelete({a: 1}, {sort: {a: 1}, projection: {_id: 1}})", old: Seen { risk: W, read: false, write: false, safe: false, preview: "findoneanddelete", exec: "err" }, new: Some(Change { seen: Seen { risk: W, read: false, write: true, safe: false, preview: "findoneanddelete", exec: r#"findOneAndDelete orders {"a":1} o={"projection":{"_id":1},"sort":{"a":1}}"# }, why: "E-09: findOneAndDelete runs with its options" }) },
    Row { text: "db.orders.bulkWrite([{updateOne: {filter: {a: 1}, update: {$set: {b: 1}}, upsert: true}}, {deleteMany: {filter: {c: 1}, collation: {locale: \"en\"}}}], {ordered: false})", old: Seen { risk: W, read: false, write: true, safe: false, preview: "bulkwrite", exec: r#"bulkWrite orders [updateOne {"a":1} {"$set":{"b":1}},deleteMany {"c":1}]"# }, new: Some(Change { seen: Seen { risk: W, read: false, write: true, safe: false, preview: "bulkwrite", exec: r#"bulkWrite orders [updateOne {"a":1} {"$set":{"b":1}} o={"upsert":true},deleteMany {"c":1} o={"collation":{"locale":"en"}}] o={"ordered":false}"# }, why: "E-09: per-operation and bulkWrite options are applied" }) },
    Row { text: "db.orders.bulkWrite([{deleteOne: {filter: {a: 1}, limit: 1}}])", old: Seen { risk: W, read: false, write: true, safe: false, preview: "bulkwrite", exec: r#"bulkWrite orders [deleteOne {"a":1}]"# }, new: Some(Change { seen: Seen { risk: W, read: false, write: false, safe: false, preview: "bulkwrite", exec: "err" }, why: "E-09: an unknown key in a bulkWrite operation is refused, not dropped" }) },
    Row { text: r#"{"op": "updateOne", "filter": {"a": 1}, "update": {"$set": {"b": 1}}, "options": {"upsert": true}}"#, old: Seen { risk: W, read: false, write: true, safe: true, preview: "updateone", exec: r#"updateOne - {"a":1} {"$set":{"b":1}}"# }, new: Some(Change { seen: Seen { risk: W, read: false, write: true, safe: true, preview: "updateone", exec: r#"updateOne - {"a":1} {"$set":{"b":1}} o={"upsert":true}"# }, why: "E-09: a command's `options` object is applied" }) },
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

/// ` o={…}` for the options that are set, in a fixed order. Empty when none.
fn opts(pairs: Vec<(&str, Option<Bson>)>) -> String {
    let mut set = Document::new();
    for (key, value) in pairs {
        if let Some(value) = value {
            set.insert(key, value);
        }
    }
    if set.is_empty() {
        String::new()
    } else {
        format!(" o={}", json(&set))
    }
}

fn int(n: Option<u64>) -> Option<Bson> {
    n.map(|n| Bson::Int64(i64::try_from(n).unwrap_or(i64::MAX)))
}

fn read_pairs(read: &ReadOptions) -> Vec<(&'static str, Option<Bson>)> {
    vec![
        ("hint", read.hint.clone()),
        ("collation", read.collation.clone().map(Bson::Document)),
        ("maxTimeMS", int(read.max_time_ms)),
        ("comment", read.comment.clone()),
    ]
}

fn write_pairs(w: &WriteOptions) -> Vec<(&'static str, Option<Bson>)> {
    vec![
        ("ordered", w.ordered.map(Bson::Boolean)),
        (
            "bypassDocumentValidation",
            w.bypass_document_validation.map(Bson::Boolean),
        ),
        ("comment", w.comment.clone()),
        ("let", w.let_vars.clone().map(Bson::Document)),
        ("writeConcern", w.write_concern.clone().map(Bson::Document)),
        ("maxTimeMS", int(w.max_time_ms)),
    ]
}

fn update_pairs(m: &UpdateModel) -> Vec<(&'static str, Option<Bson>)> {
    vec![
        ("upsert", m.upsert.map(Bson::Boolean)),
        (
            "arrayFilters",
            m.array_filters
                .clone()
                .map(|f| Bson::Array(f.into_iter().map(Bson::Document).collect())),
        ),
        ("collation", m.collation.clone().map(Bson::Document)),
        ("hint", m.hint.clone()),
        ("sort", m.sort.clone().map(Bson::Document)),
    ]
}

fn delete_pairs(m: &DeleteModel) -> Vec<(&'static str, Option<Bson>)> {
    vec![
        ("collation", m.collation.clone().map(Bson::Document)),
        ("hint", m.hint.clone()),
    ]
}

/// One model: its name, documents and own options.
fn model(m: &WriteModel) -> (String, Vec<(&'static str, Option<Bson>)>) {
    match m {
        WriteModel::InsertOne(d) => (format!("insertOne {}", json(d)), Vec::new()),
        WriteModel::UpdateOne(u) => (
            format!("updateOne {} {}", json(&u.filter), json(&u.update)),
            update_pairs(u),
        ),
        WriteModel::UpdateMany(u) => (
            format!("updateMany {} {}", json(&u.filter), json(&u.update)),
            update_pairs(u),
        ),
        WriteModel::ReplaceOne(u) => (
            format!("replaceOne {} {}", json(&u.filter), json(&u.update)),
            update_pairs(u),
        ),
        WriteModel::DeleteOne(d) => (format!("deleteOne {}", json(&d.filter)), delete_pairs(d)),
        WriteModel::DeleteMany(d) => (format!("deleteMany {}", json(&d.filter)), delete_pairs(d)),
    }
}

fn find_and_modify(at: &str, f: &FindAndModify) -> String {
    let body = match &f.modify {
        Modify::Update(u) | Modify::Replace(u) => format!(" {}", json(u)),
        Modify::Delete => String::new(),
    };
    let mut pairs = vec![
        ("upsert", f.upsert.map(Bson::Boolean)),
        (
            "returnDocument",
            f.return_after
                .map(|after| Bson::String(if after { "after" } else { "before" }.into())),
        ),
        ("projection", f.projection.clone().map(Bson::Document)),
        ("sort", f.sort.clone().map(Bson::Document)),
        (
            "arrayFilters",
            f.array_filters
                .clone()
                .map(|v| Bson::Array(v.into_iter().map(Bson::Document).collect())),
        ),
        ("collation", f.collation.clone().map(Bson::Document)),
        ("hint", f.hint.clone()),
    ];
    pairs.extend(write_pairs(&f.options));
    format!(
        "{} {at} {}{body}{}",
        f.method(),
        json(&f.filter),
        opts(pairs)
    )
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
            let mut pairs = vec![
                ("batchSize", p.batch_size.map(|n| Bson::Int64(i64::from(n)))),
                ("allowDiskUse", p.allow_disk_use.map(Bson::Boolean)),
            ];
            pairs.extend(read_pairs(&p.read));
            s.push_str(&opts(pairs));
            s
        }
        MongoOp::Aggregate(a) => {
            let mut pairs = vec![
                ("allowDiskUse", a.allow_disk_use.map(Bson::Boolean)),
                ("batchSize", a.batch_size.map(|n| Bson::Int64(i64::from(n)))),
                (
                    "bypassDocumentValidation",
                    a.bypass_document_validation.map(Bson::Boolean),
                ),
                ("let", a.let_vars.clone().map(Bson::Document)),
            ];
            pairs.extend(read_pairs(&a.read));
            format!("aggregate {at} {}{}", docs(&a.pipeline), opts(pairs))
        }
        MongoOp::Count(c) => {
            let mut pairs = vec![("limit", int(c.limit)), ("skip", int(c.skip))];
            pairs.extend(read_pairs(&c.read));
            format!("count {at} {}{}", json(&c.filter), opts(pairs))
        }
        MongoOp::EstimatedCount(read) => format!("estimatedCount {at}{}", opts(read_pairs(&read))),
        MongoOp::Distinct {
            field,
            filter,
            read,
        } => format!(
            "distinct {at} {field} {}{}",
            json(&filter),
            opts(read_pairs(&read))
        ),
        MongoOp::Write(ShellWrite::FindAndModify(f)) => find_and_modify(&at, &f),
        MongoOp::Write(ShellWrite::Models {
            method,
            models,
            options,
        }) => match method {
            WriteMethod::InsertMany => {
                let inserted: Vec<Document> = models
                    .iter()
                    .filter_map(|m| match m {
                        WriteModel::InsertOne(d) => Some(d.clone()),
                        _ => None,
                    })
                    .collect();
                format!(
                    "insertMany {at} {}{}",
                    docs(&inserted),
                    opts(write_pairs(&options))
                )
            }
            WriteMethod::BulkWrite => format!(
                "bulkWrite {at} [{}]{}",
                models
                    .iter()
                    .map(|m| {
                        let (name, pairs) = model(m);
                        format!("{name}{}", opts(pairs))
                    })
                    .collect::<Vec<_>>()
                    .join(","),
                opts(write_pairs(&options))
            ),
            // One model: `updateOne orders {filter} {update} o={…}`.
            _ => {
                let Some(only) = models.first() else {
                    return "err".into();
                };
                let (name, mut pairs) = model(only);
                pairs.extend(write_pairs(&options));
                let (_, rest) = name.split_once(' ').unwrap_or((name.as_str(), ""));
                format!("{} {at} {rest}{}", method.name(), opts(pairs))
            }
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
            let read_call = [
                "find ",
                "aggregate ",
                "count ",
                "estimatedCount ",
                "distinct ",
                "list ",
            ]
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
        "findOneAndReplace ",
        "findOneAndDelete ",
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
    use mongodb::bson::{doc, Bson};

    use super::super::parse::{
        parse, parse_executable, Executable, Modify, MongoCall, MongoOp, ShellWrite, Statement,
        WriteMethod, WriteModel, WriteOptions,
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
        let MongoOp::Write(ShellWrite::Models {
            method: WriteMethod::BulkWrite,
            models,
            ..
        }) = c.op
        else {
            panic!("bulk");
        };
        assert_eq!(models.len(), 2);
    }

    fn models(text: &str) -> (Vec<WriteModel>, WriteOptions) {
        match call(text).op {
            MongoOp::Write(ShellWrite::Models {
                models, options, ..
            }) => (models, options),
            other => panic!("{text}: {other:?}"),
        }
    }

    #[test]
    fn update_options_reach_the_statement() {
        let (models, options) = models(
            r#"db.t.updateMany({a: 1}, {$set: {"x.$[e]": 1}}, {upsert: true, arrayFilters: [{e: 2}], collation: {locale: "fr"}, hint: "a_1", let: {v: 1}, comment: "c", writeConcern: {w: 1}, maxTimeMS: 7})"#,
        );
        let [WriteModel::UpdateMany(m)] = models.as_slice() else {
            panic!("{models:?}");
        };
        assert_eq!(m.upsert, Some(true));
        assert_eq!(m.array_filters, Some(vec![doc! { "e": 2 }]));
        assert_eq!(m.collation, Some(doc! { "locale": "fr" }));
        assert_eq!(m.hint, Some(Bson::String("a_1".into())));
        assert_eq!(m.sort, None);
        assert_eq!(options.let_vars, Some(doc! { "v": 1 }));
        assert_eq!(options.comment, Some(Bson::String("c".into())));
        assert_eq!(options.write_concern, Some(doc! { "w": 1 }));
        assert_eq!(options.max_time_ms, Some(7));
        assert_eq!(options.ordered, None);
    }

    #[test]
    fn update_many_takes_no_sort() {
        let err = match parse_executable("db.t.updateMany({}, {$set: {a: 1}}, {sort: {a: 1}})") {
            Err(err) => err.to_string(),
            Ok(ok) => panic!("{ok:?}"),
        };
        assert!(err.contains("`sort`"), "{err}");
        assert!(err.contains("upsert"), "lists what it takes: {err}");
    }

    #[test]
    fn find_and_modify_options() {
        let MongoOp::Write(ShellWrite::FindAndModify(f)) = call(
            r#"db.t.findOneAndUpdate({a: 1}, {$inc: {n: 1}}, {upsert: true, returnDocument: "after", projection: {n: 1}, sort: {n: -1}, arrayFilters: [{x: 1}], maxTimeMS: 30})"#,
        )
        .op
        else {
            panic!("findAndModify");
        };
        assert_eq!(f.method(), "findOneAndUpdate");
        assert_eq!(f.modify, Modify::Update(doc! { "$inc": { "n": 1 } }));
        assert_eq!(f.upsert, Some(true));
        assert_eq!(f.return_after, Some(true));
        assert_eq!(f.projection, Some(doc! { "n": 1 }));
        assert_eq!(f.sort, Some(doc! { "n": -1 }));
        assert_eq!(f.array_filters, Some(vec![doc! { "x": 1 }]));
        assert_eq!(f.options.max_time_ms, Some(30));
    }

    #[test]
    fn return_document_must_be_before_or_after() {
        for bad in [
            r#"db.t.findOneAndUpdate({}, {$set: {a: 1}}, {returnDocument: "new"})"#,
            r#"db.t.findOneAndUpdate({}, {$set: {a: 1}}, {returnDocument: "after", returnNewDocument: false})"#,
            r#"db.t.findOneAndDelete({}, {upsert: true})"#,
            r#"db.t.findOneAndReplace({}, {a: 1}, {arrayFilters: []})"#,
        ] {
            assert!(parse_executable(bad).is_err(), "{bad}");
        }
        assert!(parse_executable(
            r#"db.t.findOneAndUpdate({}, {$set: {a: 1}}, {returnDocument: "before", returnNewDocument: false})"#
        )
        .is_ok());
    }

    #[test]
    fn option_values_are_checked() {
        for bad in [
            "db.t.find({}, {}, {limit: \"5\"})",
            "db.t.find({}, {}, {skip: -1})",
            "db.t.find({}, {}, {hint: 5})",
            "db.t.find({}, {}, {collation: {strength: 2}})",
            "db.t.find({}, {}, 5)",
            "db.t.insertOne({a: 1}, {writeConcern: {w: true}})",
            "db.t.insertMany([{a: 1}], {ordered: \"no\"})",
            "db.t.insertMany([])",
            "db.t.aggregate([], {batchSize: -5})",
        ] {
            assert!(parse_executable(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn null_options_are_absent() {
        let MongoOp::Find(p) = call("db.t.find({}, null, {limit: null, sort: null})").op else {
            panic!("find");
        };
        assert_eq!(p.limit, None);
        assert_eq!(p.sort, None);
        assert!(parse_executable("db.t.updateOne({}, {$set: {a: 1}}, null)").is_ok());
    }

    #[test]
    fn max_time_zero_is_no_limit() {
        let MongoOp::Find(p) = call("db.t.find({}, {}, {maxTimeMS: 0})").op else {
            panic!("find");
        };
        assert_eq!(p.read.max_time_ms, None);
    }

    #[test]
    fn find_one_keeps_limit_one_over_an_option() {
        let MongoOp::Find(p) = call("db.t.findOne({}, {}, {limit: 9, skip: 2})").op else {
            panic!("find");
        };
        assert_eq!(p.limit, Some(1));
        assert_eq!(p.skip, Some(2));
    }

    #[test]
    fn bulk_model_options_and_ordered() {
        let (models, options) = models(
            r#"db.t.bulkWrite([{replaceOne: {filter: {a: 1}, replacement: {a: 2}, upsert: true, hint: {a: 1}}}, {deleteOne: {filter: {a: 3}, collation: {locale: "en"}}}], {ordered: false, bypassDocumentValidation: true})"#,
        );
        assert_eq!(options.ordered, Some(false));
        assert_eq!(options.bypass_document_validation, Some(true));
        let [WriteModel::ReplaceOne(r), WriteModel::DeleteOne(d)] = models.as_slice() else {
            panic!("{models:?}");
        };
        assert_eq!(r.upsert, Some(true));
        assert_eq!(r.hint, Some(Bson::Document(doc! { "a": 1 })));
        assert_eq!(d.collation, Some(doc! { "locale": "en" }));
    }

    #[test]
    fn command_options_use_the_method_keys() {
        assert!(parse_executable(
            r#"{"op": "deleteMany", "filter": {}, "options": {"upsert": true}}"#
        )
        .is_err());
        let (models, _) =
            models(r#"{"op": "deleteOne", "filter": {"a": 1}, "options": {"hint": "a_1"}}"#);
        let [WriteModel::DeleteOne(d)] = models.as_slice() else {
            panic!("{models:?}");
        };
        assert_eq!(d.hint, Some(Bson::String("a_1".into())));
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
