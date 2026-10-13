/**
 * Query history API (local app SQLite via Tauri IPC).
 * Used by every engine (Postgres / MySQL / Mongo / SQLite).
 */
import { isTauri } from '#core/ipc'
import { invoke } from '@tauri-apps/api/core'
import type {
	AppendHistoryInput,
	QueryHistoryEntry,
	QueryHistoryItem,
} from './history-types'

export function historyItemToEntry(
	item: {
		id: string
		connectionId?: string | null
		engine?: string | null
		body: string
		language?: string | null
		success: boolean
		rowCount?: number | null
		durationMs?: number | null
		errorMessage?: string | null
		ranAt: string
	},
	connectionName?: string,
): QueryHistoryEntry {
	return {
		id: item.id,
		sql: item.body,
		ranAt: item.ranAt,
		durationMs: item.durationMs ?? 0,
		status: item.success ? 'success' : 'error',
		rowCount: item.rowCount ?? undefined,
		error: item.errorMessage ?? undefined,
		connectionId: item.connectionId ?? undefined,
		connectionName,
		engine: item.engine ?? undefined,
		language: item.language ?? undefined,
	}
}

function requireTauri() {
	if (!isTauri()) {
		throw new Error('Query history requires the Mystique desktop app.')
	}
}

/** Normalize IPC payloads (camelCase preferred; tolerate snake_case). */
export function normalizeHistoryItem(
	raw: Record<string, unknown>,
): QueryHistoryItem {
	const g = (camel: string, snake: string) =>
		raw[camel] !== undefined ? raw[camel] : raw[snake]
	return {
		id: String(g('id', 'id') ?? ''),
		connectionId:
			(g('connectionId', 'connection_id') as string | null | undefined) ?? null,
		engine: (g('engine', 'engine') as string | null | undefined) ?? null,
		body: String(g('body', 'body') ?? ''),
		language: (g('language', 'language') as string | null | undefined) ?? null,
		success: Boolean(g('success', 'success')),
		rowCount: (g('rowCount', 'row_count') as number | null | undefined) ?? null,
		durationMs:
			(g('durationMs', 'duration_ms') as number | null | undefined) ?? null,
		errorMessage:
			(g('errorMessage', 'error_message') as string | null | undefined) ?? null,
		ranAt: String(g('ranAt', 'ran_at') ?? new Date().toISOString()),
	}
}

function normalizeList(rows: unknown): QueryHistoryItem[] {
	if (!Array.isArray(rows)) return []
	return rows.map((r) =>
		normalizeHistoryItem((r ?? {}) as Record<string, unknown>),
	)
}

/**
 * True for history rows created by the app (explorer table preview, etc.),
 * not by the user typing/running in the Query editor.
 */
export function isInternalAppHistoryBody(body: string): boolean {
	const t = (body ?? '').trim()
	if (!t) return true
	// Explicit Mongo table-preview marker (legacy)
	if (/^\/\/\s*preview\b/i.test(t)) return true
	// Explorer open-table previews: single-line SELECT * … LIMIT 100
	if (
		!t.includes('\n') &&
		/^SELECT\s+\*\s+FROM\s+.+\s+LIMIT\s+100\s*;?\s*$/i.test(t)
	) {
		return true
	}
	// Catalog / system introspection (should not have been logged; filter if present)
	if (/\binformation_schema\b/i.test(t)) return true
	if (/\bpg_catalog\b/i.test(t)) return true
	if (/\bsqlite_master\b/i.test(t)) return true
	if (/^\s*pragma\s+/i.test(t)) return true
	if (/^\s*list(databases|collections)\b/i.test(t)) return true
	return false
}

/** Keep only user-run statements for History UI. */
export function filterUserHistory(
	items: QueryHistoryItem[],
): QueryHistoryItem[] {
	return items.filter((it) => !isInternalAppHistoryBody(it.body))
}

export async function appendQueryHistory(
	input: AppendHistoryInput,
): Promise<QueryHistoryItem> {
	requireTauri()
	const body = (input.body ?? '').trim()
	if (!body) {
		throw new Error('Cannot log empty query to history.')
	}
	const raw = await invoke<Record<string, unknown>>('append_query_history', {
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
	})
	return normalizeHistoryItem(raw ?? {})
}

export async function listQueryHistory(
	connectionId: string,
	limit = 100,
): Promise<QueryHistoryItem[]> {
	requireTauri()
	const id = connectionId.trim()
	if (!id) return listRecentQueryHistory(limit)
	// Fetch extra rows so filtering internal previews still fills the UI limit.
	const fetchLimit = Math.min(500, Math.max(limit * 3, limit + 50))
	try {
		// Tauri default: JS camelCase → Rust snake_case args.
		const rows = await invoke<unknown>('list_query_history', {
			connectionId: id,
			limit: fetchLimit,
		})
		return filterUserHistory(normalizeList(rows)).slice(0, limit)
	} catch (e1) {
		// Fallback if arg rename differs on some builds.
		try {
			const rows = await invoke<unknown>('list_query_history', {
				connection_id: id,
				limit: fetchLimit,
			})
			return filterUserHistory(normalizeList(rows)).slice(0, limit)
		} catch (e2) {
			const msg = e2 instanceof Error ? e2.message : String(e2 ?? e1)
			throw new Error(`list_query_history failed: ${msg}`)
		}
	}
}

export async function listRecentQueryHistory(
	limit = 100,
): Promise<QueryHistoryItem[]> {
	requireTauri()
	const fetchLimit = Math.min(500, Math.max(limit * 3, limit + 50))
	const rows = await invoke<unknown>('list_recent_query_history', {
		limit: fetchLimit,
	})
	return filterUserHistory(normalizeList(rows)).slice(0, limit)
}
