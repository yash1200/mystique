# Mystique — Local SQLite Schema

> **Status:** Canonical app-metadata schema  
> **Schema SQL:** [`crates/mystique-core/migrations/001_init.sql`](../crates/mystique-core/migrations/001_init.sql)  
> **Backend layout:** [`BACKEND_STRUCTURE.md`](./BACKEND_STRUCTURE.md)  
> **Last updated:** 2026-08-23

This document describes the **local SQLite database** used by the Mystique desktop app to persist connection profiles, saved queries, history, and settings.

It is **not** the user’s remote database (Postgres, Mongo, Redis, …). Those are accessed at runtime via **engine plugins**. This file only stores *how Mystique remembers* profiles and library data on disk.

---

## 1. Goals

| Goal | Approach |
| ---- | -------- |
| Support many engines now and later | `engine` + `plugin_config` JSON — no host/port-only lock-in |
| Safe secrets | OS keychain via `secret_ref`; never store passwords in SQLite |
| Saved queries per connection | `saved_queries` with FK cascade |
| Query history | Append-only `query_history` with retention in app code |
| Simple schema | Baseline `001_init.sql` applied once on first launch |
| Avoid Beekeeper-style column sprawl | One options bag (`plugin_config`), not N× `fooOptions` columns |

---

## 2. File location

| Item | Value |
| ---- | ----- |
| Filename | `mystique.db` |
| Directory | Tauri app data dir (`app.path().app_data_dir()`) |
| Examples | macOS: `~/Library/Application Support/<bundle-id>/` · Linux: `~/.local/share/<app>/` · Windows: `%APPDATA%\<app>\` |
| Access | Rust only (`rusqlite` / similar); Svelte via Tauri `invoke` |

Recommended PRAGMAs on open:

```sql
PRAGMA foreign_keys = ON;
PRAGMA journal_mode = WAL;
```

---

## 3. Design principles

### 3.1 Multi-engine profiles

A connection row is **engine-agnostic**:

- **`engine`** — stable plugin id (`postgres`, `mongodb`, `redis`, `snowflake`, …).
- **`plugin_config`** — JSON object owned by that plugin (host/port **or** URI **or** path **or** region/account, etc.).
- **Shared UI fields** — name, color, environment, read-only, notes.

Adding SurrealDB, Cassandra, DynamoDB, BigQuery, etc. later requires a **new plugin + JSON shape**, not new SQLite columns.

### 3.2 Secrets

| Store | Contents |
| ----- | -------- |
| SQLite `secret_ref` | Opaque keychain key / service account id |
| OS keychain | Password, token, private material |

List/get APIs expose `passwordSet: boolean` (derived from `secret_ref IS NOT NULL`), never the secret itself.

### 3.3 Lessons from Beekeeper `saved_connection`

Beekeeper uses one wide table: classic SQL columns + large SSH/SSL surface + many `*Options` JSON columns + plaintext password fields.

| Take | Leave |
| ---- | ----- |
| One profiles table + type discriminator | N× `redshiftOptions`, `snowflakeOptions`, … |
| JSON bags for engine-specific fields | Passwords / SSH passwords in the DB file |
| Color, read-only, folders/order (later) | Schema that grows one column per engine forever |
| SSH as shared transport (later) | Copying their full column set |

Mystique maps conceptually:

```text
connectionType     → engine
name, labelColor   → name, color_id
readOnlyMode       → read_only
host/port/path/url + *Options → plugin_config (single JSON)
password           → keychain + secret_ref
ssh*               → future ssh_config JSON + ssh_secret_ref (optional)
```

---

## 4. Entity-relationship overview

```text
┌──────────────────┐
│  connections     │
│  (profiles)      │
└────────┬─────────┘
         │ 1
         │
    ┌────┴────┬─────────────────┐
    │ n       │ n               │ n (nullable after delete)
    ▼         ▼                 ▼
query_groups  saved_queries   query_history
    │              │
    │ 1            │
    └──────┬───────┘
           │ n (group_id optional)
           ▼
      saved_queries.group_id

app_settings          (key-value, independent)
schema_migrations     (version tracking)
```

- Deleting a **connection** cascades to its **groups** and **saved queries**.
- **History** keeps rows but sets `connection_id` to NULL if the profile is deleted (`ON DELETE SET NULL`); `engine` remains denormalized for display.

---

## 5. Tables

### 5.1 `schema_migrations`

Tracks applied migration versions.

| Column | Type | Notes |
| ------ | ---- | ----- |
| `version` | INTEGER PK | Monotonic migration id |
| `applied_at` | TEXT | ISO-8601 UTC |

---

### 5.2 `connections`

Saved connection profiles for **any** engine.

| Column | Type | Required | Description |
| ------ | ---- | -------- | ----------- |
| `id` | TEXT PK | yes | UUID |
| `name` | TEXT | yes | Display name (left rail) |
| `engine` | TEXT | yes | Plugin id, e.g. `postgres`, `mongodb` |
| `color_id` | TEXT | yes | UI identity color (`teal`, `rose`, …) |
| `environment` | TEXT | yes | `local` \| `staging` \| `production` |
| `read_only` | INTEGER | yes | `0` / `1` |
| `notes` | TEXT | no | Free text |
| `plugin_config` | TEXT | yes | JSON object; default `'{}'` |
| `secret_ref` | TEXT | no | Keychain reference |
| `last_connected_at` | TEXT | no | ISO-8601 UTC |
| `created_at` | TEXT | yes | ISO-8601 UTC |
| `updated_at` | TEXT | yes | ISO-8601 UTC |

**Constraints**

- `environment IN ('local', 'staging', 'production')`
- `read_only IN (0, 1)`

**Indexes**

- `idx_connections_engine` on `(engine)`
- `idx_connections_name` on `(name COLLATE NOCASE)`

**`engine` values (examples; not enforced in SQL)**

PostgreSQL, MySQL/MariaDB, SQLite, SQL Server, CockroachDB, TiDB, Oracle, ClickHouse, MongoDB, Redis, Cassandra/Scylla, DynamoDB, Neo4j, libSQL, CouchDB/Couchbase, InfluxDB, Firestore, Cosmos DB, Redshift, BigQuery, Snowflake, SurrealDB, …

Availability is controlled by the **plugin registry** in app code, not by a SQLite CHECK list (so future engines don’t need a migration).

---

### 5.3 `plugin_config` (JSON)

Stored as **TEXT** containing a JSON object. The **connector plugin** for `engine` defines and validates the shape.

#### Relational / SQL-ish

```json
{
  "host": "127.0.0.1",
  "port": 5432,
  "database": "app",
  "username": "postgres",
  "sslMode": "prefer"
}
```

Applies (with port/dialect differences) to: PostgreSQL, MySQL/MariaDB, CockroachDB, TiDB, SQL Server, Redshift, ClickHouse, Oracle (may add `serviceName` / `sid`), etc.

#### File / embedded

```json
{ "path": "/absolute/path/to/file.db" }
```

```json
{ "url": "libsql://..." }
```

SQLite, libSQL/Turso, similar.

#### MongoDB

```json
{
  "uri": "mongodb://localhost:27017",
  "database": "app"
}
```

#### Redis

```json
{
  "host": "127.0.0.1",
  "port": 6379,
  "db": 0
}
```

#### Cassandra / ScyllaDB

```json
{
  "contactPoints": ["10.0.0.1"],
  "port": 9042,
  "keyspace": "ks",
  "localDc": "dc1"
}
```

#### Neo4j

```json
{
  "uri": "bolt://localhost:7687",
  "database": "neo4j",
  "username": "neo4j"
}
```

#### SurrealDB

```json
{
  "endpoint": "http://127.0.0.1:8000",
  "namespace": "app",
  "database": "app",
  "username": "root"
}
```

#### DynamoDB

```json
{
  "region": "us-east-1",
  "endpoint": null,
  "profile": "default"
}
```

#### Snowflake

```json
{
  "account": "xy12345",
  "warehouse": "COMPUTE_WH",
  "database": "DB",
  "schema": "PUBLIC",
  "role": "SYSADMIN",
  "username": "user"
}
```

#### BigQuery

```json
{
  "projectId": "my-project",
  "location": "US"
}
```

#### CouchDB / InfluxDB / Firestore / Cosmos / …

Plugin-defined objects; still stored only in `plugin_config`.

**Rule:** passwords, tokens, and private keys are **not** fields inside `plugin_config`; they use `secret_ref` (and later dedicated secret refs if needed).

---

### 5.4 `query_groups`

Folders for saved queries, **per connection**.

| Column | Type | Required | Description |
| ------ | ---- | -------- | ----------- |
| `id` | TEXT PK | yes | UUID |
| `connection_id` | TEXT FK → `connections(id)` | yes | `ON DELETE CASCADE` |
| `name` | TEXT | yes | Group label |
| `sort_order` | INTEGER | yes | Display order |
| `created_at` | TEXT | yes | ISO-8601 |
| `updated_at` | TEXT | yes | ISO-8601 |

**Index:** `(connection_id, sort_order)`

---

### 5.5 `saved_queries`

Named queries in a connection’s library.

| Column | Type | Required | Description |
| ------ | ---- | -------- | ----------- |
| `id` | TEXT PK | yes | UUID |
| `connection_id` | TEXT FK → `connections` | yes | `ON DELETE CASCADE` |
| `group_id` | TEXT FK → `query_groups` | no | `ON DELETE SET NULL` |
| `name` | TEXT | yes | Display name |
| `body` | TEXT | yes | Query text (SQL, MQL, CQL, Cypher, …) |
| `language` | TEXT | no | Hint: `sql`, `mql`, `cql`, `cypher`, `redis`, … |
| `sort_order` | INTEGER | yes | Order within group/list |
| `created_at` | TEXT | yes | ISO-8601 |
| `updated_at` | TEXT | yes | ISO-8601 |

**Indexes:** `(connection_id, sort_order)`, `(group_id)`

---

### 5.6 `query_history`

Past runs (append-heavy).

| Column | Type | Required | Description |
| ------ | ---- | -------- | ----------- |
| `id` | TEXT PK | yes | UUID |
| `connection_id` | TEXT FK → `connections` | no | `ON DELETE SET NULL` |
| `engine` | TEXT | no | Denormalized plugin id |
| `body` | TEXT | yes | Executed text |
| `language` | TEXT | no | Language hint |
| `success` | INTEGER | yes | `0` / `1` |
| `row_count` | INTEGER | no | Rows returned when known |
| `duration_ms` | INTEGER | no | Elapsed time |
| `error_message` | TEXT | no | Failure detail (safe for UI) |
| `ran_at` | TEXT | yes | ISO-8601 UTC |

**Indexes:** `(connection_id, ran_at DESC)`, `(ran_at DESC)`

**Retention:** enforced in application code (e.g. keep last N rows), not by DB triggers (unless added later).

---

### 5.7 `app_settings`

Key–value settings (theme, density, AI provider prefs, …).

| Column | Type | Required | Description |
| ------ | ---- | -------- | ----------- |
| `key` | TEXT PK | yes | Setting key |
| `value` | TEXT | yes | JSON or plain string |
| `updated_at` | TEXT | yes | ISO-8601 |

Example keys: `theme`, `density`, `ai.enabled`, `ai.ollama.base_url`. API keys live in `ai_secrets` (encrypted), never here.

### 5.8 `ai_secrets`

Encrypted provider API keys for the query-tab agent. Ciphertext only.

| Column | Type | Required | Description |
| ------ | ---- | -------- | ----------- |
| `provider` | TEXT PK | yes | `openai`, `anthropic`, `google`, `deepseek`, `openrouter` |
| `ciphertext` | TEXT | yes | `v1:` + base64(nonce \|\| AES-256-GCM blob) |
| `updated_at` | TEXT | yes | ISO-8601 UTC |

Never returned by `list_settings`. Decrypt only in `mystique-agent`.

### 5.9 `agent_chats` / `agent_messages`

Query-tab agent threads, one connection, many chats. See `001_init.sql`.

| Table | Role |
| ----- | ---- |
| `agent_chats` | Thread id, title, provider, model |
| `agent_messages` | user / assistant / tool rows + optional `draft_body` |

---

## 6. Full DDL (reference)

Canonical copy: [`crates/mystique-core/migrations/001_init.sql`](../crates/mystique-core/migrations/001_init.sql).

```sql
PRAGMA foreign_keys = ON;

CREATE TABLE IF NOT EXISTS schema_migrations (
  version    INTEGER PRIMARY KEY,
  applied_at TEXT    NOT NULL
);

CREATE TABLE IF NOT EXISTS connections (
  id                TEXT PRIMARY KEY,
  name              TEXT    NOT NULL,
  engine            TEXT    NOT NULL,
  color_id          TEXT    NOT NULL DEFAULT 'slate',
  environment       TEXT    NOT NULL DEFAULT 'local'
                      CHECK (environment IN ('local', 'staging', 'production')),
  read_only         INTEGER NOT NULL DEFAULT 1
                      CHECK (read_only IN (0, 1)),
  notes             TEXT,
  plugin_config     TEXT    NOT NULL DEFAULT '{}',
  secret_ref        TEXT,
  last_connected_at TEXT,
  created_at        TEXT    NOT NULL,
  updated_at        TEXT    NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_connections_engine ON connections (engine);
CREATE INDEX IF NOT EXISTS idx_connections_name   ON connections (name COLLATE NOCASE);

CREATE TABLE IF NOT EXISTS query_groups (
  id              TEXT PRIMARY KEY,
  connection_id   TEXT    NOT NULL
                    REFERENCES connections (id) ON DELETE CASCADE,
  name            TEXT    NOT NULL,
  sort_order      INTEGER NOT NULL DEFAULT 0,
  created_at      TEXT    NOT NULL,
  updated_at      TEXT    NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_query_groups_connection
  ON query_groups (connection_id, sort_order);

CREATE TABLE IF NOT EXISTS saved_queries (
  id              TEXT PRIMARY KEY,
  connection_id   TEXT    NOT NULL
                    REFERENCES connections (id) ON DELETE CASCADE,
  group_id        TEXT
                    REFERENCES query_groups (id) ON DELETE SET NULL,
  name            TEXT    NOT NULL,
  body            TEXT    NOT NULL,
  language        TEXT,
  sort_order      INTEGER NOT NULL DEFAULT 0,
  created_at      TEXT    NOT NULL,
  updated_at      TEXT    NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_saved_queries_connection
  ON saved_queries (connection_id, sort_order);
CREATE INDEX IF NOT EXISTS idx_saved_queries_group
  ON saved_queries (group_id);

CREATE TABLE IF NOT EXISTS query_history (
  id              TEXT PRIMARY KEY,
  connection_id   TEXT
                    REFERENCES connections (id) ON DELETE SET NULL,
  engine          TEXT,
  body            TEXT    NOT NULL,
  language        TEXT,
  success         INTEGER NOT NULL CHECK (success IN (0, 1)),
  row_count       INTEGER,
  duration_ms     INTEGER,
  error_message   TEXT,
  ran_at          TEXT    NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_query_history_connection_ran
  ON query_history (connection_id, ran_at DESC);
CREATE INDEX IF NOT EXISTS idx_query_history_ran
  ON query_history (ran_at DESC);

CREATE TABLE IF NOT EXISTS app_settings (
  key        TEXT PRIMARY KEY,
  value      TEXT NOT NULL,
  updated_at TEXT NOT NULL
);

INSERT OR IGNORE INTO schema_migrations (version, applied_at)
VALUES (1, strftime('%Y-%m-%dT%H:%M:%fZ', 'now'));
```

---

## 7. Common application SQL

### 7.1 Connections

```sql
-- List (left rail)
SELECT id, name, engine, color_id, environment, read_only,
       plugin_config,
       secret_ref IS NOT NULL AS password_set,
       last_connected_at, notes, created_at, updated_at
FROM connections
ORDER BY name COLLATE NOCASE;

-- Insert
INSERT INTO connections (
  id, name, engine, color_id, environment, read_only, notes,
  plugin_config, secret_ref, last_connected_at, created_at, updated_at
) VALUES (
  :id, :name, :engine, :color_id, :environment, :read_only, :notes,
  :plugin_config, :secret_ref, NULL, :now, :now
);

-- Update
UPDATE connections SET
  name = :name,
  engine = :engine,
  color_id = :color_id,
  environment = :environment,
  read_only = :read_only,
  notes = :notes,
  plugin_config = :plugin_config,
  secret_ref = COALESCE(:secret_ref, secret_ref),
  updated_at = :now
WHERE id = :id;

-- Delete (cascades groups + saved queries)
DELETE FROM connections WHERE id = :id;

-- Mark last connected
UPDATE connections
SET last_connected_at = :now, updated_at = :now
WHERE id = :id;
```

### 7.2 Query groups & saved queries

```sql
SELECT * FROM query_groups
WHERE connection_id = :connection_id
ORDER BY sort_order, name COLLATE NOCASE;

INSERT INTO query_groups (id, connection_id, name, sort_order, created_at, updated_at)
VALUES (:id, :connection_id, :name, :sort_order, :now, :now);

SELECT * FROM saved_queries
WHERE connection_id = :connection_id
ORDER BY sort_order, name COLLATE NOCASE;

INSERT INTO saved_queries (
  id, connection_id, group_id, name, body, language, sort_order, created_at, updated_at
) VALUES (
  :id, :connection_id, :group_id, :name, :body, :language, :sort_order, :now, :now
);

UPDATE saved_queries SET
  group_id = :group_id,
  name = :name,
  body = :body,
  language = :language,
  sort_order = :sort_order,
  updated_at = :now
WHERE id = :id;

DELETE FROM saved_queries WHERE id = :id;
```

### 7.3 History

```sql
INSERT INTO query_history (
  id, connection_id, engine, body, language, success,
  row_count, duration_ms, error_message, ran_at
) VALUES (
  :id, :connection_id, :engine, :body, :language, :success,
  :row_count, :duration_ms, :error_message, :ran_at
);

SELECT * FROM query_history
WHERE connection_id = :connection_id
ORDER BY ran_at DESC
LIMIT :limit;

-- Example retention: keep latest 500 globally
DELETE FROM query_history
WHERE id NOT IN (
  SELECT id FROM query_history ORDER BY ran_at DESC LIMIT 500
);
```

### 7.4 Settings

```sql
INSERT INTO app_settings (key, value, updated_at)
VALUES (:key, :value, :now)
ON CONFLICT(key) DO UPDATE SET
  value = excluded.value,
  updated_at = excluded.updated_at;

SELECT value FROM app_settings WHERE key = :key;
```

---

## 8. Frontend DTO mapping

Align camelCase in TypeScript with snake_case in SQLite (serde rename or map in commands).

| SQLite | Frontend (example) |
| ------ | ------------------ |
| `color_id` | `colorId` |
| `read_only` | `readOnly` |
| `plugin_config` | `pluginConfig` (object after JSON.parse) |
| `secret_ref IS NOT NULL` | `passwordSet` |
| `defaultDatabase` (Beekeeper) | inside `pluginConfig.database` |
| `connectionType` (Beekeeper) | `engine` |

Do **not** put `password` on list/get DTOs.

---

## 9. Future extensions (planned, not in v1 DDL)

| Feature | Suggested approach |
| ------- | ------------------ |
| SSH tunnel | `ssh_config TEXT DEFAULT '{}'` + `ssh_secret_ref` (or nested secrets) |
| TLS file paths | Fields inside `plugin_config` or shared `tls_config TEXT` |
| Connection folders | `connection_folders` table + `connections.folder_id`, `position` |
| Workspaces | `workspace_id` on connections if multi-workspace ships |
| Import Beekeeper | Mapper from their columns/`*Options` → our `plugin_config` + keychain |
| Git-synced queries | Optional export of `saved_queries` to files; SQLite remains source of truth |

When extending:

1. Add a new file `00N_….sql` under `crates/mystique-core/migrations/`.
2. Insert into `schema_migrations`.
3. Prefer JSON columns over many typed columns for plugin-specific data.

---

## 10. Security checklist

- [ ] `PRAGMA foreign_keys = ON` on every connection  
- [ ] No password/token columns in SQLite  
- [ ] Keychain write/delete paired with profile create/update/delete  
- [ ] History retention and optional “disable history” per connection (app)  
- [ ] Do not log `plugin_config` if it might contain sensitive non-secret fields  
- [ ] App data directory permissions left to OS defaults; document backup risk of profile metadata  

---

## 11. Implementation map

| Layer | Responsibility |
| ----- | -------------- |
| `crates/mystique-core/migrations/*.sql` | DDL |
| `crates/mystique-core` `domain` / `storage` / `application` | types, SQLite, services |
| `src-tauri/src/ipc/` | Thin Tauri invoke adapters |
| OS keychain | secrets (planned) |
| Engine plugins | interpret `plugin_config`, open real sessions (planned crates) |
| Svelte `src/lib/api/` | invoke wrappers (not wired yet) |
| Home UI | list/save/delete profiles (no direct SQL) |

---

## 12. Document history

| Version | Date | Notes |
| ------- | ---- | ----- |
| 1.0 | 2026-07-25 | Initial multi-engine schema: `plugin_config`, queries, history, settings; Beekeeper comparison |

---

*Local SQLite remembers profiles and libraries. Plugins talk to the real databases.*
