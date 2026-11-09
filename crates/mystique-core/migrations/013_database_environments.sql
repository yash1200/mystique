-- Per-database environment marks: a dev server can host one production
-- database. JSON array of {"database", "schema"?, "environment"}, where
-- environment is local | staging | production. The default '[]' keeps the
-- connection environment for every database, as before this column.
ALTER TABLE connections ADD COLUMN database_environments TEXT NOT NULL DEFAULT '[]';
