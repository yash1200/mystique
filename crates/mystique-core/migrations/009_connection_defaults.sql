-- Connection defaults.
-- read_only now defaults to off, as the connection form does. Rows keep
-- their stored value; only the column default changes.
-- query_timeout_secs: NULL uses the app setting, 0 means no timeout.
-- SQLite cannot change a column default in place, so the table is rebuilt:
-- this file creates the new table, then migrate.rs copies the rows, drops the
-- old table, renames the new one and recreates the indexes. All of it runs
-- with foreign keys off, inside one transaction.

CREATE TABLE connections_new (
  id                INTEGER PRIMARY KEY AUTOINCREMENT,
  name              TEXT NOT NULL,
  engine            TEXT NOT NULL,
  color_id          TEXT NOT NULL DEFAULT 'slate',
  environment       TEXT NOT NULL DEFAULT 'local' CHECK (
    environment IN ('local', 'staging', 'production')
  ),
  read_only         INTEGER NOT NULL DEFAULT 0 CHECK (
    read_only IN (0, 1)
  ),
  notes             TEXT,
  plugin_config     TEXT NOT NULL DEFAULT '{}',
  secret_ref        TEXT,
  last_connected_at TEXT,
  created_at        TEXT NOT NULL,
  updated_at        TEXT NOT NULL,
  favorite          INTEGER NOT NULL DEFAULT 0,
  ssh_config        TEXT NOT NULL DEFAULT '{}',
  ssh_secret_ref    TEXT,
  query_timeout_secs INTEGER CHECK (
    query_timeout_secs IS NULL OR query_timeout_secs >= 0
  )
);
