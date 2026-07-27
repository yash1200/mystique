/** Shared query UI types + helpers. History is loaded from BE (not mock). */

export type SavedQuery = {
  id: string;
  name: string;
  folder?: string;
  engine?: string;
  sql: string;
  updatedAt: string;
  connectionId?: string;
};

export type QueryHistoryEntry = {
  id: string;
  sql: string;
  ranAt: string;
  durationMs: number;
  status: "success" | "error";
  rowCount?: number;
  error?: string;
  connectionId?: string;
  connectionName?: string;
  engine?: string;
  language?: string;
};

/** Temporary FE-only saved queries until library UI is wired to BE. */
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
];

export function historyItemToEntry(
  item: {
    id: string;
    connectionId?: string | null;
    engine?: string | null;
    body: string;
    language?: string | null;
    success: boolean;
    rowCount?: number | null;
    durationMs?: number | null;
    errorMessage?: string | null;
    ranAt: string;
  },
  connectionName?: string,
): QueryHistoryEntry {
  return {
    id: item.id,
    sql: item.body,
    ranAt: item.ranAt,
    durationMs: item.durationMs ?? 0,
    status: item.success ? "success" : "error",
    rowCount: item.rowCount ?? undefined,
    error: item.errorMessage ?? undefined,
    connectionId: item.connectionId ?? undefined,
    connectionName,
    engine: item.engine ?? undefined,
    language: item.language ?? undefined,
  };
}

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
