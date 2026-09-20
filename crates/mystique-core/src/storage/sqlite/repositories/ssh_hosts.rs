//! TOFU store for SSH bastion host keys.

use rusqlite::{params, Connection, OptionalExtension};

use crate::domain::now_iso;
use crate::error::AppResult;

#[derive(Debug, Clone)]
pub struct KnownHost {
    pub host: String,
    pub port: u16,
    pub key_type: String,
    pub fingerprint: String,
    pub public_key: String,
}

fn normalize_host(host: &str) -> String {
    host.trim().to_ascii_lowercase()
}

pub fn lookup(conn: &Connection, host: &str, port: u16) -> AppResult<Option<KnownHost>> {
    let host = normalize_host(host);
    conn.query_row(
        "
        SELECT host, port, key_type, fingerprint, public_key
        FROM ssh_known_hosts
        WHERE host = ?1 AND port = ?2
        ",
        params![host, port as i64],
        |row| {
            Ok(KnownHost {
                host: row.get(0)?,
                port: row.get::<_, i64>(1)? as u16,
                key_type: row.get(2)?,
                fingerprint: row.get(3)?,
                public_key: row.get(4)?,
            })
        },
    )
    .optional()
    .map_err(Into::into)
}

pub fn upsert(conn: &Connection, host: &KnownHost) -> AppResult<()> {
    conn.execute(
        "
        INSERT INTO ssh_known_hosts (host, port, key_type, fingerprint, public_key, accepted_at)
        VALUES (?1, ?2, ?3, ?4, ?5, ?6)
        ON CONFLICT(host, port) DO UPDATE SET
          key_type = excluded.key_type,
          fingerprint = excluded.fingerprint,
          public_key = excluded.public_key,
          accepted_at = excluded.accepted_at
        ",
        params![
            normalize_host(&host.host),
            host.port as i64,
            host.key_type,
            host.fingerprint,
            host.public_key,
            now_iso(),
        ],
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::sqlite::migrate;

    fn db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
        migrate(&conn).unwrap();
        conn
    }

    #[test]
    fn upsert_then_lookup() {
        let conn = db();
        upsert(
            &conn,
            &KnownHost {
                host: "Bastion.Example".into(),
                port: 22,
                key_type: "ssh-ed25519".into(),
                fingerprint: "SHA256:abc".into(),
                public_key: "ssh-ed25519 AAAA".into(),
            },
        )
        .unwrap();
        let got = lookup(&conn, "bastion.example", 22).unwrap().unwrap();
        assert_eq!(got.fingerprint, "SHA256:abc");
        assert_eq!(got.host, "bastion.example");
    }
}
