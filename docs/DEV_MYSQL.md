# Local MySQL (dev / connector testing)

## Container

```bash
docker run -d \
  --name mystique-mysql \
  --restart unless-stopped \
  -p 3306:3306 \
  -e MYSQL_ROOT_PASSWORD=123 \
  -e MYSQL_DATABASE=mystique \
  -e MYSQL_USER=admin \
  -e MYSQL_PASSWORD=123 \
  mysql:8
```

| Setting | Value |
| ------- | ----- |
| Host | `127.0.0.1` |
| Port | `3306` |
| Database | `mystique` |
| Username | `admin` |
| Password | `123` |
| Root password | `123` |

## Connection (MystiqueDB MySQL plugin)

In the app: **New connection → MySQL**, then:

```text
host: 127.0.0.1
port: 3306
database: mystique
username: admin
password: 123
```

After connect, the explorer lists tables from non-system schemas (e.g. `categories`, `users`, `products`, `orders`, `order_items`). Click a table to load a data preview.

## Seed data

```bash
docker exec -i mystique-mysql mysql -uadmin -p123 mystique < scripts/seed/mysql_seed_mystique.sql
```

| Table | Rows | FK |
| ----- | ---- | -- |
| `categories` | 10 | — |
| `users` | 200 | — |
| `products` | 300 | `category_id` → categories |
| `orders` | 400 | `user_id` → users |
| `order_items` | 800 | `order_id` → orders, `product_id` → products |

### Sample SQL

```sql
SELECT * FROM users LIMIT 20;
SELECT o.order_number, u.email, o.status, o.total_cents
FROM orders o
JOIN users u ON u.id = o.user_id
ORDER BY o.placed_at DESC
LIMIT 20;
SELECT p.sku, c.name AS category, p.price_cents
FROM products p
JOIN categories c ON c.id = p.category_id
WHERE p.active = 1
LIMIT 20;
```

## Shell

```bash
docker exec -it mystique-mysql mysql -uadmin -p123 mystique
```
