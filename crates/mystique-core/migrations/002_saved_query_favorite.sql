-- Pin favourites to the top of the saved-query list.
ALTER TABLE saved_queries ADD COLUMN favorite INTEGER NOT NULL DEFAULT 0;

CREATE INDEX IF NOT EXISTS idx_saved_queries_favorite
  ON saved_queries (connection_id, favorite, sort_order);
