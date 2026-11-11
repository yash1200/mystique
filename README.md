# Mystique

<img src="static/brand/mystique-mark.svg" width="48" height="48" alt="" />

A calm, fast desktop client for your databases. Connections, queries and
history stay on your machine.

## Highlights

- **Connections, organised.** Save profiles with a colour, an environment
  (production gets a clear warning), favourites and read-only mode. Connect
  over SSL or an SSH tunnel, or paste a connection URL.
- **A workspace per connection.** Keep several databases open side by side and
  switch between them from the session rail.
- **Browse and edit data.** Filter, sort and page through rows, edit cells
  inline, and review the exact SQL before you save.
- **Structure at a glance.** Columns, keys, indexes, foreign keys, triggers and
  constraints for any table, with a schema compare.
- **ER diagrams.** See a table and its relations on a canvas, auto-arrange it,
  export a PNG or copy it as DBML.
- **Query editor.** Run the statement at the cursor or the whole script, explain
  plans, keep history and saved queries per connection.
- **Agent in the query tab (optional).** Ask it to draft or fix a query against
  the open connection. The same read-only tools are available to Claude Code,
  Codex, Cursor and other MCP clients on your machine.
- **Server metrics.** Live sessions and slow queries where the engine supports
  them.
- **Import and export** rows as CSV, JSON or JSON Lines (export also writes
  SQL inserts).
- **Quick open (⌘K / Ctrl+K)** for connections, tables, saved queries and
  commands.

## Look and feel

Quiet surfaces, loud data: the interface stays out of the way so connection
colours, environment badges and results carry the signal. Pick a dark theme
(Night, Abyss) or a light one (Day, Linen), or follow the system. The full
design language is in [docs/DESIGN_LANGUAGE.md](./docs/DESIGN_LANGUAGE.md).

## Supported databases

| Database                         | Language | Connect     | ER diagram     | Metrics |
| -------------------------------- | -------- | ----------- | -------------- | ------- |
| PostgreSQL, CockroachDB          | SQL      | Host / port | Yes            | Yes     |
| MySQL, MariaDB, TiDB             | SQL      | Host / port | Yes            | Yes     |
| SQLite                           | SQL      | File        | Yes            | Yes     |
| MongoDB, DocumentDB              | MQL      | URI         | Collection map | Yes     |

## Run it locally

You need Node 18+, pnpm, Rust and the usual Tauri prerequisites for your OS.

```bash
pnpm install
pnpm tauri dev
```

More setup notes, checks and the release process are in
[CONTRIBUTING.md](./CONTRIBUTING.md).

## License

MIT. See [LICENSE](./LICENSE) and [CHANGELOG.md](./CHANGELOG.md).
