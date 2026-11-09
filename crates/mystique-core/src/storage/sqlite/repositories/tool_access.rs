//! `tool_access`: per-connection and per-database access levels for MCP
//! and the built-in agent (F-02). The global defaults are app settings.

use rusqlite::{params, Connection, OptionalExtension};

use crate::domain::now_iso;
use crate::error::AppResult;

use super::ids::parse_id;
use super::settings;

/// Global default for MCP. Absent = `none`.
pub const SETTING_MCP_LEVEL: &str = "mcp.access.level";
/// Global default for the built-in agent. Absent = `read`.
pub const SETTING_AGENT_LEVEL: &str = "agent.access.level";

/// Settings the levels replace (migration 014 reads and removes them).
pub const LEGACY_MCP_POLICY: &str = "mcp.policy";
pub const LEGACY_MCP_ALLOWLIST: &str = "mcp.allowlist";

/// One stored override. `database` is empty for the connection itself.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolAccessRow {
    pub connection_id: String,
    pub database: String,
    pub level: Option<String>,
    pub ask_high_risk: bool,
}

fn row(r: &rusqlite::Row<'_>) -> rusqlite::Result<ToolAccessRow> {
    Ok(ToolAccessRow {
        connection_id: r.get::<_, i64>(0)?.to_string(),
        database: r.get(1)?,
        level: r.get(2)?,
        ask_high_risk: r.get::<_, i64>(3)? != 0,
    })
}

pub fn list_all(conn: &Connection) -> AppResult<Vec<ToolAccessRow>> {
    let mut stmt = conn.prepare(
        "SELECT connection_id, database, level, ask_high_risk FROM tool_access
         ORDER BY connection_id, database",
    )?;
    let rows = stmt.query_map([], row)?;
    Ok(rows.collect::<Result<_, _>>()?)
}

pub fn for_connection(conn: &Connection, connection_id: &str) -> AppResult<Vec<ToolAccessRow>> {
    let id = parse_id(connection_id)?;
    let mut stmt = conn.prepare(
        "SELECT connection_id, database, level, ask_high_risk FROM tool_access
         WHERE connection_id = ?1 ORDER BY database",
    )?;
    let rows = stmt.query_map([id], row)?;
    Ok(rows.collect::<Result<_, _>>()?)
}

/// Set (or with `None`, clear) the level for a connection or one database.
pub fn set_level(
    conn: &Connection,
    connection_id: &str,
    database: &str,
    level: Option<&str>,
) -> AppResult<()> {
    let id = parse_id(connection_id)?;
    conn.execute(
        "INSERT INTO tool_access (connection_id, database, level, updated_at)
         VALUES (?1, ?2, ?3, ?4)
         ON CONFLICT (connection_id, database) DO UPDATE SET
           level = excluded.level,
           updated_at = excluded.updated_at",
        params![id, database.trim(), level, now_iso()],
    )?;
    prune(conn)
}

/// "Ask me for high-risk writes" for one connection.
pub fn set_ask(conn: &Connection, connection_id: &str, ask: bool) -> AppResult<()> {
    let id = parse_id(connection_id)?;
    conn.execute(
        "INSERT INTO tool_access (connection_id, database, ask_high_risk, updated_at)
         VALUES (?1, '', ?2, ?3)
         ON CONFLICT (connection_id, database) DO UPDATE SET
           ask_high_risk = excluded.ask_high_risk,
           updated_at = excluded.updated_at",
        params![id, i64::from(ask), now_iso()],
    )?;
    prune(conn)
}

pub fn ask_high_risk(conn: &Connection, connection_id: &str) -> AppResult<bool> {
    let id = parse_id(connection_id)?;
    let ask: Option<i64> = conn
        .query_row(
            "SELECT ask_high_risk FROM tool_access WHERE connection_id = ?1 AND database = ''",
            [id],
            |r| r.get(0),
        )
        .optional()?;
    Ok(ask.unwrap_or(0) != 0)
}

/// Rows that inherit everything and ask nothing are the same as no row.
fn prune(conn: &Connection) -> AppResult<()> {
    conn.execute(
        "DELETE FROM tool_access WHERE level IS NULL AND ask_high_risk = 0",
        [],
    )?;
    Ok(())
}

/// Migration 014: the old `mcp.policy` + `mcp.allowlist` become levels.
///
/// The old model let MCP see only allowlisted connections, so the MCP default
/// becomes `none` and each allowlisted connection gets the policy's level.
/// That level is what the policy could actually do there: the write guard
/// asked for an unlock MCP could never give on production (so `read`), and
/// for a typed confirmation it could never type on staging (so destructive
/// statements were out: `write` at most). The agent was never allowlisted
/// and only reads; it keeps the `read` default. Both old keys are removed.
pub fn migrate_legacy_settings(conn: &Connection) -> AppResult<()> {
    let policy = settings::get(conn, LEGACY_MCP_POLICY)?.map(|s| s.value);
    let allowlist = settings::get(conn, LEGACY_MCP_ALLOWLIST)?.map(|s| s.value);
    if policy.is_none() && allowlist.is_none() {
        return Ok(());
    }
    let level = match policy
        .as_deref()
        .unwrap_or("")
        .trim()
        .to_ascii_lowercase()
        .replace('-', "_")
        .as_str()
    {
        "safe_write" => "write",
        "high_risk_write" => "full",
        _ => "read",
    };
    // Invalid JSON was "none" before, and still is.
    let ids: Vec<String> =
        serde_json::from_str(allowlist.as_deref().unwrap_or("[]").trim()).unwrap_or_default();
    if settings::get(conn, SETTING_MCP_LEVEL)?.is_none() {
        settings::set(conn, SETTING_MCP_LEVEL, "none")?;
    }
    for raw in ids {
        let Ok(id) = parse_id(&raw) else {
            continue;
        };
        let environment: Option<String> = conn
            .query_row(
                "SELECT environment FROM connections WHERE id = ?1",
                [id],
                |r| r.get(0),
            )
            .optional()?;
        let Some(environment) = environment else {
            continue;
        };
        let capped = match (environment.as_str(), level) {
            ("production", _) => "read",
            ("staging", "full") => "write",
            (_, level) => level,
        };
        conn.execute(
            "INSERT INTO tool_access (connection_id, database, level, updated_at)
             VALUES (?1, '', ?2, ?3)
             ON CONFLICT (connection_id, database) DO NOTHING",
            params![id, capped, now_iso()],
        )?;
    }
    settings::delete(conn, LEGACY_MCP_POLICY)?;
    settings::delete(conn, LEGACY_MCP_ALLOWLIST)?;
    Ok(())
}
