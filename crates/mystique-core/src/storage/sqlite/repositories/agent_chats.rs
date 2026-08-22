use rusqlite::{params, Connection};
use uuid::Uuid;

use crate::domain::{
    now_iso, AgentChat, AgentMessage, AppendAgentMessageInput, CreateAgentChatInput,
};
use crate::error::{AppError, AppResult};

fn map_chat(row: &rusqlite::Row<'_>) -> rusqlite::Result<AgentChat> {
    Ok(AgentChat {
        id: row.get("id")?,
        connection_id: row.get("connection_id")?,
        title: row.get("title")?,
        provider: row.get("provider")?,
        model: row.get("model")?,
        created_at: row.get("created_at")?,
        updated_at: row.get("updated_at")?,
    })
}

fn map_message(row: &rusqlite::Row<'_>) -> rusqlite::Result<AgentMessage> {
    Ok(AgentMessage {
        id: row.get("id")?,
        chat_id: row.get("chat_id")?,
        role: row.get("role")?,
        content: row.get("content")?,
        tool_name: row.get("tool_name")?,
        draft_body: row.get("draft_body")?,
        created_at: row.get("created_at")?,
    })
}

pub fn list_for_connection(
    conn: &Connection,
    connection_id: &str,
) -> AppResult<Vec<AgentChat>> {
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
    conn.query_row(
        "
        SELECT id, connection_id, title, provider, model, created_at, updated_at
        FROM agent_chats
        WHERE id = ?1
        ",
        params![id],
        map_chat,
    )
    .map_err(|e| match e {
        rusqlite::Error::QueryReturnedNoRows => AppError::NotFound(format!("chat `{id}`")),
        other => other.into(),
    })
}

pub fn create(conn: &Connection, input: CreateAgentChatInput) -> AppResult<AgentChat> {
    let id = Uuid::new_v4().to_string();
    let now = now_iso();
    let title = input
        .title
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or("New chat")
        .to_string();
    conn.execute(
        "
        INSERT INTO agent_chats (id, connection_id, title, provider, model, created_at, updated_at)
        VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
        ",
        params![
            id,
            input.connection_id,
            title,
            input.provider,
            input.model,
            now,
            now
        ],
    )?;
    get_by_id(conn, &id)
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
    conn.execute(
        "
        UPDATE agent_chats
           SET title = ?2, provider = ?3, model = ?4, updated_at = ?5
         WHERE id = ?1
        ",
        params![id, title, provider, model, now],
    )?;
    get_by_id(conn, id)
}

pub fn touch(conn: &Connection, id: &str) -> AppResult<()> {
    conn.execute(
        "UPDATE agent_chats SET updated_at = ?2 WHERE id = ?1",
        params![id, now_iso()],
    )?;
    Ok(())
}

pub fn delete(conn: &Connection, id: &str) -> AppResult<bool> {
    let n = conn.execute("DELETE FROM agent_chats WHERE id = ?1", params![id])?;
    Ok(n > 0)
}

pub fn list_messages(conn: &Connection, chat_id: &str) -> AppResult<Vec<AgentMessage>> {
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
    let id = Uuid::new_v4().to_string();
    let now = now_iso();
    conn.execute(
        "
        INSERT INTO agent_messages (id, chat_id, role, content, tool_name, draft_body, created_at)
        VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
        ",
        params![
            id,
            input.chat_id,
            input.role,
            input.content,
            input.tool_name,
            input.draft_body,
            now
        ],
    )?;
    touch(conn, &input.chat_id)?;
    Ok(AgentMessage {
        id,
        chat_id: input.chat_id,
        role: input.role,
        content: input.content,
        tool_name: input.tool_name,
        draft_body: input.draft_body,
        created_at: now,
    })
}
