-- Query history v2.
-- duration_ms already exists. source is who ran the statement:
-- ui, agent, or mcp. The 2_000-row cap per connection lives in the
-- history repository (QUERY_HISTORY_CAP), not in this file.
-- SQL text is stored on purpose. Passwords and URI userinfo are removed
-- before insert.

ALTER TABLE query_history ADD COLUMN "database" TEXT;
ALTER TABLE query_history ADD COLUMN affected_rows INTEGER;
ALTER TABLE query_history ADD COLUMN source TEXT NOT NULL DEFAULT 'ui'
  CHECK (source IN ('ui', 'agent', 'mcp'));

DROP INDEX IF EXISTS idx_query_history_connection_ran;
CREATE INDEX IF NOT EXISTS idx_query_history_connection_keyset
  ON query_history (connection_id, ran_at DESC, id DESC);
