-- Mystique app metadata: the whole schema, created once on a new database.
--
-- There are no migrations. `PRAGMA user_version` holds the schema version
-- (`SCHEMA_VERSION` in storage/sqlite/schema.rs). A database with another
-- version is moved aside and a new one is created; see docs/SQLITE_SCHEMA.md.
--
-- Conventions: ids are INTEGER AUTOINCREMENT (never reused); timestamps are
-- ISO-8601 TEXT in UTC; flags are INTEGER 0/1; JSON is TEXT.

-- -----------------------------------------------------------------------------
-- Connections
-- -----------------------------------------------------------------------------
CREATE TABLE connections (
  id                 INTEGER PRIMARY KEY AUTOINCREMENT,
  name               TEXT    NOT NULL,
  engine             TEXT    NOT NULL,
  color_id           TEXT    NOT NULL DEFAULT 'slate',
  environment        TEXT    NOT NULL DEFAULT 'local'
                       CHECK (environment IN ('local', 'staging', 'production')),
  read_only          INTEGER NOT NULL DEFAULT 0 CHECK (read_only IN (0, 1)),
  favorite           INTEGER NOT NULL DEFAULT 0 CHECK (favorite IN (0, 1)),
  notes              TEXT,
  -- Engine-owned options (host/port, URI, file path…).
  plugin_config      TEXT    NOT NULL DEFAULT '{}',
  ssh_config         TEXT    NOT NULL DEFAULT '{}',
  -- 'vault' when the password / SSH secret is sealed in `secrets`, else NULL.
  secret_ref         TEXT,
  ssh_secret_ref     TEXT,
  save_password      INTEGER NOT NULL DEFAULT 1 CHECK (save_password IN (0, 1)),
  -- NULL: the engine default. 0: no timeout.
  query_timeout_secs INTEGER CHECK (query_timeout_secs IS NULL OR query_timeout_secs >= 0),
  last_connected_at  TEXT,
  created_at         TEXT    NOT NULL,
  updated_at         TEXT    NOT NULL
);

CREATE INDEX idx_connections_name ON connections (name COLLATE NOCASE);
CREATE INDEX idx_connections_favorite ON connections (favorite, name COLLATE NOCASE);

-- -----------------------------------------------------------------------------
-- Secrets: AES-GCM envelopes sealed with the vault key (core/secrets.rs).
-- owner_kind 'connection' (owner_id = connections.id) or 'app' (owner_id 'app').
-- -----------------------------------------------------------------------------
CREATE TABLE secrets (
  owner_kind  TEXT    NOT NULL,
  owner_id    TEXT    NOT NULL,
  kind        TEXT    NOT NULL,
  envelope    TEXT    NOT NULL,
  key_version INTEGER NOT NULL,
  updated_at  TEXT    NOT NULL,
  PRIMARY KEY (owner_kind, owner_id, kind)
);

-- -----------------------------------------------------------------------------
-- Saved queries and history
-- -----------------------------------------------------------------------------
CREATE TABLE saved_queries (
  id            INTEGER PRIMARY KEY AUTOINCREMENT,
  connection_id INTEGER NOT NULL REFERENCES connections (id) ON DELETE CASCADE,
  name          TEXT    NOT NULL,
  body          TEXT    NOT NULL,
  language      TEXT,
  favorite      INTEGER NOT NULL DEFAULT 0 CHECK (favorite IN (0, 1)),
  sort_order    INTEGER NOT NULL DEFAULT 0,
  created_at    TEXT    NOT NULL,
  updated_at    TEXT    NOT NULL
);

CREATE INDEX idx_saved_queries_connection ON saved_queries (connection_id, sort_order);
CREATE INDEX idx_saved_queries_favorite ON saved_queries (connection_id, favorite, sort_order);

-- Append-only. Survives its connection (connection_id goes NULL).
CREATE TABLE query_history (
  id            INTEGER PRIMARY KEY AUTOINCREMENT,
  connection_id INTEGER REFERENCES connections (id) ON DELETE SET NULL,
  engine        TEXT,
  "database"    TEXT,
  kind          TEXT    NOT NULL DEFAULT 'query'
                  CHECK (kind IN ('query', 'grid_save', 'ddl', 'rollback', 'explain_analyze')),
  source        TEXT    NOT NULL DEFAULT 'ui' CHECK (source IN ('ui', 'agent', 'mcp')),
  body          TEXT    NOT NULL,
  language      TEXT,
  success       INTEGER NOT NULL CHECK (success IN (0, 1)),
  row_count     INTEGER,
  affected_rows INTEGER,
  duration_ms   INTEGER,
  error_message TEXT,
  ran_at        TEXT    NOT NULL
);

CREATE INDEX idx_query_history_connection_keyset
  ON query_history (connection_id, ran_at DESC, id DESC);
CREATE INDEX idx_query_history_ran ON query_history (ran_at DESC);

-- -----------------------------------------------------------------------------
-- Workspace: restored tabs, grid column layout, grid save rollbacks
-- -----------------------------------------------------------------------------
CREATE TABLE workspace_tabs (
  id             TEXT    PRIMARY KEY,
  connection_id  INTEGER NOT NULL REFERENCES connections (id) ON DELETE CASCADE,
  kind           TEXT    NOT NULL CHECK (kind IN ('query', 'table', 'model')),
  title          TEXT    NOT NULL,
  sort_order     INTEGER NOT NULL DEFAULT 0,
  is_active      INTEGER NOT NULL DEFAULT 0 CHECK (is_active IN (0, 1)),
  is_dirty       INTEGER NOT NULL DEFAULT 0 CHECK (is_dirty IN (0, 1)),
  saved_query_id INTEGER REFERENCES saved_queries (id) ON DELETE SET NULL,
  body           TEXT,
  language       TEXT,
  write_enabled  INTEGER NOT NULL DEFAULT 0 CHECK (write_enabled IN (0, 1)),
  table_id       TEXT,
  model_database TEXT,
  created_at     TEXT    NOT NULL,
  updated_at     TEXT    NOT NULL
);

CREATE INDEX idx_workspace_tabs_connection ON workspace_tabs (connection_id, sort_order);

-- connection_id is the profile id as text, or a session id for a connection
-- that has no saved profile, so it cannot be a foreign key. The trigger below
-- clears a profile's rows when it is deleted.
CREATE TABLE grid_column_layout (
  connection_id TEXT NOT NULL,
  table_id      TEXT NOT NULL,
  column_order  TEXT NOT NULL DEFAULT '[]',
  hidden        TEXT NOT NULL DEFAULT '[]',
  frozen        TEXT NOT NULL DEFAULT '[]',
  widths        TEXT NOT NULL DEFAULT '{}',
  updated_at    TEXT NOT NULL,
  PRIMARY KEY (connection_id, table_id)
);

CREATE TABLE grid_saves (
  save_id       TEXT PRIMARY KEY,
  connection_id TEXT NOT NULL,
  table_key     TEXT NOT NULL,
  undo          TEXT NOT NULL,
  created_at    TEXT NOT NULL
);

CREATE INDEX idx_grid_saves_table ON grid_saves (connection_id, table_key);

CREATE TRIGGER connections_clear_grid AFTER DELETE ON connections
BEGIN
  DELETE FROM grid_column_layout WHERE connection_id = CAST(OLD.id AS TEXT);
  DELETE FROM grid_saves WHERE connection_id = CAST(OLD.id AS TEXT);
END;

-- -----------------------------------------------------------------------------
-- Agent: tool access per connection / database, and query-tab chats
-- -----------------------------------------------------------------------------
-- database '' is the connection-wide row. level NULL inherits.
CREATE TABLE tool_access (
  connection_id INTEGER NOT NULL REFERENCES connections (id) ON DELETE CASCADE,
  database      TEXT    NOT NULL DEFAULT '',
  level         TEXT    CHECK (level IS NULL OR level IN ('none', 'read', 'write', 'full')),
  ask_high_risk INTEGER NOT NULL DEFAULT 0 CHECK (ask_high_risk IN (0, 1)),
  updated_at    TEXT    NOT NULL,
  PRIMARY KEY (connection_id, database)
);

CREATE TABLE agent_chats (
  id            INTEGER PRIMARY KEY AUTOINCREMENT,
  connection_id INTEGER NOT NULL REFERENCES connections (id) ON DELETE CASCADE,
  title         TEXT    NOT NULL,
  provider      TEXT    NOT NULL,
  model         TEXT    NOT NULL,
  created_at    TEXT    NOT NULL,
  updated_at    TEXT    NOT NULL
);

CREATE INDEX idx_agent_chats_connection ON agent_chats (connection_id, updated_at DESC);

CREATE TABLE agent_messages (
  id         INTEGER PRIMARY KEY AUTOINCREMENT,
  chat_id    INTEGER NOT NULL REFERENCES agent_chats (id) ON DELETE CASCADE,
  role       TEXT    NOT NULL CHECK (role IN ('user', 'assistant', 'tool')),
  content    TEXT    NOT NULL,
  tool_name  TEXT,
  draft_body TEXT,
  created_at TEXT    NOT NULL
);

CREATE INDEX idx_agent_messages_chat ON agent_messages (chat_id, created_at);

-- -----------------------------------------------------------------------------
-- App: settings and trusted SSH host keys
-- -----------------------------------------------------------------------------
CREATE TABLE app_settings (
  key        TEXT PRIMARY KEY,
  value      TEXT NOT NULL,
  updated_at TEXT NOT NULL
);

CREATE TABLE ssh_known_hosts (
  host        TEXT    NOT NULL,
  port        INTEGER NOT NULL,
  key_type    TEXT    NOT NULL,
  fingerprint TEXT    NOT NULL,
  public_key  TEXT    NOT NULL,
  accepted_at TEXT    NOT NULL,
  PRIMARY KEY (host, port)
);
