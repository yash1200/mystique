/** Recently closed tabs, newest last. ⌘⇧T reopens them. */
import type { WorkspaceTab } from './types'

export const CLOSED_TAB_LIMIT = 20

export type ClosedTab = {
	tab: WorkspaceTab
	/** Where it sat in the open tab list. */
	index: number
	/** Query tabs: editor text and the saved body it is compared with. */
	text?: string
	baseline?: string
}

export function closedTabEntry(
	tab: WorkspaceTab,
	index: number,
	drafts: Record<string, string>,
	baselines: Record<string, string>,
): ClosedTab {
	// Filters and the edit buffer go with the tab, as they do on close.
	const copy: WorkspaceTab = { ...tab, dirty: false }
	delete copy.preview
	const entry: ClosedTab = { tab: copy, index }
	if (tab.kind === 'query') {
		entry.text = drafts[tab.id] ?? ''
		entry.baseline = baselines[tab.id] ?? ''
	}
	return entry
}

export function rememberClosed(
	stack: readonly ClosedTab[],
	entry: ClosedTab,
	limit = CLOSED_TAB_LIMIT,
): ClosedTab[] {
	const next = stack.filter((item) => item.tab.id !== entry.tab.id)
	next.push(entry)
	return next.slice(Math.max(0, next.length - limit))
}

export function forgetConnection(
	stack: readonly ClosedTab[],
	connectionId: string,
): ClosedTab[] {
	return stack.filter((item) => item.tab.connectionId !== connectionId)
}

/**
 * Newest entry that can come back: its connection is live and the tab is
 * not open again already. Entries skipped on the way are dropped.
 */
export function takeClosed(
	stack: readonly ClosedTab[],
	isLive: (connectionId: string) => boolean,
	isOpen: (tabId: string) => boolean,
): { entry: ClosedTab | null; rest: ClosedTab[] } {
	const rest = [...stack]
	while (rest.length > 0) {
		const entry = rest.pop()
		if (!entry) break
		if (isLive(entry.tab.connectionId) && !isOpen(entry.tab.id)) {
			return { entry, rest }
		}
	}
	return { entry: null, rest }
}

/** The tab as it reopens, and the open list with it back in place. */
export function reopenTab(
	openTabs: readonly WorkspaceTab[],
	entry: ClosedTab,
): { tab: WorkspaceTab; openTabs: WorkspaceTab[] } {
	const dirty =
		entry.tab.kind === 'query' && (entry.text ?? '') !== (entry.baseline ?? '')
	const tab: WorkspaceTab = { ...entry.tab, dirty }
	const index = Math.max(0, Math.min(entry.index, openTabs.length))
	const next = [...openTabs]
	next.splice(index, 0, tab)
	return { tab, openTabs: next }
}
