/** Agent conversation cache. Survives query-tab remounts and pane close. */

import {
	deleteAgentChat,
	listAgentChats,
	listAgentMessages,
	listAgentModels,
} from './api'
import type { AgentChat, AgentMessage, AgentModelInfo } from './types'

class AgentChatStore {
	connectionId = $state<string | null>(null)
	chats = $state<AgentChat[]>([])
	messages = $state<AgentMessage[]>([])
	chatId = $state<string | null>(null)
	provider = $state('')
	model = $state('')
	models = $state<AgentModelInfo[]>([])
	modelsLoading = $state(false)
	modelsError = $state<string | null>(null)
	modelsFor = $state<string | null>(null)
	draft = $state('')
	#chatsFor: string | null = null
	#messagesFor: string | null = null

	loadChats = async (connectionId: string, force = false) => {
		if (!force && this.#chatsFor === connectionId) return this.chats
		const rows = await listAgentChats(connectionId)
		this.connectionId = connectionId
		this.#chatsFor = connectionId
		this.chats = rows
		return rows
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

	loadModels = async (provider: string, force = false) => {
		if (!force && this.modelsFor === provider && this.models.length > 0) {
			return this.models
		}
		this.modelsLoading = true
		try {
			const rows = await listAgentModels(provider)
			this.models = rows
			this.modelsFor = provider
			this.modelsError = null
			return rows
		} catch (e) {
			this.models = []
			this.modelsFor = provider
			this.modelsError = e instanceof Error ? e.message : String(e)
			throw e
		} finally {
			this.modelsLoading = false
		}
	}

	removeChat = async (id: string) => {
		await deleteAgentChat(id)
		this.chats = this.chats.filter((c) => String(c.id) !== String(id))
		if (this.chatId === id) {
			this.chatId = null
			this.messages = []
			this.#messagesFor = null
		}
	}

	resetForConnection = (connectionId: string) => {
		if (this.connectionId === connectionId) return
		this.connectionId = connectionId
		this.#chatsFor = null
		this.#messagesFor = null
		this.chats = []
		this.messages = []
		this.chatId = null
		this.draft = ''
	}
}

export const agentChat = new AgentChatStore()
