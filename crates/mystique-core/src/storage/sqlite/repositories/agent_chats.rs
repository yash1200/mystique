use rusqlite::{params, Connection};

use crate::domain::{
    now_iso, AgentChat, AgentMessage, AppendAgentMessageInput, CreateAgentChatInput,
};
use crate::error::{AppError, AppResult};
use crate::storage::sqlite::repositories::ids::{id_string, parse_id};

fn map_chat(row: &rusqlite::Row<'_>) -> rusqlite::Result<AgentChat> {
    Ok(AgentChat {
        id: id_string(row, "id")?,
        connection_id: id_string(row, "connection_id")?,
        title: row.get("title")?,
        provider: row.get("provider")?,
        model: row.get("model")?,
        created_at: row.get("created_at")?,
        updated_at: row.get("updated_at")?,
    })
}

fn map_message(row: &rusqlite::Row<'_>) -> rusqlite::Result<AgentMessage> {
    Ok(AgentMessage {
        id: id_string(row, "id")?,
        chat_id: id_string(row, "chat_id")?,
        role: row.get("role")?,
        content: row.get("content")?,
        tool_name: row.get("tool_name")?,
        draft_body: row.get("draft_body")?,
        created_at: row.get("created_at")?,
    })
}

pub fn list_for_connection(conn: &Connection, connection_id: &str) -> AppResult<Vec<AgentChat>> {
    let connection_id = parse_id(connection_id)?;
    let mut stmt = conn.prepare(
        "
        SELECT id, connection_id, title, provider, model, created_at, updated_at
        FROM agent_chats
        WHERE connection_id = ?1
        ORDER BY updated_at DESC
        LIMIT 50
        ",
    )?;
    let rows = stmt.query_map(params![connection_id], map_chat)?;
    rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
}

pub fn get_by_id(conn: &Connection, id: &str) -> AppResult<AgentChat> {
    let id_n = parse_id(id)?;
    conn.query_row(
        "
        SELECT id, connection_id, title, provider, model, created_at, updated_at
        FROM agent_chats
        WHERE id = ?1
        ",
        params![id_n],
        map_chat,
    )
    .map_err(|e| match e {
        rusqlite::Error::QueryReturnedNoRows => AppError::NotFound(format!("chat `{id}`")),
        other => other.into(),
    })
}

pub fn create(conn: &Connection, input: CreateAgentChatInput) -> AppResult<AgentChat> {
    let now = now_iso();
    let title = input
        .title
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or("New chat")
        .to_string();
    let connection_id = parse_id(&input.connection_id)?;
    conn.execute(
        "
        INSERT INTO agent_chats (connection_id, title, provider, model, created_at, updated_at)
        VALUES (?1, ?2, ?3, ?4, ?5, ?6)
        ",
        params![connection_id, title, input.provider, input.model, now, now],
    )?;
    get_by_id(conn, &conn.last_insert_rowid().to_string())
}

pub fn update_meta(
    conn: &Connection,
    id: &str,
    title: Option<&str>,
    provider: Option<&str>,
    model: Option<&str>,
) -> AppResult<AgentChat> {
    let existing = get_by_id(conn, id)?;
    let title = title
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or(&existing.title);
    let provider = provider
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or(&existing.provider);
    let model = model
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or(&existing.model);
    let now = now_iso();
    let id_n = parse_id(id)?;
    conn.execute(
        "
        UPDATE agent_chats
           SET title = ?2, provider = ?3, model = ?4, updated_at = ?5
         WHERE id = ?1
        ",
        params![id_n, title, provider, model, now],
    )?;
    get_by_id(conn, id)
}

pub fn touch(conn: &Connection, id: &str) -> AppResult<()> {
    let id_n = parse_id(id)?;
    conn.execute(
        "UPDATE agent_chats SET updated_at = ?2 WHERE id = ?1",
        params![id_n, now_iso()],
    )?;
    Ok(())
}

pub fn delete(conn: &Connection, id: &str) -> AppResult<bool> {
    let id_n = parse_id(id)?;
    let n = conn.execute("DELETE FROM agent_chats WHERE id = ?1", params![id_n])?;
    Ok(n > 0)
}

pub fn list_messages(conn: &Connection, chat_id: &str) -> AppResult<Vec<AgentMessage>> {
    let chat_id = parse_id(chat_id)?;
    let mut stmt = conn.prepare(
        "
        SELECT id, chat_id, role, content, tool_name, draft_body, created_at
        FROM agent_messages
        WHERE chat_id = ?1
        ORDER BY created_at ASC
        ",
    )?;
    let rows = stmt.query_map(params![chat_id], map_message)?;
    rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
}

pub fn append_message(
    conn: &Connection,
    input: AppendAgentMessageInput,
) -> AppResult<AgentMessage> {
    let now = now_iso();
    let chat_id = parse_id(&input.chat_id)?;
    conn.execute(
        "
        INSERT INTO agent_messages (chat_id, role, content, tool_name, draft_body, created_at)
        VALUES (?1, ?2, ?3, ?4, ?5, ?6)
        ",
        params![
            chat_id,
            input.role,
            input.content,
            input.tool_name,
            input.draft_body,
            now
        ],
    )?;
    touch(conn, &input.chat_id)?;
    Ok(AgentMessage {
        id: conn.last_insert_rowid().to_string(),
        chat_id: input.chat_id,
        role: input.role,
        content: input.content,
        tool_name: input.tool_name,
        draft_body: input.draft_body,
        created_at: now,
    })
}
