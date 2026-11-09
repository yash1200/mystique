-- Real-world SQLite reads. Every statement must pass ReadProof and Read mode.

-- Pragmas that only read
PRAGMA table_info(users);
PRAGMA table_info('users');
PRAGMA main.table_info(users);
PRAGMA table_xinfo(users);
PRAGMA index_list(users);
PRAGMA index_info(idx_users_email);
PRAGMA foreign_key_list(users);
PRAGMA database_list;
PRAGMA user_version;
PRAGMA journal_mode;
PRAGMA integrity_check;
PRAGMA quick_check;
PRAGMA compile_options;
PRAGMA foreign_keys;
PRAGMA page_count;

-- Schema
SELECT name, sql FROM sqlite_master WHERE type = 'table';
SELECT name FROM sqlite_schema WHERE type = 'index' AND tbl_name = 'users';
SELECT * FROM pragma_table_info('users');
SELECT m.name, p.name FROM sqlite_master m JOIN pragma_table_info(m.name) p WHERE m.type = 'table';
SELECT * FROM pragma_index_list('users');
SELECT * FROM pragma_foreign_key_list('users');
SELECT name FROM pragma_function_list ORDER BY name;

-- JSON
SELECT json_extract(doc, '$.a'), doc ->> '$.b', json_array_length(doc, '$.c') FROM t;
SELECT key, value FROM json_each('{"a":1,"b":2}');
SELECT j.value FROM t, json_each(t.tags) AS j;
SELECT * FROM json_tree('{"a":{"b":1}}');
SELECT json_group_array(name), json_group_object(id, name) FROM users;
SELECT json_object('a', 1), json_array(1, 2), json_set(doc, '$.a', 1), json_valid(doc) FROM t;

-- Dates, strings, math
SELECT date('now'), datetime('now', '-1 day'), strftime('%Y-%m', created_at), julianday('now') - julianday(created_at), unixepoch() FROM t;
SELECT typeof(x), quote(x), hex(x), length(x), instr(x, 'a'), printf('%05d', id), format('%s', name) FROM t;
SELECT ifnull(a, 0), iif(a > 0, 'pos', 'neg'), coalesce(a, b, 0), nullif(a, 0) FROM t;
SELECT group_concat(name, ', ') FROM users;
SELECT total(amount), sum(amount), avg(amount), count(*), min(amount), max(amount) FROM orders;
SELECT max(a, b), min(a, b) FROM t;
SELECT sqlite_version(), random(), abs(random()) % 100, randomblob(16);
SELECT last_insert_rowid(), changes(), total_changes();
SELECT substr(name, 1, 3), replace(name, 'a', 'b'), trim(name), upper(name), lower(name), round(price, 2) FROM t;
SELECT unicode('a'), char(97), zeroblob(4), likely(1);
SELECT CAST(x AS INTEGER), x || 'suffix' FROM t;

-- Query shapes
SELECT id, row_number() OVER (ORDER BY id), lag(id) OVER (ORDER BY id) FROM t;
WITH RECURSIVE cnt(x) AS (SELECT 1 UNION ALL SELECT x + 1 FROM cnt WHERE x < 10) SELECT x FROM cnt;
SELECT * FROM users WHERE name LIKE 'b%' ESCAPE '!';
SELECT * FROM t LIMIT 10 OFFSET 5;
SELECT * FROM a LEFT JOIN b USING (id);
SELECT * FROM (SELECT 1 AS x UNION ALL SELECT 2) ORDER BY x DESC;
SELECT * FROM users WHERE rowid = 1;
SELECT * FROM users WHERE id = ? AND name = :name;

-- Statements other than SELECT
EXPLAIN QUERY PLAN SELECT * FROM users WHERE id = 1;
EXPLAIN SELECT 1;
