-- Column widths the user set in the table grid, by column name, in
-- unscaled CSS pixels. Columns not listed use the automatic width.

ALTER TABLE grid_column_layout ADD COLUMN widths TEXT NOT NULL DEFAULT '{}';
