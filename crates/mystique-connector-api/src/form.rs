use serde::{Deserialize, Serialize};

/// JSON-schema-ish form description for dynamic connection UIs.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct ConnectionFormSchema {
    pub engine: String,
    pub fields: Vec<FormField>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
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
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub enum FormFieldType {
    Text,
    Number,
    Password,
    Boolean,
    Path,
    Uri,
}

/// Extra SSL fields shared by host/URI engines (not SQLite).
pub fn tls_form_fields() -> Vec<FormField> {
    vec![
        FormField {
            name: "sslMode".into(),
            label: "SSL mode".into(),
            field_type: FormFieldType::Text,
            required: false,
            secret: false,
            default: Some(serde_json::json!("disable")),
            placeholder: None,
            hint: Some("disable | prefer | require | verify-ca | verify-full".into()),
        },
        FormField {
            name: "caCertPath".into(),
            label: "CA certificate".into(),
            field_type: FormFieldType::Path,
            required: false,
            secret: false,
            default: None,
            placeholder: Some("/path/to/ca.pem".into()),
            hint: None,
        },
        FormField {
            name: "clientCertPath".into(),
            label: "Client certificate".into(),
            field_type: FormFieldType::Path,
            required: false,
            secret: false,
            default: None,
            placeholder: None,
            hint: None,
        },
        FormField {
            name: "clientKeyPath".into(),
            label: "Client key".into(),
            field_type: FormFieldType::Path,
            required: false,
            secret: true,
            default: None,
            placeholder: None,
            hint: None,
        },
    ]
}
