/** Dummy saved queries & history for workspace UI. */

export type SavedQuery = {
  id: string;
  name: string;
  /** Optional folder / group label */
  folder?: string;
  engine?: string;
  sql: string;
  updatedAt: string;
  connectionId?: string;
};

export type QueryHistoryEntry = {
  id: string;
  sql: string;
  /** ISO timestamp */
  ranAt: string;
  durationMs: number;
  status: "success" | "error";
  rowCount?: number;
  error?: string;
  connectionId?: string;
  connectionName?: string;
};

export const MOCK_SAVED_QUERIES: SavedQuery[] = [
  {
    id: "sq-1",
    name: "Active users",
    folder: "Reporting",
    sql: "SELECT id, email, display_name\nFROM users\nWHERE is_active = true\nORDER BY created_at DESC\nLIMIT 100;",
    updatedAt: "2026-07-20T14:22:00Z",
  },
  {
    id: "sq-2",
    name: "Orders last 7 days",
    folder: "Reporting",
    sql: "SELECT date_trunc('day', placed_at) AS day, count(*), sum(total_cents)\nFROM orders\nWHERE placed_at >= now() - interval '7 days'\nGROUP BY 1\nORDER BY 1;",
    updatedAt: "2026-07-18T09:10:00Z",
  },
  {
    id: "sq-3",
    name: "Slow query candidates",
    folder: "Ops",
    sql: "SELECT query, mean_exec_time, calls\nFROM pg_stat_statements\nORDER BY mean_exec_time DESC\nLIMIT 20;",
    updatedAt: "2026-07-12T16:40:00Z",
  },
  {
    id: "sq-4",
    name: "Schema inventory",
    folder: "Ops",
    sql: "SELECT table_schema, table_name, table_type\nFROM information_schema.tables\nWHERE table_schema NOT IN ('pg_catalog', 'information_schema')\nORDER BY 1, 2;",
    updatedAt: "2026-07-01T11:00:00Z",
  },
  {
    id: "sq-5",
    name: "Find user by email",
    folder: "Snippets",
    sql: "SELECT *\nFROM users\nWHERE email = $1;",
    updatedAt: "2026-06-28T08:15:00Z",
  },
];

export const MOCK_QUERY_HISTORY: QueryHistoryEntry[] = [
  {
    id: "qh-1",
    sql: "SELECT * FROM orders WHERE status = 'pending' LIMIT 50;",
    ranAt: "2026-07-25T18:42:11Z",
    durationMs: 18,
    status: "success",
    rowCount: 12,
    connectionName: "local-pg",
  },
  {
    id: "qh-2",
    sql: "UPDATE users SET is_active = false WHERE id = 3;",
    ranAt: "2026-07-25T18:40:02Z",
    durationMs: 6,
    status: "success",
    rowCount: 1,
    connectionName: "local-pg",
  },
  {
    id: "qh-3",
    sql: "SELECT count(*) FROM projects WHERE status = 'active';",
    ranAt: "2026-07-25T17:55:40Z",
    durationMs: 4,
    status: "success",
    rowCount: 1,
    connectionName: "local-pg",
  },
  {
    id: "qh-4",
    sql: "SELECT * FROM missing_table;",
    ranAt: "2026-07-25T16:12:09Z",
    durationMs: 2,
    status: "error",
    error: 'relation "missing_table" does not exist',
    connectionName: "local-pg",
  },
  {
    id: "qh-5",
    sql: "EXPLAIN ANALYZE SELECT * FROM users WHERE email LIKE '%@example.com';",
    ranAt: "2026-07-24T22:01:33Z",
    durationMs: 42,
    status: "success",
    rowCount: 8,
    connectionName: "local-pg",
  },
  {
    id: "qh-6",
    sql: "SELECT id, name FROM projects ORDER BY budget DESC LIMIT 10;",
    ranAt: "2026-07-24T20:18:00Z",
    durationMs: 11,
    status: "success",
    rowCount: 4,
    connectionName: "local-pg",
  },
];

export function formatRelativeTime(iso: string): string {
  const t = new Date(iso).getTime();
  if (Number.isNaN(t)) return iso;
  const diff = Date.now() - t;
  const mins = Math.floor(diff / 60000);
  if (mins < 1) return "just now";
  if (mins < 60) return `${mins}m ago`;
  const hours = Math.floor(mins / 60);
  if (hours < 24) return `${hours}h ago`;
  const days = Math.floor(hours / 24);
  if (days < 14) return `${days}d ago`;
  return new Intl.DateTimeFormat(undefined, { dateStyle: "medium" }).format(
    new Date(iso),
  );
}

export function previewSql(sql: string, max = 72): string {
  const one = sql.replace(/\s+/g, " ").trim();
  if (one.length <= max) return one;
  return `${one.slice(0, max - 1)}…`;
}
