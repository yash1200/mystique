/**
 * Query history API (local app SQLite via Tauri IPC).
 * Used by every engine (Postgres / MySQL / Mongo / SQLite).
 */
import type { QueryHistoryPage } from '#core/contracts'
import { errorMessage, invoke, isTauri } from '#core/ipc'
import type {
	AppendHistoryInput,
	QueryHistoryEntry,
	QueryHistoryItem,
	QueryHistorySource,
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
		database?: string | null
		affectedRows?: number | null
		source?: QueryHistorySource | null
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
		database: item.database?.trim() || undefined,
		affectedRows: item.affectedRows ?? undefined,
		source: historySource(item.source),
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
		database: (g('database', 'database') as string | null | undefined) ?? null,
		affectedRows:
			(g('affectedRows', 'affected_rows') as number | null | undefined) ?? null,
		source: historySource(g('source', 'source')),
	}
}

function historySource(value: unknown): QueryHistorySource {
	if (value === 'agent' || value === 'mcp' || value === 'ui') return value
	return 'ui'
}

export type { QueryHistoryPage }

export type HistoryListOptions = {
	search?: string
	cursorRanAt?: string
	cursorId?: string
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

/** Manual insert. Query runs are recorded by the connection manager. */
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
			database: input.database ?? null,
			affectedRows: input.affectedRows ?? null,
			source: input.source ?? 'ui',
		},
	})
	return normalizeHistoryItem(raw ?? {})
}

function normalizePage(raw: unknown): QueryHistoryPage {
	if (Array.isArray(raw)) {
		return { items: filterUserHistory(normalizeList(raw)), hasMore: false }
	}
	if (raw && typeof raw === 'object') {
		const obj = raw as Record<string, unknown>
		const items = obj.items ?? obj.Items
		return {
			items: filterUserHistory(normalizeList(items)),
			hasMore: Boolean(obj.hasMore ?? obj.has_more),
		}
	}
	return { items: [], hasMore: false }
}

function listArgs(
	limit: number,
	opts?: HistoryListOptions,
): Record<string, unknown> {
	const search = opts?.search?.trim() || null
	return {
		limit,
		search,
		cursorRanAt: opts?.cursorRanAt ?? null,
		cursorId: opts?.cursorId ?? null,
	}
}

export async function listQueryHistory(
	connectionId: string,
	limit = 100,
	opts?: HistoryListOptions,
): Promise<QueryHistoryPage> {
	requireTauri()
	const id = connectionId.trim()
	if (!id) return listRecentQueryHistory(limit, opts)
	const args = listArgs(limit, opts)
	try {
		const raw = await invoke<unknown>('list_query_history', {
			connectionId: id,
			...args,
		})
		return normalizePage(raw)
	} catch (e1) {
		try {
			const raw = await invoke<unknown>('list_query_history', {
				connection_id: id,
				...args,
			})
			return normalizePage(raw)
		} catch (e2) {
			const msg = errorMessage(e2 ?? e1)
			throw new Error(`list_query_history failed: ${msg}`)
		}
	}
}

export async function listRecentQueryHistory(
	limit = 100,
	opts?: HistoryListOptions,
): Promise<QueryHistoryPage> {
	requireTauri()
	const raw = await invoke<unknown>('list_recent_query_history', listArgs(limit, opts))
	return normalizePage(raw)
}
