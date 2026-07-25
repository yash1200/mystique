# Local MongoDB (dev / connector testing)

## Container

```bash
docker run -d \
  --name mystique-mongo \
  --restart unless-stopped \
  -p 27017:27017 \
  -e MONGO_INITDB_ROOT_USERNAME=abc \
  -e MONGO_INITDB_ROOT_PASSWORD=abc \
  mongo:7
```

| Setting | Value |
| ------- | ----- |
| Host | `127.0.0.1` |
| Port | `27017` |
| Username | `abc` |
| Password | `abc` |
| Auth DB | `admin` |
| Sample app DB | `mystique_dev` |

## Connection strings

```text
mongodb://abc:abc@127.0.0.1:27017/?authSource=admin
mongodb://abc:abc@127.0.0.1:27017/mystique_dev?authSource=admin
```

## Mystique `OpenSessionRequest` example

```json
{
  "engine": "mongodb",
  "pluginConfig": {
    "uri": "mongodb://abc:abc@127.0.0.1:27017/?authSource=admin",
    "database": "mystique_dev"
  },
  "pool": { "maxSize": 5, "minIdle": 1, "acquireTimeoutMs": 10000 }
}
```

Or host form:

```json
{
  "engine": "mongodb",
  "pluginConfig": {
    "host": "127.0.0.1",
    "port": 27017,
    "database": "mystique_dev",
    "username": "abc",
    "authSource": "admin"
  },
  "password": "abc"
}
```

## Sample collections (`mystique_dev`)

| Collection | Docs | Notes |
| ---------- | ---- | ----- |
| `users` | 3 | email unique index |
| `products` | 4 | sku unique index |
| `orders` | 3 | refs users by email |
| `order_items` | 4 | refs orders by orderId |

## Useful commands

```bash
# shell
docker exec -it mystique-mongo mongosh -u abc -p abc --authenticationDatabase admin

# ping
docker exec mystique-mongo mongosh -u abc -p abc --authenticationDatabase admin --eval 'db.runCommand({ping:1})'

# stop / start
docker stop mystique-mongo
docker start mystique-mongo
```
