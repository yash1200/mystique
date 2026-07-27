-- MystiqueDB MySQL dummy schema + data (relational)
USE mystique;

SET FOREIGN_KEY_CHECKS = 0;
DROP TABLE IF EXISTS order_items;
DROP TABLE IF EXISTS orders;
DROP TABLE IF EXISTS products;
DROP TABLE IF EXISTS categories;
DROP TABLE IF EXISTS users;
SET FOREIGN_KEY_CHECKS = 1;

CREATE TABLE categories (
  id INT UNSIGNED NOT NULL AUTO_INCREMENT PRIMARY KEY,
  slug VARCHAR(64) NOT NULL UNIQUE,
  name VARCHAR(128) NOT NULL,
  sort_order INT NOT NULL DEFAULT 0,
  created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP
) ENGINE=InnoDB;

CREATE TABLE users (
  id INT UNSIGNED NOT NULL AUTO_INCREMENT PRIMARY KEY,
  email VARCHAR(191) NOT NULL UNIQUE,
  name VARCHAR(128) NOT NULL,
  city VARCHAR(64) NULL,
  is_active TINYINT(1) NOT NULL DEFAULT 1,
  created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
  INDEX idx_users_city (city),
  INDEX idx_users_active (is_active)
) ENGINE=InnoDB;

CREATE TABLE products (
  id INT UNSIGNED NOT NULL AUTO_INCREMENT PRIMARY KEY,
  sku VARCHAR(32) NOT NULL UNIQUE,
  name VARCHAR(191) NOT NULL,
  category_id INT UNSIGNED NOT NULL,
  price_cents INT UNSIGNED NOT NULL,
  stock INT NOT NULL DEFAULT 0,
  active TINYINT(1) NOT NULL DEFAULT 1,
  created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
  CONSTRAINT fk_products_category
    FOREIGN KEY (category_id) REFERENCES categories(id)
    ON UPDATE CASCADE ON DELETE RESTRICT,
  INDEX idx_products_category (category_id),
  INDEX idx_products_active (active)
) ENGINE=InnoDB;

CREATE TABLE orders (
  id INT UNSIGNED NOT NULL AUTO_INCREMENT PRIMARY KEY,
  order_number VARCHAR(32) NOT NULL UNIQUE,
  user_id INT UNSIGNED NOT NULL,
  status ENUM('pending','paid','shipped','delivered','cancelled') NOT NULL DEFAULT 'pending',
  total_cents INT UNSIGNED NOT NULL DEFAULT 0,
  placed_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
  CONSTRAINT fk_orders_user
    FOREIGN KEY (user_id) REFERENCES users(id)
    ON UPDATE CASCADE ON DELETE RESTRICT,
  INDEX idx_orders_user (user_id),
  INDEX idx_orders_status_placed (status, placed_at)
) ENGINE=InnoDB;

CREATE TABLE order_items (
  id INT UNSIGNED NOT NULL AUTO_INCREMENT PRIMARY KEY,
  order_id INT UNSIGNED NOT NULL,
  product_id INT UNSIGNED NOT NULL,
  quantity INT UNSIGNED NOT NULL,
  unit_price_cents INT UNSIGNED NOT NULL,
  line_total_cents INT UNSIGNED NOT NULL,
  CONSTRAINT fk_items_order
    FOREIGN KEY (order_id) REFERENCES orders(id)
    ON UPDATE CASCADE ON DELETE CASCADE,
  CONSTRAINT fk_items_product
    FOREIGN KEY (product_id) REFERENCES products(id)
    ON UPDATE CASCADE ON DELETE RESTRICT,
  INDEX idx_items_order (order_id),
  INDEX idx_items_product (product_id)
) ENGINE=InnoDB;

-- Categories
INSERT INTO categories (slug, name, sort_order) VALUES
('electronics','Electronics',1),
('books','Books',2),
('clothing','Clothing',3),
('home','Home',4),
('sports','Sports',5),
('toys','Toys',6),
('beauty','Beauty',7),
('grocery','Grocery',8),
('automotive','Automotive',9),
('garden','Garden',10);

-- Users (200) — n = 0..199 → ids 1..200
INSERT INTO users (email, name, city, is_active)
SELECT
  CONCAT('user', n + 1, '@example.com'),
  CONCAT(
    ELT(1 + (n % 10), 'Ada','Lin','Sam','Jordan','Riley','Casey','Morgan','Avery','Quinn','Jamie'),
    ' ',
    ELT(1 + ((n * 3) % 10), 'Nguyen','Patel','Garcia','Kim','Smith','Lee','Brown','Davis','Wilson','Moore')
  ),
  ELT(1 + (n % 8), 'Austin','Seattle','Berlin','Tokyo','Lagos','Toronto','Sydney','Mumbai'),
  IF((n + 1) % 17 = 0, 0, 1)
FROM (
  SELECT a.N + b.N * 10 + c.N * 100 AS n
  FROM
    (SELECT 0 N UNION SELECT 1 UNION SELECT 2 UNION SELECT 3 UNION SELECT 4
     UNION SELECT 5 UNION SELECT 6 UNION SELECT 7 UNION SELECT 8 UNION SELECT 9) a,
    (SELECT 0 N UNION SELECT 1 UNION SELECT 2 UNION SELECT 3 UNION SELECT 4
     UNION SELECT 5 UNION SELECT 6 UNION SELECT 7 UNION SELECT 8 UNION SELECT 9) b,
    (SELECT 0 N UNION SELECT 1) c
) seq
WHERE n BETWEEN 0 AND 199;

-- Products (300) — n = 0..299 → ids 1..300, category_id 1..10
INSERT INTO products (sku, name, category_id, price_cents, stock, active)
SELECT
  CONCAT('SKU-', LPAD(n + 1, 5, '0')),
  CONCAT(
    ELT(1 + (n % 8), 'Soft','Bright','Classic','Modern','Compact','Premium','Eco','Smart'),
    ' ',
    ELT(1 + ((n * 2) % 10), 'Lamp','Chair','Bottle','Speaker','Bag','Watch','Keyboard','Mug','Notebook','Headset'),
    ' ',
    n + 1
  ),
  1 + (n % 10),
  499 + ((n + 1) * 37) % 50000,
  ((n + 1) * 11) % 400,
  IF((n + 1) % 23 = 0, 0, 1)
FROM (
  SELECT a.N + b.N * 10 + c.N * 100 AS n
  FROM
    (SELECT 0 N UNION SELECT 1 UNION SELECT 2 UNION SELECT 3 UNION SELECT 4
     UNION SELECT 5 UNION SELECT 6 UNION SELECT 7 UNION SELECT 8 UNION SELECT 9) a,
    (SELECT 0 N UNION SELECT 1 UNION SELECT 2 UNION SELECT 3 UNION SELECT 4
     UNION SELECT 5 UNION SELECT 6 UNION SELECT 7 UNION SELECT 8 UNION SELECT 9) b,
    (SELECT 0 N UNION SELECT 1 UNION SELECT 2) c
) seq
WHERE n BETWEEN 0 AND 299;

-- Orders (400) — user_id always 1..200
INSERT INTO orders (order_number, user_id, status, total_cents, placed_at)
SELECT
  CONCAT('ORD-', LPAD(n + 1, 6, '0')),
  1 + (n % 200),
  ELT(1 + (n % 5), 'pending','paid','shipped','delivered','cancelled'),
  0,
  DATE_SUB(NOW(), INTERVAL (n % 120) DAY)
FROM (
  SELECT a.N + b.N * 10 + c.N * 100 AS n
  FROM
    (SELECT 0 N UNION SELECT 1 UNION SELECT 2 UNION SELECT 3 UNION SELECT 4
     UNION SELECT 5 UNION SELECT 6 UNION SELECT 7 UNION SELECT 8 UNION SELECT 9) a,
    (SELECT 0 N UNION SELECT 1 UNION SELECT 2 UNION SELECT 3 UNION SELECT 4
     UNION SELECT 5 UNION SELECT 6 UNION SELECT 7 UNION SELECT 8 UNION SELECT 9) b,
    (SELECT 0 N UNION SELECT 1 UNION SELECT 2 UNION SELECT 3) c
) seq
WHERE n BETWEEN 0 AND 399;

-- Order items (800) — 2 lines for first 400 orders conceptually
INSERT INTO order_items (order_id, product_id, quantity, unit_price_cents, line_total_cents)
SELECT
  o.id,
  p.id,
  1 + (o.id % 4),
  p.price_cents,
  p.price_cents * (1 + (o.id % 4))
FROM orders o
JOIN products p ON p.id = 1 + ((o.id * 3) % 300); -- product ids 1..300

INSERT INTO order_items (order_id, product_id, quantity, unit_price_cents, line_total_cents)
SELECT
  o.id,
  p.id,
  1 + (o.id % 3),
  p.price_cents,
  p.price_cents * (1 + (o.id % 3))
FROM orders o
JOIN products p ON p.id = 1 + ((o.id * 7 + 11) % 300);

-- Roll up totals
UPDATE orders o
JOIN (
  SELECT order_id, SUM(line_total_cents) AS s
  FROM order_items
  GROUP BY order_id
) t ON t.order_id = o.id
SET o.total_cents = t.s;

-- Sanity counts
SELECT 'categories' AS t, COUNT(*) AS c FROM categories
UNION ALL SELECT 'users', COUNT(*) FROM users
UNION ALL SELECT 'products', COUNT(*) FROM products
UNION ALL SELECT 'orders', COUNT(*) FROM orders
UNION ALL SELECT 'order_items', COUNT(*) FROM order_items;

-- Sample join (relations check)
SELECT o.order_number, u.email, COUNT(i.id) AS line_count, o.total_cents, o.status
FROM orders o
JOIN users u ON u.id = o.user_id
JOIN order_items i ON i.order_id = o.id
GROUP BY o.id, o.order_number, u.email, o.total_cents, o.status
ORDER BY o.id
LIMIT 5;
