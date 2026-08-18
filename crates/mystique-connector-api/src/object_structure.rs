//! Plugin-owned object structure (columns, indexes, triggers, relations, …).
//! The frontend only renders sections; each engine fills them.

use serde::{Deserialize, Serialize};

/// Request structure for one table / view / collection.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ObjectStructureRequest {
    pub session_id: String,
    pub table: String,
    #[serde(default)]
    pub schema: Option<String>,
    #[serde(default)]
    pub database: Option<String>,
    /// `table` | `view` | `collection` (hint only; plugins may ignore).
    #[serde(default)]
    pub kind: Option<String>,
}

/// Full structure payload returned by a plugin.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ObjectStructure {
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subtitle: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub object_kind: Option<String>,
    pub sections: Vec<StructureSection>,
}

/// One named section of structure (columns, indexes, …).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StructureSection {
    pub id: String,
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Column headers for the table body.
    pub columns: Vec<String>,
    /// Rows of display strings (aligned to [`columns`]).
    pub rows: Vec<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub empty_message: Option<String>,
}

impl StructureSection {
    pub fn table(
        id: impl Into<String>,
        title: impl Into<String>,
        columns: Vec<String>,
        rows: Vec<Vec<String>>,
    ) -> Self {
        Self {
            id: id.into(),
            title: title.into(),
            description: None,
            columns,
            rows,
            empty_message: None,
        }
    }

    pub fn empty(
        id: impl Into<String>,
        title: impl Into<String>,
        empty_message: impl Into<String>,
    ) -> Self {
        Self {
            id: id.into(),
            title: title.into(),
            description: None,
            columns: Vec::new(),
            rows: Vec::new(),
            empty_message: Some(empty_message.into()),
        }
    }

    pub fn with_description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }

    pub fn with_empty_message(mut self, msg: impl Into<String>) -> Self {
        self.empty_message = Some(msg.into());
        self
    }
}

impl ObjectStructure {
    pub fn new(title: impl Into<String>, sections: Vec<StructureSection>) -> Self {
        Self {
            title: title.into(),
            subtitle: None,
            object_kind: None,
            sections,
        }
    }

    pub fn with_subtitle(mut self, subtitle: impl Into<String>) -> Self {
        self.subtitle = Some(subtitle.into());
        self
    }

    pub fn with_object_kind(mut self, kind: impl Into<String>) -> Self {
        self.object_kind = Some(kind.into());
        self
    }
}

/// Format optional values for structure tables.
pub fn cell_opt(v: Option<impl ToString>) -> String {
    v.map(|x| x.to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "—".into())
}

pub fn cell_bool(v: bool) -> String {
    if v {
        "yes".into()
    } else {
        "no".into()
    }
}
