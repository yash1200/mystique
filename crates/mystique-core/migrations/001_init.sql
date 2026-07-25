-- MystiqueDB app metadata v1 — multi-engine profiles + query library
-- Applied by db::migrate (schema_migrations row recorded in Rust).

-- -----------------------------------------------------------------------------
-- 1. Connections
-- -----------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS connections (
  id                TEXT PRIMARY KEY,
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
-- 2. Query Groups
-- -----------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS query_groups (
  id            TEXT PRIMARY KEY,
  connection_id TEXT NOT NULL REFERENCES connections (id) ON DELETE CASCADE,
  name          TEXT NOT NULL,
  sort_order    INTEGER NOT NULL DEFAULT 0,
  created_at    TEXT NOT NULL,
  updated_at    TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_query_groups_connection ON query_groups (connection_id, sort_order);

-- -----------------------------------------------------------------------------
-- 3. Saved Queries
-- -----------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS saved_queries (
  id            TEXT PRIMARY KEY,
  connection_id TEXT NOT NULL REFERENCES connections (id) ON DELETE CASCADE,
  group_id      TEXT REFERENCES query_groups (id) ON DELETE SET NULL,
  name          TEXT NOT NULL,
  body          TEXT NOT NULL,
  language      TEXT,
  sort_order    INTEGER NOT NULL DEFAULT 0,
  created_at    TEXT NOT NULL,
  updated_at    TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_saved_queries_connection ON saved_queries (connection_id, sort_order);
CREATE INDEX IF NOT EXISTS idx_saved_queries_group ON saved_queries (group_id);

-- -----------------------------------------------------------------------------
-- 4. Query History
-- -----------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS query_history (
  id            TEXT PRIMARY KEY,
  connection_id TEXT REFERENCES connections (id) ON DELETE SET NULL,
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

CREATE INDEX IF NOT EXISTS idx_query_history_connection_ran ON query_history (connection_id, ran_at DESC);
CREATE INDEX IF NOT EXISTS idx_query_history_ran ON query_history (ran_at DESC);

-- -----------------------------------------------------------------------------
-- 5. App Settings
-- -----------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS app_settings (
  key        TEXT PRIMARY KEY,
  value      TEXT NOT NULL,
  updated_at TEXT NOT NULL
);
