use rusqlite::{params, Connection, OptionalExtension};

use crate::domain::now_iso;
use crate::error::AppResult;

#[derive(Debug, Clone)]
pub struct SecretRow {
    pub envelope: String,
    pub key_version: i64,
}

pub fn get(
    conn: &Connection,
    owner_kind: &str,
    owner_id: &str,
    kind: &str,
) -> AppResult<Option<SecretRow>> {
    let row = conn
        .query_row(
            "SELECT envelope, key_version FROM secrets
             WHERE owner_kind = ?1 AND owner_id = ?2 AND kind = ?3",
            params![owner_kind, owner_id, kind],
            |r| {
                Ok(SecretRow {
                    envelope: r.get(0)?,
                    key_version: r.get(1)?,
                })
            },
        )
        .optional()?;
    Ok(row)
}

pub fn exists(conn: &Connection, owner_kind: &str, owner_id: &str, kind: &str) -> AppResult<bool> {
    Ok(get(conn, owner_kind, owner_id, kind)?.is_some())
}

pub fn upsert(
    conn: &Connection,
    owner_kind: &str,
    owner_id: &str,
    kind: &str,
    envelope: &str,
    key_version: i64,
) -> AppResult<()> {
    conn.execute(
        "
        INSERT INTO secrets (owner_kind, owner_id, kind, envelope, key_version, updated_at)
        VALUES (?1, ?2, ?3, ?4, ?5, ?6)
        ON CONFLICT(owner_kind, owner_id, kind) DO UPDATE SET
          envelope = excluded.envelope,
          key_version = excluded.key_version,
          updated_at = excluded.updated_at
        ",
        params![owner_kind, owner_id, kind, envelope, key_version, now_iso()],
    )?;
    Ok(())
}

pub fn delete(conn: &Connection, owner_kind: &str, owner_id: &str, kind: &str) -> AppResult<bool> {
    let n = conn.execute(
        "DELETE FROM secrets WHERE owner_kind = ?1 AND owner_id = ?2 AND kind = ?3",
        params![owner_kind, owner_id, kind],
    )?;
    Ok(n > 0)
}

pub fn delete_owner(conn: &Connection, owner_kind: &str, owner_id: &str) -> AppResult<usize> {
    Ok(conn.execute(
        "DELETE FROM secrets WHERE owner_kind = ?1 AND owner_id = ?2",
        params![owner_kind, owner_id],
    )?)
}
