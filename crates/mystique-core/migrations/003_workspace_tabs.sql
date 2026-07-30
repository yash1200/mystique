-- Drop query groups; saved queries are per-connection only.
-- Persist open workspace tabs (query / table / model) per connection.

PRAGMA foreign_keys = OFF;

-- Recreate saved_queries without group_id
CREATE TABLE IF NOT EXISTS saved_queries_v3 (
  id            TEXT PRIMARY KEY,
  connection_id TEXT NOT NULL REFERENCES connections (id) ON DELETE CASCADE,
  name          TEXT NOT NULL,
  body          TEXT NOT NULL,
  language      TEXT,
  sort_order    INTEGER NOT NULL DEFAULT 0,
  created_at    TEXT NOT NULL,
  updated_at    TEXT NOT NULL
);

INSERT INTO saved_queries_v3 (id, connection_id, name, body, language, sort_order, created_at, updated_at)
SELECT id, connection_id, name, body, language, sort_order, created_at, updated_at
FROM saved_queries;

DROP TABLE IF EXISTS saved_queries;
ALTER TABLE saved_queries_v3 RENAME TO saved_queries;

DROP TABLE IF EXISTS query_groups;

CREATE INDEX IF NOT EXISTS idx_saved_queries_connection
  ON saved_queries (connection_id, sort_order);

-- Open tabs restored when a connection is opened again
CREATE TABLE IF NOT EXISTS workspace_tabs (
  id              TEXT PRIMARY KEY,
  connection_id   TEXT NOT NULL REFERENCES connections (id) ON DELETE CASCADE,
  kind            TEXT NOT NULL CHECK (kind IN ('query', 'table', 'model')),
  title           TEXT NOT NULL,
  sort_order      INTEGER NOT NULL DEFAULT 0,
  is_active       INTEGER NOT NULL DEFAULT 0 CHECK (is_active IN (0, 1)),
  saved_query_id  TEXT REFERENCES saved_queries (id) ON DELETE SET NULL,
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

PRAGMA foreign_keys = ON;
