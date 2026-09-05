<script lang="ts">
	import { isTauri } from '$core/ipc'
	import {
		Alert,
		Button,
		Icon,
		IconButton,
		Resizable,
		Select,
		Textarea,
	} from '$core/ui'
	import { cn } from '$core/util'
	import { appSettingsUi } from '$features/settings'
	import { isMacPlatform, labelForKey } from '$features/shortcuts'
	import { workspaceSession } from '$features/workspace/shell'
	import { labradorRest, labradorSit } from '$illustrations'
	import {
		AGENT_CHAT_PANEL_WIDTH_DEFAULT,
		AGENT_CHAT_PANEL_WIDTH_MAX,
		AGENT_CHAT_PANEL_WIDTH_MIN,
		appPrefs,
	} from '$lib/app'
	import { onMount, untrack } from 'svelte'
	import { fade } from 'svelte/transition'
	import {
		listAgentChats,
		listAgentMessages,
		sendAgentMessage,
		updateAgentChat,
	} from '../api'
	import { agentChat } from '../chat.svelte'
	import { configuredProviders } from '../configured'
	import { inferFenceLang, isQueryLang, parseChatMarkdown } from '../markdown'
	import { aiSettings } from '../settings.svelte'
	import type { AgentMessage } from '../types'
	import ChatMarkdown from './ChatMarkdown.svelte'

	const STARTERS = [
		{
			label: 'Map the schema',
			text: 'What are the main tables and how do they relate?',
			icon: 'schema' as const,
		},
		{
			label: 'Start from the editor',
			text: 'Explain the query in the editor and suggest a cleaner version.',
			icon: 'code' as const,
		},
		{
			label: 'Draft a first query',
			text: 'Draft a readable query to explore the most important table.',
			icon: 'table' as const,
		},
	] as const

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
	const canSend = $derived(
		!sending &&
			Boolean(agentChat.draft.trim()) &&
			desktop &&
			hasProvider &&
			Boolean(agentChat.provider) &&
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
	let openTools = $state<Record<string, boolean>>({})
	const threadEmpty = $derived(agentChat.messages.length === 0 && !sending)
	const connectionLine = $derived(
		database ? `${connectionName} / ${database}` : connectionName,
	)

	function messageHasApplyableFence(content: string): boolean {
		return parseChatMarkdown(content).some(
			(block) =>
				block.type === 'code' &&
				isQueryLang(inferFenceLang(block.lang, block.value)),
		)
	}

	function toolLabel(name?: string | null): string {
		switch (name) {
			case 'get_schema_dbml':
				return 'Fetched schema'
			case 'list_objects':
				return 'Listed objects'
			case 'describe_object':
				return 'Described object'
			case 'execute_readonly_query':
				return 'Probed rows'
			case 'list_saved_queries':
				return 'Listed saved queries'
			case 'list_connections':
				return 'Listed connections'
			case 'save_query':
				return 'Saved query'
			default:
				return name?.replace(/_/g, ' ') || 'Tool'
		}
	}

	function toggleTool(id: string) {
		openTools = { ...openTools, [id]: !openTools[id] }
	}

	function looksLikeJson(text: string): boolean {
		const t = text.trim()
		return t.startsWith('{') || t.startsWith('[')
	}

	function fenceAs(lang: string, text: string): string {
		return '```' + lang + '\n' + text.replace(/\s+$/, '') + '\n```'
	}

	async function loadSettings() {
		if (!desktop) return
		const view = await aiSettings.load()
		const ready = configuredProviders(view)
		if (!ready.some((p) => p.id === agentChat.provider)) {
			const first = ready[0]
			agentChat.provider = first?.id ?? ''
			if (first) agentChat.model = first.defaultModel
		}
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
			agentChat.provider = c.provider
			agentChat.model = c.model
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

	async function removeChat() {
		if (sending || !agentChat.chatId) return
		await agentChat.removeChat(agentChat.chatId)
	}

	async function send(preset?: string) {
		const text = (preset ?? agentChat.draft).trim()
		if (!text || sending) return
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
		const queued = workspaceSession.pendingAgentPrompt
		if (!queued || sending || !desktop || !settings) return
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
					await hydrateMessages(agentChat.chatId)
				} else if (agentChat.chats[0]) {
					agentChat.chatId = String(agentChat.chats[0].id)
					agentChat.provider = agentChat.chats[0].provider
					agentChat.model = agentChat.chats[0].model
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
					onclick={() => void removeChat()}
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
				<div
					class="m-auto flex max-w-panel-sm flex-col items-center py-8 text-center"
				>
					<span
						class="grid size-10 place-items-center rounded-xl bg-elevated text-fg-muted ring-1 ring-line"
						aria-hidden="true"
					>
						<Icon name="chat" class="size-5" />
					</span>
					<p class="mt-3 mb-0 text-body font-semibold text-fg">Desktop only</p>
					<p
						class="mt-1.5 mb-0 max-w-prose-ui text-caption leading-relaxed text-fg-muted"
					>
						The agent runs in the Mystique desktop app, next to the editor.
					</p>
				</div>
			{:else if settings && !settings.enabled}
				<div
					class="m-auto flex max-w-panel-sm flex-col items-center py-8 text-center"
				>
					<span
						class="grid size-10 place-items-center rounded-xl bg-elevated text-fg-muted ring-1 ring-line"
						aria-hidden="true"
					>
						<Icon name="settings" class="size-5" />
					</span>
					<p class="mt-3 mb-0 text-body font-semibold text-fg">Agent is off</p>
					<p
						class="mt-1.5 mb-3 max-w-prose-ui text-caption leading-relaxed text-fg-muted"
					>
						Turn it on and add a provider key in Settings.
					</p>
					<Button size="sm" onclick={() => appSettingsUi.show('ai')}>
						Open AI settings
					</Button>
				</div>
			{:else if !hasProvider}
				<div
					class="m-auto flex max-w-panel-sm flex-col items-center py-8 text-center"
				>
					<span
						class="grid size-10 place-items-center rounded-xl bg-elevated text-fg-muted ring-1 ring-line"
						aria-hidden="true"
					>
						<Icon name="settings" class="size-5" />
					</span>
					<p class="mt-3 mb-0 text-body font-semibold text-fg">
						No provider set up
					</p>
					<p
						class="mt-1.5 mb-3 max-w-prose-ui text-caption leading-relaxed text-fg-muted"
					>
						Add an API key or Ollama in Settings. Only providers you add show up
						here.
					</p>
					<Button size="sm" onclick={() => appSettingsUi.show('ai')}>
						Add a provider
					</Button>
				</div>
			{:else if threadEmpty}
				<div class="flex min-h-full flex-col">
					<div class="m-auto flex w-full max-w-panel-sm flex-col py-4">
						<span
							class="grid size-10 place-items-center rounded-xl bg-accent-muted text-accent"
							aria-hidden="true"
						>
							<Icon name="chat" class="size-5" />
						</span>
						<p
							class="mt-3 mb-0 text-pretty text-body-lg font-semibold tracking-tight text-fg"
						>
							Ask in the language of the schema
						</p>
						<p class="mt-1.5 mb-0 text-caption leading-relaxed text-fg-muted">
							Drafts land in the editor. You run them. The agent can read tables
							and probe with SELECT only.
						</p>
						<ul class="mt-4 mb-0 flex list-none flex-col gap-1 p-0">
							{#each STARTERS as starter (starter.label)}
								<li>
									<button
										type="button"
										class="flex w-full items-start gap-2 rounded-lg border border-line bg-elevated px-2.5 py-2 text-left transition-[border-color,background-color,transform] duration-150 ease-out hover:border-line-strong hover:bg-hover enabled:active:scale-[0.99]"
										onclick={() => useStarter(starter.text)}
									>
										<Icon name={starter.icon} class="mt-0.5 text-accent" />
										<span class="min-w-0">
											<span class="block text-label font-medium text-fg">
												{starter.label}
											</span>
											<span class="mt-0.5 block text-caption text-fg-muted">
												{starter.text}
											</span>
										</span>
									</button>
								</li>
							{/each}
						</ul>
						<p class="mt-3 mb-0 text-caption text-fg-muted">
							{sendChord} to send
						</p>
					</div>
					<img
						src={loaf ? labradorRest : labradorSit}
						alt=""
						width={loaf ? 120 : 88}
						height={loaf ? 69 : 96}
						draggable="false"
						class={cn(
							'agent-labrador pointer-events-none mx-auto mt-auto mb-1 select-none',
							loaf ? 'w-28' : 'w-20',
						)}
					/>
				</div>
			{:else}
				<ul class="m-0 flex list-none flex-col gap-4 p-0">
					{#each agentChat.messages as msg, i (`${msg.role}-${String(msg.id)}-${i}`)}
						{#if msg.role === 'tool'}
							<li class="self-stretch">
								<button
									type="button"
									class="flex w-full items-center gap-1.5 rounded-md px-1 py-1 text-left transition-colors duration-150 ease-out hover:bg-hover"
									aria-expanded={openTools[msg.id] === true}
									onclick={() => toggleTool(msg.id)}
								>
									<Icon name="schema" class="text-fg-muted" />
									<span
										class="min-w-0 flex-1 truncate text-caption text-fg-secondary"
									>
										{toolLabel(msg.toolName)}
									</span>
									<Icon
										name="chevron-down"
										class={cn(
											'text-fg-muted transition-transform duration-150 ease-out',
											openTools[msg.id] && 'rotate-180',
										)}
									/>
								</button>
								{#if openTools[msg.id]}
									<div
										class="mt-1 overflow-hidden rounded-lg border border-line bg-elevated p-2"
										data-chat-message
										transition:fade={{ duration: 140 }}
									>
										<ChatMarkdown
											source={looksLikeJson(String(msg.content ?? ''))
												? fenceAs('json', String(msg.content ?? ''))
												: String(msg.content ?? '')}
											{engine}
											compact
										/>
									</div>
								{/if}
							</li>
						{:else if msg.role === 'user'}
							<li class="flex justify-end">
								<div
									class="min-w-0 max-w-[88%] rounded-xl rounded-br-sm bg-accent-muted px-3 py-2"
									data-chat-message
								>
									<ChatMarkdown
										source={String(msg.content ?? '')}
										{engine}
										compact
									/>
								</div>
							</li>
						{:else}
							<li class="self-stretch" data-chat-message>
								<ChatMarkdown
									source={String(msg.content ?? '')}
									{engine}
									onApply={applyDraft}
								/>
								{#if msg.draftBody && !messageHasApplyableFence(msg.content)}
									<div class="mt-2">
										<Button
											size="sm"
											variant="secondary"
											onclick={() => applyDraft(msg.draftBody ?? '')}
										>
											Apply to editor
										</Button>
									</div>
								{/if}
							</li>
						{/if}
					{/each}
					{#if sending}
						<li
							class="flex items-center gap-2 self-stretch rounded-lg bg-elevated px-2.5 py-2"
							aria-live="polite"
							transition:fade={{ duration: 160 }}
						>
							<span class="agent-typing" aria-hidden="true">
								<span></span>
								<span></span>
								<span></span>
							</span>
							<p class="m-0 min-w-0 flex-1 text-caption text-fg-secondary">
								{workingLabel}
							</p>
							<Button size="sm" variant="ghost" onclick={stopWaiting}>
								Stop
							</Button>
						</li>
					{/if}
				</ul>
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
					value={agentChat.provider}
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

<style>
	.agent-thread {
		scrollbar-width: thin;
		scrollbar-color: var(--border-strong) transparent;
	}

	.agent-labrador {
		opacity: 0.22;
	}

	:global([data-theme='light']) .agent-labrador {
		opacity: 0.16;
	}

	.agent-typing {
		display: inline-flex;
		align-items: center;
		gap: 0.1875rem;
	}

	.agent-typing span {
		display: block;
		width: 0.25rem;
		height: 0.25rem;
		border-radius: 999px;
		background: var(--accent);
		opacity: 0.28;
		animation: agent-typing 1s var(--ease-in-out) infinite;
	}

	.agent-typing span:nth-child(2) {
		animation-delay: 0.14s;
	}

	.agent-typing span:nth-child(3) {
		animation-delay: 0.28s;
	}

	@keyframes agent-typing {
		0%,
		100% {
			opacity: 0.28;
			transform: translateY(0);
		}
		50% {
			opacity: 1;
			transform: translateY(-1px);
		}
	}

	@media (prefers-reduced-motion: reduce) {
		.agent-typing span {
			animation: none;
			opacity: 0.7;
		}
	}
</style>
