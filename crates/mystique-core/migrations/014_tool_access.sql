-- Access levels for MCP and the built-in agent (F-02): none < read < write <
-- full. The global defaults live in app_settings (`mcp.access.level`,
-- `agent.access.level`). A row here overrides them for one connection
-- (`database` = '') or one database on it. A NULL level inherits.
-- `ask_high_risk` is per connection: a write above the level goes to the
-- in-app approval dock instead of being denied.
CREATE TABLE IF NOT EXISTS tool_access (
  connection_id INTEGER NOT NULL REFERENCES connections (id) ON DELETE CASCADE,
  database      TEXT    NOT NULL DEFAULT '',
  level         TEXT    CHECK (level IS NULL OR level IN ('none', 'read', 'write', 'full')),
  ask_high_risk INTEGER NOT NULL DEFAULT 0 CHECK (ask_high_risk IN (0, 1)),
  updated_at    TEXT    NOT NULL,
  PRIMARY KEY (connection_id, database)
);
