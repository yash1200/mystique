-- One production setting: the connection's environment. Per-database marks
-- (013) are gone. A connection with any database or schema marked
-- production becomes a production connection, so nothing that asked before
-- runs without asking now. Staging and local marks are dropped; the
-- connection keeps its environment. Malformed JSON is skipped.
UPDATE connections
   SET environment = 'production'
 WHERE environment <> 'production'
   AND json_valid(database_environments)
   AND EXISTS (
     SELECT 1
       FROM json_each(connections.database_environments) AS mark
      WHERE mark.type = 'object'
        AND lower(trim(json_extract(mark.value, '$.environment'))) = 'production'
   );

ALTER TABLE connections DROP COLUMN database_environments;
