/**
 * Persisted open workspace tabs (query / table / model) per connection.
 */
import type { SaveWorkspaceTabInput, WorkspaceTabRecord } from '#core/contracts'
import { invoke } from '#core/ipc'
import type { WorkspaceTab, WorkspaceTabKind } from './types'

export type { SaveWorkspaceTabInput, WorkspaceTabRecord }

export type ReplaceWorkspaceTabsInput = {
	connectionId: string
	tabs: SaveWorkspaceTabInput[]
	activeTabId?: string | null
}

export async function listWorkspaceTabs(
	connectionId: string,
): Promise<WorkspaceTabRecord[]> {
	return invoke('list_workspace_tabs', { connectionId })
}

export async function replaceWorkspaceTabs(
	input: ReplaceWorkspaceTabsInput,
): Promise<WorkspaceTabRecord[]> {
	return invoke('replace_workspace_tabs', { input })
}

/** One tab. Keystrokes use this so the other tabs' bodies stay put. */
export async function upsertWorkspaceTab(
	input: SaveWorkspaceTabInput,
): Promise<WorkspaceTabRecord> {
	return invoke('upsert_workspace_tab', { input })
}

export function recordToWorkspaceTab(r: WorkspaceTabRecord): WorkspaceTab {
	const kind = r.kind as WorkspaceTabKind
	return {
		id: r.id,
		connectionId: r.connectionId,
		kind,
		title: r.title,
		tableId: r.tableId ?? undefined,
		modelDatabase: r.modelDatabase ?? undefined,
		savedQueryId: r.savedQueryId ?? null,
		dirty: !!r.isDirty,
	}
}

export function workspaceTabToSaveInput(
	tab: WorkspaceTab,
	opts: {
		body?: string
		writeEnabled?: boolean
		language?: string | null
		sortOrder: number
		isActive: boolean
	},
): SaveWorkspaceTabInput {
	return {
		id: tab.id,
		connectionId: tab.connectionId,
		kind: tab.kind,
		title: tab.title,
		sortOrder: opts.sortOrder,
		isActive: opts.isActive,
		savedQueryId: tab.savedQueryId ?? null,
		body: tab.kind === 'query' ? (opts.body ?? '') : null,
		language: opts.language ?? null,
		writeEnabled: opts.writeEnabled ?? true,
		tableId: tab.tableId ?? null,
		modelDatabase: tab.modelDatabase ?? null,
		isDirty: !!tab.dirty,
	}
}
