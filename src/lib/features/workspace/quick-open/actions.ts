import { commands } from '#core/commands'
import { tablePreview } from '#features/workspace/grid'
import {
	findCatalogTable,
	workspaceSession,
	workspaceStudio,
} from '#features/workspace/session'
import { appChrome, cmd } from '#lib/app'
import { goto } from '$app/navigation'
import { tick } from 'svelte'
import type { QuickOpenCommand, QuickOpenItem } from './items'

export async function runQuickOpenItem(item: QuickOpenItem) {
	if (item.kind === 'command' && item.command) {
		runCommand(item.command)
		return
	}
	if (item.kind === 'connection' && item.connectionId) {
		await openConnection(item.connectionId)
		return
	}
	if (item.kind === 'catalog' && item.connectionId && item.tableId) {
		await openCatalogObject(item.connectionId, item.tableId)
		return
	}
	if (
		item.kind === 'saved' &&
		item.connectionId &&
		item.savedId &&
		item.body != null
	) {
		await focusConnection(item.connectionId)
		workspaceSession.openSavedQuery(item.connectionId, {
			id: item.savedId,
			name: item.title,
			body: item.body,
		})
		return
	}
	if (item.kind === 'history' && item.body) {
		const connectionId =
			item.connectionId ?? workspaceSession.activeConnectionId
		if (!connectionId) return
		await focusConnection(connectionId)
		workspaceSession.openNewQueryWithText(connectionId, item.body, item.title)
	}
}

export function runCommand(command: QuickOpenCommand) {
	const id = workspaceSession.activeConnectionId
	if (command === 'toggle-rail') {
		appChrome.toggle(window.location.pathname)
		return
	}
	if (command === 'focus-filter') {
		focusExplorerFilter()
		return
	}
	if (command === 'prev-tab') {
		cycleConnection(-1)
		return
	}
	if (command === 'next-tab') {
		cycleConnection(1)
		return
	}
	if (!id) return
	if (command === 'new-query') {
		workspaceStudio.sideMode = 'explorer'
		workspaceSession.openNewQuery(id)
		return
	}
	if (command === 'metrics') {
		workspaceSession.openMetrics(id)
	}
}

export function focusExplorerFilter() {
	if (!workspaceSession.activeConnectionId) return
	workspaceStudio.sideMode = 'explorer'
	void tick().then(() => {
		const el = document.getElementById('explorer-filter')
		if (el instanceof HTMLElement) el.focus()
	})
}

export function focusTabIndex(index: number) {
	const tabs = tabsForActive()
	const tab = tabs[index]
	if (!tab) return
	workspaceSession.setActiveTab(tab.id)
}

export function cycleConnection(delta: -1 | 1) {
	const sessions = workspaceSession.sessions
	if (sessions.length < 2) return
	const current = sessions.findIndex(
		(session) => session.profile.id === workspaceSession.activeConnectionId,
	)
	const start = current < 0 ? 0 : current
	const next = (start + delta + sessions.length) % sessions.length
	const session = sessions[next]
	if (!session) return
	workspaceSession.setActiveConnection(session.profile.id)
}

function tabsForActive() {
	const id = workspaceSession.activeConnectionId
	if (!id) return []
	return workspaceSession.openTabs.filter((tab) => tab.connectionId === id)
}

async function openConnection(id: string) {
	const open = workspaceSession.sessions.some(
		(session) => session.profile.id === id,
	)
	if (open) {
		await focusConnection(id)
		return
	}
	// Home or the workspace connects it, whichever is mounted.
	commands.run(cmd.openConnectionById, { id })
}

async function openCatalogObject(connectionId: string, tableId: string) {
	await focusConnection(connectionId)
	workspaceStudio.sideMode = 'explorer'
	const table = findCatalogTable(
		workspaceStudio.catalogs[connectionId] ?? [],
		tableId,
	)
	if (!table) return
	await tablePreview.open(table)
}

async function focusConnection(id: string) {
	if (workspaceSession.sessions.some((session) => session.profile.id === id)) {
		workspaceSession.setActiveConnection(id)
	}
	const target = `/workspace?c=${encodeURIComponent(id)}`
	const here = `${window.location.pathname}${window.location.search}`
	if (here !== target) await goto(target)
}
