-- Mystique app metadata — current schema.
-- Fresh installs apply this once. Incremental SQL lives in later files if added.

-- -----------------------------------------------------------------------------
-- 1. Connections
-- -----------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS connections (
  id                INTEGER PRIMARY KEY AUTOINCREMENT,
  name              TEXT NOT NULL,
  engine            TEXT NOT NULL,
  color_id          TEXT NOT NULL DEFAULT 'slate',
  environment       TEXT NOT NULL DEFAULT 'local' CHECK (
    environment IN ('local', 'staging', 'production')
  ),
  read_only         INTEGER NOT NULL DEFAULT 1 CHECK (
    read_only IN (0, 1)
  ),
  notes             TEXT,
  plugin_config     TEXT NOT NULL DEFAULT '{}',
  secret_ref        TEXT,
  last_connected_at TEXT,
  created_at        TEXT NOT NULL,
  updated_at        TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_connections_engine ON connections (engine);
CREATE INDEX IF NOT EXISTS idx_connections_name ON connections (name COLLATE NOCASE);

-- -----------------------------------------------------------------------------
-- 2. Saved Queries
-- -----------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS saved_queries (
  id            INTEGER PRIMARY KEY AUTOINCREMENT,
  connection_id INTEGER NOT NULL REFERENCES connections (id) ON DELETE CASCADE,
  name          TEXT NOT NULL,
  body          TEXT NOT NULL,
  language      TEXT,
  sort_order    INTEGER NOT NULL DEFAULT 0,
  created_at    TEXT NOT NULL,
  updated_at    TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_saved_queries_connection
  ON saved_queries (connection_id, sort_order);

-- -----------------------------------------------------------------------------
-- 3. Query History
-- -----------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS query_history (
  id            INTEGER PRIMARY KEY AUTOINCREMENT,
  connection_id INTEGER REFERENCES connections (id) ON DELETE SET NULL,
  engine        TEXT,
  body          TEXT NOT NULL,
  language      TEXT,
  success       INTEGER NOT NULL CHECK (
    success IN (0, 1)
  ),
  row_count     INTEGER,
  duration_ms   INTEGER,
  error_message TEXT,
  ran_at        TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_query_history_connection_ran
  ON query_history (connection_id, ran_at DESC);
CREATE INDEX IF NOT EXISTS idx_query_history_ran
  ON query_history (ran_at DESC);

-- -----------------------------------------------------------------------------
-- 4. App Settings
-- -----------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS app_settings (
  key        TEXT PRIMARY KEY,
  value      TEXT NOT NULL,
  updated_at TEXT NOT NULL
);

-- -----------------------------------------------------------------------------
-- 5. Workspace tabs
-- -----------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS workspace_tabs (
  id              TEXT PRIMARY KEY,
  connection_id   INTEGER NOT NULL REFERENCES connections (id) ON DELETE CASCADE,
  kind            TEXT NOT NULL CHECK (kind IN ('query', 'table', 'model')),
  title           TEXT NOT NULL,
  sort_order      INTEGER NOT NULL DEFAULT 0,
  is_active       INTEGER NOT NULL DEFAULT 0 CHECK (is_active IN (0, 1)),
  saved_query_id  INTEGER REFERENCES saved_queries (id) ON DELETE SET NULL,
  body            TEXT,
  language        TEXT,
  write_enabled   INTEGER NOT NULL DEFAULT 0 CHECK (write_enabled IN (0, 1)),
  table_id        TEXT,
  model_database  TEXT,
  is_dirty        INTEGER NOT NULL DEFAULT 0 CHECK (is_dirty IN (0, 1)),
  created_at      TEXT NOT NULL,
  updated_at      TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_workspace_tabs_connection
  ON workspace_tabs (connection_id, sort_order);

-- -----------------------------------------------------------------------------
-- 6. Agent chats
-- -----------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS agent_chats (
  id             INTEGER PRIMARY KEY AUTOINCREMENT,
  connection_id  INTEGER NOT NULL REFERENCES connections (id) ON DELETE CASCADE,
  title          TEXT NOT NULL,
  provider       TEXT NOT NULL,
  model          TEXT NOT NULL,
  created_at     TEXT NOT NULL,
  updated_at     TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_agent_chats_connection
  ON agent_chats (connection_id, updated_at DESC);

CREATE TABLE IF NOT EXISTS agent_messages (
  id          INTEGER PRIMARY KEY AUTOINCREMENT,
  chat_id     INTEGER NOT NULL REFERENCES agent_chats (id) ON DELETE CASCADE,
  role        TEXT NOT NULL CHECK (role IN ('user', 'assistant', 'tool')),
  content     TEXT NOT NULL,
  tool_name   TEXT,
  draft_body  TEXT,
  created_at  TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_agent_messages_chat
  ON agent_messages (chat_id, created_at);

-- -----------------------------------------------------------------------------
-- 7. Encrypted AI provider keys
-- -----------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS ai_secrets (
  provider    TEXT PRIMARY KEY,
  ciphertext  TEXT NOT NULL,
  updated_at  TEXT NOT NULL
);
