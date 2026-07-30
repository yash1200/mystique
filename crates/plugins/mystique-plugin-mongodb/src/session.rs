use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

use async_trait::async_trait;
use mongodb::bson::{doc, Bson, Document};
use mongodb::Client;
use mystique_connector_api::{
    CatalogColumnRow, ColumnInfo, ConnectorError, ConnectorResult, ConnectorSession,
    EngineCapabilities, PoolStatus, QueryColumn, QueryMode, QueryResult, ValidatedQuery,
};
use serde_json::{json, Value as JsonValue};

pub struct MongoSession {
    client: Client,
    max_size: u32,
    default_db: Option<String>,
    closed: AtomicBool,
}

impl MongoSession {
    pub fn new(client: Client, max_size: u32, default_db: Option<String>) -> Self {
        Self {
            client,
            max_size,
            default_db,
            closed: AtomicBool::new(false),
        }
    }
}

fn bson_to_json(b: Bson) -> JsonValue {
    match b {
        Bson::Double(f) => json!(f),
        Bson::String(s) => JsonValue::String(s),
        Bson::Array(arr) => JsonValue::Array(arr.into_iter().map(bson_to_json).collect()),
        Bson::Document(d) => {
            let mut map = serde_json::Map::new();
            for (k, v) in d {
                map.insert(k, bson_to_json(v));
            }
            JsonValue::Object(map)
        }
        Bson::Boolean(b) => JsonValue::Bool(b),
        Bson::Null => JsonValue::Null,
        Bson::Int32(i) => json!(i),
        Bson::Int64(i) => json!(i),
        Bson::ObjectId(oid) => JsonValue::String(oid.to_hex()),
        Bson::DateTime(dt) => JsonValue::String(dt.to_string()),
        other => JsonValue::String(other.to_string()),
    }
}

fn parse_filter(text: &str) -> ConnectorResult<Document> {
    let t = text.trim();

    // Bare JSON filter document
    if t.starts_with('{') {
        return parse_json_object_filter(t);
    }

    let lower = t.to_ascii_lowercase();

    // Shell-style: db.users.find() / db.users.find({}) / find({...})
    if let Some(find_at) = lower.find("find") {
        // Argument object after find(
        if let Some(brace_rel) = t[find_at..].find('{') {
            let abs = find_at + brace_rel;
            let end = t[abs..]
                .rfind('}')
                .map(|e| abs + e)
                .ok_or_else(|| ConnectorError::Query("unclosed find filter object `{`".into()))?;
            let slice = &t[abs..=end];
            let compact: String = slice.chars().filter(|c| !c.is_whitespace()).collect();
            if compact == "{}" {
                return Ok(Document::new());
            }
            return parse_json_object_filter(slice);
        }
        // db.users.find() or find() with no object → match all
        return Ok(Document::new());
    }

    Err(ConnectorError::Query(
        "Mongo query must be a JSON filter `{ ... }`, `db.collection.find()`, or `find({ ... })`."
            .into(),
    ))
}

fn parse_json_object_filter(slice: &str) -> ConnectorResult<Document> {
    crate::shell_filter::parse_filter_object(slice)
}

fn extract_collection(text: &str, explicit: Option<&str>) -> ConnectorResult<String> {
    if let Some(c) = explicit.filter(|s| !s.is_empty()) {
        return Ok(c.to_string());
    }
    // db.name.find
    let lower = text.to_ascii_lowercase();
    if let Some(idx) = lower.find(".find") {
        let before = &text[..idx];
        if let Some(dot) = before.rfind('.') {
            let name = before[dot + 1..].trim();
            if !name.is_empty() {
                return Ok(name.to_string());
            }
        }
    }
    Err(ConnectorError::InvalidConfig(
        "Mongo queries need a collection (use the Collection field or db.collection.find(...))."
            .into(),
    ))
}

#[async_trait]
impl ConnectorSession for MongoSession {
    fn engine_id(&self) -> &str {
        "mongodb"
    }

    fn capabilities(&self) -> EngineCapabilities {
        EngineCapabilities::mongodb_defaults()
    }

    async fn ping(&self) -> ConnectorResult<()> {
        if self.closed.load(Ordering::SeqCst) {
            return Err(ConnectorError::Connect("session closed".into()));
        }
        self.client
            .database("admin")
            .run_command(doc! { "ping": 1 })
            .await
            .map_err(|e| ConnectorError::Connect(e.to_string()))?;
        Ok(())
    }

    fn pool_status(&self) -> Option<PoolStatus> {
        Some(PoolStatus {
            engine: "mongodb".into(),
            max_size: self.max_size,
            active: 0,
            idle: 0,
            size: self.max_size,
        })
    }

    async fn close(&self) -> ConnectorResult<()> {
        if self
            .closed
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_ok()
        {
            self.client.clone().shutdown().await;
        }
        Ok(())
    }

    async fn execute_query(&self, query: &ValidatedQuery) -> ConnectorResult<QueryResult> {
        if self.closed.load(Ordering::SeqCst) {
            return Err(ConnectorError::Connect("session closed".into()));
        }
        if query.mode == QueryMode::Write {
            return Err(ConnectorError::NotSupported(
                "Mongo write execute is not implemented yet; only find/filter reads return a table."
                    .into(),
            ));
        }

        let start = Instant::now();
        let text_key = normalize_meta_command(&query.text);

        if matches!(
            text_key.as_str(),
            "listdatabases" | "showdatabases" | "showdbs"
        ) {
            return self.list_databases(query.mode, start).await;
        }
        if matches!(
            text_key.as_str(),
            "listcollections" | "showcollections"
        ) {
            let db_name = resolve_db_name(self, query)?;
            return self
                .list_collections(&db_name, query.mode, query.max_rows, start)
                .await;
        }

        let db_name = resolve_db_name(self, query)?;
        let coll_name = extract_collection(&query.text, query.collection.as_deref())?;
        let filter = parse_filter(&query.text)?;

        let coll = self
            .client
            .database(&db_name)
            .collection::<Document>(&coll_name);
        let mut cursor = coll
            .find(filter)
            .limit(query.max_rows as i64)
            .await
            .map_err(|e| ConnectorError::Query(e.to_string()))?;

        let mut docs = Vec::new();
        while cursor.advance().await.map_err(|e| ConnectorError::Query(e.to_string()))? {
            let d = cursor
                .deserialize_current()
                .map_err(|e| ConnectorError::Query(e.to_string()))?;
            docs.push(d);
            if docs.len() >= query.max_rows as usize {
                break;
            }
        }

        // Flatten documents into a table: union of keys
        let mut key_set: Vec<String> = Vec::new();
        for d in &docs {
            for k in d.keys() {
                if !key_set.iter().any(|x| x == k) {
                    key_set.push(k.clone());
                }
            }
        }
        // Prefer _id first
        if let Some(i) = key_set.iter().position(|k| k == "_id") {
            let id = key_set.remove(i);
            key_set.insert(0, id);
        }

        let columns: Vec<QueryColumn> = key_set
            .iter()
            .map(|k| {
                let sample = docs.iter().find_map(|d| d.get(k)).cloned();
                QueryColumn {
                    name: k.clone(),
                    data_type: Some(bson_type_label(sample.as_ref())),
                }
            })
            .collect();

        let mut rows = Vec::new();
        for d in docs {
            let mut cells = Vec::with_capacity(key_set.len());
            for k in &key_set {
                let v = d.get(k).cloned().unwrap_or(Bson::Null);
                cells.push(bson_to_json(v));
            }
            rows.push(cells);
        }

        let truncated = rows.len() as u32 >= query.max_rows;
        Ok(QueryResult {
            engine: "mongodb".into(),
            mode: query.mode,
            columns,
            row_count: rows.len() as u64,
            rows,
            truncated,
            duration_ms: start.elapsed().as_millis() as u64,
            message: Some(format!("find on {db_name}.{coll_name}")),
        })
    }

    async fn describe_table(
        &self,
        database: Option<&str>,
        _schema: Option<&str>,
        table: &str,
    ) -> ConnectorResult<Vec<ColumnInfo>> {
        if self.closed.load(Ordering::SeqCst) {
            return Err(ConnectorError::Connect("session closed".into()));
        }
        let db_name = database
            .map(str::to_string)
            .or_else(|| self.default_db.clone())
            .ok_or_else(|| {
                ConnectorError::InvalidConfig("Mongo describe_table requires database".into())
            })?;
        let coll = self
            .client
            .database(&db_name)
            .collection::<Document>(table);

        let mut cursor = coll
            .find(doc! {})
            .limit(50)
            .await
            .map_err(|e| ConnectorError::Query(e.to_string()))?;

        let mut keys: Vec<String> = Vec::new();
        let mut samples: std::collections::HashMap<String, Bson> =
            std::collections::HashMap::new();
        while cursor
            .advance()
            .await
            .map_err(|e| ConnectorError::Query(e.to_string()))?
        {
            let d = cursor
                .deserialize_current()
                .map_err(|e| ConnectorError::Query(e.to_string()))?;
            for (k, v) in d {
                if !keys.iter().any(|x| x == &k) {
                    keys.push(k.clone());
                }
                samples.entry(k).or_insert(v);
            }
        }
        if let Some(i) = keys.iter().position(|k| k == "_id") {
            let id = keys.remove(i);
            keys.insert(0, id);
        }

        Ok(keys
            .into_iter()
            .map(|name| {
                let pk = name == "_id";
                let sample = samples.get(&name);
                ColumnInfo {
                    name,
                    data_type: bson_type_label(sample),
                    nullable: !pk,
                    pk,
                    enum_values: None,
                }
            })
            .collect())
    }

    async fn list_catalog_columns(
        &self,
        database: Option<&str>,
        _schema: Option<&str>,
    ) -> ConnectorResult<Vec<CatalogColumnRow>> {
        if self.closed.load(Ordering::SeqCst) {
            return Err(ConnectorError::Connect("session closed".into()));
        }
        let db_name = database
            .map(str::to_string)
            .or_else(|| self.default_db.clone())
            .ok_or_else(|| {
                ConnectorError::InvalidConfig(
                    "Mongo list_catalog_columns requires database".into(),
                )
            })?;
        let names = self
            .client
            .database(&db_name)
            .list_collection_names()
            .await
            .map_err(|e| ConnectorError::Query(e.to_string()))?;

        let mut out = Vec::new();
        for name in names {
            let cols = self
                .describe_table(Some(&db_name), None, &name)
                .await
                .unwrap_or_default();
            for c in cols {
                out.push(CatalogColumnRow {
                    database: Some(db_name.clone()),
                    schema: None,
                    table: name.clone(),
                    column: c.name,
                    data_type: c.data_type,
                    nullable: c.nullable,
                    pk: c.pk,
                });
            }
        }
        Ok(out)
    }
}

fn bson_type_label(sample: Option<&Bson>) -> String {
    match sample {
        None | Some(Bson::Null) => "mixed".into(),
        Some(Bson::Double(_)) => "double".into(),
        Some(Bson::String(_)) => "string".into(),
        Some(Bson::Array(_)) => "array".into(),
        Some(Bson::Document(_)) => "object".into(),
        Some(Bson::Boolean(_)) => "bool".into(),
        Some(Bson::Int32(_)) => "int32".into(),
        Some(Bson::Int64(_)) => "int64".into(),
        Some(Bson::ObjectId(_)) => "ObjectId".into(),
        Some(Bson::DateTime(_)) => "date".into(),
        Some(Bson::Binary(_)) => "binData".into(),
        Some(Bson::RegularExpression(_)) => "regex".into(),
        Some(Bson::JavaScriptCode(_)) | Some(Bson::JavaScriptCodeWithScope(_)) => "javascript".into(),
        Some(Bson::Timestamp(_)) => "timestamp".into(),
        Some(Bson::Decimal128(_)) => "decimal".into(),
        Some(Bson::Symbol(_)) => "symbol".into(),
        Some(Bson::Undefined) => "undefined".into(),
        Some(Bson::MaxKey) => "maxKey".into(),
        Some(Bson::MinKey) => "minKey".into(),
        Some(Bson::DbPointer(_)) => "dbPointer".into(),
    }
}

fn normalize_meta_command(text: &str) -> String {
    text.trim()
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect::<String>()
        .to_ascii_lowercase()
}

fn resolve_db_name(session: &MongoSession, query: &ValidatedQuery) -> ConnectorResult<String> {
    if let Some(db) = query
        .database
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        return Ok(db.to_string());
    }
    if let Some(db) = session.default_db.as_deref().filter(|s| !s.is_empty()) {
        return Ok(db.to_string());
    }
    Err(ConnectorError::InvalidConfig(
        "No MongoDB database selected. Put `/dbname` in the connection URI, set pluginConfig.database, or pass database on the query."
            .into(),
    ))
}

impl MongoSession {
    async fn list_databases(
        &self,
        mode: QueryMode,
        start: Instant,
    ) -> ConnectorResult<QueryResult> {
        let names = self
            .client
            .list_database_names()
            .await
            .map_err(|e| ConnectorError::Query(e.to_string()))?;

        let columns = vec![QueryColumn {
            name: "name".into(),
            data_type: Some("string".into()),
        }];
        let rows: Vec<Vec<JsonValue>> = names
            .into_iter()
            .map(|n| vec![JsonValue::String(n)])
            .collect();
        let count = rows.len() as u64;

        Ok(QueryResult {
            engine: "mongodb".into(),
            mode,
            columns,
            row_count: count,
            rows,
            truncated: false,
            duration_ms: start.elapsed().as_millis() as u64,
            message: Some("listDatabases".into()),
        })
    }

    async fn list_collections(
        &self,
        db_name: &str,
        mode: QueryMode,
        max_rows: u32,
        start: Instant,
    ) -> ConnectorResult<QueryResult> {
        let mut names = self
            .client
            .database(db_name)
            .list_collection_names()
            .await
            .map_err(|e| ConnectorError::Query(e.to_string()))?;
        names.sort();
        let truncated = names.len() as u32 > max_rows;
        if truncated {
            names.truncate(max_rows as usize);
        }

        // Estimated counts (best-effort; empty on error)
        let mut rows = Vec::with_capacity(names.len());
        for name in &names {
            let count = self
                .client
                .database(db_name)
                .collection::<Document>(name)
                .estimated_document_count()
                .await
                .ok();
            rows.push(vec![
                JsonValue::String(name.clone()),
                count.map(|c| json!(c)).unwrap_or(JsonValue::Null),
            ]);
        }

        Ok(QueryResult {
            engine: "mongodb".into(),
            mode,
            columns: vec![
                QueryColumn {
                    name: "name".into(),
                    data_type: Some("string".into()),
                },
                QueryColumn {
                    name: "estimatedCount".into(),
                    data_type: Some("number".into()),
                },
            ],
            row_count: rows.len() as u64,
            rows,
            truncated,
            duration_ms: start.elapsed().as_millis() as u64,
            message: Some(format!("listCollections on {db_name}")),
        })
    }
}

#[cfg(test)]
mod catalog_live_tests {
    use super::*;
    use mystique_connector_api::{QueryMode, ValidatedQuery};
    use mongodb::Client;

    #[tokio::test]
    async fn list_databases_and_find_users_on_mystique() {
        let uri = "mongodb://admin:123@127.0.0.1:27017/mystique?authSource=admin";
        let client = match Client::with_uri_str(uri).await {
            Ok(c) => c,
            Err(e) => {
                eprintln!("skip: {e}");
                return;
            }
        };
        if client.database("admin").run_command(doc! { "ping": 1 }).await.is_err() {
            eprintln!("skip: mongo not reachable");
            return;
        }
        let session = MongoSession::new(client, 2, Some("mystique".into()));
        let dbs = session
            .execute_query(&ValidatedQuery {
                text: "listDatabases".into(),
                mode: QueryMode::Read,
                max_rows: 50,
                timeout_ms: 5000,
                collection: None,
                database: None,
            })
            .await
            .expect("listDatabases");
        let names: Vec<String> = dbs
            .rows
            .iter()
            .map(|r| r[0].as_str().unwrap_or("").to_string())
            .collect();
        assert!(names.iter().any(|n| n == "mystique"), "names={names:?}");

        let colls = session
            .execute_query(&ValidatedQuery {
                text: "listCollections".into(),
                mode: QueryMode::Read,
                max_rows: 50,
                timeout_ms: 5000,
                collection: None,
                database: Some("mystique".into()),
            })
            .await
            .expect("listCollections");
        assert!(colls.row_count >= 6, "expected seed collections, got {}", colls.row_count);

        let users = session
            .execute_query(&ValidatedQuery {
                text: "{}".into(),
                mode: QueryMode::Read,
                max_rows: 10,
                timeout_ms: 5000,
                collection: Some("users".into()),
                database: Some("mystique".into()),
            })
            .await
            .expect("find users");
        assert!(users.row_count >= 1, "expected user docs");
        assert!(users.columns.iter().any(|c| c.name == "_id"));
        println!(
            "ok: dbs={:?} collections={} sample_users={}",
            names, colls.row_count, users.row_count
        );
    }
}
