# Local SQLite schema

> **Schema SQL (canonical):** [`crates/mystique-core/schema/schema.sql`](../crates/mystique-core/schema/schema.sql)
> **Code:** `crates/mystique-core/src/storage/sqlite/schema.rs` · repositories in `storage/sqlite/repositories/`
> **Backend layout:** [`BACKEND_STRUCTURE.md`](./BACKEND_STRUCTURE.md)

Mystique keeps its own data (connection profiles, saved queries, history,
chats, settings) in one SQLite file. It is not the user's database; those are
reached at runtime through the engine plugins.

## File

| | |
| --- | --- |
| Name | `mystique.db` |
| Where | Tauri app data dir (macOS: `~/Library/Application Support/<bundle id>/`) |
| Opened with | `foreign_keys = ON`, `journal_mode = WAL`, `busy_timeout = 5000` |
| Access | Rust only (`rusqlite`); the UI goes through Tauri commands |

## Versioning: one schema, no migrations

The whole schema is one file, created in one transaction on a new database.
`PRAGMA user_version` holds `SCHEMA_VERSION` (currently **1**).

On open (`AppDatabase::open`):

| The file holds | What happens |
| --- | --- |
| Nothing | The schema is created and stamped. |
| This version | Opened as is. |
| An older schema (any build before the squash, or a lower version) | The file and its `-wal` / `-shm` are renamed to `mystique.db.<UTC time>.bak`, and a new database is created. Nothing is converted. |
| A newer version | Refused with an error; the file is left untouched. |

To change the schema: edit `schema.sql`, bump `SCHEMA_VERSION`, and update the
repositories and tests. There is no upgrade path between versions, so a bump
means users start with an empty database (their old file is kept as `.bak`).
Write a real migration instead only once that is no longer acceptable.

## Design

- **Engine-agnostic profiles.** A connection row stores `engine` plus
  `plugin_config`, a JSON object owned by that engine's plugin (host and port,
  a URI, a file path…). A new engine needs no new columns.
- **No secrets in rows.** Passwords, SSH secrets and TLS passphrases are
  AES-256-GCM envelopes in `secrets`, sealed with one vault key that lives in
  the OS keychain (a 0600 file in debug builds). `connections.secret_ref` /
  `ssh_secret_ref` only say *whether* a sealed secret exists (`'vault'` or
  NULL). AI provider keys and agent environment secrets are envelopes too,
  owned by `'app'`. See `crates/mystique-core/src/secrets.rs`.
- **Conventions.** Ids are `INTEGER PRIMARY KEY AUTOINCREMENT` (never reused);
  timestamps are ISO-8601 UTC text; flags are `INTEGER` 0/1 with a `CHECK`;
  JSON is `TEXT`.

## Tables

| Table | Holds | Key points |
| --- | --- | --- |
| `connections` | Saved profiles | `environment` ∈ local / staging / production; `read_only`, `favorite`, `save_password` flags; `plugin_config` and `ssh_config` JSON; `query_timeout_secs` NULL = engine default, 0 = none. |
| `secrets` | Sealed secrets | Key `(owner_kind, owner_id, kind)`: `'connection'` + profile id, or `'app'`. Deleted with their profile in the same transaction. |
| `saved_queries` | Saved queries per connection | Cascades with the connection; `favorite`, `sort_order`. |
| `query_history` | Every run, append-only | `connection_id` goes NULL when the profile is deleted, so history survives. `kind` (query, grid_save, ddl, rollback, explain_analyze) and `source` (ui, agent, mcp). Retention is enforced in app code. |
| `workspace_tabs` | Tabs restored on reopen | Query, table and model tabs; cascades with the connection. |
| `grid_column_layout` | Column order, hidden, frozen, widths per table | `connection_id` is text: a profile id, or a session id for an unsaved connection, so it is not a foreign key. |
| `grid_saves` | Undo for the last grid saves (20 per table) | Same `connection_id` rule. |
| `tool_access` | MCP / agent access level per connection or database | `database = ''` is the connection-wide row; `level` NULL inherits the global default (an app setting). |
| `agent_chats`, `agent_messages` | Query-tab agent threads | Cascade with the connection / chat. |
| `app_settings` | Key-value settings | Theme, density, AI and MCP options, `install_id` (part of every envelope's AAD). |
| `ssh_known_hosts` | Accepted SSH host keys | Key `(host, port)`. |

### Trigger

`connections_clear_grid` deletes a profile's `grid_column_layout` and
`grid_saves` rows when the profile is deleted (they cannot cascade; see above).

### Indexes

Name and favourite lookups on `connections`; `(connection_id, sort_order)` and
`(connection_id, favorite, sort_order)` on `saved_queries`; a keyset index
`(connection_id, ran_at DESC, id DESC)` plus `ran_at DESC` on `query_history`;
one per parent for tabs, grid saves, chats and messages.

## `plugin_config` examples

Passwords never go here; the profile service strips them before a save.

```json
{ "host": "db.internal", "port": 5432, "database": "app", "username": "app_user", "sslMode": "require" }
```

```json
{ "path": "/Users/me/data/app.sqlite" }
```

```json
{ "uri": "mongodb+srv://cluster0.example.net/app", "username": "reader" }
```

A Mongo URI is stored without its password; the username moves to
`username`.
