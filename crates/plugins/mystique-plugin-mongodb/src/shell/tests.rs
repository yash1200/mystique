//! Differential corpus for Mongo shell text.
//!
//! Each row records what the write policy (connector-api `policy::mongo`)
//! and the executor (`shell_query::parse_mongo_call`) did with the same text
//! before they shared a parser. `exec` is a structural summary of the parsed
//! call (`err` when the executor refused it).

use mongodb::bson::{Bson, Document};
use mystique_connector_api::{
    mongo_is_safe_dml, mongo_op_preview, EngineQueryPolicy, MongoQueryPolicy, QueryMode, WriteRisk,
};

use crate::shell_query::{parse_mongo_call, BulkModel, MongoCall, ShellWrite};

use WriteRisk::{Destructive as X, Read as R, Write as W};

struct Row {
    text: &'static str,
    /// `MongoQueryPolicy::classify`.
    risk: WriteRisk,
    /// `validate(text, Read)` passed.
    read: bool,
    /// `validate(text, Write)` passed.
    write: bool,
    /// `mongo_is_safe_dml`.
    safe: bool,
    /// `mongo_op_preview`.
    preview: &'static str,
    /// Executor parse, summarised.
    exec: &'static str,
}

#[rustfmt::skip]
const CORPUS: &[Row] = &[
    Row { text: r#"db.orders.find({status: "open"})"#, risk: R, read: true, write: true, safe: false, preview: "find", exec: r#"find orders {"status":"open"}"# },
    Row { text: "db.orders.find()", risk: R, read: true, write: true, safe: false, preview: "find", exec: "find orders {}" },
    Row { text: "db.orders.find({}).sort({a: -1}).limit(5)", risk: R, read: true, write: true, safe: false, preview: "find", exec: r#"find orders {} s={"a":-1} l=5"# },
    Row { text: "db.orders.find({}, {a: 1}).skip(10).limit(5).pretty()", risk: R, read: true, write: true, safe: false, preview: "find", exec: r#"find orders {} p={"a":1} l=5 k=10"# },
    Row { text: "db.orders.findOne({a: 1})", risk: R, read: true, write: true, safe: false, preview: "findone", exec: r#"find orders {"a":1} l=1"# },
    Row { text: "db.orders.find({a: 1}).count()", risk: R, read: true, write: true, safe: false, preview: "find", exec: "err" },
    Row { text: "db.orders.find({a: 1}).explain()", risk: R, read: true, write: true, safe: false, preview: "find", exec: r#"find orders {"a":1}"# },
    Row { text: "db.orders.find({}).forEach(printjson)", risk: R, read: true, write: true, safe: false, preview: "find", exec: "err" },
    Row { text: "db.orders.find", risk: W, read: false, write: false, safe: false, preview: "db.orders.find", exec: "find orders {}" },
    Row { text: "find({a: 1})", risk: W, read: false, write: false, safe: false, preview: "find({a:1})", exec: r#"find - {"a":1}"# },
    Row { text: "orders.find({a: 1})", risk: R, read: true, write: true, safe: false, preview: "find", exec: r#"find - {"a":1}"# },
    Row { text: "db.system.profile.find({})", risk: R, read: true, write: true, safe: false, preview: "find", exec: "find profile {}" },
    Row { text: "db.orders.aggregate([{$match: {}}, {$limit: 5}])", risk: R, read: true, write: true, safe: false, preview: "aggregate", exec: r#"aggregate orders [{"$match":{}},{"$limit":5}]"# },
    Row { text: r#"db.orders.aggregate([{$match: {}}, {$out: "t"}])"#, risk: W, read: false, write: true, safe: false, preview: "aggregate", exec: r#"aggregate orders [{"$match":{}},{"$out":"t"}]"# },
    Row { text: r#"db.orders.aggregate([{"$out": "t"}])"#, risk: W, read: false, write: true, safe: false, preview: "aggregate", exec: r#"aggregate orders [{"$out":"t"}]"# },
    Row { text: r#"db.orders.aggregate([{'$merge': {into: "t"}}])"#, risk: W, read: false, write: true, safe: false, preview: "aggregate", exec: "err" },
    Row { text: r#"db.orders.aggregate([{`$out`: "t"}])"#, risk: W, read: false, write: true, safe: false, preview: "aggregate", exec: "err" },
    Row { text: r#"db.orders.aggregate([{"$out": "t"}])"#, risk: W, read: false, write: true, safe: false, preview: "aggregate", exec: r#"aggregate orders [{"$out":"t"}]"# },
    Row { text: r#"db.orders.aggregate([{$match: {note: "$out"}}])"#, risk: R, read: true, write: true, safe: false, preview: "aggregate", exec: r#"aggregate orders [{"$match":{"note":"$out"}}]"# },
    Row { text: r#"db.orders.aggregate([{$lookup: {from: "x", pipeline: [{$out: "y"}], as: "z"}}])"#, risk: W, read: false, write: true, safe: false, preview: "aggregate", exec: r#"aggregate orders [{"$lookup":{"from":"x","pipeline":[{"$out":"y"}],"as":"z"}}]"# },
    Row { text: r#"db.orders.aggregate([{$match: {}}, {$drop: "t"}])"#, risk: X, read: false, write: true, safe: false, preview: "aggregate", exec: r#"aggregate orders [{"$match":{}},{"$drop":"t"}]"# },
    Row { text: "db.orders.aggregate([{$match: {}}]).pretty()", risk: R, read: true, write: true, safe: false, preview: "aggregate", exec: r#"aggregate orders [{"$match":{}}]"# },
    Row { text: "db.orders.aggregate([{$match: {}}], {allowDiskUse: true})", risk: R, read: true, write: true, safe: false, preview: "aggregate", exec: r#"aggregate orders [{"$match":{}}]"# },
    Row { text: "db.orders.insertOne({a: 1})", risk: W, read: false, write: true, safe: true, preview: "insertone", exec: r#"insertOne orders {"a":1}"# },
    Row { text: "db.orders.insertMany([{a: 1}, {a: 2}])", risk: W, read: false, write: true, safe: true, preview: "insertmany", exec: r#"insertMany orders [{"a":1},{"a":2}]"# },
    Row { text: "db.orders.updateOne({a: 1}, {$set: {b: 2}})", risk: W, read: false, write: true, safe: true, preview: "updateone", exec: r#"updateOne orders {"a":1} {"$set":{"b":2}}"# },
    Row { text: "db.orders.updateMany({a: 1}, {$set: {b: 2}}, {upsert: true})", risk: W, read: false, write: true, safe: true, preview: "updatemany", exec: r#"updateMany orders {"a":1} {"$set":{"b":2}}"# },
    Row { text: "db.orders.updateMany({}, [{$set: {b: 2}}])", risk: W, read: false, write: true, safe: true, preview: "updatemany", exec: "err" },
    Row { text: r#"db.orders.deleteOne({_id: ObjectId("64f123456789abcdef123456")})"#, risk: W, read: false, write: true, safe: false, preview: "deleteone", exec: r#"deleteOne orders {"_id":{"$oid":"64f123456789abcdef123456"}}"# },
    Row { text: "db.orders.deleteMany({})", risk: W, read: false, write: true, safe: false, preview: "deletemany", exec: "deleteMany orders {}" },
    Row { text: "db.orders.deleteMany()", risk: W, read: false, write: true, safe: false, preview: "deletemany", exec: "err" },
    Row { text: "db.orders.replaceOne({a: 1}, {a: 2})", risk: W, read: false, write: true, safe: true, preview: "replaceone", exec: r#"replaceOne orders {"a":1} {"a":2}"# },
    Row { text: r#"db.orders.findOneAndUpdate({a: 1}, {$set: {b: 2}}, {returnDocument: "after"})"#, risk: W, read: false, write: true, safe: false, preview: "findoneandupdate", exec: r#"findOneAndUpdate orders {"a":1} {"$set":{"b":2}}"# },
    Row { text: "db.orders.findOneAndDelete({a: 1})", risk: W, read: false, write: true, safe: false, preview: "findoneanddelete", exec: "err" },
    Row { text: "db.orders.findOneAndReplace({a: 1}, {a: 2})", risk: W, read: false, write: true, safe: false, preview: "findoneandreplace", exec: "err" },
    Row { text: "db.orders.bulkWrite([{insertOne: {document: {a: 1}}}, {deleteOne: {filter: {a: 1}}}])", risk: W, read: false, write: true, safe: false, preview: "bulkwrite", exec: r#"bulkWrite orders [insertOne {"a":1},deleteOne {"a":1}]"# },
    Row { text: "db.orders.insert({a: 1})", risk: W, read: false, write: true, safe: true, preview: "insert", exec: "err" },
    Row { text: "db.orders.update({a: 1}, {$set: {b: 1}})", risk: W, read: false, write: true, safe: true, preview: "update", exec: "err" },
    Row { text: "db.orders.remove({a: 1})", risk: W, read: false, write: true, safe: false, preview: "remove", exec: "err" },
    Row { text: "db.orders.insertOne({a: 1}).pretty()", risk: W, read: false, write: true, safe: true, preview: "insertone", exec: r#"insertOne orders {"a":1}"# },
    Row { text: r#"db.orders.deleteMany({op: "find"})"#, risk: W, read: false, write: true, safe: false, preview: "deletemany", exec: r#"deleteMany orders {"op":"find"}"# },
    Row { text: r#"db.orders.deleteMany({"op": "insertOne"})"#, risk: W, read: false, write: true, safe: false, preview: "deletemany", exec: r#"deleteMany orders {"op":"insertOne"}"# },
    Row { text: r#"db.orders.find({op: "deleteMany"})"#, risk: R, read: true, write: true, safe: false, preview: "find", exec: r#"find orders {"op":"deleteMany"}"# },
    Row { text: r#"db.orders.insertOne({op: "find"})"#, risk: W, read: false, write: true, safe: true, preview: "insertone", exec: r#"insertOne orders {"op":"find"}"# },
    Row { text: "db.orders.drop()", risk: X, read: false, write: true, safe: false, preview: "drop", exec: "err" },
    Row { text: r#"db.orders.drop({op: "find"})"#, risk: X, read: false, write: true, safe: false, preview: "drop", exec: r#"find drop({op: " {}"# },
    Row { text: "db.dropDatabase()", risk: X, read: false, write: true, safe: false, preview: "dropdatabase", exec: "err" },
    Row { text: "db.orders.createIndex({a: 1})", risk: W, read: false, write: true, safe: false, preview: "createindex", exec: "err" },
    Row { text: r#"db.orders.dropIndex("a_1")"#, risk: W, read: false, write: true, safe: false, preview: "dropindex", exec: "err" },
    Row { text: "db.orders.dropIndexes()", risk: W, read: false, write: true, safe: false, preview: "dropindexes", exec: "err" },
    Row { text: r#"db.orders.renameCollection("orders2")"#, risk: W, read: false, write: true, safe: false, preview: "renamecollection", exec: "err" },
    Row { text: r#"db.createCollection("x")"#, risk: W, read: false, write: true, safe: false, preview: "createcollection", exec: "err" },
    Row { text: r#"db.runCommand({drop: "orders"})"#, risk: W, read: false, write: true, safe: false, preview: "runcommand", exec: "err" },
    Row { text: "db.runCommand({ping: 1})", risk: W, read: false, write: true, safe: false, preview: "runcommand", exec: "err" },
    Row { text: "db.adminCommand({shutdown: 1})", risk: W, read: false, write: true, safe: false, preview: "admincommand", exec: "err" },
    Row { text: "db.getCollectionNames()", risk: W, read: false, write: true, safe: false, preview: "getcollectionnames", exec: "err" },
    Row { text: "db.orders.getIndexes()", risk: R, read: true, write: true, safe: false, preview: "getindexes", exec: "err" },
    Row { text: "db.orders.stats()", risk: R, read: true, write: true, safe: false, preview: "stats", exec: "err" },
    Row { text: r#"db.getSiblingDB("other").orders.find({})"#, risk: R, read: true, write: true, safe: false, preview: "find", exec: "find orders {}" },
    Row { text: r#"db.getSiblingDB("other").orders.deleteMany({})"#, risk: W, read: false, write: true, safe: false, preview: "deletemany", exec: "err" },
    Row { text: r#"db.getSiblingDB("other").dropDatabase()"#, risk: X, read: false, write: true, safe: false, preview: "dropdatabase", exec: "err" },
    Row { text: r#"db.getCollection("orders").find({a: 1})"#, risk: R, read: true, write: true, safe: false, preview: "find", exec: r#"find getCollection("orders") {"a":1}"# },
    Row { text: r#"db.getCollection("orders").deleteMany({a: 1})"#, risk: W, read: false, write: true, safe: false, preview: "deletemany", exec: "err" },
    Row { text: r#"db["order-items"].find({})"#, risk: R, read: true, write: true, safe: false, preview: "find", exec: "find - {}" },
    Row { text: "db['order-items'].deleteMany({a: 1})", risk: W, read: false, write: true, safe: false, preview: "deletemany", exec: r#"deleteMany - {"a":1}"# },
    Row { text: r#"db.getMongo().getDB("other").orders.find({})"#, risk: R, read: true, write: true, safe: false, preview: "find", exec: "find orders {}" },
    Row { text: r#"db.Zürich.find({city: "Zürich"})"#, risk: R, read: true, write: true, safe: false, preview: "find", exec: r#"find Zürich {"city":"Zürich"}"# },
    Row { text: "// comment\ndb.orders.find({})", risk: R, read: true, write: true, safe: false, preview: "find", exec: "find orders {}" },
    Row { text: "db.orders.find({}) // trailing comment", risk: R, read: true, write: true, safe: false, preview: "find", exec: "err" },
    Row { text: "/* block */ db.orders.deleteMany({})", risk: W, read: false, write: true, safe: false, preview: "deletemany", exec: "deleteMany orders {}" },
    Row { text: "db.orders.find({a: 1 /* inline */})", risk: R, read: true, write: true, safe: false, preview: "find", exec: "err" },
    Row { text: "db.orders.find({});", risk: R, read: true, write: true, safe: false, preview: "find", exec: "find orders {}" },
    Row { text: "db.orders.find({});;  ", risk: R, read: true, write: true, safe: false, preview: "find", exec: "find orders {}" },
    Row { text: "db.orders.find({}); db.orders.deleteMany({})", risk: W, read: false, write: true, safe: false, preview: "deletemany", exec: "err" },
    Row { text: "db.orders.insertOne({a: 1}); db.orders.deleteMany({})", risk: W, read: false, write: true, safe: false, preview: "deletemany", exec: "err" },
    Row { text: "  db . orders . find ( { a : 1 } )  ", risk: W, read: false, write: false, safe: false, preview: "db.orders.find({a:1})", exec: r#"find orders {"a":1}"# },
    Row { text: "db.orders\n  .find({a: 1})\n  .limit(2)", risk: R, read: true, write: true, safe: false, preview: "find", exec: r#"find orders {"a":1} l=2"# },
    Row { text: r#"db.orders.find({note: "a) b; c"})"#, risk: R, read: true, write: true, safe: false, preview: "find", exec: r#"find orders {"note":"a) b; c"}"# },
    Row { text: r#"db.orders.find({note: 'it\'s ) here'})"#, risk: R, read: true, write: true, safe: false, preview: "find", exec: "err" },
    Row { text: r#"db.orders.find({note: ".deleteMany("})"#, risk: R, read: true, write: true, safe: false, preview: "find", exec: r#"find orders {"note":".deleteMany("}"# },
    Row { text: r#"db.orders.find({note: "// not a comment"})"#, risk: R, read: true, write: true, safe: false, preview: "find", exec: r#"find orders {"note":"// not a comment"}"# },
    Row { text: r#"db.orders.find({_id: ObjectId("64f123456789abcdef123456"), n: NumberLong(5), i: NumberInt("7")})"#, risk: R, read: true, write: true, safe: false, preview: "find", exec: r#"find orders {"_id":{"$oid":"64f123456789abcdef123456"},"n":5,"i":7}"# },
    Row { text: r#"db.orders.find({at: ISODate("2024-01-01T00:00:00Z")})"#, risk: R, read: true, write: true, safe: false, preview: "find", exec: r#"find orders {"at":{"$date":"2024-01-01T00:00:00Z"}}"# },
    Row { text: r#"db.orders.find({at: {$gt: new Date("2024-01-01T00:00:00Z")}})"#, risk: R, read: true, write: true, safe: false, preview: "find", exec: "err" },
    Row { text: r#"db.orders.find({_id: ObjectId("nothex")})"#, risk: R, read: true, write: true, safe: false, preview: "find", exec: "err" },
    Row { text: "db.orders.find({a: 1,})", risk: R, read: true, write: true, safe: false, preview: "find", exec: r#"find orders {"a":1}"# },
    Row { text: r#"{"status": "open"}"#, risk: R, read: true, write: true, safe: false, preview: r#"{"status":"open"}"#, exec: r#"find - {"status":"open"}"# },
    Row { text: r#"{status: "open"}"#, risk: R, read: true, write: true, safe: false, preview: r#"{status:"open"}"#, exec: r#"find - {"status":"open"}"# },
    Row { text: "{}", risk: R, read: true, write: true, safe: false, preview: "{}", exec: "find - {}" },
    Row { text: r#"{"status": "open"};"#, risk: R, read: true, write: true, safe: false, preview: r#"{"status":"open"};"#, exec: r#"find - {"status":"open"}"# },
    Row { text: r#"[{"$match": {"a": 1}}, {"$merge": "t"}]"#, risk: W, read: false, write: true, safe: false, preview: r#"[{"$match":{"a":1}},{"$merge":"t"}]"#, exec: r#"aggregate - [{"$match":{"a":1}},{"$merge":"t"}]"# },
    Row { text: r#"[{"$match": {"a": 1}}]"#, risk: R, read: true, write: true, safe: false, preview: r#"[{"$match":{"a":1}}]"#, exec: r#"aggregate - [{"$match":{"a":1}}]"# },
    Row { text: r#"{"op": "deleteMany", "filter": {"a": 1}}"#, risk: W, read: false, write: true, safe: false, preview: "deletemany", exec: r#"deleteMany - {"a":1}"# },
    Row { text: r#"{"op": "insertOne", "doc": {"a": 1}}"#, risk: W, read: false, write: true, safe: true, preview: "insertone", exec: r#"insertOne - {"a":1}"# },
    Row { text: r#"{"op": "countDocuments", "filter": {"a": 1}}"#, risk: R, read: true, write: true, safe: false, preview: "countdocuments", exec: r#"count - {"a":1}"# },
    Row { text: r#"{"op": "find", "filter": {"op": "deleteMany"}}"#, risk: R, read: true, write: true, safe: false, preview: "find", exec: "err" },
    Row { text: r#"{"op": "deleteMany", "filter": {"op": "find"}}"#, risk: W, read: false, write: true, safe: false, preview: "deletemany", exec: r#"deleteMany - {"op":"find"}"# },
    Row { text: r#"{"op": "find", "op": "deleteMany"}"#, risk: W, read: false, write: true, safe: false, preview: "deletemany", exec: "err" },
    Row { text: r#"{"op": "deleteMany", "op": "insertOne"}"#, risk: W, read: false, write: true, safe: false, preview: "insertone", exec: "err" },
    Row { text: r#"{"op": "drop"}"#, risk: X, read: false, write: true, safe: false, preview: "drop", exec: "err" },
    Row { text: r#"{"op": "frobnicate"}"#, risk: W, read: false, write: true, safe: false, preview: "frobnicate", exec: "err" },
    Row { text: r#"{"op": 5}"#, risk: W, read: false, write: true, safe: false, preview: r#"{"op":5}"#, exec: "err" },
    Row { text: r#"{op: "deleteMany", filter: {note: "delete"}}"#, risk: W, read: false, write: true, safe: false, preview: "deletemany", exec: r#"deleteMany - {"note":"delete"}"# },
    Row { text: r#"{"op": "aggregate", "pipeline": [{"$out": "t"}]}"#, risk: W, read: false, write: true, safe: false, preview: "aggregate", exec: r#"aggregate - [{"$out":"t"}]"# },
    Row { text: r#"{"op": "find", "pipeline": [{"$merge": "t"}]}"#, risk: W, read: false, write: true, safe: false, preview: "find", exec: "err" },
    Row { text: r#"{"op": "bulkWrite", "ops": [{"insertOne": {"document": {"a": 1}}}]}"#, risk: W, read: false, write: true, safe: false, preview: "bulkwrite", exec: r#"bulkWrite - [insertOne {"a":1}]"# },
    Row { text: r#"{"op": "insertOne", "doc": {"note": "please delete this"}}"#, risk: W, read: false, write: true, safe: true, preview: "insertone", exec: r#"insertOne - {"note":"please delete this"}"# },
    Row { text: "listDatabases", risk: R, read: true, write: true, safe: false, preview: "listDatabases", exec: "list dbs" },
    Row { text: "show dbs", risk: R, read: true, write: true, safe: false, preview: "showdbs", exec: "list dbs" },
    Row { text: "show  collections", risk: R, read: true, write: true, safe: false, preview: "showcollections", exec: "list colls" },
    Row { text: "listCollections", risk: R, read: true, write: true, safe: false, preview: "listCollections", exec: "list colls" },
    Row { text: "listdatabase", risk: R, read: true, write: true, safe: false, preview: "listdatabase", exec: "err" },
    Row { text: "", risk: W, read: false, write: false, safe: false, preview: "write", exec: "err" },
    Row { text: "hello", risk: W, read: false, write: false, safe: false, preview: "hello", exec: "err" },
    Row { text: "db.orders.find({a: 1}", risk: R, read: true, write: true, safe: false, preview: "find", exec: "err" },
    Row { text: r#"db.orders.find({a: "unterminated})"#, risk: R, read: true, write: true, safe: false, preview: "find", exec: "err" },
];

fn json(doc: &Document) -> String {
    Bson::Document(doc.clone())
        .into_relaxed_extjson()
        .to_string()
}

fn coll(c: &Option<String>) -> &str {
    c.as_deref().unwrap_or("-")
}

fn listing(text: &str) -> Option<&'static str> {
    let n: String = text
        .trim()
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect::<String>()
        .to_ascii_lowercase();
    match n.as_str() {
        "listdatabases" | "showdatabases" | "showdbs" => Some("list dbs"),
        "listcollections" | "showcollections" => Some("list colls"),
        _ => None,
    }
}

fn old_exec(text: &str) -> String {
    if let Some(l) = listing(text) {
        return l.into();
    }
    let call = match parse_mongo_call(text) {
        Ok(c) => c,
        Err(_) => return "err".into(),
    };
    match call {
        MongoCall::Find(p) => {
            let mut s = format!("find {} {}", coll(&p.collection), json(&p.filter));
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
        MongoCall::Aggregate(a) => format!(
            "aggregate {} [{}]",
            coll(&a.collection),
            a.pipeline.iter().map(json).collect::<Vec<_>>().join(",")
        ),
        MongoCall::Count { collection, filter } => {
            format!("count {} {}", coll(&collection), json(&filter))
        }
        MongoCall::Distinct {
            collection,
            field,
            filter,
        } => format!("distinct {} {field} {}", coll(&collection), json(&filter)),
        MongoCall::Write(w) => match w {
            ShellWrite::InsertOne {
                collection,
                document,
            } => format!("insertOne {} {}", coll(&collection), json(&document)),
            ShellWrite::InsertMany {
                collection,
                documents,
            } => format!(
                "insertMany {} [{}]",
                coll(&collection),
                documents.iter().map(json).collect::<Vec<_>>().join(",")
            ),
            ShellWrite::UpdateOne {
                collection,
                filter,
                update,
            } => format!(
                "updateOne {} {} {}",
                coll(&collection),
                json(&filter),
                json(&update)
            ),
            ShellWrite::UpdateMany {
                collection,
                filter,
                update,
            } => format!(
                "updateMany {} {} {}",
                coll(&collection),
                json(&filter),
                json(&update)
            ),
            ShellWrite::ReplaceOne {
                collection,
                filter,
                replacement,
            } => format!(
                "replaceOne {} {} {}",
                coll(&collection),
                json(&filter),
                json(&replacement)
            ),
            ShellWrite::DeleteOne { collection, filter } => {
                format!("deleteOne {} {}", coll(&collection), json(&filter))
            }
            ShellWrite::DeleteMany { collection, filter } => {
                format!("deleteMany {} {}", coll(&collection), json(&filter))
            }
            ShellWrite::FindOneAndUpdate {
                collection,
                filter,
                update,
            } => format!(
                "findOneAndUpdate {} {} {}",
                coll(&collection),
                json(&filter),
                json(&update)
            ),
            ShellWrite::Bulk { collection, models } => format!(
                "bulkWrite {} [{}]",
                coll(&collection),
                models
                    .iter()
                    .map(|m| match m {
                        BulkModel::InsertOne(d) => format!("insertOne {}", json(d)),
                        BulkModel::UpdateOne { filter, update } =>
                            format!("updateOne {} {}", json(filter), json(update)),
                        BulkModel::UpdateMany { filter, update } =>
                            format!("updateMany {} {}", json(filter), json(update)),
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

#[test]
fn old_paths_match_the_recorded_corpus() {
    let policy = MongoQueryPolicy;
    let mut failures = Vec::new();
    for row in CORPUS {
        let text = row.text;
        let got = (
            policy.classify(text),
            policy.validate(text, QueryMode::Read).is_ok(),
            policy.validate(text, QueryMode::Write).is_ok(),
            mongo_is_safe_dml(text),
            mongo_op_preview(text),
            old_exec(text),
        );
        let want = (
            row.risk,
            row.read,
            row.write,
            row.safe,
            row.preview.to_string(),
            row.exec.to_string(),
        );
        if got != want {
            failures.push(format!("{text:?}\n  got  {got:?}\n  want {want:?}"));
        }
    }
    assert!(failures.is_empty(), "\n{}", failures.join("\n"));
}
