-- Grid saves that can be rolled back. The backend builds the inverse SQL and
-- the row checks; the grid only holds `save_id`. Rollback deletes the row.
-- The old history held SQL the frontend sent back, so it is dropped.

DROP TABLE IF EXISTS grid_edit_history;

CREATE TABLE IF NOT EXISTS grid_saves (
  save_id       TEXT PRIMARY KEY,
  connection_id TEXT NOT NULL,
  table_key     TEXT NOT NULL,
  undo          TEXT NOT NULL,
  created_at    TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_grid_saves_table
  ON grid_saves (connection_id, table_key);
