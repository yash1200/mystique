/** Save-query flow. Modal state lives here, not on WorkspaceShell. */
import { isTauri } from '#core/ipc'
import { workspaceSession, workspaceStudio } from '#features/workspace/session'
import { queryLanguageFor } from './policy'
import { saveQuery } from './saved-api'

class QuerySave {
	modalOpen = $state(false)
	tabId = $state<string | null>(null)
	initialName = $state('Query')
	error = $state<string | null>(null)
	thenClose = $state(false)
	private resolver: ((ok: boolean) => void) | null = null

	needsNamePrompt(title: string): boolean {
		const name = title?.trim() || ''
		if (!name) return true
		if (/^untitled query(?:\s+\d+)?$/i.test(name)) return true
		if (/^query(?:\s+\d+)?$/i.test(name)) return true
		return false
	}

	closeModal(ok: boolean) {
		this.modalOpen = false
		this.error = null
		const resolve = this.resolver
		this.resolver = null
		this.tabId = null
		resolve?.(ok)
	}

	async persist(tabId: string, name: string): Promise<boolean> {
		const tab = workspaceSession.openTabs.find((t) => t.id === tabId)
		const session = workspaceSession.sessions.find(
			(s) => s.profile.id === tab?.connectionId,
		)
		if (!tab || tab.kind !== 'query' || !session) {
			throw new Error('No query tab / connection to save.')
		}
		if (!isTauri()) {
			throw new Error('Saving requires the desktop app (Tauri).')
		}
		const body = workspaceSession.queryDrafts[tab.id] ?? ''
		if (!body.trim()) {
			throw new Error('Query is empty — nothing to save.')
		}
		const language = queryLanguageFor(
			String(session.profile.engine),
			session.capabilities,
		)
		const saved = await saveQuery({
			id: tab.savedQueryId ?? null,
			connectionId: tab.connectionId,
			name: name.trim(),
			body,
			language,
		})
		workspaceSession.markQuerySaved(tab.id, {
			id: saved.id,
			name: saved.name,
			body: saved.body,
		})
		const { refreshSavedQueries } = await import('./library.svelte')
		await refreshSavedQueries(tab.connectionId, true)
		workspaceStudio.selectedQueryId = saved.id
		return true
	}

	async confirmName(name: string) {
		const tabId = this.tabId
		if (!tabId) {
			this.closeModal(false)
			return
		}
		workspaceStudio.querySaving = true
		this.error = null
		try {
			await this.persist(tabId, name)
			this.thenClose = false
			this.closeModal(true)
		} catch (e) {
			this.error = e instanceof Error ? e.message : String(e)
			console.warn('[mystique] save query:', e)
		} finally {
			workspaceStudio.querySaving = false
		}
	}

	async saveTab(
		tabId: string,
		opts?: { thenClose?: boolean },
	): Promise<boolean> {
		const tab = workspaceSession.openTabs.find((t) => t.id === tabId)
		const session = workspaceSession.sessions.find(
			(s) => s.profile.id === tab?.connectionId,
		)
		if (!tab || tab.kind !== 'query' || !session) return false
		if (!isTauri()) {
			alert('Saving requires the desktop app (run with Tauri).')
			return false
		}
		const body = workspaceSession.queryDrafts[tab.id] ?? ''
		if (!body.trim()) {
			workspaceSession.discardQueryDraft(tab.id)
			return true
		}
		if (this.needsNamePrompt(tab.title)) {
			this.tabId = tabId
			this.initialName = 'Query'
			this.error = null
			this.thenClose = !!opts?.thenClose
			this.modalOpen = true
			return await new Promise<boolean>((resolve) => {
				this.resolver = resolve
			})
		}
		workspaceStudio.querySaving = true
		try {
			await this.persist(tabId, tab.title.trim())
			return true
		} catch (e) {
			console.warn('[mystique] save query:', e)
			alert(e instanceof Error ? e.message : String(e))
			return false
		} finally {
			workspaceStudio.querySaving = false
		}
	}

	async saveActive() {
		const tab = workspaceSession.activeTab
		if (!tab || tab.kind !== 'query') return
		await this.saveTab(tab.id)
	}
}

export const querySave = new QuerySave()
