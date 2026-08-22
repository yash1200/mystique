-- In-app query-tab agent chats (one connection, many threads).

CREATE TABLE IF NOT EXISTS agent_chats (
  id             TEXT PRIMARY KEY,
  connection_id  TEXT NOT NULL REFERENCES connections (id) ON DELETE CASCADE,
  title          TEXT NOT NULL,
  provider       TEXT NOT NULL,
  model          TEXT NOT NULL,
  created_at     TEXT NOT NULL,
  updated_at     TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_agent_chats_connection
  ON agent_chats (connection_id, updated_at DESC);

CREATE TABLE IF NOT EXISTS agent_messages (
  id          TEXT PRIMARY KEY,
  chat_id     TEXT NOT NULL REFERENCES agent_chats (id) ON DELETE CASCADE,
  role        TEXT NOT NULL CHECK (role IN ('user', 'assistant', 'tool')),
  content     TEXT NOT NULL,
  tool_name   TEXT,
  draft_body  TEXT,
  created_at  TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_agent_messages_chat
  ON agent_messages (chat_id, created_at);
