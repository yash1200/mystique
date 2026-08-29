/** Workspace chrome types: what a tab is, and what row is selected. */

/** Main workspace tab: query canvas, table data, ER / model, or object structure. */
export type WorkspaceTabKind = 'query' | 'table' | 'model' | 'structure'

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
}

export type SelectedRow = {
	connectionId: string
	tableId: string
	rowIndex: number
}
