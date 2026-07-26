-- ER / model diagrams (JSON body)
CREATE TABLE IF NOT EXISTS er_diagrams (
  id         TEXT PRIMARY KEY,
  name       TEXT NOT NULL,
  family     TEXT NOT NULL CHECK (family IN ('sql', 'nosql', 'redis')),
  body       TEXT NOT NULL DEFAULT '{}',
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_er_diagrams_family ON er_diagrams (family);
CREATE INDEX IF NOT EXISTS idx_er_diagrams_updated ON er_diagrams (updated_at DESC);
