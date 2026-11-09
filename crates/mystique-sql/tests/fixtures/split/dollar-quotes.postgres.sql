CREATE FUNCTION f() RETURNS int LANGUAGE plpgsql AS $$
BEGIN
  RAISE NOTICE 'a;b';
  RETURN 1;
END;
$$;
DO $body$
DECLARE x text := $inner$ ; $inner$;
BEGIN
  PERFORM 1;
END
$body$;
SELECT $1, $2 FROM t WHERE a = $1;
SELECT a$b$ FROM t;
SELECT 2;
