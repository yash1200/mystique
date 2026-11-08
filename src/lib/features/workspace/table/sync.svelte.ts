/**
 * What happened to a table tab's rows, per tab. The table feature owns these
 * signals; whoever changes the rows calls the method, and the grid or the
 * row inspector of that tab reacts.
 *
 * - `rowsEdited`: the row inspector wrote into the table's rows in place.
 *   The grid adopts them but keeps its saved baseline, so they stay unsaved.
 * - `previewLoaded`: the server replaced the preview page (open, filter,
 *   sort, page). The grid adopts it as the new saved baseline unless the tab
 *   holds unsaved edits.
 * - `baselineChanged`: the saved baseline moved (save, refresh, reset,
 *   rollback, discard). The inspector drops its field drafts.
 *
 * Each signal is a per-tab version. A listener compares it with the version
 * it last applied, so a signal that arrives while a cell is being edited is
 * applied when the edit ends, and a tab that is not mounted ignores it.
 */
class TableSync {
	#rows = $state<Record<string, number>>({})
	#previews = $state<Record<string, number>>({})
	#baselines = $state<Record<string, number>>({})

	rowsEdited(tabId: string) {
		this.#rows[tabId] = (this.#rows[tabId] ?? 0) + 1
	}

	previewLoaded(tabId: string) {
		this.#previews[tabId] = (this.#previews[tabId] ?? 0) + 1
	}

	baselineChanged(tabId: string) {
		this.#baselines[tabId] = (this.#baselines[tabId] ?? 0) + 1
	}

	rowsVersion(tabId: string | null | undefined): number {
		return tabId ? (this.#rows[tabId] ?? 0) : 0
	}

	previewVersion(tabId: string | null | undefined): number {
		return tabId ? (this.#previews[tabId] ?? 0) : 0
	}

	baselineVersion(tabId: string | null | undefined): number {
		return tabId ? (this.#baselines[tabId] ?? 0) : 0
	}
}

export const tableSync = new TableSync()
