<script lang="ts">
	import {
		deleteAgentChat,
		getAiSettings,
		listAgentChats,
		listAgentMessages,
		listAgentModels,
		sendAgentMessage,
		updateAgentChat,
		type AgentChat,
		type AgentMessage,
		type AgentModelInfo,
		type AiSettingsView,
	} from '$lib/api/agent'
	import { isTauri } from '$lib/api/tauri'
	import {
		Button,
		Icon,
		IconButton,
		Select,
		StatusDot,
		Textarea,
	} from '$lib/components/core'
	import {
		AGENT_CHAT_PANEL_WIDTH_DEFAULT,
		appPrefs,
	} from '$lib/stores/app-prefs.svelte'
	import { appSettingsUi } from '$lib/stores/app-settings.svelte'
	import { workspaceSession } from '$lib/stores/workspace-session.svelte'
	import {
		inferFenceLang,
		isQueryLang,
		parseChatMarkdown,
	} from '$lib/utils/chatMarkdown'
	import { cn } from '$lib/utils/cn'
	import { onMount } from 'svelte'
	import ChatMarkdown from './ChatMarkdown.svelte'

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

	let settings = $state<AiSettingsView | null>(null)
	let chats = $state<AgentChat[]>([])
	let messages = $state<AgentMessage[]>([])
	let chatId = $state<string | null>(null)
	let provider = $state('openai')
	let model = $state('gpt-5.6-terra')
	let models = $state<AgentModelInfo[]>([])
	let modelsLoading = $state(false)
	let modelsError = $state<string | null>(null)
	let draft = $state('')
	let sending = $state(false)
	let elapsedSec = $state(0)
	let sendGen = $state(0)
	let error = $state<string | null>(null)
	let listEl = $state<HTMLDivElement | null>(null)
	let dragging = $state(false)

	const chatOptions = $derived([
		{ value: '', label: 'New chat' },
		...chats.map((c) => ({ value: c.id, label: c.title })),
	])
	const providerOptions = $derived(
		(settings?.providers ?? []).map((p) => ({
			value: p.id,
			label: p.label,
		})),
	)
	const modelOptions = $derived(
		models.length
			? models.map((m) => ({ value: m.id, label: m.label }))
			: model
				? [{ value: model, label: model }]
				: [],
	)
	const modelPlaceholder = $derived(
		modelsLoading
			? 'Loading models…'
			: provider === 'ollama'
				? 'No local models'
				: 'Select model',
	)
	const canSend = $derived(
		!sending &&
			Boolean(draft.trim()) &&
			desktop &&
			!modelsLoading &&
			(provider !== 'ollama' || Boolean(model.trim())),
	)
	const workingLabel = $derived.by(() => {
		if (!sending) return ''
		const who = provider === 'ollama' ? 'Ollama' : 'the model'
		if (elapsedSec < 8) return `Working… ${elapsedSec}s`
		if (elapsedSec < 25) return `Waiting on ${who}… ${elapsedSec}s`
		return `Still waiting on ${who}… ${elapsedSec}s`
	})
	const lastDraft = $derived(
		[...messages].reverse().find((m) => m.role === 'assistant' && m.draftBody)
			?.draftBody ?? null,
	)
	let openTools = $state<Record<string, boolean>>({})

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
		settings = await getAiSettings()
		if (!settings.providers.some((p) => p.id === provider)) {
			provider = settings.providers[0]?.id ?? 'openai'
		}
	}

	async function loadModels() {
		if (!desktop || sending) return
		modelsLoading = true
		try {
			models = await listAgentModels(provider)
			modelsError = null
			if (provider === 'ollama' && models.length === 0) {
				modelsError =
					'Ollama has no local models. Pull one with `ollama pull <name>`.'
			}
			if (!models.some((m) => m.id === model)) {
				const fallback =
					provider === 'ollama'
						? ''
						: (settings?.providers.find((p) => p.id === provider)
								?.defaultModel ?? '')
				model = models[0]?.id ?? fallback
			}
		} catch (e) {
			models = []
			modelsError = e instanceof Error ? e.message : String(e)
			if (provider === 'ollama' && isHardcodedOllamaModel(model)) {
				model = ''
			}
		} finally {
			modelsLoading = false
		}
	}

	function isHardcodedOllamaModel(id: string): boolean {
		return id === 'llama3.2' || id === 'qwen2.5' || id === 'mistral'
	}

	async function loadChats() {
		if (!desktop) return
		chats = await listAgentChats(connectionId)
		if (chatId && !chats.some((c) => c.id === chatId)) {
			chatId = chats[0]?.id ?? null
		}
	}

	async function loadMessages() {
		if (sending) return
		if (!desktop || !chatId) {
			messages = []
			return
		}
		messages = await listAgentMessages(chatId)
		queueMicrotask(() => {
			listEl?.scrollTo({ top: listEl.scrollHeight })
		})
	}

	async function onPickChat(id: string) {
		if (sending) return
		chatId = id || null
		if (id) {
			const c = chats.find((x) => x.id === id)
			if (c) {
				provider = c.provider
				model = c.model
				await loadModels()
			}
			await loadMessages()
		} else {
			messages = []
		}
	}

	async function newChat() {
		if (sending) return
		chatId = null
		messages = []
		draft = ''
		error = null
	}

	async function removeChat() {
		if (sending || !chatId) return
		await deleteAgentChat(chatId)
		chatId = null
		await loadChats()
		messages = []
	}

	async function send() {
		const text = draft.trim()
		if (!text || sending) return
		const gen = ++sendGen
		sending = true
		elapsedSec = 0
		const tick = setInterval(() => {
			if (gen === sendGen) elapsedSec += 1
		}, 1000)
		error = null
		draft = ''
		const optimisticId = `local-${crypto.randomUUID()}`
		const optimistic: AgentMessage = {
			id: optimisticId,
			chatId: chatId ?? '',
			role: 'user',
			content: text,
			createdAt: new Date().toISOString(),
		}
		messages = [...messages, optimistic]
		queueMicrotask(() => {
			listEl?.scrollTo({ top: listEl.scrollHeight })
		})
		try {
			const editorQuery = (workspaceSession.queryDrafts[tabId] ?? '').slice(
				0,
				40_000,
			)
			const result = await withTimeout(
				sendAgentMessage({
					connectionId,
					connectionName,
					engine,
					database,
					chatId,
					provider,
					model,
					text,
					editorQuery,
				}),
				55_000,
				'The model did not reply in time. It may still be running — try again, or pick a smaller Ollama model.',
			)
			if (gen !== sendGen) return
			chatId = result.chat.id
			messages = [
				...messages.filter(
					(m) => m.id !== optimisticId && m.id !== result.user.id,
				),
				result.user,
				result.assistant,
			]
			void loadChats()
			queueMicrotask(() => {
				listEl?.scrollTo({ top: listEl.scrollHeight })
			})
		} catch (e) {
			if (gen !== sendGen) return
			error = e instanceof Error ? e.message : String(e)
		} finally {
			clearInterval(tick)
			if (gen === sendGen) {
				sending = false
				elapsedSec = 0
			}
		}
	}

	function stopWaiting() {
		sendGen += 1
		sending = false
		elapsedSec = 0
		error = 'Stopped waiting. Send again if you still want a reply.'
	}

	function withTimeout<T>(
		promise: Promise<T>,
		ms: number,
		message: string,
	): Promise<T> {
		return new Promise((resolve, reject) => {
			const timer = setTimeout(() => reject(new Error(message)), ms)
			promise.then(
				(value) => {
					clearTimeout(timer)
					resolve(value)
				},
				(err) => {
					clearTimeout(timer)
					reject(err)
				},
			)
		})
	}

	function applyDraft(sql: string) {
		workspaceSession.setQueryDraft(tabId, sql)
	}

	function onResizePointerDown(e: PointerEvent) {
		if (e.button !== 0) return
		e.preventDefault()
		const startX = e.clientX
		const startW = appPrefs.agentChatPanelWidth
		dragging = true
		const target = e.currentTarget as HTMLElement
		target.setPointerCapture(e.pointerId)
		document.body.style.cursor = 'col-resize'
		document.body.style.userSelect = 'none'

		const onMove = (ev: PointerEvent) => {
			appPrefs.setAgentChatPanelWidth(startW - (ev.clientX - startX), false)
		}
		const onUp = (ev: PointerEvent) => {
			dragging = false
			document.body.style.cursor = ''
			document.body.style.userSelect = ''
			try {
				target.releasePointerCapture(ev.pointerId)
			} catch {
				/* ignore */
			}
			target.removeEventListener('pointermove', onMove)
			target.removeEventListener('pointerup', onUp)
			target.removeEventListener('pointercancel', onUp)
			appPrefs.setAgentChatPanelWidth(appPrefs.agentChatPanelWidth, true)
		}
		target.addEventListener('pointermove', onMove)
		target.addEventListener('pointerup', onUp)
		target.addEventListener('pointercancel', onUp)
	}

	function onResizeDblClick() {
		appPrefs.setAgentChatPanelWidth(AGENT_CHAT_PANEL_WIDTH_DEFAULT, true)
	}

	function onComposerKey(e: KeyboardEvent) {
		if (e.key === 'Enter' && (e.metaKey || e.ctrlKey)) {
			e.preventDefault()
			void send()
		}
	}

	$effect(() => {
		void provider
		if (desktop) void loadModels()
	})

	onMount(() => {
		void (async () => {
			if (!desktop) return
			try {
				await loadSettings()
				await loadModels()
				await loadChats()
				if (chats[0]) {
					chatId = chats[0].id
					provider = chats[0].provider
					model = chats[0].model
					await loadModels()
					await loadMessages()
				}
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
	<button
		type="button"
		aria-label="Resize agent panel ({appPrefs.agentChatPanelWidth}px)"
		title="Drag to resize · double-click to reset"
		class={cn(
			'group absolute top-0 left-0 z-10 flex h-full w-2 shrink-0 -translate-x-1/2 cursor-col-resize items-center justify-center border-none bg-transparent p-0',
			'hover:bg-hover/80',
			dragging && 'bg-accent-muted',
		)}
		onpointerdown={onResizePointerDown}
		ondblclick={onResizeDblClick}
		onkeydown={(e) => {
			if (e.key === 'ArrowLeft') {
				e.preventDefault()
				appPrefs.setAgentChatPanelWidth(appPrefs.agentChatPanelWidth + 12, true)
			} else if (e.key === 'ArrowRight') {
				e.preventDefault()
				appPrefs.setAgentChatPanelWidth(appPrefs.agentChatPanelWidth - 12, true)
			} else if (e.key === 'Home') {
				e.preventDefault()
				appPrefs.setAgentChatPanelWidth(AGENT_CHAT_PANEL_WIDTH_DEFAULT, true)
			}
		}}
	>
		<span
			class={cn(
				'pointer-events-none h-10 w-0.5 rounded-full bg-line-strong opacity-0 transition-opacity',
				'group-hover:opacity-100 group-focus-visible:opacity-100',
				dragging && 'bg-accent opacity-100',
			)}
			aria-hidden="true"
		></span>
	</button>
	<header
		class="flex h-control shrink-0 items-center gap-1.5 border-b border-line px-2"
	>
		<Icon name="chat" class="text-accent" />
		<span class="min-w-0 flex-1 truncate text-label font-semibold text-fg"
			>Agent</span
		>
		<IconButton
			label="Close agent"
			variant="ghost"
			size="sm"
			onclick={() => appPrefs.setAgentChatOpen(false)}
		>
			<Icon name="close" />
		</IconButton>
	</header>

	<div
		class="flex shrink-0 items-center gap-1 border-b border-line px-2 py-1.5"
	>
		<Select
			class="min-w-0 flex-1"
			inputSize="sm"
			options={chatOptions}
			value={chatId ?? ''}
			onchange={(v) => void onPickChat(v)}
		/>
		<IconButton label="New chat" variant="ghost" size="sm" onclick={newChat}>
			<Icon name="plus" />
		</IconButton>
		{#if chatId}
			<IconButton
				label="Delete chat"
				variant="ghost"
				size="sm"
				onclick={() => void removeChat()}
			>
				<Icon name="close" />
			</IconButton>
		{/if}
	</div>

	<div bind:this={listEl} class="min-h-0 flex-1 overflow-auto px-2.5 py-2">
		{#if !desktop}
			<p class="m-0 text-caption text-fg-muted">
				Agent requires the desktop app.
			</p>
		{:else if settings && !settings.enabled}
			<div class="flex flex-col gap-2 py-6">
				<p class="m-0 text-body text-fg">Agent is off</p>
				<p class="m-0 text-caption text-fg-muted">
					Turn it on and add a provider key in Settings.
				</p>
				<Button size="sm" onclick={() => appSettingsUi.show('ai')}>
					Open AI settings
				</Button>
			</div>
		{:else if messages.length === 0 && !sending}
			<div class="flex flex-col gap-1.5 py-6">
				<p class="m-0 text-body text-fg">Ask in the language of the schema</p>
				<p class="m-0 text-caption text-fg-muted">
					Drafts land in the editor. You run them. The agent can read tables and
					probe with SELECT only.
				</p>
			</div>
		{:else}
			<ul class="m-0 flex list-none flex-col gap-3 p-0">
				{#each messages as msg (msg.id)}
					{#if msg.role === 'tool'}
						<li class="self-stretch">
							<button
								type="button"
								class="flex w-full items-center gap-1.5 rounded-md border border-line bg-elevated px-2 py-1 text-left hover:bg-hover"
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
										'text-fg-muted transition-transform duration-150',
										openTools[msg.id] && 'rotate-180',
									)}
								/>
							</button>
							{#if openTools[msg.id]}
								<div class="mt-1" data-chat-message>
									<ChatMarkdown
										source={looksLikeJson(msg.content)
											? fenceAs('json', msg.content)
											: msg.content}
										{engine}
										compact
									/>
								</div>
							{/if}
						</li>
					{:else if msg.role === 'user'}
						<li class="flex justify-end">
							<div
								class="min-w-0 max-w-[92%] rounded-md rounded-br-sm bg-accent-muted px-2.5 py-2"
								data-chat-message
							>
								<ChatMarkdown source={msg.content} {engine} compact />
							</div>
						</li>
					{:else}
						<li class="self-stretch" data-chat-message>
							<ChatMarkdown
								source={msg.content}
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
						class="flex items-center gap-1.5 self-stretch"
						aria-live="polite"
					>
						<StatusDot status="running" label="Working" />
						<p class="m-0 min-w-0 flex-1 text-caption text-fg-muted">
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

	{#if lastDraft}
		<div class="shrink-0 border-t border-line px-2 py-1.5">
			<Button
				size="sm"
				variant="secondary"
				onclick={() => applyDraft(lastDraft)}
			>
				Apply last draft
			</Button>
		</div>
	{/if}

	{#if modelsError && provider === 'ollama'}
		<p class="m-0 shrink-0 px-2.5 pb-1 text-caption text-danger" role="alert">
			{modelsError}
		</p>
	{/if}

	{#if error}
		<p class="m-0 shrink-0 px-2.5 pb-1 text-caption text-danger" role="alert">
			{error}
		</p>
	{/if}

	<form
		class="shrink-0 border-t border-line p-2"
		onsubmit={(e) => {
			e.preventDefault()
			void send()
		}}
	>
		<div
			class="overflow-hidden rounded-md border border-line bg-input focus-within:border-focus focus-within:ring-2 focus-within:ring-accent-muted"
		>
			<Textarea
				rows={3}
				class="min-h-16 resize-none rounded-none border-0 bg-transparent focus-visible:border-transparent focus-visible:ring-0"
				placeholder="Ask for a query…"
				bind:value={draft}
				disabled={sending || !desktop}
				onkeydown={onComposerKey}
			/>
			<div
				class="flex items-center gap-1 border-t border-line bg-elevated px-1.5 py-1"
			>
				<Select
					class="min-w-0 flex-1"
					inputSize="sm"
					options={providerOptions}
					value={provider}
					onchange={(v) => {
						provider = v
						if (chatId) void updateAgentChat({ id: chatId, provider: v, model })
					}}
				/>
				<Select
					class="min-w-0 flex-1"
					inputSize="sm"
					options={modelOptions}
					value={model}
					placeholder={modelPlaceholder}
					disabled={modelsLoading}
					onchange={(v) => {
						model = v
						if (chatId) void updateAgentChat({ id: chatId, provider, model: v })
					}}
				/>
				<Button type="submit" size="sm" disabled={!canSend}>
					{sending ? '…' : 'Send'}
				</Button>
			</div>
		</div>
	</form>
</aside>
