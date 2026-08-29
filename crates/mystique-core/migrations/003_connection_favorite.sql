-- Pin favourites to the top of the connection list.
ALTER TABLE connections ADD COLUMN favorite INTEGER NOT NULL DEFAULT 0;

CREATE INDEX IF NOT EXISTS idx_connections_favorite
  ON connections (favorite, name COLLATE NOCASE);
