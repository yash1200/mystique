use rusqlite::{params, Connection, OptionalExtension};

use crate::domain::{now_iso, AiSecretBlob};
use crate::error::AppResult;

pub fn get(conn: &Connection, provider: &str) -> AppResult<Option<AiSecretBlob>> {
    let row = conn
        .query_row(
            "SELECT provider, ciphertext, updated_at FROM ai_secrets WHERE provider = ?1",
            params![provider],
            |r| {
                Ok(AiSecretBlob {
                    provider: r.get(0)?,
                    ciphertext: r.get(1)?,
                    updated_at: r.get(2)?,
                })
            },
        )
        .optional()?;
    Ok(row)
}

pub fn set(conn: &Connection, provider: &str, ciphertext: &str) -> AppResult<AiSecretBlob> {
    let now = now_iso();
    conn.execute(
        "
        INSERT INTO ai_secrets (provider, ciphertext, updated_at)
        VALUES (?1, ?2, ?3)
        ON CONFLICT(provider) DO UPDATE SET
          ciphertext = excluded.ciphertext,
          updated_at = excluded.updated_at
        ",
        params![provider, ciphertext, now],
    )?;
    Ok(AiSecretBlob {
        provider: provider.to_string(),
        ciphertext: ciphertext.to_string(),
        updated_at: now,
    })
}

pub fn delete(conn: &Connection, provider: &str) -> AppResult<bool> {
    let n = conn.execute("DELETE FROM ai_secrets WHERE provider = ?1", params![provider])?;
    Ok(n > 0)
}

pub fn list_providers(conn: &Connection) -> AppResult<Vec<String>> {
    let mut stmt = conn.prepare("SELECT provider FROM ai_secrets ORDER BY provider")?;
    let rows = stmt.query_map([], |r| r.get(0))?;
    let mut out = Vec::new();
    for row in rows {
        out.push(row?);
    }
    Ok(out)
}
