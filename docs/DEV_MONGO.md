# Local MongoDB (dev / connector testing)

## Container

Already running as `mystique-mongo` (mongo:7 on port **27017**).

Root-style user for app testing:

| Setting | Value |
| ------- | ----- |
| Host | `127.0.0.1` |
| Port | `27017` |
| Username | `admin` |
| Password | `123` |
| Auth DB | `admin` |
| App DB | `mystique` |

Also present (legacy seed user): `abc` / `abc` (authSource `admin`).

## Connection strings

```text
mongodb://admin:123@127.0.0.1:27017/mystique?authSource=admin
mongodb://admin:123@127.0.0.1:27017/?authSource=admin
```

## MystiqueDB connection form

- Engine: **MongoDB**
- URI: `mongodb://admin:123@127.0.0.1:27017/mystique?authSource=admin`
- **Include `/mystique` in the URI** — that path is the default database for finds and is preferred in the explorer.

Also present: legacy DB `mystique_dev` (small sample). The workspace explorer loads **live** databases/collections from the server (not mock data). Pick **`mystique`** in the database dropdown to see the 10k seed; refresh the explorer after reseeding.

If the UI still shows only a few rows, you are likely on `mystique_dev` or an old mock session — disconnect, reconnect with the URI above, then click Refresh on the explorer.

## Sample data (`mystique`, ~10k docs)

Reseed:

```bash
docker cp scripts/seed/mongo_seed_mystique.js mystique-mongo:/tmp/mongo_seed_mystique.js
docker exec mystique-mongo mongosh -u admin -p 123 --authenticationDatabase admin --file /tmp/mongo_seed_mystique.js
```

| Collection | ~Docs | Relations |
| ---------- | ----- | --------- |
| `categories` | 20 | — |
| `users` | 2000 | — |
| `products` | 1500 | `categoryId` → categories |
| `orders` | 3500 | `userId` / `userEmail` → users |
| `order_items` | 2200 | `orderId` → orders, `productId` → products |
| `reviews` | 780 | `userId` → users, `productId` → products |
| **Total** | **10000** | |

### Try in Query canvas

```js
db.users.find()
db.orders.find({ status: "paid" })
db.order_items.find({ orderNumber: "ORD-000001" })
db.users.find({ _id: ObjectId("…paste from results…") })
```

## Shell

```bash
docker exec -it mystique-mongo mongosh -u admin -p 123 --authenticationDatabase admin mystique
```
