-- Column layout and the inverse SQL for the last grid saves.
-- These live in Mystique's database, not the database being edited.

CREATE TABLE IF NOT EXISTS grid_column_layout (
  connection_id TEXT NOT NULL,
  table_id      TEXT NOT NULL,
  column_order  TEXT NOT NULL DEFAULT '[]',
  hidden        TEXT NOT NULL DEFAULT '[]',
  frozen        TEXT NOT NULL DEFAULT '[]',
  updated_at    TEXT NOT NULL,
  PRIMARY KEY (connection_id, table_id)
);

CREATE TABLE IF NOT EXISTS grid_edit_history (
  id            INTEGER PRIMARY KEY AUTOINCREMENT,
  connection_id TEXT NOT NULL,
  table_id      TEXT NOT NULL,
  rollback_sql  TEXT NOT NULL,
  created_at    TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_grid_edit_history_table
  ON grid_edit_history (connection_id, table_id, id);
