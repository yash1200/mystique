use std::path::Path;

use rusqlite::Connection;

use crate::error::AppResult;

pub const DB_FILE_NAME: &str = "mystique.db";

pub fn db_file_name() -> &'static str {
    DB_FILE_NAME
}

pub fn open_path(path: &Path) -> AppResult<Connection> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let conn = Connection::open(path)?;
    conn.execute_batch(
        "
        PRAGMA foreign_keys = ON;
        PRAGMA journal_mode = WAL;
        PRAGMA busy_timeout = 5000;
        ",
    )?;
    Ok(conn)
}
