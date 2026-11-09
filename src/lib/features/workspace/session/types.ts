/** Workspace chrome types: what a tab is, and what row is selected. */

import type { TablePreviewFilter, TablePreviewSort } from '#core/contracts'

/** Main workspace tab: query canvas, table data, ER / model, structure, or server metrics. */
export type WorkspaceTabKind = 'query' | 'table' | 'model' | 'structure' | 'metrics'

/** Table-grid preview for one tab. In-memory only — not persisted with the tab. */
export type TableTabPreview = {
	filters: TablePreviewFilter[]
	codeFilter: string | null
	sort: TablePreviewSort | null
	offset: number
}

function emptyTablePreview(): TableTabPreview {
	return { filters: [], codeFilter: null, sort: null, offset: 0 }
}

/**
 * Decide which preview a table tab should use.
 * `replace` is pager / apply / clear — always take the incoming query.
 * Otherwise keep the tab's existing query (explorer re-open / tab focus)
 * unless the caller is seeding a new predicate (FK jump).
 */
export function resolveTablePreview(
	existing: TableTabPreview | undefined,
	incoming: {
		offset?: number
		filters?: TablePreviewFilter[]
		sort?: TablePreviewSort | null
		codeFilter?: string | null
		replace?: boolean
	},
): TableTabPreview {
	const next: TableTabPreview = {
		filters: incoming.filters ?? [],
		codeFilter: incoming.codeFilter?.trim() || null,
		sort: incoming.sort ?? null,
		offset: incoming.offset ?? 0,
	}
	if (incoming.replace) return next
	const seeded =
		next.filters.length > 0 || !!next.codeFilter || !!next.sort
	if (seeded) return next
	if (!existing) return emptyTablePreview()
	// The grid keeps its page across tab switches, so keep that offset too.
	return { ...existing }
}

export type WorkspaceTab = {
	id: string
	connectionId: string
	kind: WorkspaceTabKind
	/** Present when kind === "table", "structure", or a table-scoped "model" tab. */
	tableId?: string
	/**
	 * Database name the model/ER tab was opened for (kind === "model").
	 * Empty string for single-db engines when unknown.
	 */
	modelDatabase?: string
	title: string
	/**
	 * Linked `saved_queries.id` when this query tab is persisted in the library.
	 * Null/undefined = never saved (or table/model tab).
	 */
	savedQueryId?: string | null
	/**
	 * Unsaved state: query draft differs from saved body, or table grid has edits.
	 * Shown as a dirty indicator on the tab chrome.
	 */
	dirty?: boolean
	/** Filters / sort / page for a table tab. Dropped when the tab closes. */
	preview?: TableTabPreview
}

export type SelectedRow = {
	connectionId: string
	tableId: string
	rowIndex: number
}
