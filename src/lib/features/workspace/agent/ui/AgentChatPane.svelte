<script lang="ts">
	import { commands } from '#core/commands'
	import { errorMessage } from '#core/ipc'
	import { Alert, Button, Icon, IconButton, Resizable, Select } from '#core/ui'
	import { cn } from '#core/util'
	import { isMacPlatform, labelForKey } from '#features/shortcuts'
	import { showToast } from '#features/toast'
	import { catalogStore } from '#features/workspace/catalog'
	import { workspaceSession } from '#features/workspace/session'
	import {
		AGENT_CHAT_PANEL_WIDTH_DEFAULT,
		AGENT_CHAT_PANEL_WIDTH_MAX,
		AGENT_CHAT_PANEL_WIDTH_MIN,
		appPrefs,
		cmd,
	} from '#lib/app'
	import { onMount, untrack } from 'svelte'
	import { sendAgentMessage, updateAgentChat } from '../api'
	import { agentChat } from '../chat.svelte'
	import { configuredProviders, resolveAgentProvider } from '../configured'
	import { formatTokens } from '../live-run'
	import { resolveMentions, type MentionTable } from '../mentions'
	import { aiSettings } from '../settings.svelte'
	import type { AgentFixRequest, AgentMessage } from '../types'
	import AgentChatNotice from './AgentChatNotice.svelte'
	import AgentComposer from './AgentComposer.svelte'
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

	const settings = $derived(aiSettings.view)
	/** A run is streaming. It lives in the store, so it survives a remount. */
	const sending = $derived(agentChat.live !== null)
	let elapsedSec = $state(0)
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
	/** Tables of this connection (and database) for `@` references. */
	const mentionTables = $derived.by((): MentionTable[] => {
		const dbs = catalogStore.databases(connectionId)
		const scoped = database ? dbs.filter((d) => d.name === database) : dbs
		return scoped.flatMap((d) =>
			d.tables.map((t) => ({
				database: d.name,
				schema: t.schema,
				name: t.name,
			})),
		)
	})
	const canContinue = $derived(
		!sending &&
			agentChat.exhausted !== null &&
			agentChat.exhausted.chatId === agentChat.chatId,
	)
	const tokensLine = $derived.by(() => {
		const t = agentChat.lastTokens
		if (!t || (t.input === 0 && t.output === 0)) return ''
		const cached = t.cachedInput > 0 ? `, ${formatTokens(t.cachedInput)} cached` : ''
		return `${formatTokens(t.input)} in · ${formatTokens(t.output)} out${cached}`
	})
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

	snapProvider()

	async function loadSettings() {
		await aiSettings.load()
		snapProvider()
	}

	async function loadModels() {
		if (sending) return
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

	async function hydrateMessages(id: string | null, force = false) {
		if (!id) return
		await agentChat.loadMessages(id, force)
		queueMicrotask(() => {
			listEl?.scrollTo({ top: listEl.scrollHeight })
		})
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
			error = errorMessage(e)
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

	function scrollToEnd() {
		queueMicrotask(() => {
			listEl?.scrollTo({ top: listEl.scrollHeight })
		})
	}

	/** Follow the stream only while the reader is already at the bottom. */
	function followStream() {
		const el = listEl
		if (!el) return
		if (el.scrollHeight - el.scrollTop - el.clientHeight < 80) scrollToEnd()
	}

	/**
	 * Start a turn. The reply streams in through events; the saved rows
	 * replace it when the run ends. No polling.
	 */
	async function send(preset?: string, fix?: AgentFixRequest) {
		const text = (preset ?? agentChat.draft).trim()
		if ((!text && !fix) || sending) return
		snapProvider()
		if (
			!configuredProviders(aiSettings.view).some(
				(p) => p.id === agentChat.provider,
			)
		) {
			error = 'Add a provider key in Settings.'
			return
		}
		error = null
		const forConnection = connectionId
		const key = agentChat.beginRun(forConnection)
		elapsedSec = 0
		if (preset === undefined) agentChat.draft = ''
		const optimisticId = `local-${key}`
		const optimistic: AgentMessage = {
			id: optimisticId,
			chatId: agentChat.chatId ?? '',
			role: 'user',
			content: text || 'Fix this query.',
			toolName: null,
			draftBody: null,
			createdAt: new Date().toISOString(),
		}
		agentChat.messages = [...agentChat.messages, optimistic]
		scrollToEnd()
		try {
			const editorQuery = (workspaceSession.queryDrafts[tabId] ?? '').slice(
				0,
				40_000,
			)
			const started = await sendAgentMessage(
				{
					connectionId,
					connectionName,
					engine,
					database,
					chatId: agentChat.chatId,
					provider: agentChat.provider,
					model: agentChat.model,
					text,
					editorQuery,
					mentions: resolveMentions(text, mentionTables),
					fix,
				},
				(event) => {
					agentChat.onRunEvent(key, event)
					followStream()
				},
			)
			// Switched connection meanwhile: the old run is not this view's.
			if (agentChat.connectionId !== forConnection) return
			agentChat.chatId = started.chat.id
			agentChat.messages = agentChat.messages.map((m) =>
				m.id === optimisticId ? started.user : m,
			)
			agentChat.runStarted(key, started.chat.id)
			void loadChats(true)
		} catch (e) {
			agentChat.runFailedToStart(key)
			agentChat.messages = agentChat.messages.filter(
				(m) => m.id !== optimisticId,
			)
			if (preset === undefined && !agentChat.draft) agentChat.draft = text
			error = errorMessage(e)
		}
	}

	function stop() {
		agentChat.stopRun()
	}

	function continueRun() {
		void send('Continue where you stopped.')
	}

	function applyDraft(sql: string) {
		workspaceSession.setQueryDraft(tabId, sql)
	}

	function useStarter(text: string) {
		if (sending) return
		agentChat.draft = text
		queueMicrotask(() => {
			document.getElementById('agent-composer')?.focus()
		})
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
			void openConnectionChats().catch((e: unknown) => {
				error = errorMessage(e)
			})
		})
	})

	$effect(() => {
		void readyProviders
		untrack(() => void loadModels())
	})

	$effect(() => {
		void settings
		void readyProviders
		if (!settings) return
		untrack(() => snapProvider())
	})

	$effect(() => {
		const queued = workspaceSession.pendingAgentPrompt
		if (!queued || sending || !settings || !providerReady) return
		untrack(() => {
			const text = workspaceSession.takePendingAgentPrompt()
			if (text) void send(text)
		})
	})

	// "Fix with AI" from the error banner.
	$effect(() => {
		const queued = workspaceSession.pendingAgentFix
		if (!queued || sending || !settings || !providerReady) return
		untrack(() => {
			const fix = workspaceSession.takePendingAgentFix()
			if (fix) void send('Fix this query.', fix)
		})
	})

	// The elapsed label while a run streams. The interval dies with the run
	// and with the pane (effect cleanup on unmount).
	$effect(() => {
		if (!sending) {
			elapsedSec = 0
			return
		}
		const tick = setInterval(() => {
			elapsedSec += 1
		}, 1000)
		return () => clearInterval(tick)
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
			try {
				agentChat.resetForConnection(connectionId)
				await loadSettings()
				await openConnectionChats()
				void loadModels()
			} catch (e) {
				error = errorMessage(e)
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
				<p class="m-0 truncate text-body font-semibold leading-tight text-fg">
					Agent
				</p>
				<p
					class="m-0 mt-0.5 truncate text-caption leading-tight text-fg-muted"
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
			{#if settings && !settings.enabled}
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
					live={agentChat.live}
					{workingLabel}
					onapply={applyDraft}
					onstop={stop}
				/>
			{/if}
		</div>
	</div>

	{#if agentChat.modelsError && agentChat.provider === 'ollama'}
		<div class="shrink-0 bg-app px-2 pb-1">
			<Alert tone="danger">{agentChat.modelsError}</Alert>
		</div>
	{/if}

	{#if error || agentChat.runError}
		<div class="shrink-0 bg-app px-2 pb-1">
			<Alert tone="danger">{error ?? agentChat.runError}</Alert>
		</div>
	{/if}

	{#if canContinue}
		<div
			class="flex shrink-0 items-center gap-2 bg-app px-3 pb-1"
			aria-live="polite"
		>
			<p class="m-0 min-w-0 flex-1 text-caption text-fg-secondary">
				Stopped after {agentChat.exhausted?.turns} steps without an answer.
			</p>
			<Button size="sm" variant="secondary" onclick={continueRun}>
				Continue
			</Button>
		</div>
	{/if}

	<AgentComposer
		bind:value={agentChat.draft}
		{sending}
		{canSend}
		{sendChord}
		tables={mentionTables}
		combo={{
			options: comboOptions,
			value: comboValue,
			placeholder: comboPlaceholder,
			shortLabel: comboShortLabel,
			disabled:
				!hasProvider || (agentChat.modelsLoading && comboOptions.length === 0),
		}}
		oncombo={applyCombo}
		onsend={() => void send()}
		onstop={stop}
	>
		{#snippet footer()}
			{#if tokensLine && !sending}
				<span
					class="truncate text-caption text-fg-muted tabular-nums"
					title="Tokens used by the last reply"
				>
					{tokensLine}
				</span>
			{/if}
		{/snippet}
	</AgentComposer>
</aside>

<style>
	.agent-thread {
		scrollbar-width: thin;
		scrollbar-color: var(--border-strong) transparent;
	}
</style>
