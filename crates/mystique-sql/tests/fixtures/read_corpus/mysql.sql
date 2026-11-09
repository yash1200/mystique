-- Real-world MySQL reads. Every statement must pass ReadProof and Read mode.

-- information_schema and performance_schema
SELECT table_name, table_rows, data_length + index_length AS size FROM information_schema.tables WHERE table_schema = DATABASE() ORDER BY size DESC;
SELECT column_name, data_type, is_nullable, column_default FROM information_schema.columns WHERE table_schema = 'mystique' AND table_name = 'users' ORDER BY ordinal_position;
SELECT * FROM information_schema.processlist WHERE command <> 'Sleep';
SELECT constraint_name, referenced_table_name FROM information_schema.key_column_usage WHERE referenced_table_name IS NOT NULL;
SELECT routine_name, routine_type FROM information_schema.routines WHERE routine_schema = DATABASE();
SELECT * FROM performance_schema.events_statements_summary_by_digest ORDER BY sum_timer_wait DESC LIMIT 10;
SELECT * FROM sys.schema_table_statistics;
SELECT user, host FROM mysql.user;
SELECT FORMAT_BYTES(SUM(data_length)) FROM information_schema.tables;

-- JSON
SELECT JSON_EXTRACT(doc, '$.a'), doc->'$.b', doc->>'$.c', JSON_UNQUOTE(JSON_EXTRACT(doc, '$.d')) FROM t;
SELECT JSON_OBJECT('id', id, 'name', name), JSON_ARRAYAGG(tag) FROM t GROUP BY id, name;
SELECT JSON_CONTAINS(doc, '1', '$.a'), JSON_LENGTH(doc), JSON_KEYS(doc), JSON_VALID(doc) FROM t;
SELECT JSON_SET(doc, '$.a', 1), JSON_REMOVE(doc, '$.b'), JSON_MERGE_PATCH(doc, '{}') FROM t;
SELECT JSON_TYPE(doc), JSON_DEPTH(doc), JSON_PRETTY(doc), JSON_SEARCH(doc, 'one', 'x') FROM t;

-- Window functions and CTEs
SELECT id, ROW_NUMBER() OVER (PARTITION BY customer_id ORDER BY created_at DESC) AS rn FROM orders;
SELECT id, SUM(amount) OVER (ORDER BY created_at ROWS BETWEEN 6 PRECEDING AND CURRENT ROW) FROM orders;
SELECT id, LAG(amount) OVER w, LEAD(amount) OVER w FROM orders WINDOW w AS (ORDER BY id);
WITH RECURSIVE seq (n) AS (SELECT 1 UNION ALL SELECT n + 1 FROM seq WHERE n < 10) SELECT n FROM seq;
WITH ranked AS (SELECT *, RANK() OVER (ORDER BY score DESC) r FROM scores) SELECT * FROM ranked WHERE r <= 3;

-- Strings, dates, math
SELECT DATE_FORMAT(created_at, '%Y-%m') AS month, COUNT(*) FROM orders GROUP BY month ORDER BY month;
SELECT DATEDIFF(NOW(), created_at), TIMESTAMPDIFF(HOUR, created_at, NOW()), DATE_ADD(created_at, INTERVAL 1 DAY) FROM orders;
SELECT DATE_SUB(NOW(), INTERVAL 7 DAY), ADDDATE(CURDATE(), 1), LAST_DAY(NOW());
SELECT CONCAT_WS('-', first, last), SUBSTRING_INDEX(email, '@', -1), UPPER(name), LPAD(id, 6, '0') FROM users;
SELECT UNIX_TIMESTAMP(), FROM_UNIXTIME(1700000000), CURDATE(), CURTIME(), UTC_TIMESTAMP();
SELECT UUID(), RAND(), MD5('x'), SHA2('x', 256), CRC32('x');
SELECT CAST(price AS DECIMAL(10,2)), CONVERT(name, CHAR) FROM products;
SELECT GROUP_CONCAT(name ORDER BY name SEPARATOR ', ') FROM users;
SELECT IF(amount > 100, 'big', 'small'), IFNULL(note, ''), COALESCE(a, b) FROM orders;
SELECT YEAR(d), MONTH(d), DAY(d), WEEK(d), DAYNAME(d), HOUR(d), MINUTE(d) FROM t;
SELECT GREATEST(1, 2), LEAST(1, 2), ABS(-1), ROUND(1.5), TRUNCATE(1.55, 1), FLOOR(1.5), CEIL(1.5), MOD(5, 2);
SELECT FIND_IN_SET('b', 'a,b,c'), FIELD('b', 'a', 'b'), ELT(1, 'a', 'b'), INSTR('abc', 'b'), LOCATE('b', 'abc');
SELECT HEX('a'), UNHEX('61'), TO_BASE64('a'), FROM_BASE64('YQ==');
SELECT INET_ATON('10.0.0.1'), INET_NTOA(167772161);
SELECT STR_TO_DATE('2024-01-01', '%Y-%m-%d'), MAKEDATE(2024, 1), SEC_TO_TIME(3600);
SELECT BIN_TO_UUID(id), UUID_TO_BIN(UUID()) FROM t;
SELECT FORMAT(1234567.891, 2), CHAR_LENGTH(name), LENGTH(name), REVERSE(name), REPLACE(name, 'a', 'b') FROM users;
SELECT ST_AsText(ST_GeomFromText('POINT(1 1)'));

-- Session information
SELECT VERSION(), DATABASE(), CURRENT_USER(), CONNECTION_ID(), USER();
SELECT @@version, @@sql_mode;
SELECT LAST_INSERT_ID(), ROW_COUNT(), FOUND_ROWS();
SELECT @x;

-- Query shapes
SELECT * FROM users WHERE name REGEXP '^a' AND email LIKE '%@x.com';
SELECT * FROM users u LEFT JOIN orders o ON o.user_id = u.id WHERE o.id IS NULL;
SELECT COUNT(DISTINCT user_id) FROM orders;
SELECT * FROM users WHERE id IN (SELECT user_id FROM orders WHERE amount > 100);
SELECT name FROM users ORDER BY name LIMIT 10 OFFSET 20;
SELECT name FROM users LIMIT 20, 10;
SELECT `order`.`id` FROM `order`;
SELECT `a` FROM `t` WHERE b = 'x' LIMIT 5;
SELECT * FROM t WHERE MATCH(title, body) AGAINST ('database' IN NATURAL LANGUAGE MODE);
SELECT * FROM t WHERE id = ?;
SELECT a, SUM(b) FROM t GROUP BY a HAVING SUM(b) > 10;

-- Statements other than SELECT
SHOW TABLES;
SHOW FULL TABLES;
SHOW COLUMNS FROM users;
SHOW CREATE TABLE users;
SHOW DATABASES;
SHOW VARIABLES LIKE 'max_connections';
SHOW STATUS LIKE 'Threads%';
DESCRIBE users;
EXPLAIN SELECT * FROM users WHERE id = 1;
EXPLAIN ANALYZE SELECT * FROM users;
SET NAMES utf8mb4;
SET @x = 1;
SET time_zone = '+00:00';
