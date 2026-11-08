<script lang="ts">
	import { commands } from '#core/commands'
	import { isTauri } from '#core/ipc'
	import {
		Alert,
		Icon,
		IconButton,
		Resizable,
		Select,
		Textarea,
	} from '#core/ui'
	import { cn } from '#core/util'
	import { isMacPlatform, labelForKey } from '#features/shortcuts'
	import { showToast } from '#features/toast'
	import { workspaceSession } from '#features/workspace/session'
	import {
		AGENT_CHAT_PANEL_WIDTH_DEFAULT,
		AGENT_CHAT_PANEL_WIDTH_MAX,
		AGENT_CHAT_PANEL_WIDTH_MIN,
		appPrefs,
		cmd,
	} from '#lib/app'
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
	let sendGen = $state(0)
	let error = $state<string | null>(null)
	let listEl = $state<HTMLDivElement | null>(null)
	let dragging = $state(false)
	let loaf = $state(false)
	let seenConnection = $state<string | null>(null)
	const mac = isMacPlatform()
	const sendChord = $derived(
		`${labelForKey('mod', mac)}+${labelForKey('enter', mac)}`,
	)

	/** Through the registry: the settings feature already depends on this one. */
	function openAiSettings() {
		commands.run(cmd.settings, { section: 'ai' })
	}

	function comboKey(provider: string, model: string): string {
		return `${provider}::${model}`
	}

	function parseCombo(raw: string): { provider: string; model: string } | null {
		const i = raw.indexOf('::')
		if (i <= 0) return null
		const provider = raw.slice(0, i)
		const model = raw.slice(i + 2)
		if (!provider || !model) return null
		return { provider, model }
	}

	function applyCombo(raw: string) {
		const next = parseCombo(raw)
		if (!next) return
		agentChat.provider = next.provider
		agentChat.model = next.model
		if (agentChat.chatId)
			void updateAgentChat({
				id: agentChat.chatId,
				provider: next.provider,
				model: next.model,
			})
	}

	const chatOptions = $derived([
		{ value: '', label: 'New chat' },
		...agentChat.chats.map((c) => ({ value: String(c.id), label: c.title })),
	])
	const readyProviders = $derived(configuredProviders(settings))
	const hasProvider = $derived(readyProviders.length > 0)
	const providerReady = $derived(
		readyProviders.some((p) => p.id === agentChat.provider),
	)
	const comboOptions = $derived.by(() => {
		const out: { value: string; label: string }[] = []
		for (const p of readyProviders) {
			const models = agentChat.modelsByProvider[p.id] ?? []
			const list =
				models.length > 0
					? models
					: p.id !== 'ollama' && p.defaultModel
						? [{ id: p.defaultModel, label: p.defaultModel }]
						: []
			for (const m of list) {
				out.push({
					value: comboKey(p.id, m.id),
					label: `${p.label} · ${m.label}`,
				})
			}
		}
		return out
	})
	const comboValue = $derived(
		providerReady && agentChat.model
			? comboKey(agentChat.provider, agentChat.model)
			: '',
	)
	const comboPlaceholder = $derived(
		!hasProvider
			? 'No provider'
			: agentChat.modelsLoading && comboOptions.length === 0
				? 'Loading models…'
				: agentChat.provider === 'ollama' && comboOptions.length === 0
					? 'No local models'
					: 'Select model',
	)
	const comboShortLabel = $derived.by(() => {
		const opt = comboOptions.find((o) => o.value === comboValue)
		if (!opt) return comboPlaceholder
		if (readyProviders.length > 1) return opt.label
		const sep = opt.label.indexOf(' · ')
		return sep >= 0 ? opt.label.slice(sep + 3) : opt.label
	})
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
		if (elapsedSec < 8) return `Working… ${elapsedSec}s`
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
		if (!desktop || sending) return
		const ids = readyProviders.map((p) => p.id)
		if (ids.length === 0) return
		try {
			await agentChat.loadModelsFor(ids)
			const ollamaModels = agentChat.modelsByProvider.ollama ?? []
			if (ids.includes('ollama') && ollamaModels.length === 0) {
				agentChat.modelsError =
					'Ollama has no local models. Pull one with `ollama pull <name>`.'
			}
			const models = agentChat.modelsByProvider[agentChat.provider] ?? []
			if (!models.some((m) => m.id === agentChat.model)) {
				const fallback =
					agentChat.provider === 'ollama'
						? ''
						: (readyProviders.find((p) => p.id === agentChat.provider)
								?.defaultModel ?? '')
				agentChat.model = models[0]?.id ?? fallback
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
		const forConnection = connectionId
		await agentChat.loadChats(forConnection, force)
		if (connectionId !== forConnection) return
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

	/** Delete at once; the toast's Undo brings it back. */
	function deleteChat() {
		const id = agentChat.chatId
		if (sending || !id) return
		const title = agentChat.chats.find((c) => String(c.id) === String(id))?.title
		error = null
		const undo = agentChat.removeChat(id, (message) =>
			showToast('danger', `Could not delete chat: ${message}`),
		)
		showToast('info', title ? `Deleted “${title}”.` : 'Chat deleted.', {
			action: { label: 'Undo', run: undo },
		})
	}

	async function send(preset?: string) {
		const text = (preset ?? agentChat.draft).trim()
		if (!text || sending) return
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
				agentChat.setChats(await listAgentChats(connectionId))
				id = agentChat.chats[0]?.id ?? null
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
		const id = connectionId
		if (seenConnection === null) {
			seenConnection = id
			return
		}
		if (seenConnection === id) return
		seenConnection = id
		untrack(() => {
			agentChat.resetForConnection(id)
			if (!desktop) return
			void openConnectionChats().catch((e: unknown) => {
				error = e instanceof Error ? e.message : String(e)
			})
		})
	})

	$effect(() => {
		void readyProviders
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

	async function openConnectionChats() {
		const forConnection = connectionId
		await loadChats()
		if (connectionId !== forConnection) return
		if (agentChat.chatId) {
			const current = agentChat.chats.find(
				(c) => String(c.id) === String(agentChat.chatId),
			)
			if (current) snapProvider(current)
			await hydrateMessages(agentChat.chatId)
			return
		}
		const first = agentChat.chats[0]
		if (!first) return
		agentChat.chatId = String(first.id)
		snapProvider(first)
		await hydrateMessages(agentChat.chatId)
	}

	onMount(() => {
		const hour = new Date().getHours()
		if (hour >= 20 || hour < 6) loaf = true
		void (async () => {
			if (!desktop) return
			try {
				agentChat.resetForConnection(connectionId)
				await loadSettings()
				await openConnectionChats()
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
					onclick={deleteChat}
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
					onaction={openAiSettings}
				/>
			{:else if !hasProvider}
				<AgentChatNotice
					icon="settings"
					title="No provider set up"
					description="Add an API key or Ollama in Settings. Only providers you add show up here."
					actionLabel="Add a provider"
					onaction={openAiSettings}
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
				chrome={false}
				class="min-h-16 rounded-none px-3 pt-2.5 pb-1"
				placeholder="Ask for a query…"
				bind:value={agentChat.draft}
				disabled={sending || !desktop}
				enterkeyhint="send"
				onkeydown={onComposerKey}
			/>
			<div class="flex items-center justify-between gap-2 px-2 pt-0.5 pb-1.5">
				<Select
					variant="quiet"
					inputSize="sm"
					options={comboOptions}
					value={comboValue}
					placeholder={comboPlaceholder}
					triggerLabel={comboShortLabel}
					ariaLabel="Model"
					disabled={!hasProvider ||
						(agentChat.modelsLoading && comboOptions.length === 0)}
					onchange={applyCombo}
				>
					{#snippet leading()}
						<Icon name="sparkle" class="size-3.5" />
					{/snippet}
				</Select>
				<IconButton
					type="submit"
					label={sending ? 'Sending' : `Send (${sendChord})`}
					variant="accent"
					size="sm"
					disabled={!canSend}
				>
					<Icon name="send" />
				</IconButton>
			</div>
		</div>
	</form>
</aside>

<style>
	.agent-thread {
		scrollbar-width: thin;
		scrollbar-color: var(--border-strong) transparent;
	}
</style>
