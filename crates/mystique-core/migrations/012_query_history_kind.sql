-- What wrote a history row. `query` is a run from the editor, the agent or
-- MCP. The others are app actions kept so a change can be traced: a grid
-- save, a structure change or schema diff, a grid rollback, and EXPLAIN
-- ANALYZE (which runs the statement). Existing rows are queries.
ALTER TABLE query_history ADD COLUMN kind TEXT NOT NULL DEFAULT 'query' CHECK (
  kind IN ('query', 'grid_save', 'ddl', 'rollback', 'explain_analyze')
);
