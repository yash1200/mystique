use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

use async_trait::async_trait;
use mongodb::bson::{doc, Bson, Document};
use mongodb::Client;
use mystique_connector_api::{
    cell_bool, ApplyTableEditsInput, ApplyTableEditsResult, CatalogColumnRow, CatalogDatabase,
    CatalogObject, ColumnInfo, ConnectorError, ConnectorResult, ConnectorSession,
    EngineCapabilities, FetchTablePreviewInput, ObjectStructure, PoolStatus, QueryColumn,
    QueryMode, QueryResult, StructureSection, TableAdminInput, TableAdminOp, TableAdminResult,
    TablePreviewResult, ValidatedQuery,
};
use serde::Deserialize;
use serde_json::{json, Value as JsonValue};

use crate::preview_filter::compile_mongo_preview_filter;
use crate::shell_filter::json_to_document;

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
            return self.execute_write(query).await;
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
                    // Documents have no column defaults; leave unset.
                    default_value: None,
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

    async fn apply_table_edits(
        &self,
        edits: &ApplyTableEditsInput,
    ) -> ConnectorResult<ApplyTableEditsResult> {
        if self.closed.load(Ordering::SeqCst) {
            return Err(ConnectorError::Connect("session closed".into()));
        }
        let db_name = edits
            .database
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_string)
            .or_else(|| self.default_db.clone())
            .ok_or_else(|| {
                ConnectorError::InvalidConfig("Mongo apply_table_edits requires database".into())
            })?;
        let coll = self
            .client
            .database(&db_name)
            .collection::<Document>(&edits.table);

        let mut updated = 0u32;
        let mut inserted = 0u32;
        let mut deleted = 0u32;

        for u in &edits.updates {
            let id = u
                .original
                .get("_id")
                .or_else(|| u.original.get("id"))
                .ok_or_else(|| {
                    ConnectorError::Query("Cannot update document: missing _id.".into())
                })?;
            if matches!(id, JsonValue::Null) || id.as_str() == Some("") {
                return Err(ConnectorError::Query(
                    "Cannot update document: missing _id.".into(),
                ));
            }
            let mut set_obj = serde_json::Map::new();
            for (k, v) in &u.values {
                if k == "_id" {
                    continue;
                }
                set_obj.insert(k.clone(), coerce_mongo_json(v));
            }
            if set_obj.is_empty() {
                continue;
            }
            let filter = json_to_document(&json!({ "_id": mongo_id_json(id) }))?;
            let update = json_to_document(&json!({ "$set": JsonValue::Object(set_obj) }))?;
            coll.update_one(filter, update)
                .await
                .map_err(|e| ConnectorError::Query(e.to_string()))?;
            updated += 1;
        }

        for row in &edits.inserts {
            let mut doc_map = serde_json::Map::new();
            for (k, v) in &row.values {
                if matches!(v, JsonValue::Null) || v.as_str() == Some("") {
                    continue;
                }
                if k == "_id" {
                    doc_map.insert(k.clone(), mongo_id_json(v));
                } else {
                    doc_map.insert(k.clone(), coerce_mongo_json(v));
                }
            }
            if doc_map.is_empty() {
                return Err(ConnectorError::Query(
                    "New document is empty — fill at least one field before saving.".into(),
                ));
            }
            let doc = json_to_document(&JsonValue::Object(doc_map))?;
            coll.insert_one(doc)
                .await
                .map_err(|e| ConnectorError::Query(e.to_string()))?;
            inserted += 1;
        }

        for d in &edits.deletes {
            let id = d
                .original
                .get("_id")
                .or_else(|| d.original.get("id"))
                .ok_or_else(|| {
                    ConnectorError::Query("Cannot delete document: missing _id.".into())
                })?;
            if matches!(id, JsonValue::Null) || id.as_str() == Some("") {
                return Err(ConnectorError::Query(
                    "Cannot delete document: missing _id.".into(),
                ));
            }
            let filter = json_to_document(&json!({ "_id": mongo_id_json(id) }))?;
            coll.delete_one(filter)
                .await
                .map_err(|e| ConnectorError::Query(e.to_string()))?;
            deleted += 1;
        }

        Ok(ApplyTableEditsResult {
            updated,
            inserted,
            deleted,
        })
    }

    async fn fetch_table_preview(
        &self,
        input: &FetchTablePreviewInput,
    ) -> ConnectorResult<TablePreviewResult> {
        if self.closed.load(Ordering::SeqCst) {
            return Err(ConnectorError::Connect("session closed".into()));
        }
        let start = Instant::now();
        let db_name = input
            .database
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_string)
            .or_else(|| self.default_db.clone())
            .ok_or_else(|| {
                ConnectorError::InvalidConfig("Mongo fetch_table_preview requires database".into())
            })?;
        let columns = self
            .describe_table(Some(&db_name), None, &input.table)
            .await?;
        let coll = self
            .client
            .database(&db_name)
            .collection::<Document>(&input.table);
        let filter = compile_mongo_preview_filter(&input.filters)?;
        let total_rows = if filter.is_empty() {
            coll.estimated_document_count()
                .await
                .unwrap_or(0)
        } else {
            coll.count_documents(filter.clone())
                .await
                .map_err(|e| ConnectorError::Query(e.to_string()))?
        };
        let mut find = coll.find(filter).sort(doc! { "_id": 1 });
        if input.offset > 0 {
            find = find.skip(input.offset as u64);
        }
        if input.max_rows > 0 {
            find = find.limit(input.max_rows as i64);
        }
        let mut cursor = find
            .await
            .map_err(|e| ConnectorError::Query(e.to_string()))?;

        let mut docs = Vec::new();
        while cursor
            .advance()
            .await
            .map_err(|e| ConnectorError::Query(e.to_string()))?
        {
            let d = cursor
                .deserialize_current()
                .map_err(|e| ConnectorError::Query(e.to_string()))?;
            docs.push(d);
            if docs.len() >= input.max_rows as usize {
                break;
            }
        }

        let key_set: Vec<String> = if columns.is_empty() {
            let mut keys = Vec::new();
            for d in &docs {
                for k in d.keys() {
                    if !keys.iter().any(|x| x == k) {
                        keys.push(k.clone());
                    }
                }
            }
            if let Some(i) = keys.iter().position(|k| k == "_id") {
                let id = keys.remove(i);
                keys.insert(0, id);
            }
            keys
        } else {
            columns.iter().map(|c| c.name.clone()).collect()
        };

        let mut rows = Vec::new();
        for d in &docs {
            let mut cells = Vec::with_capacity(key_set.len());
            for k in &key_set {
                let v = d.get(k).cloned().unwrap_or(Bson::Null);
                cells.push(bson_to_json(v));
            }
            rows.push(cells);
        }

        let preview_columns = if columns.is_empty() {
            key_set
                .iter()
                .map(|k| ColumnInfo {
                    name: k.clone(),
                    data_type: "mixed".into(),
                    nullable: k != "_id",
                    pk: k == "_id",
                    enum_values: None,
                    default_value: None,
                })
                .collect()
        } else {
            columns
        };

        let row_count = rows.len() as u64;
        let truncated = input.offset as u64 + row_count < total_rows;
        Ok(TablePreviewResult {
            engine: "mongodb".into(),
            columns: preview_columns,
            rows,
            row_count,
            total_rows,
            truncated,
            duration_ms: start.elapsed().as_millis() as u64,
        })
    }

    async fn list_catalog(
        &self,
        preferred_database: Option<&str>,
    ) -> ConnectorResult<Vec<CatalogDatabase>> {
        if self.closed.load(Ordering::SeqCst) {
            return Err(ConnectorError::Connect("session closed".into()));
        }
        let mut names = self
            .client
            .list_database_names()
            .await
            .map_err(|e| ConnectorError::Query(e.to_string()))?;
        names.sort();
        const SYSTEM: &[&str] = &["admin", "local", "config"];
        let pref = preferred_database.map(str::trim).unwrap_or("");
        names.sort_by(|a, b| {
            let asys = SYSTEM.contains(&a.as_str());
            let bsys = SYSTEM.contains(&b.as_str());
            if asys != bsys {
                return asys.cmp(&bsys);
            }
            if !pref.is_empty() && a == pref {
                return std::cmp::Ordering::Less;
            }
            if !pref.is_empty() && b == pref {
                return std::cmp::Ordering::Greater;
            }
            a.cmp(b)
        });
        if !pref.is_empty() && !names.iter().any(|n| n == pref) {
            names.insert(0, pref.to_string());
        }

        let mut out = Vec::new();
        for db_name in names {
            let is_system = SYSTEM.contains(&db_name.as_str());
            let mut objects = Vec::new();
            if !is_system {
                if let Ok(mut colls) = self.client.database(&db_name).list_collection_names().await {
                    colls.sort();
                    for name in colls {
                        let count = self
                            .client
                            .database(&db_name)
                            .collection::<Document>(&name)
                            .estimated_document_count()
                            .await
                            .unwrap_or(0);
                        objects.push(CatalogObject {
                            name,
                            schema: None,
                            database: Some(db_name.clone()),
                            kind: "collection".into(),
                            row_count: count,
                        });
                    }
                }
            }
            let meta = if is_system {
                Some("system".into())
            } else if objects.is_empty() {
                None
            } else {
                Some(format!("{} collections", objects.len()))
            };
            out.push(CatalogDatabase {
                name: db_name,
                meta,
                system: is_system,
                objects,
            });
        }
        Ok(out)
    }

    async fn apply_table_admin(
        &self,
        input: &TableAdminInput,
    ) -> ConnectorResult<TableAdminResult> {
        if self.closed.load(Ordering::SeqCst) {
            return Err(ConnectorError::Connect("session closed".into()));
        }
        let db_name = input
            .database
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .or(self.default_db.as_deref().filter(|s| !s.is_empty()))
            .ok_or_else(|| {
                ConnectorError::InvalidConfig(
                    "No MongoDB database selected for this collection action.".into(),
                )
            })?
            .to_string();
        let db = self.client.database(&db_name);
        let name = input.name.trim();
        let coll = db.collection::<Document>(name);

        match input.op {
            TableAdminOp::CreateStatement => {
                let statement = format!(
                    "// Collection {name} in database {db_name}\ndb.createCollection({});\n",
                    json!(name)
                );
                Ok(TableAdminResult::statement(input.op, statement))
            }
            TableAdminOp::Rename => {
                let new_name = input.new_name.as_deref().unwrap_or("").trim();
                mongo_rename_collection(&self.client, &db_name, name, new_name).await?;
                Ok(TableAdminResult::executed(
                    input.op,
                    format!("Renamed {name} to {new_name}."),
                ))
            }
            TableAdminOp::Drop => {
                coll.drop()
                    .await
                    .map_err(|e| ConnectorError::Query(e.to_string()))?;
                Ok(TableAdminResult::executed(
                    input.op,
                    format!("Dropped collection {name}."),
                ))
            }
            TableAdminOp::Truncate => {
                // Native clear: delete every document, keep the collection + indexes.
                coll.delete_many(doc! {})
                    .await
                    .map_err(|e| ConnectorError::Query(e.to_string()))?;
                Ok(TableAdminResult::executed(
                    input.op,
                    format!("Truncated collection {name}."),
                ))
            }
            TableAdminOp::Duplicate => {
                let new_name = input.new_name.as_deref().unwrap_or("").trim();
                mongo_copy_collection(&db, name, new_name).await?;
                Ok(TableAdminResult::executed(
                    input.op,
                    format!("Duplicated {name} to {new_name}."),
                ))
            }
        }
    }

    async fn describe_object_structure(
        &self,
        database: Option<&str>,
        _schema: Option<&str>,
        table: &str,
        kind: Option<&str>,
    ) -> ConnectorResult<ObjectStructure> {
        if self.closed.load(Ordering::SeqCst) {
            return Err(ConnectorError::Connect("session closed".into()));
        }
        let db_name = database
            .map(str::to_string)
            .or_else(|| self.default_db.clone())
            .ok_or_else(|| {
                ConnectorError::InvalidConfig(
                    "Mongo object structure requires database".into(),
                )
            })?;
        let db = self.client.database(&db_name);
        let coll = db.collection::<Document>(table);

        let cols = self
            .describe_table(Some(&db_name), None, table)
            .await?;
        let field_rows: Vec<Vec<String>> = cols
            .iter()
            .map(|c| {
                vec![
                    c.name.clone(),
                    c.data_type.clone(),
                    cell_bool(c.nullable),
                    cell_bool(c.pk),
                ]
            })
            .collect();
        let mut fields_sec = StructureSection::table(
            "fields",
            "Fields",
            vec![
                "Name".into(),
                "Sample type".into(),
                "Nullable".into(),
                "Id".into(),
            ],
            field_rows,
        )
        .with_description("Inferred from up to 50 sample documents.")
        .with_empty_message("No fields sampled (empty collection).");

        let mut index_rows = Vec::new();
        let mut cursor = coll
            .list_indexes()
            .await
            .map_err(|e| ConnectorError::Query(e.to_string()))?;
        while cursor
            .advance()
            .await
            .map_err(|e| ConnectorError::Query(e.to_string()))?
        {
            let model = cursor
                .deserialize_current()
                .map_err(|e| ConnectorError::Query(e.to_string()))?;
            let opts = model.options.as_ref();
            let name = opts
                .and_then(|o| o.name.clone())
                .unwrap_or_else(|| "—".into());
            let key = model
                .keys
                .iter()
                .map(|(k, v)| format!("{k}: {v}"))
                .collect::<Vec<_>>()
                .join(", ");
            let unique = opts.and_then(|o| o.unique).unwrap_or(false);
            let sparse = opts.and_then(|o| o.sparse).unwrap_or(false);
            index_rows.push(vec![
                name,
                if key.is_empty() {
                    "—".into()
                } else {
                    key
                },
                cell_bool(unique),
                cell_bool(sparse),
            ]);
        }
        let mut indexes_sec = StructureSection::table(
            "indexes",
            "Indexes",
            vec![
                "Name".into(),
                "Key".into(),
                "Unique".into(),
                "Sparse".into(),
            ],
            index_rows,
        )
        .with_empty_message("No indexes.");

        // Collection options / validator from listCollections.
        let mut validation_rows = Vec::new();
        let mut listed = db
            .list_collections()
            .filter(doc! { "name": table })
            .await
            .map_err(|e| ConnectorError::Query(e.to_string()))?;
        while listed
            .advance()
            .await
            .map_err(|e| ConnectorError::Query(e.to_string()))?
        {
            let spec = listed
                .deserialize_current()
                .map_err(|e| ConnectorError::Query(e.to_string()))?;
            validation_rows.push(vec![
                "type".into(),
                format!("{:?}", spec.collection_type),
            ]);
            let options = &spec.options;
            if let Some(validator) = &options.validator {
                validation_rows.push(vec![
                    "validator".into(),
                    serde_json::to_string_pretty(validator)
                        .unwrap_or_else(|_| format!("{validator:?}")),
                ]);
            }
            if let Some(level) = &options.validation_level {
                validation_rows.push(vec!["validationLevel".into(), format!("{level:?}")]);
            }
            if let Some(action) = &options.validation_action {
                validation_rows.push(vec!["validationAction".into(), format!("{action:?}")]);
            }
            if let Some(capped) = options.capped {
                validation_rows.push(vec!["capped".into(), cell_bool(capped)]);
            }
        }
        let mut options_sec = StructureSection::table(
            "options",
            "Collection options",
            vec!["Property".into(), "Value".into()],
            validation_rows,
        )
        .with_empty_message("No special options / validator.");

        let mut stats_rows = Vec::new();
        let count = coll
            .estimated_document_count()
            .await
            .map_err(|e| ConnectorError::Query(e.to_string()))?;
        stats_rows.push(vec!["estimatedDocumentCount".into(), count.to_string()]);
        let stats_sec = StructureSection::table(
            "stats",
            "Stats",
            vec!["Metric".into(), "Value".into()],
            stats_rows,
        );

        let object_kind = kind.unwrap_or("collection").to_string();
        Ok(ObjectStructure::new(
            format!("{db_name}.{table}"),
            vec![fields_sec, indexes_sec, options_sec, stats_sec],
        )
        .with_subtitle(format!("MongoDB · {object_kind}"))
        .with_object_kind(object_kind))
    }
}

async fn mongo_rename_collection(
    client: &Client,
    db_name: &str,
    from: &str,
    to: &str,
) -> ConnectorResult<()> {
    let renamed = client
        .database("admin")
        .run_command(doc! {
            "renameCollection": format!("{db_name}.{from}"),
            "to": format!("{db_name}.{to}"),
            "dropTarget": false,
        })
        .await;
    if renamed.is_ok() {
        return Ok(());
    }
    // Atlas / restricted users often cannot run renameCollection on admin.
    // Fall back to copy + drop so the SPI still works.
    let db = client.database(db_name);
    mongo_copy_collection(&db, from, to).await?;
    db.collection::<Document>(from)
        .drop()
        .await
        .map_err(|e| ConnectorError::Query(e.to_string()))?;
    Ok(())
}

async fn mongo_copy_collection(
    db: &mongodb::Database,
    from: &str,
    to: &str,
) -> ConnectorResult<()> {
    if to.is_empty() || to == from {
        return Err(ConnectorError::InvalidConfig(
            "Duplicate/rename target name is invalid.".into(),
        ));
    }
    let src = db.collection::<Document>(from);
    let dest = db.collection::<Document>(to);
    let _ = db.create_collection(to).await;
    let mut cursor = src
        .find(doc! {})
        .await
        .map_err(|e| ConnectorError::Query(e.to_string()))?;
    let mut batch: Vec<Document> = Vec::new();
    while cursor
        .advance()
        .await
        .map_err(|e| ConnectorError::Query(e.to_string()))?
    {
        let doc = cursor
            .deserialize_current()
            .map_err(|e| ConnectorError::Query(e.to_string()))?;
        batch.push(doc);
        if batch.len() >= 500 {
            dest.insert_many(std::mem::take(&mut batch))
                .await
                .map_err(|e| ConnectorError::Query(e.to_string()))?;
        }
    }
    if !batch.is_empty() {
        dest.insert_many(batch)
            .await
            .map_err(|e| ConnectorError::Query(e.to_string()))?;
    }
    Ok(())
}

fn mongo_id_json(id: &JsonValue) -> JsonValue {
    if let Some(s) = id.as_str() {
        if s.len() == 24 && s.chars().all(|c| c.is_ascii_hexdigit()) {
            return json!({ "$oid": s });
        }
    }
    id.clone()
}

fn coerce_mongo_json(v: &JsonValue) -> JsonValue {
    if let Some(s) = v.as_str() {
        let t = s.trim();
        if (t.starts_with('{') && t.ends_with('}')) || (t.starts_with('[') && t.ends_with(']')) {
            if let Ok(parsed) = serde_json::from_str::<JsonValue>(t) {
                return parsed;
            }
        }
    }
    v.clone()
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

#[derive(Debug, Deserialize)]
struct MongoWriteCmd {
    op: String,
    #[serde(default)]
    filter: Option<JsonValue>,
    #[serde(default)]
    update: Option<JsonValue>,
    #[serde(default)]
    doc: Option<JsonValue>,
}

impl MongoSession {
    async fn execute_write(&self, query: &ValidatedQuery) -> ConnectorResult<QueryResult> {
        let start = Instant::now();
        let cmd: MongoWriteCmd = serde_json::from_str(query.text.trim()).map_err(|e| {
            ConnectorError::Query(format!(
                "Mongo write expects JSON {{ op, filter?, update?, doc? }}: {e}"
            ))
        })?;
        let op = cmd.op.to_ascii_lowercase();
        let db_name = resolve_db_name(self, query)?;
        let coll_name = query
            .collection
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .ok_or_else(|| {
                ConnectorError::InvalidConfig("Mongo write requires collection".into())
            })?;
        let coll = self
            .client
            .database(&db_name)
            .collection::<Document>(coll_name);

        let message = match op.as_str() {
            "updateone" => {
                let filter_v = cmd.filter.ok_or_else(|| {
                    ConnectorError::Query("updateOne requires `filter`".into())
                })?;
                let update_v = cmd.update.ok_or_else(|| {
                    ConnectorError::Query("updateOne requires `update`".into())
                })?;
                let filter = json_to_document(&filter_v)?;
                let update = json_to_document(&update_v)?;
                let result = coll
                    .update_one(filter, update)
                    .await
                    .map_err(|e| ConnectorError::Query(e.to_string()))?;
                format!(
                    "updateOne on {db_name}.{coll_name} matched={} modified={}",
                    result.matched_count, result.modified_count
                )
            }
            "insertone" => {
                let doc_v = cmd.doc.ok_or_else(|| {
                    ConnectorError::Query("insertOne requires `doc`".into())
                })?;
                let doc = json_to_document(&doc_v)?;
                let result = coll
                    .insert_one(doc)
                    .await
                    .map_err(|e| ConnectorError::Query(e.to_string()))?;
                format!(
                    "insertOne on {db_name}.{coll_name} id={:?}",
                    result.inserted_id
                )
            }
            other => {
                return Err(ConnectorError::NotSupported(format!(
                    "Mongo write op `{other}` is not implemented."
                )));
            }
        };

        Ok(QueryResult {
            engine: "mongodb".into(),
            mode: query.mode,
            columns: Vec::new(),
            row_count: 0,
            rows: Vec::new(),
            truncated: false,
            duration_ms: start.elapsed().as_millis() as u64,
            message: Some(message),
        })
    }

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
