use serde::{Deserialize, Serialize};

/// JSON-schema-ish form description for dynamic connection UIs.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionFormSchema {
    pub engine: String,
    pub fields: Vec<FormField>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FormField {
    pub name: String,
    pub label: String,
    pub field_type: FormFieldType,
    #[serde(default)]
    pub required: bool,
    #[serde(default)]
    pub secret: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub placeholder: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hint: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum FormFieldType {
    Text,
    Number,
    Password,
    Boolean,
    Path,
    Uri,
}
