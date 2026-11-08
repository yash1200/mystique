/**
 * One tab's draft, for the keystroke upsert.
 * The payload is that tab only, so other bodies never go out on a keystroke.
 */
import type { SaveWorkspaceTabInput } from './api'
import type { WorkspaceTab } from './types'

export function tabDraftUpsert(
	tabs: readonly WorkspaceTab[],
	tabId: string,
	body: string,
	activeTabId: string | null,
): SaveWorkspaceTabInput | null {
	const index = tabs.findIndex((tab) => tab.id === tabId)
	const tab = index >= 0 ? tabs[index] : undefined
	if (!tab) return null
	return {
		id: tab.id,
		connectionId: tab.connectionId,
		kind: tab.kind,
		title: tab.title,
		sortOrder: index,
		isActive: tab.id === activeTabId,
		savedQueryId: tab.savedQueryId ?? null,
		body: tab.kind === 'query' ? body : null,
		language: null,
		writeEnabled: true,
		tableId: tab.tableId ?? null,
		modelDatabase: tab.modelDatabase ?? null,
		isDirty: !!tab.dirty,
	}
}
