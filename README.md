# Mystique

<img src="static/brand/mystique-mark.svg" width="48" height="48" alt="" />

Desktop database client. Profiles, queries, and history stay on your machine.

## What it does

- Connect to a saved profile and keep it open in a workspace
- Browse databases, schemas, tables, and collections
- Run SQL or MQL, with history and saved queries per connection
- Inspect, filter, import, and export rows
- Draw an ER diagram (SQL) or a collection map (Mongo)
- Watch live sessions and slow queries when the engine supports it
- Draft a query from the open connection (optional agent)
- Expose the same read tools to Claude, Grok, Codex, and other MCP clients on this machine

## Supported engines

| Engine     | Language | Connect     | ER             | Metrics |
| ---------- | -------- | ----------- | -------------- | ------- |
| PostgreSQL | SQL      | Host / port | Yes            | Yes     |
| MySQL      | SQL      | Host / port | Yes            | Yes     |
| SQLite     | SQL      | File        | Yes            | Yes     |
| MongoDB    | MQL      | URI         | Collection map | Yes     |

## License

MIT. See [CHANGELOG.md](./CHANGELOG.md) and [CONTRIBUTING.md](./CONTRIBUTING.md).
