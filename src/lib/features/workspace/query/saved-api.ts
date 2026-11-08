/**
 * Saved queries API (per connection, no groups).
 */
import { errorMessage, invoke, isTauri } from '#core/ipc'
import type {
	SaveQueryInput,
	SavedQuery,
	SavedQueryListItem,
} from './saved-types'

function requireTauri() {
	if (!isTauri()) {
		throw new Error('Saved queries require the Mystique desktop app.')
	}
}

/** Normalize camelCase / snake_case from IPC. */
function normalizeSavedQuery(raw: Record<string, unknown>): SavedQuery {
	const g = (camel: string, snake: string) =>
		raw[camel] !== undefined ? raw[camel] : raw[snake]
	return {
		id: String(g('id', 'id') ?? ''),
		connectionId: String(g('connectionId', 'connection_id') ?? ''),
		name: String(g('name', 'name') ?? ''),
		body: String(g('body', 'body') ?? ''),
		language: (g('language', 'language') as string | null | undefined) ?? null,
		sortOrder: Number(g('sortOrder', 'sort_order') ?? 0) || 0,
		favorite: Boolean(g('favorite', 'favorite')),
		createdAt: String(g('createdAt', 'created_at') ?? ''),
		updatedAt: String(g('updatedAt', 'updated_at') ?? ''),
	}
}

export async function listSavedQueries(
	connectionId: string,
): Promise<SavedQuery[]> {
	requireTauri()
	const id = connectionId.trim()
	if (!id) return []
	try {
		const rows = await invoke<unknown[]>('list_saved_queries', {
			connectionId: id,
		})
		return (Array.isArray(rows) ? rows : []).map((r) =>
			normalizeSavedQuery(r as Record<string, unknown>),
		)
	} catch (e1) {
		try {
			const rows = await invoke<unknown[]>('list_saved_queries', {
				connection_id: id,
			})
			return (Array.isArray(rows) ? rows : []).map((r) =>
				normalizeSavedQuery(r as Record<string, unknown>),
			)
		} catch (e2) {
			const msg = errorMessage(e2 ?? e1)
			throw new Error(`list_saved_queries failed: ${msg}`)
		}
	}
}

export async function saveQuery(input: SaveQueryInput): Promise<SavedQuery> {
	requireTauri()
	const payload = {
		id: input.id?.trim() || null,
		connectionId: input.connectionId.trim(),
		name: input.name.trim(),
		body: input.body,
		language: input.language ?? null,
		sortOrder: input.sortOrder ?? 0,
		favorite: input.favorite,
	}
	if (!payload.connectionId) {
		throw new Error('connectionId is required to save a query')
	}
	if (!payload.name) {
		throw new Error('query name is required')
	}
	try {
		const raw = await invoke<Record<string, unknown>>('save_query', {
			input: payload,
		})
		return normalizeSavedQuery(raw ?? {})
	} catch (e) {
		// Some builds expect snake_case field names inside the DTO.
		try {
			const raw = await invoke<Record<string, unknown>>('save_query', {
				input: {
					id: payload.id,
					connection_id: payload.connectionId,
					name: payload.name,
					body: payload.body,
					language: payload.language,
					sort_order: payload.sortOrder,
					favorite: payload.favorite,
				},
			})
			return normalizeSavedQuery(raw ?? {})
		} catch (e2) {
			const msg = errorMessage(e2 ?? e)
			throw new Error(`save_query failed: ${msg}`)
		}
	}
}

export async function deleteSavedQuery(id: string): Promise<void> {
	requireTauri()
	await invoke('delete_saved_query', { id })
}

/** Map BE SavedQuery → list panel shape. */
export function toListSavedQuery(q: SavedQuery): SavedQueryListItem {
	return {
		id: q.id,
		name: q.name,
		sql: q.body,
		favorite: q.favorite,
		createdAt: q.createdAt,
		updatedAt: q.updatedAt,
		connectionId: q.connectionId,
	}
}
