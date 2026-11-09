//! Field inference for document stores: a bounded sample of a collection
//! becomes a histogram of field paths and value types.
//!
//! The plugin samples and names each value's type; [`FieldHistogram`] counts.
//! Nested documents become dotted paths (`address.city`), array elements
//! `path[]` (`items[].sku`). Nothing here writes: a plugin samples with a
//! fixed read pipeline (`$sample`), never `$out` or `$merge`.

use std::collections::HashMap;
use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::cancel::QueryControl;

/// Documents sampled when the caller does not say.
pub const INFER_DEFAULT_SAMPLE: u32 = 100;
/// Most documents one inference reads.
pub const INFER_MAX_SAMPLE: u32 = 500;
/// Distinct paths kept. Further paths are counted in `fields_truncated`.
pub const INFER_MAX_FIELDS: usize = 500;
/// Nesting walked below the top level. Deeper values are typed, not opened.
pub const INFER_MAX_DEPTH: usize = 8;
/// Elements of one array walked. The rest of a long array is skipped.
pub const INFER_MAX_ARRAY_ELEMENTS: usize = 50;
/// Sampled document bytes read before the sample stops early.
pub const INFER_MAX_SAMPLE_BYTES: usize = 32 * 1024 * 1024;

/// Plugin input (session already resolved).
#[derive(Debug, Clone)]
pub struct InferFieldsInput {
    pub collection: String,
    pub database: Option<String>,
    /// Already clamped to `1..=INFER_MAX_SAMPLE`.
    pub sample: u32,
    pub timeout_ms: u64,
    pub control: Arc<QueryControl>,
}

/// Manager request: the session plus the collection.
#[derive(Debug, Clone)]
pub struct InferFieldsRequest {
    pub session_id: String,
    pub collection: String,
    pub database: Option<String>,
    pub sample: Option<u32>,
}

/// Clamp a requested sample size to `1..=INFER_MAX_SAMPLE`.
pub fn clamp_infer_sample(sample: Option<u32>) -> u32 {
    sample
        .unwrap_or(INFER_DEFAULT_SAMPLE)
        .clamp(1, INFER_MAX_SAMPLE)
}

/// How often one value type was seen at a path.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FieldTypeCount {
    #[serde(rename = "type")]
    pub type_name: String,
    pub count: u64,
}

/// One field path in the sample.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InferredField {
    /// Dotted path; `[]` marks array elements (`items[].sku`).
    pub path: String,
    /// Sampled documents that have this path at least once.
    pub present: u64,
    /// Values seen at this path by type, most frequent first.
    pub types: Vec<FieldTypeCount>,
}

/// A sampled collection's fields.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FieldInference {
    pub engine: String,
    pub collection: String,
    /// Documents read.
    pub sampled: u64,
    /// Paths in first-seen order.
    pub fields: Vec<InferredField>,
    /// More than [`INFER_MAX_FIELDS`] distinct paths: the rest are left out.
    pub fields_truncated: bool,
    /// The sample stopped at [`INFER_MAX_SAMPLE_BYTES`] before its size.
    pub sample_truncated: bool,
    pub duration_ms: u64,
}

struct FieldStats {
    path: String,
    present: u64,
    /// Document number that last counted toward `present`.
    last_doc: u64,
    types: Vec<FieldTypeCount>,
}

/// Counts paths and types across documents.
///
/// Call [`FieldHistogram::begin_document`] before each document's
/// [`FieldHistogram::observe`] calls, so a path seen twice in one document
/// (array elements) counts once toward `present`.
#[derive(Default)]
pub struct FieldHistogram {
    fields: Vec<FieldStats>,
    index: HashMap<String, usize>,
    documents: u64,
    fields_truncated: bool,
}

impl FieldHistogram {
    pub fn begin_document(&mut self) {
        self.documents += 1;
    }

    pub fn documents(&self) -> u64 {
        self.documents
    }

    /// One value of `type_name` at `path` in the current document.
    pub fn observe(&mut self, path: &str, type_name: &str) {
        let doc = self.documents;
        let slot = match self.index.get(path) {
            Some(&i) => i,
            None => {
                if self.fields.len() >= INFER_MAX_FIELDS {
                    self.fields_truncated = true;
                    return;
                }
                self.fields.push(FieldStats {
                    path: path.to_string(),
                    present: 0,
                    last_doc: 0,
                    types: Vec::new(),
                });
                self.index.insert(path.to_string(), self.fields.len() - 1);
                self.fields.len() - 1
            }
        };
        let Some(stats) = self.fields.get_mut(slot) else {
            return;
        };
        if stats.last_doc != doc {
            stats.last_doc = doc;
            stats.present += 1;
        }
        match stats.types.iter_mut().find(|t| t.type_name == type_name) {
            Some(seen) => seen.count += 1,
            None => stats.types.push(FieldTypeCount {
                type_name: type_name.to_string(),
                count: 1,
            }),
        }
    }

    pub fn finish(
        self,
        engine: &str,
        collection: &str,
        sample_truncated: bool,
        duration_ms: u64,
    ) -> FieldInference {
        let fields = self
            .fields
            .into_iter()
            .map(|mut f| {
                // Stable: equal counts keep first-seen order.
                f.types.sort_by_key(|t| std::cmp::Reverse(t.count));
                InferredField {
                    path: f.path,
                    present: f.present,
                    types: f.types,
                }
            })
            .collect();
        FieldInference {
            engine: engine.to_string(),
            collection: collection.to_string(),
            sampled: self.documents,
            fields,
            fields_truncated: self.fields_truncated,
            sample_truncated,
            duration_ms,
        }
    }
}

/// `parent.key`, or `key` at the top level.
pub fn child_path(parent: &str, key: &str) -> String {
    if parent.is_empty() {
        key.to_string()
    } else {
        format!("{parent}.{key}")
    }
}

/// `parent[]`: the path of an array's elements.
pub fn element_path(parent: &str) -> String {
    format!("{parent}[]")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counts_presence_once_per_document_and_types_per_value() {
        let mut h = FieldHistogram::default();
        h.begin_document();
        h.observe("_id", "objectId");
        h.observe("tags", "array");
        h.observe("tags[]", "string");
        h.observe("tags[]", "string");
        h.observe("tags[]", "int32");
        h.begin_document();
        h.observe("_id", "objectId");
        h.observe("tags", "null");
        let out = h.finish("mongodb", "c", false, 0);
        assert_eq!(out.sampled, 2);
        let tags = &out.fields[1];
        assert_eq!(tags.path, "tags");
        assert_eq!(tags.present, 2);
        assert_eq!(tags.types.len(), 2);
        let elements = &out.fields[2];
        assert_eq!(elements.path, "tags[]");
        assert_eq!(elements.present, 1);
        assert_eq!(
            elements.types,
            vec![
                FieldTypeCount {
                    type_name: "string".into(),
                    count: 2
                },
                FieldTypeCount {
                    type_name: "int32".into(),
                    count: 1
                },
            ]
        );
    }

    #[test]
    fn stops_adding_paths_at_the_cap() {
        let mut h = FieldHistogram::default();
        h.begin_document();
        for i in 0..(INFER_MAX_FIELDS + 5) {
            h.observe(&format!("f{i}"), "string");
        }
        h.observe("f0", "string");
        let out = h.finish("mongodb", "c", false, 0);
        assert_eq!(out.fields.len(), INFER_MAX_FIELDS);
        assert!(out.fields_truncated);
        assert_eq!(out.fields[0].types[0].count, 2);
    }

    #[test]
    fn sample_size_is_clamped() {
        assert_eq!(clamp_infer_sample(None), INFER_DEFAULT_SAMPLE);
        assert_eq!(clamp_infer_sample(Some(0)), 1);
        assert_eq!(clamp_infer_sample(Some(10_000)), INFER_MAX_SAMPLE);
    }

    #[test]
    fn paths_nest_with_dots_and_brackets() {
        assert_eq!(child_path("", "a"), "a");
        assert_eq!(child_path("a", "b"), "a.b");
        assert_eq!(child_path(&element_path("items"), "sku"), "items[].sku");
    }
}
