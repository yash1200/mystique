<script lang="ts">
	import { isTauri } from '$core/ipc'
	import {
		Alert,
		Icon,
		IconButton,
		Resizable,
		Select,
		Textarea,
	} from '$core/ui'
	import { cn, randomLoadingPun } from '$core/util'
	import { appSettingsUi } from '$features/settings'
	import { isMacPlatform, labelForKey } from '$features/shortcuts'
	import { workspaceSession } from '$features/workspace/shell'
	import {
		AGENT_CHAT_PANEL_WIDTH_DEFAULT,
		AGENT_CHAT_PANEL_WIDTH_MAX,
		AGENT_CHAT_PANEL_WIDTH_MIN,
		appPrefs,
	} from '$lib/app'
	import { onMount, untrack } from 'svelte'
	import {
		listAgentChats,
		listAgentMessages,
		sendAgentMessage,
		updateAgentChat,
	} from '../api'
	import { agentChat } from '../chat.svelte'
	import { configuredProviders, resolveAgentProvider } from '../configured'
	import { aiSettings } from '../settings.svelte'
	import type { AgentMessage } from '../types'
	import AgentChatNotice from './AgentChatNotice.svelte'
	import AgentChatThread from './AgentChatThread.svelte'
	import AgentChatWelcome from './AgentChatWelcome.svelte'
	import DeleteChatModal from './DeleteChatModal.svelte'

	interface Props {
		tabId: string
		connectionId: string
		connectionName: string
		engine: string
		database?: string | null
	}

	let {
		tabId,
		connectionId,
		connectionName,
		engine,
		database = null,
	}: Props = $props()

	const desktop = isTauri()

	const settings = $derived(aiSettings.view)
	let sending = $state(false)
	let elapsedSec = $state(0)
	let sendPun = $state(randomLoadingPun())
	let sendGen = $state(0)
	let error = $state<string | null>(null)
	let listEl = $state<HTMLDivElement | null>(null)
	let dragging = $state(false)
	let loaf = $state(false)
	const mac = isMacPlatform()
	const sendChord = $derived(
		`${labelForKey('mod', mac)}+${labelForKey('enter', mac)}`,
	)

	const chatOptions = $derived([
		{ value: '', label: 'New chat' },
		...agentChat.chats.map((c) => ({ value: String(c.id), label: c.title })),
	])
	const readyProviders = $derived(configuredProviders(settings))
	const providerOptions = $derived(
		readyProviders.map((p) => ({
			value: p.id,
			label: p.label,
		})),
	)
	const hasProvider = $derived(readyProviders.length > 0)
	const modelOptions = $derived(
		agentChat.models.length
			? agentChat.models.map((m) => ({ value: m.id, label: m.label }))
			: agentChat.model
				? [{ value: agentChat.model, label: agentChat.model }]
				: [],
	)
	const modelPlaceholder = $derived(
		agentChat.modelsLoading
			? 'Loading models…'
			: agentChat.provider === 'ollama'
				? 'No local models'
				: 'Select model',
	)
	const providerReady = $derived(
		readyProviders.some((p) => p.id === agentChat.provider),
	)
	const canSend = $derived(
		!sending &&
			Boolean(agentChat.draft.trim()) &&
			desktop &&
			providerReady &&
			!agentChat.modelsLoading &&
			(agentChat.provider !== 'ollama' || Boolean(agentChat.model.trim())),
	)
	const workingLabel = $derived.by(() => {
		if (!sending) return ''
		const who = agentChat.provider === 'ollama' ? 'Ollama' : 'the model'
		if (elapsedSec < 8) return `${sendPun} ${elapsedSec}s`
		if (elapsedSec < 25) return `Waiting on ${who}… ${elapsedSec}s`
		return `Still waiting on ${who}… ${elapsedSec}s`
	})
	const threadEmpty = $derived(agentChat.messages.length === 0 && !sending)
	const connectionLine = $derived(
		database ? `${connectionName} / ${database}` : connectionName,
	)

	function snapProvider(
		preferred?: { provider: string; model: string } | null,
	) {
		if (!aiSettings.view) return
		const next = resolveAgentProvider(
			aiSettings.view,
			{ provider: agentChat.provider, model: agentChat.model },
			preferred,
		)
		agentChat.provider = next.provider
		agentChat.model = next.model
	}

	if (desktop) snapProvider()

	async function loadSettings() {
		if (!desktop) return
		await aiSettings.load()
		snapProvider()
	}

	async function loadModels() {
		if (!desktop || sending || !agentChat.provider) return
		try {
			await agentChat.loadModels(agentChat.provider)
			if (agentChat.provider === 'ollama' && agentChat.models.length === 0) {
				agentChat.modelsError =
					'Ollama has no local models. Pull one with `ollama pull <name>`.'
			}
			if (!agentChat.models.some((m) => m.id === agentChat.model)) {
				const fallback =
					agentChat.provider === 'ollama'
						? ''
						: (settings?.providers.find((p) => p.id === agentChat.provider)
								?.defaultModel ?? '')
				agentChat.model = agentChat.models[0]?.id ?? fallback
			}
		} catch {
			if (
				agentChat.provider === 'ollama' &&
				isHardcodedOllamaModel(agentChat.model)
			) {
				agentChat.model = ''
			}
		}
	}

	function isHardcodedOllamaModel(id: string): boolean {
		return id === 'llama3.2' || id === 'qwen2.5' || id === 'mistral'
	}

	async function loadChats(force = false) {
		if (!desktop) return
		await agentChat.loadChats(connectionId, force)
		if (
			agentChat.chatId &&
			!agentChat.chats.some((c) => String(c.id) === String(agentChat.chatId))
		) {
			agentChat.chatId = agentChat.chats[0]
				? String(agentChat.chats[0].id)
				: null
		}
	}

	async function loadMessages() {
		if (sending) return
		await hydrateMessages(agentChat.chatId)
	}

	async function hydrateMessages(id: string | null, force = false) {
		if (!desktop || !id) return
		await agentChat.loadMessages(id, force)
		queueMicrotask(() => {
			listEl?.scrollTo({ top: listEl.scrollHeight })
		})
	}

	function hasUnseenAssistant(
		remote: AgentMessage[],
		knownIds: Set<string>,
	): boolean {
		return remote.some((m) => m.role === 'assistant' && !knownIds.has(m.id))
	}

	async function onPickChat(id: string) {
		if (sending) return
		const next = id ? String(id) : null
		agentChat.chatId = next
		if (!next) {
			agentChat.messages = []
			return
		}
		const c = agentChat.chats.find((x) => String(x.id) === next)
		if (c) {
			snapProvider(c)
			void loadModels()
		}
		try {
			error = null
			await hydrateMessages(next)
		} catch (e) {
			error = e instanceof Error ? e.message : String(e)
		}
	}

	async function newChat() {
		if (sending) return
		agentChat.chatId = null
		agentChat.messages = []
		agentChat.draft = ''
		error = null
	}

	let deleteOpen = $state(false)
	let deleteBusy = $state(false)

	const deleteTitle = $derived(
		agentChat.chats.find((c) => String(c.id) === String(agentChat.chatId))
			?.title ?? 'this chat',
	)

	function askDelete() {
		if (sending || !agentChat.chatId) return
		deleteOpen = true
	}

	function cancelDelete() {
		if (deleteBusy) return
		deleteOpen = false
	}

	async function confirmDelete() {
		if (deleteBusy || sending || !agentChat.chatId) return
		deleteBusy = true
		try {
			error = null
			await agentChat.removeChat(agentChat.chatId)
			deleteOpen = false
		} catch (e) {
			error = e instanceof Error ? e.message : String(e)
		} finally {
			deleteBusy = false
		}
	}

	async function send(preset?: string) {
		const text = (preset ?? agentChat.draft).trim()
		if (!text || sending) return
		sendPun = randomLoadingPun()
		snapProvider()
		if (
			!configuredProviders(aiSettings.view).some(
				(p) => p.id === agentChat.provider,
			)
		) {
			error = 'Add a provider key in Settings.'
			return
		}
		const gen = ++sendGen
		sending = true
		elapsedSec = 0
		const sentAt = new Date().toISOString()
		const knownAssistantIds = new Set(
			agentChat.messages.filter((m) => m.role === 'assistant').map((m) => m.id),
		)
		const tick = setInterval(() => {
			if (gen === sendGen) elapsedSec += 1
		}, 1000)
		const poll = setInterval(() => {
			void reconcileInFlight(gen, knownAssistantIds)
		}, 800)
		error = null
		if (!preset) agentChat.draft = ''
		const optimisticId = `local-${crypto.randomUUID()}`
		const optimistic: AgentMessage = {
			id: optimisticId,
			chatId: agentChat.chatId ?? '',
			role: 'user',
			content: text,
			createdAt: sentAt,
		}
		agentChat.messages = [...agentChat.messages, optimistic]
		queueMicrotask(() => {
			listEl?.scrollTo({ top: listEl.scrollHeight })
		})
		try {
			const editorQuery = (workspaceSession.queryDrafts[tabId] ?? '').slice(
				0,
				40_000,
			)
			const result = await sendAgentMessage({
				connectionId,
				connectionName,
				engine,
				database,
				chatId: agentChat.chatId,
				provider: agentChat.provider,
				model: agentChat.model,
				text,
				editorQuery,
			})
			if (gen !== sendGen) return
			agentChat.chatId = result.chat.id
			try {
				await hydrateMessages(result.chat.id, true)
				if (!agentChat.messages.some((m) => m.id === result.assistant.id)) {
					agentChat.messages = [
						...agentChat.messages.filter(
							(m) => m.id !== optimisticId && m.id !== result.user.id,
						),
						result.user,
						result.assistant,
					]
				}
			} catch {
				agentChat.messages = [
					...agentChat.messages.filter(
						(m) => m.id !== optimisticId && m.id !== result.user.id,
					),
					result.user,
					result.assistant,
				]
			}
			void loadChats(true)
		} catch (e) {
			if (gen !== sendGen) return
			error = e instanceof Error ? e.message : String(e)
			await recoverFromDb(knownAssistantIds)
		} finally {
			clearInterval(tick)
			clearInterval(poll)
			if (gen === sendGen) {
				sending = false
				elapsedSec = 0
			}
		}
	}

	async function reconcileInFlight(gen: number, knownIds: Set<string>) {
		if (gen !== sendGen || !sending) return
		try {
			let id = agentChat.chatId
			if (!id) {
				const listed = await listAgentChats(connectionId)
				agentChat.chats = listed
				id = listed[0]?.id ?? null
			}
			if (!id || gen !== sendGen) return
			const remote = await listAgentMessages(id)
			if (gen !== sendGen || !sending) return
			if (!hasUnseenAssistant(remote, knownIds)) return
			agentChat.chatId = id
			agentChat.messages = remote
			sending = false
			elapsedSec = 0
			sendGen += 1
			queueMicrotask(() => {
				listEl?.scrollTo({ top: listEl.scrollHeight })
			})
		} catch {
			/* next tick retries */
		}
	}

	async function recoverFromDb(knownIds: Set<string>) {
		try {
			if (!agentChat.chatId) {
				await loadChats()
			}
			const id = agentChat.chatId ?? agentChat.chats[0]?.id ?? null
			if (!id) return
			const remote = await listAgentMessages(id)
			if (remote.length === 0) return
			agentChat.chatId = id
			agentChat.messages = remote
			if (hasUnseenAssistant(remote, knownIds)) {
				error = null
			}
		} catch {
			/* keep the invoke error */
		}
	}

	function stopWaiting() {
		sendGen += 1
		sending = false
		elapsedSec = 0
		void (async () => {
			if (!agentChat.chatId) await loadChats()
			const id = agentChat.chatId ?? agentChat.chats[0]?.id ?? null
			if (id) {
				agentChat.chatId = id
				await hydrateMessages(id)
				if (agentChat.messages.some((m) => m.role === 'assistant')) {
					error = null
					return
				}
			}
			error = 'Stopped waiting. Send again if you still want a reply.'
		})()
	}

	function applyDraft(sql: string) {
		workspaceSession.setQueryDraft(tabId, sql)
	}

	function useStarter(text: string) {
		if (sending || !desktop) return
		agentChat.draft = text
		queueMicrotask(() => {
			document.getElementById('agent-composer')?.focus()
		})
	}

	function onComposerKey(e: KeyboardEvent) {
		if (
			e.key === 'Enter' &&
			(e.metaKey || e.ctrlKey) &&
			!e.shiftKey &&
			!e.altKey
		) {
			e.preventDefault()
			e.stopPropagation()
			void send()
		}
	}

	$effect(() => {
		void agentChat.provider
		if (desktop) untrack(() => void loadModels())
	})

	$effect(() => {
		void settings
		void readyProviders
		if (!desktop || !settings) return
		untrack(() => snapProvider())
	})

	$effect(() => {
		const queued = workspaceSession.pendingAgentPrompt
		if (!queued || sending || !desktop || !settings || !providerReady) return
		untrack(() => {
			const text = workspaceSession.takePendingAgentPrompt()
			if (text) void send(text)
		})
	})

	onMount(() => {
		const hour = new Date().getHours()
		if (hour >= 20 || hour < 6) loaf = true
		void (async () => {
			if (!desktop) return
			try {
				agentChat.resetForConnection(connectionId)
				await loadSettings()
				await loadChats()
				if (agentChat.chatId) {
					const current = agentChat.chats.find(
						(c) => String(c.id) === String(agentChat.chatId),
					)
					if (current) snapProvider(current)
					await hydrateMessages(agentChat.chatId)
				} else if (agentChat.chats[0]) {
					agentChat.chatId = String(agentChat.chats[0].id)
					snapProvider(agentChat.chats[0])
					await hydrateMessages(agentChat.chatId)
				}
				void loadModels()
			} catch (e) {
				error = e instanceof Error ? e.message : String(e)
			}
		})()
	})
</script>

<aside
	class={cn(
		'relative flex h-full min-h-0 w-full min-w-0 flex-col border-l border-line bg-panel',
		dragging && 'select-none',
	)}
	aria-label="Query agent"
>
	<Resizable
		size={appPrefs.agentChatPanelWidth}
		defaultSize={AGENT_CHAT_PANEL_WIDTH_DEFAULT}
		min={AGENT_CHAT_PANEL_WIDTH_MIN}
		max={AGENT_CHAT_PANEL_WIDTH_MAX}
		edge="start"
		label="Resize agent panel"
		bind:dragging
		onresize={(n) => appPrefs.setAgentChatPanelWidth(n, false)}
		oncommit={(n) => appPrefs.setAgentChatPanelWidth(n, true)}
	/>

	<header class="shrink-0 border-b border-line">
		<div class="flex items-center gap-1.5 px-2 py-2">
			<span
				class="grid size-7 shrink-0 place-items-center rounded-lg bg-accent-muted text-accent"
				aria-hidden="true"
			>
				<Icon name="chat" />
			</span>
			<div class="min-w-0 flex-1">
				<p class="m-0 truncate text-body font-semibold leading-none text-fg">
					Agent
				</p>
				<p
					class="m-0 mt-1 truncate text-caption leading-none text-fg-muted"
					title={connectionLine}
				>
					{connectionLine}
				</p>
			</div>
			<IconButton label="New chat" variant="quiet" size="sm" onclick={newChat}>
				<Icon name="plus" />
			</IconButton>
			{#if agentChat.chatId}
				<IconButton
					label="Delete chat"
					variant="quiet"
					size="sm"
					class="hover:bg-danger/10 hover:text-danger"
					onclick={askDelete}
				>
					<Icon name="trash" />
				</IconButton>
			{/if}
			<IconButton
				label="Close agent"
				variant="quiet"
				size="sm"
				onclick={() => appPrefs.setAgentChatOpen(false)}
			>
				<Icon name="close" />
			</IconButton>
		</div>
		<div class="px-2 pb-2">
			<Select
				class="min-w-0"
				inputSize="sm"
				options={chatOptions}
				value={agentChat.chatId ?? ''}
				onchange={(v) => void onPickChat(v)}
			/>
		</div>
	</header>

	<div class="relative flex min-h-0 flex-1 flex-col bg-app">
		<div
			class="pointer-events-none absolute inset-x-0 top-0 z-1 h-4 bg-linear-to-b from-app to-transparent"
			aria-hidden="true"
		></div>
		<div
			bind:this={listEl}
			class={cn(
				'agent-thread min-h-0 flex-1 overflow-auto px-3 py-3',
				threadEmpty && 'flex flex-col',
			)}
		>
			{#if !desktop}
				<AgentChatNotice
					icon="chat"
					title="Desktop only"
					description="The agent runs in the Mystique desktop app, next to the editor."
				/>
			{:else if settings && !settings.enabled}
				<AgentChatNotice
					icon="settings"
					title="Agent is off"
					description="Turn it on and add a provider key in Settings."
					actionLabel="Open AI settings"
					onaction={() => appSettingsUi.show('ai')}
				/>
			{:else if !hasProvider}
				<AgentChatNotice
					icon="settings"
					title="No provider set up"
					description="Add an API key or Ollama in Settings. Only providers you add show up here."
					actionLabel="Add a provider"
					onaction={() => appSettingsUi.show('ai')}
				/>
			{:else if threadEmpty}
				<AgentChatWelcome {sendChord} {loaf} onpick={useStarter} />
			{:else}
				<AgentChatThread
					messages={agentChat.messages}
					{engine}
					{sending}
					{workingLabel}
					onapply={applyDraft}
					onstop={stopWaiting}
				/>
			{/if}
		</div>
	</div>

	{#if agentChat.modelsError && agentChat.provider === 'ollama'}
		<div class="shrink-0 bg-app px-2 pb-1">
			<Alert tone="danger">{agentChat.modelsError}</Alert>
		</div>
	{/if}

	{#if error}
		<div class="shrink-0 bg-app px-2 pb-1">
			<Alert tone="danger">{error}</Alert>
		</div>
	{/if}

	<form
		class="shrink-0 bg-app p-2 pt-1"
		onsubmit={(e) => {
			e.preventDefault()
			void send()
		}}
	>
		<div
			class="overflow-hidden rounded-xl border border-line bg-elevated shadow-sm focus-within:border-focus focus-within:ring-2 focus-within:ring-accent-muted"
		>
			<Textarea
				id="agent-composer"
				rows={3}
				resize="none"
				class="min-h-16 rounded-none border-0 bg-transparent px-3 pt-2.5 pb-1 focus-visible:border-transparent focus-visible:ring-0"
				placeholder="Ask for a query…"
				bind:value={agentChat.draft}
				disabled={sending || !desktop}
				enterkeyhint="send"
				onkeydown={onComposerKey}
			/>
			<div class="flex items-center gap-1 px-1.5 pt-0.5 pb-1.5">
				<Select
					class="min-w-0 flex-1"
					inputSize="sm"
					options={providerOptions}
					value={providerReady ? agentChat.provider : ''}
					placeholder="No provider"
					disabled={!hasProvider}
					onchange={(v) => {
						agentChat.provider = v
						if (agentChat.chatId)
							void updateAgentChat({
								id: agentChat.chatId,
								provider: v,
								model: agentChat.model,
							})
					}}
				/>
				<Select
					class="min-w-0 flex-1"
					inputSize="sm"
					options={modelOptions}
					value={agentChat.model}
					placeholder={modelPlaceholder}
					disabled={agentChat.modelsLoading}
					onchange={(v) => {
						agentChat.model = v
						if (agentChat.chatId)
							void updateAgentChat({
								id: agentChat.chatId,
								provider: agentChat.provider,
								model: v,
							})
					}}
				/>
				<IconButton
					type="submit"
					label={sending ? 'Sending' : `Send (${sendChord})`}
					variant="accent"
					size="sm"
					disabled={!canSend}
				>
					<Icon name="arrow-up" />
				</IconButton>
			</div>
		</div>
	</form>
</aside>

<DeleteChatModal
	open={deleteOpen}
	chatTitle={deleteTitle}
	busy={deleteBusy}
	onconfirm={() => void confirmDelete()}
	oncancel={cancelDelete}
/>

<style>
	.agent-thread {
		scrollbar-width: thin;
		scrollbar-color: var(--border-strong) transparent;
	}
</style>
