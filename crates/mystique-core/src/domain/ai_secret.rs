use serde::{Deserialize, Serialize};

/// Opaque encrypted API-key row. The plaintext never leaves `mystique-agent`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiSecretBlob {
    pub provider: String,
    pub ciphertext: String,
    pub updated_at: String,
}
