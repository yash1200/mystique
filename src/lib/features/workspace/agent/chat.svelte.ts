/** Agent conversation cache. Survives query-tab remounts and pane close. */

import { errorMessage } from '#core/ipc'
import { ACTION_DISMISS_MS } from '#features/toast'
import {
	cancelAgentMessage,
	deleteAgentChat,
	listAgentChats,
	listAgentMessages,
	listAgentModels,
} from './api'
import { chatAfterConnectionChange } from './connection-chat'
import { DeferredDeletes, restoreAt } from './deferred-delete'
import { applyAgentEvent, isFinished, newLiveRun, type LiveRun } from './live-run'
import type {
	AgentChat,
	AgentEvent,
	AgentMessage,
	AgentModelInfo,
	AgentTokens,
} from './types'

class AgentChatStore {
	connectionId = $state<string | null>(null)
	chats = $state<AgentChat[]>([])
	messages = $state<AgentMessage[]>([])
	chatId = $state<string | null>(null)
	provider = $state('')
	model = $state('')
	models = $state<AgentModelInfo[]>([])
	modelsByProvider = $state<Record<string, AgentModelInfo[]>>({})
	modelsLoading = $state(false)
	modelsError = $state<string | null>(null)
	modelsFor = $state<string | null>(null)
	draft = $state('')
	/** The reply streaming now. Lives here so a remounted pane picks it up. */
	live = $state<LiveRun | null>(null)
	/** The last run hit its turn budget: the pane offers Continue. */
	exhausted = $state<{ chatId: string; turns: number } | null>(null)
	/** Token use of the last finished run. */
	lastTokens = $state<AgentTokens | null>(null)
	/** Why the last run ended without an answer (not shown for Stop). */
	runError = $state<string | null>(null)
	#chatsFor: string | null = null
	#messagesFor: string | null = null
	#loadedProviders = new Set<string>()
	#deletes = new DeferredDeletes(
		(id) => this.#commitDelete(id),
		ACTION_DISMISS_MS,
	)
	#deleteFailed: ((message: string) => void) | null = null

	loadChats = async (connectionId: string, force = false) => {
		if (!force && this.#chatsFor === connectionId) return this.chats
		const rows = await listAgentChats(connectionId)
		// A slower response for the previous connection must not replace this one.
		if (this.connectionId !== connectionId) return rows
		this.#chatsFor = connectionId
		this.setChats(rows)
		return this.chats
	}

	/** Replace the list, hiding chats whose delete is still in its Undo window. */
	setChats = (rows: AgentChat[]) => {
		this.chats = rows.filter((c) => !this.#deletes.has(String(c.id)))
	}

	loadMessages = async (chatId: string | null, force = false) => {
		if (!chatId) {
			this.messages = []
			this.#messagesFor = null
			return []
		}
		if (!force && this.#messagesFor === chatId && this.messages.length > 0) {
			return this.messages
		}
		const rows = await listAgentMessages(String(chatId))
		this.chatId = chatId
		this.#messagesFor = chatId
		this.messages = rows
		return rows
	}

	#fetchModels = async (provider: string, force = false) => {
		if (!force && this.#loadedProviders.has(provider)) {
			return this.modelsByProvider[provider] ?? []
		}
		const rows = await listAgentModels(provider)
		this.modelsByProvider = { ...this.modelsByProvider, [provider]: rows }
		this.#loadedProviders.add(provider)
		return rows
	}

	loadModels = async (provider: string, force = false) => {
		if (!force && this.modelsFor === provider && this.models.length > 0) {
			return this.models
		}
		this.modelsLoading = true
		try {
			const rows = await this.#fetchModels(provider, force)
			this.models = rows
			this.modelsFor = provider
			this.modelsError = null
			return rows
		} catch (e) {
			this.#loadedProviders.delete(provider)
			this.models = []
			this.modelsFor = provider
			this.modelsError = errorMessage(e)
			throw e
		} finally {
			this.modelsLoading = false
		}
	}

	loadModelsFor = async (providers: string[], force = false) => {
		const ids = providers.filter(Boolean)
		if (ids.length === 0) return
		const missing = force
			? ids
			: ids.filter((p) => !this.#loadedProviders.has(p))
		if (missing.length === 0) return
		this.modelsLoading = true
		this.modelsError = null
		try {
			await Promise.all(
				ids.map(async (p) => {
					try {
						const rows = await this.#fetchModels(p, force)
						if (this.modelsFor === p || !this.modelsFor) {
							this.models = rows
							this.modelsFor = p
						}
					} catch (e) {
						this.#loadedProviders.delete(p)
						this.modelsByProvider = {
							...this.modelsByProvider,
							[p]: [],
						}
						if (p === 'ollama') {
							this.modelsError = errorMessage(e)
						}
					}
				}),
			)
		} finally {
			this.modelsLoading = false
		}
	}

	/**
	 * Hide the chat now and delete it on the backend once the Undo window
	 * closes. Returns the undo. `onError` reports a failed backend delete.
	 */
	removeChat = (
		id: string,
		onError: (message: string) => void,
	): (() => void) => {
		const index = this.chats.findIndex((c) => String(c.id) === String(id))
		const chat = this.chats[index]
		const connectionId = this.connectionId
		const wasActive = this.chatId === id
		const messages = wasActive ? this.messages : []

		this.#deleteFailed = onError
		this.#deletes.schedule(id)
		this.chats = this.chats.filter((c) => String(c.id) !== String(id))
		if (wasActive) {
			this.chatId = null
			this.messages = []
			this.#messagesFor = null
		}

		return () => {
			if (!this.#deletes.cancel(id)) return
			if (!chat || this.connectionId !== connectionId) return
			this.chats = restoreAt(this.chats, chat, index)
			if (wasActive && !this.chatId) {
				this.chatId = id
				this.messages = messages
				this.#messagesFor = id
			}
		}
	}

	/** Run pending deletes now, e.g. before the page unloads. */
	flushDeletes = () => this.#deletes.flush()

	#commitDelete = async (id: string) => {
		try {
			await deleteAgentChat(id)
		} catch (e) {
			// Still on disk: let the next load show it again.
			this.#chatsFor = null
			this.#deleteFailed?.(errorMessage(e))
		}
	}

	/** A new run for `connectionId`. Returns its key for `onRunEvent`. */
	beginRun = (connectionId: string): string => {
		const key = crypto.randomUUID()
		this.live = newLiveRun(key, connectionId)
		this.exhausted = null
		this.runError = null
		return key
	}

	/** Apply one streamed event. Events of an older or dropped run are ignored. */
	onRunEvent = (key: string, event: AgentEvent) => {
		const run = this.live
		if (!run || run.key !== key) return
		const next = applyAgentEvent(run, event)
		this.live = next
		if (isFinished(next) && next.chatId) void this.#settle(next)
	}

	/** `send_agent_message` returned: the run belongs to `chatId`. */
	runStarted = (key: string, chatId: string) => {
		const run = this.live
		if (!run || run.key !== key) return
		const next = { ...run, chatId }
		this.live = next
		if (run.stopRequested) void this.#cancel(chatId)
		if (isFinished(next)) void this.#settle(next)
	}

	/** The send failed before a run started. */
	runFailedToStart = (key: string) => {
		if (this.live?.key === key) this.live = null
	}

	/** Stop: cancels the provider call on the backend. */
	stopRun = () => {
		const run = this.live
		if (!run || isFinished(run)) return
		if (run.chatId) void this.#cancel(run.chatId)
		else this.live = { ...run, stopRequested: true }
	}

	#cancel = async (chatId: string) => {
		try {
			await cancelAgentMessage(chatId)
		} catch {
			/* the run ends on its own; its terminal event still arrives */
		}
	}

	#settling: string | null = null

	/** Swap the live reply for the saved rows (tool cards + answer). */
	#settle = async (run: LiveRun) => {
		if (this.#settling === run.key || !run.chatId) return
		this.#settling = run.key
		if (run.exhausted !== null) {
			this.exhausted = { chatId: run.chatId, turns: run.exhausted }
		}
		if (run.done) this.lastTokens = run.done.tokens
		if (run.error && run.error.kind !== 'cancelled') {
			this.runError = run.error.message
		}
		if (this.connectionId === run.connectionId && this.chatId === run.chatId) {
			try {
				await this.loadMessages(run.chatId, true)
			} catch {
				/* the next open reloads them */
			}
		}
		if (this.live?.key === run.key) this.live = null
		this.#settling = null
	}

	resetForConnection = (connectionId: string) => {
		const next = chatAfterConnectionChange(
			{
				connectionId: this.connectionId,
				chatId: this.chatId,
				chats: this.chats,
				messages: this.messages,
			},
			connectionId,
		)
		if (next.connectionId === this.connectionId) return
		this.connectionId = next.connectionId
		this.#chatsFor = null
		this.#messagesFor = null
		this.chats = []
		this.messages = []
		this.chatId = null
		this.draft = ''
		// A run of the old connection keeps going on the backend, but its
		// events no longer reach this view and its chat id is not reused.
		this.live = null
		this.exhausted = null
		this.lastTokens = null
		this.runError = null
	}
}

export const agentChat = new AgentChatStore()

if (typeof window !== 'undefined') {
	window.addEventListener('pagehide', () => void agentChat.flushDeletes())
}
