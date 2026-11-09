CREATE TRIGGER audit AFTER UPDATE ON t
WHEN old.v <> new.v
BEGIN
  INSERT INTO log VALUES (old.id, CASE WHEN new.v IS NULL THEN 'null' ELSE 'set' END);
  UPDATE t SET touched = 1 WHERE id = new.id;
END;
SELECT [weird;name] FROM t;
BEGIN IMMEDIATE;
DELETE FROM t WHERE "end" = 1;
END TRANSACTION;
