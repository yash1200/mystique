//! Field inference: `$sample` a bounded number of documents and count the
//! field paths and BSON types. The pipeline is fixed here; it never takes
//! user stages, so `$out` and `$merge` cannot reach it. DocumentDB supports
//! `$sample` too, so both run this path.

use std::time::{Duration, Instant};

use mongodb::bson::{doc, Bson, Document};
use mystique_connector_api::{
    child_path, element_path, ConnectorError, ConnectorResult, FieldHistogram, FieldInference,
    InferFieldsInput, INACTIVITY_TIMEOUT_MS, INFER_MAX_ARRAY_ELEMENTS, INFER_MAX_DEPTH,
    INFER_MAX_SAMPLE_BYTES,
};

use super::encode::bson_type_label;
use super::query_failed;
use super::MongoSession;

pub(super) async fn infer_fields(
    session: &MongoSession,
    input: &InferFieldsInput,
) -> ConnectorResult<FieldInference> {
    session.ensure_open()?;
    let start = Instant::now();
    let db_name = session.resolve_database(input.database.as_deref())?;
    let coll = session
        .client
        .database(&db_name)
        .collection::<Document>(&input.collection);
    let pipeline = sample_pipeline(input.sample);
    let aggregate = coll
        .aggregate(pipeline)
        .max_time(Duration::from_millis(input.timeout_ms.max(1)));
    let mut cursor = tokio::select! {
        biased;
        _ = input.control.token.cancelled() => {
            return Err(ConnectorError::Query("Query cancelled.".into()));
        }
        opened = aggregate => opened.map_err(query_failed)?,
    };

    let mut histogram = FieldHistogram::default();
    let mut bytes = 0usize;
    let mut sample_truncated = false;
    let mut saw_doc = false;
    loop {
        let advanced = tokio::select! {
            biased;
            _ = input.control.token.cancelled() => {
                drop(cursor);
                return Err(ConnectorError::Query("Query cancelled.".into()));
            }
            _ = tokio::time::sleep(Duration::from_millis(INACTIVITY_TIMEOUT_MS)), if saw_doc => {
                input.control.cancel();
                drop(cursor);
                return Err(ConnectorError::Timeout(format!(
                    "No documents arrived for {INACTIVITY_TIMEOUT_MS} ms."
                )));
            }
            advanced = cursor.advance() => advanced.map_err(query_failed)?,
        };
        if !advanced {
            break;
        }
        saw_doc = true;
        bytes = bytes.saturating_add(cursor.current().as_bytes().len());
        let document = cursor.deserialize_current().map_err(query_failed)?;
        observe_document(&mut histogram, &document);
        if histogram.documents() >= u64::from(input.sample) {
            break;
        }
        if bytes >= INFER_MAX_SAMPLE_BYTES {
            sample_truncated = true;
            break;
        }
    }
    Ok(histogram.finish(
        "mongodb",
        &input.collection,
        sample_truncated,
        start.elapsed().as_millis() as u64,
    ))
}

/// The only pipeline this tool runs: one `$sample` stage.
pub(super) fn sample_pipeline(size: u32) -> Vec<Document> {
    vec![doc! { "$sample": { "size": i64::from(size.max(1)) } }]
}

/// Count one document's paths and types.
pub(super) fn observe_document(histogram: &mut FieldHistogram, document: &Document) {
    histogram.begin_document();
    for (key, value) in document {
        observe_value(histogram, &child_path("", key), value, 0);
    }
}

fn observe_value(histogram: &mut FieldHistogram, path: &str, value: &Bson, depth: usize) {
    histogram.observe(path, &type_name(value));
    if depth >= INFER_MAX_DEPTH {
        return;
    }
    match value {
        Bson::Document(inner) => {
            for (key, child) in inner {
                observe_value(histogram, &child_path(path, key), child, depth + 1);
            }
        }
        Bson::Array(items) => {
            let elements = element_path(path);
            for item in items.iter().take(INFER_MAX_ARRAY_ELEMENTS) {
                observe_value(histogram, &elements, item, depth + 1);
            }
        }
        _ => {}
    }
}

/// `bson_type_label`, except null is `null` (it says `mixed` for a column).
fn type_name(value: &Bson) -> String {
    match value {
        Bson::Null => "null".into(),
        other => bson_type_label(Some(other)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mongodb::bson::oid::ObjectId;
    use mongodb::bson::DateTime;

    fn fixture() -> Vec<Document> {
        vec![
            doc! {
                "_id": ObjectId::new(),
                "name": "Ada",
                "age": 36_i32,
                "address": { "city": "London", "zip": "N1" },
                "tags": ["math", "poetry"],
                "orders": [ { "sku": "A1", "qty": 2_i32 }, { "sku": "B2", "qty": 1_i64 } ],
                "joined": DateTime::from_millis(0),
            },
            doc! {
                "_id": ObjectId::new(),
                "name": "Grace",
                "age": 45.5,
                "address": { "city": "Arlington" },
                "tags": [],
                "nickname": Bson::Null,
            },
            doc! {
                "_id": ObjectId::new(),
                "name": "Edsger",
                "tags": ["graphs", 7_i32],
            },
        ]
    }

    fn infer(docs: &[Document]) -> FieldInference {
        let mut histogram = FieldHistogram::default();
        for document in docs {
            observe_document(&mut histogram, document);
        }
        histogram.finish("mongodb", "people", false, 0)
    }

    fn field<'a>(out: &'a FieldInference, path: &str) -> &'a mystique_connector_api::InferredField {
        out.fields
            .iter()
            .find(|f| f.path == path)
            .unwrap_or_else(|| panic!("no field {path}: {:?}", out.fields))
    }

    fn types(out: &FieldInference, path: &str) -> Vec<(String, u64)> {
        field(out, path)
            .types
            .iter()
            .map(|t| (t.type_name.clone(), t.count))
            .collect()
    }

    #[test]
    fn histogram_covers_nested_paths_arrays_and_mixed_types() {
        let out = infer(&fixture());
        assert_eq!(out.sampled, 3);
        assert_eq!(out.fields[0].path, "_id");
        assert_eq!(types(&out, "_id"), vec![("ObjectId".into(), 3)]);
        assert_eq!(field(&out, "name").present, 3);
        assert_eq!(field(&out, "age").present, 2);
        assert_eq!(
            types(&out, "age"),
            vec![("int32".into(), 1), ("double".into(), 1)]
        );
        assert_eq!(field(&out, "address.city").present, 2);
        assert_eq!(field(&out, "address.zip").present, 1);
        assert_eq!(field(&out, "tags").present, 3);
        assert_eq!(types(&out, "tags"), vec![("array".into(), 3)]);
        // Three string elements and one int32, over two documents.
        assert_eq!(
            types(&out, "tags[]"),
            vec![("string".into(), 3), ("int32".into(), 1)]
        );
        assert_eq!(field(&out, "tags[]").present, 2);
        assert_eq!(types(&out, "orders[]"), vec![("object".into(), 2)]);
        assert_eq!(
            types(&out, "orders[].qty"),
            vec![("int32".into(), 1), ("int64".into(), 1)]
        );
        assert_eq!(field(&out, "orders[].sku").present, 1);
        assert_eq!(types(&out, "joined"), vec![("date".into(), 1)]);
        assert_eq!(types(&out, "nickname"), vec![("null".into(), 1)]);
        assert!(!out.fields_truncated);
    }

    #[test]
    fn long_arrays_and_deep_nesting_are_bounded() {
        let long: Vec<Bson> = (0..1_000).map(Bson::Int32).collect();
        let mut deep = doc! { "leaf": 1_i32 };
        for _ in 0..20 {
            deep = doc! { "n": deep };
        }
        let out = infer(&[doc! { "long": long, "deep": deep }]);
        assert_eq!(
            types(&out, "long[]"),
            vec![("int32".into(), INFER_MAX_ARRAY_ELEMENTS as u64)]
        );
        let deepest = out
            .fields
            .iter()
            .map(|f| f.path.matches('.').count())
            .max()
            .unwrap_or(0);
        assert_eq!(deepest, INFER_MAX_DEPTH);
        assert!(out.fields.iter().all(|f| !f.path.ends_with("leaf")));
    }

    #[test]
    fn the_pipeline_is_one_sample_stage() {
        let pipeline = sample_pipeline(25);
        assert_eq!(pipeline.len(), 1);
        assert_eq!(pipeline[0], doc! { "$sample": { "size": 25_i64 } });
        let text = format!("{pipeline:?}");
        assert!(!text.contains("$out") && !text.contains("$merge"));
    }
}
