/**
 * Query history API (local app SQLite via Tauri IPC).
 * Used by every engine (Postgres / MySQL / Mongo / SQLite).
 */
import { invoke } from "@tauri-apps/api/core";
import { isTauri } from "./tauri";

export interface QueryHistoryItem {
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
}

export interface AppendHistoryInput {
  connectionId?: string | null;
  engine?: string | null;
  body: string;
  language?: string | null;
  success: boolean;
  rowCount?: number | null;
  durationMs?: number | null;
  errorMessage?: string | null;
}

function requireTauri() {
  if (!isTauri()) {
    throw new Error("Query history requires Tauri (`npm run tauri dev`).");
  }
}

/** Normalize IPC payloads (camelCase preferred; tolerate snake_case). */
export function normalizeHistoryItem(raw: Record<string, unknown>): QueryHistoryItem {
  const g = (camel: string, snake: string) =>
    raw[camel] !== undefined ? raw[camel] : raw[snake];
  return {
    id: String(g("id", "id") ?? ""),
    connectionId: (g("connectionId", "connection_id") as string | null | undefined) ?? null,
    engine: (g("engine", "engine") as string | null | undefined) ?? null,
    body: String(g("body", "body") ?? ""),
    language: (g("language", "language") as string | null | undefined) ?? null,
    success: Boolean(g("success", "success")),
    rowCount: (g("rowCount", "row_count") as number | null | undefined) ?? null,
    durationMs: (g("durationMs", "duration_ms") as number | null | undefined) ?? null,
    errorMessage:
      (g("errorMessage", "error_message") as string | null | undefined) ?? null,
    ranAt: String(g("ranAt", "ran_at") ?? new Date().toISOString()),
  };
}

function normalizeList(rows: unknown): QueryHistoryItem[] {
  if (!Array.isArray(rows)) return [];
  return rows.map((r) =>
    normalizeHistoryItem((r ?? {}) as Record<string, unknown>),
  );
}

export async function appendQueryHistory(
  input: AppendHistoryInput,
): Promise<QueryHistoryItem> {
  requireTauri();
  const body = (input.body ?? "").trim();
  if (!body) {
    throw new Error("Cannot log empty query to history.");
  }
  const raw = await invoke<Record<string, unknown>>("append_query_history", {
    input: {
      connectionId: input.connectionId ?? null,
      engine: input.engine ?? null,
      body,
      language: input.language ?? null,
      success: input.success,
      rowCount: input.rowCount ?? null,
      durationMs: input.durationMs ?? null,
      errorMessage: input.errorMessage ?? null,
    },
  });
  return normalizeHistoryItem(raw ?? {});
}

export async function listQueryHistory(
  connectionId: string,
  limit = 100,
): Promise<QueryHistoryItem[]> {
  requireTauri();
  const id = connectionId.trim();
  if (!id) return listRecentQueryHistory(limit);
  try {
    // Tauri default: JS camelCase → Rust snake_case args.
    const rows = await invoke<unknown>("list_query_history", {
      connectionId: id,
      limit,
    });
    return normalizeList(rows);
  } catch (e1) {
    // Fallback if arg rename differs on some builds.
    try {
      const rows = await invoke<unknown>("list_query_history", {
        connection_id: id,
        limit,
      });
      return normalizeList(rows);
    } catch (e2) {
      const msg = e2 instanceof Error ? e2.message : String(e2 ?? e1);
      throw new Error(`list_query_history failed: ${msg}`);
    }
  }
}

export async function listRecentQueryHistory(
  limit = 100,
): Promise<QueryHistoryItem[]> {
  requireTauri();
  const rows = await invoke<unknown>("list_recent_query_history", { limit });
  return normalizeList(rows);
}

export async function clearQueryHistory(
  connectionId: string,
): Promise<number> {
  requireTauri();
  try {
    return await invoke<number>("clear_query_history", {
      connectionId,
    });
  } catch {
    return invoke<number>("clear_query_history", {
      connection_id: connectionId,
    });
  }
}
