-- Real-world Postgres reads. Every statement must pass ReadProof and Read mode.
-- One statement per `;`. Add the query that broke when a legitimate read is refused.

-- Window functions
SELECT id, amount, sum(amount) OVER (PARTITION BY customer_id ORDER BY created_at ROWS BETWEEN UNBOUNDED PRECEDING AND CURRENT ROW) AS running FROM orders;
SELECT *, rank() OVER w, dense_rank() OVER w, percent_rank() OVER w, cume_dist() OVER w FROM scores WINDOW w AS (ORDER BY score DESC);
SELECT customer_id, lag(amount) OVER (PARTITION BY customer_id ORDER BY created_at), lead(amount, 2, 0) OVER (ORDER BY created_at) FROM orders;
SELECT ntile(4) OVER (ORDER BY salary), first_value(name) OVER (PARTITION BY dept ORDER BY salary DESC), last_value(name) OVER (PARTITION BY dept), nth_value(name, 2) OVER (PARTITION BY dept) FROM employees;
SELECT id, row_number() OVER (PARTITION BY b ORDER BY c) FROM t;
SELECT avg(latency_ms) OVER (ORDER BY ts RANGE BETWEEN INTERVAL '5 minutes' PRECEDING AND CURRENT ROW) FROM requests;

-- JSON
SELECT data->'user'->>'name', data#>>'{address,city}', jsonb_array_length(data->'tags') FROM events WHERE data ? 'user';
SELECT jsonb_build_object('id', id, 'tags', to_jsonb(tags)) FROM posts;
SELECT key, value FROM events, jsonb_each(events.data);
SELECT id FROM docs WHERE body @> '{"status":"active"}'::jsonb;
SELECT jsonb_path_query(data, '$.items[*] ? (@.price > 10)') FROM orders;
SELECT json_agg(t) FROM (SELECT id, name FROM users LIMIT 10) t;
SELECT jsonb_pretty(data) FROM events LIMIT 1;
SELECT * FROM jsonb_to_recordset('[{"a":1}]'::jsonb) AS x(a int);
SELECT jsonb_set(data, '{a}', '1'), data - 'b', data || '{"c":3}' FROM t;
SELECT jsonb_agg(x) FILTER (WHERE x > 0) FROM t;
SELECT row_to_json(u), to_json(now()) FROM users u LIMIT 1;
SELECT jsonb_object_keys(data) FROM events;
SELECT jsonb_typeof(data->'a'), jsonb_strip_nulls(data) FROM events;

-- Lateral joins
SELECT u.id, o.* FROM users u CROSS JOIN LATERAL (SELECT * FROM orders o WHERE o.user_id = u.id ORDER BY created_at DESC LIMIT 3) o;
SELECT u.id, t.tag FROM users u LEFT JOIN LATERAL unnest(u.tags) AS t(tag) ON true;
SELECT u.id, latest.at FROM users u JOIN LATERAL (SELECT max(created_at) AS at FROM orders WHERE user_id = u.id) latest ON true;

-- Recursive CTEs
WITH RECURSIVE tree AS (SELECT id, parent_id, name, 1 AS depth FROM categories WHERE parent_id IS NULL UNION ALL SELECT c.id, c.parent_id, c.name, t.depth + 1 FROM categories c JOIN tree t ON c.parent_id = t.id) SELECT * FROM tree ORDER BY depth;
WITH RECURSIVE r(n) AS (SELECT 1 UNION ALL SELECT n + 1 FROM r WHERE n < 5) SELECT * FROM r;
WITH a AS (SELECT 1 AS x), b AS (SELECT x + 1 AS y FROM a) SELECT * FROM a, b;

-- generate_series and set-returning functions
SELECT d::date FROM generate_series(now() - interval '30 days', now(), interval '1 day') AS d;
SELECT g.day, count(o.id) FROM generate_series(date_trunc('month', now()), now(), '1 day') g(day) LEFT JOIN orders o ON date_trunc('day', o.created_at) = g.day GROUP BY g.day ORDER BY g.day;
SELECT * FROM generate_series(1, 10) AS n;
SELECT * FROM unnest(ARRAY[1, 2], ARRAY['a', 'b']) AS x(n, s);
SELECT generate_subscripts(ARRAY[1, 2, 3], 1);
SELECT regexp_split_to_table('a,b,c', ',');

-- Catalog and monitoring
SELECT n.nspname, c.relname, c.relkind, pg_size_pretty(pg_total_relation_size(c.oid)) FROM pg_class c JOIN pg_namespace n ON n.oid = c.relnamespace WHERE n.nspname NOT IN ('pg_catalog', 'information_schema') ORDER BY pg_total_relation_size(c.oid) DESC LIMIT 20;
SELECT pid, usename, state, query, now() - query_start AS duration FROM pg_stat_activity WHERE state <> 'idle';
SELECT a.attname, format_type(a.atttypid, a.atttypmod), a.attnotnull, pg_get_expr(d.adbin, d.adrelid) FROM pg_attribute a LEFT JOIN pg_attrdef d ON d.adrelid = a.attrelid AND d.adnum = a.attnum WHERE a.attrelid = 'public.users'::regclass AND a.attnum > 0 AND NOT a.attisdropped;
SELECT conname, pg_get_constraintdef(oid) FROM pg_constraint WHERE conrelid = 'users'::regclass;
SELECT indexname, indexdef FROM pg_indexes WHERE tablename = 'users';
SELECT pg_get_viewdef('my_view'::regclass, true);
SELECT pg_get_functiondef(p.oid) FROM pg_proc p WHERE proname = 'f';
SELECT table_name, column_name, data_type FROM information_schema.columns WHERE table_schema = 'public' ORDER BY table_name, ordinal_position;
SELECT * FROM pg_locks l JOIN pg_stat_activity a ON a.pid = l.pid WHERE NOT l.granted;
SELECT current_database(), current_user, session_user, version(), current_setting('server_version'), pg_backend_pid();
SELECT current_schema(), current_schemas(true), inet_server_addr(), pg_postmaster_start_time();
SELECT relname, n_live_tup, n_dead_tup, last_autovacuum FROM pg_stat_user_tables ORDER BY n_dead_tup DESC;
SELECT pg_database_size(current_database()), pg_relation_size('users'), pg_indexes_size('users'), pg_table_size('users');
SELECT has_table_privilege('users', 'SELECT'), has_schema_privilege('public', 'USAGE'), pg_has_role('admin', 'MEMBER');
SELECT obj_description('users'::regclass, 'pg_class'), col_description('users'::regclass, 1);
SELECT pg_is_in_recovery(), pg_last_wal_replay_lsn(), pg_current_wal_lsn();
SELECT extname, extversion FROM pg_extension;
SELECT query, calls, total_exec_time, mean_exec_time FROM pg_stat_statements ORDER BY total_exec_time DESC LIMIT 10;
SELECT blocked.pid, pg_blocking_pids(blocked.pid) FROM pg_stat_activity blocked WHERE cardinality(pg_blocking_pids(blocked.pid)) > 0;
SELECT * FROM pg_settings WHERE name LIKE 'work_mem%';
SELECT pg_stat_get_live_tuples('users'::regclass);
SELECT to_regclass('public.users') IS NOT NULL;
SELECT pg_typeof(1.5), pg_column_size('abc'::text);
SELECT pg_catalog.lower('X'), pg_catalog.count(*) FROM t;
SELECT tableoid::regclass, * FROM parent_table;
SELECT currval('users_id_seq'), lastval();
SELECT last_value FROM users_id_seq;
SELECT txid_current_if_assigned(), pg_current_snapshot();
SELECT pg_size_pretty(pg_total_relation_size('t'));
SELECT datname, numbackends, xact_commit FROM pg_stat_database;
SELECT * FROM pg_stat_replication;
SELECT s.schema_name FROM information_schema.schemata s;

-- Strings, dates, math
SELECT to_char(created_at, 'YYYY-MM'), count(*) FROM orders GROUP BY 1 ORDER BY 1;
SELECT extract(epoch FROM now() - created_at) / 3600 AS hours FROM jobs;
SELECT date_part('dow', created_at), avg(amount)::numeric(10,2) FROM orders GROUP BY 1;
SELECT age(now(), birth_date), justify_interval(interval '50 days') FROM people;
SELECT regexp_replace(phone, '[^0-9]', '', 'g'), split_part(email, '@', 2), initcap(name) FROM contacts;
SELECT string_agg(DISTINCT tag, ', ' ORDER BY tag) FROM tags;
SELECT array_agg(id ORDER BY id) FILTER (WHERE active), count(*) FILTER (WHERE NOT active) FROM users;
SELECT percentile_cont(0.95) WITHIN GROUP (ORDER BY latency_ms) FROM requests;
SELECT mode() WITHIN GROUP (ORDER BY status) FROM orders;
SELECT md5(email), encode(sha256(email::bytea), 'hex') FROM users;
SELECT to_tsvector('english', body) @@ plainto_tsquery('english', 'database client') FROM articles;
SELECT ts_rank(to_tsvector(body), to_tsquery('db & client')) FROM articles ORDER BY 1 DESC;
SELECT gen_random_uuid(), random(), clock_timestamp(), statement_timestamp();
SELECT CASE WHEN amount > 100 THEN 'big' ELSE 'small' END, coalesce(note, '') FROM orders;
SELECT greatest(a, b), least(a, b), nullif(a, 0), num_nulls(a, b) FROM t;
SELECT width_bucket(amount, 0, 1000, 10), count(*) FROM orders GROUP BY 1;
SELECT date_bin('15 minutes', ts, '2000-01-01'), count(*) FROM metrics GROUP BY 1;
SELECT make_date(2024, 1, 1), to_date('2024-01-01', 'YYYY-MM-DD'), to_timestamp(1700000000);
SELECT bool_and(active), bool_or(active), every(active) FROM users;
SELECT stddev(amount), variance(amount), corr(a, b), regr_slope(a, b) FROM t;
SELECT format('%s-%s', a, b), quote_ident(name), quote_literal(v) FROM t;
SELECT array_to_string(array_agg(name), ','), array_length(ARRAY[1, 2], 1) FROM users;
SELECT left(name, 3), right(name, 3), lpad(id::text, 6, '0'), repeat('-', 10), reverse(name), position('a' IN name), substring(name FROM 2 FOR 3), trim(BOTH ' ' FROM name) FROM users;
SELECT upper(name), lower(name), length(name), btrim(name), strpos(name, 'a'), translate(name, 'ab', 'xy') FROM users;
SELECT abs(-1), round(2.567, 2), ceil(1.2), floor(1.8), trunc(1.9), mod(7, 3), power(2, 10), sqrt(16), div(7, 2);
SELECT date_trunc('week', created_at), count(*) FROM orders GROUP BY 1;
SELECT now()::date, '2024-01-01'::timestamptz, CAST('42' AS integer), '1 day'::interval;
SELECT id, created_at AT TIME ZONE 'UTC' FROM events;
SELECT int4range(1, 10) @> 5, daterange('2024-01-01', '2024-02-01') && daterange('2024-01-15', '2024-03-01');
SELECT '192.168.0.1'::inet << '192.168.0.0/24'::inet, host('10.0.0.1/8'::inet), masklen('10.0.0.0/8'::cidr);
SELECT ST_AsText(ST_MakePoint(1, 2)), ST_Distance(a.geom, b.geom) FROM places a, places b LIMIT 1;

-- Query shapes
SELECT DISTINCT ON (customer_id) customer_id, created_at FROM orders ORDER BY customer_id, created_at DESC;
SELECT * FROM orders WHERE created_at BETWEEN '2024-01-01' AND '2024-12-31' AND status = ANY(ARRAY['paid', 'shipped']);
SELECT id FROM users WHERE email ILIKE '%@example.com' AND name NOT LIKE 'x%';
SELECT * FROM t1 FULL OUTER JOIN t2 ON t1.id = t2.id;
SELECT * FROM a NATURAL JOIN b;
SELECT 1 INTERSECT SELECT 1 EXCEPT SELECT 2;
SELECT * FROM (VALUES (1, 'a'), (2, 'b')) AS v(id, name);
VALUES (1, 'a'), (2, 'b');
SELECT ROW(1, 'a'), ARRAY(SELECT id FROM users LIMIT 3);
SELECT a, b, sum(c) FROM t GROUP BY ROLLUP (a, b);
SELECT a, b, sum(c) FROM t GROUP BY CUBE (a, b);
SELECT * FROM users ORDER BY id FETCH FIRST 10 ROWS ONLY;
SELECT * FROM users WHERE id = (SELECT max(id) FROM users);
SELECT * FROM users u WHERE NOT EXISTS (SELECT 1 FROM orders o WHERE o.user_id = u.id);
SELECT * FROM users WHERE id IN (SELECT user_id FROM orders) ORDER BY name DESC NULLS LAST LIMIT 10 OFFSET 5;
SELECT a, sum(b) FROM t GROUP BY a HAVING sum(b) > 1;
SELECT count(*) AS n, max(a), coalesce(b, 0), now(), current_date FROM t GROUP BY b;
SELECT * FROM users WHERE id = :id AND name = :name;
SELECT * FROM users WHERE id = $1;

-- Statements other than SELECT
TABLE users;
SHOW search_path;
SHOW work_mem;
EXPLAIN SELECT * FROM users WHERE id = 1;
EXPLAIN (ANALYZE, BUFFERS, FORMAT JSON) SELECT count(*) FROM orders;
EXPLAIN ANALYZE SELECT * FROM users u JOIN orders o ON o.user_id = u.id;
EXPLAIN SELECT my_function(id) FROM users;
SET search_path TO public;
SET TIME ZONE 'UTC';
SET application_name = 'mystique';
