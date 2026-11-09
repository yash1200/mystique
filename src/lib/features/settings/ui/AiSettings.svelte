<script lang="ts">
	import {
		Button,
		EmptyState,
		Field,
		Icon,
		Input,
		PasswordInput,
		Select,
		Switch,
	} from '#core/ui'
	import {
		aiSettings,
		availableToAdd,
		configuredProviders,
		type AgentProviderInfo,
	} from '#features/workspace/agent'
	import { onMount } from 'svelte'
	import AgentAccessDefault from './AgentAccessDefault.svelte'
	import AgentBackendSettings from './AgentBackendSettings.svelte'

	const view = $derived(aiSettings.view)
	const configured = $derived(configuredProviders(view))
	const unused = $derived(availableToAdd(view))

	let drafts = $state<Record<string, string>>({})
	let ollamaUrl = $state('')
	let busy = $state(false)
	let saved = $state<string | null>(null)
	let adding = $state(false)
	let addId = $state('')
	let addKey = $state('')
	let addOllamaUrl = $state('')

	const error = $derived(aiSettings.error)
	const addProvider = $derived(
		unused.find((p) => p.id === addId) ?? unused[0] ?? null,
	)
	const addOptions = $derived(
		unused.map((p) => ({ value: p.id, label: p.label })),
	)

	function flash(key: string) {
		saved = key
		setTimeout(() => {
			if (saved === key) saved = null
		}, 1400)
	}

	function statusLabel(p: AgentProviderInfo): string {
		if (p.id === 'ollama') return 'Local'
		return 'Key saved'
	}

	async function load() {
		try {
			const next = await aiSettings.load()
			if (next) ollamaUrl = next.ollamaBaseUrl
		} catch {
			/* surfaced via aiSettings.error */
		}
	}

	async function onEnable(on: boolean) {
		busy = true
		try {
			await aiSettings.setEnabled(on)
		} catch {
			await load()
		} finally {
			busy = false
		}
	}

	async function saveKey(provider: string) {
		busy = true
		try {
			await aiSettings.setApiKey(provider, drafts[provider] ?? '')
			drafts = { ...drafts, [provider]: '' }
			flash(provider)
		} catch {
			/* surfaced via aiSettings.error */
		} finally {
			busy = false
		}
	}

	async function removeProvider(p: AgentProviderInfo) {
		busy = true
		try {
			if (p.id === 'ollama') {
				await aiSettings.setOllamaUrl('')
			} else {
				await aiSettings.setApiKey(p.id, '')
			}
			drafts = { ...drafts, [p.id]: '' }
		} catch {
			/* surfaced via aiSettings.error */
		} finally {
			busy = false
		}
	}

	async function saveOllama() {
		busy = true
		try {
			await aiSettings.setOllamaUrl(ollamaUrl)
			flash('ollama-url')
		} catch {
			/* surfaced via aiSettings.error */
		} finally {
			busy = false
		}
	}

	function startAdd() {
		adding = true
		addId = unused[0]?.id ?? ''
		addKey = ''
		addOllamaUrl = view?.ollamaBaseUrl ?? 'http://127.0.0.1:11434'
	}

	function cancelAdd() {
		adding = false
		addKey = ''
	}

	async function confirmAdd() {
		const p = addProvider
		if (!p) return
		if (p.needsKey && !addKey.trim()) return
		busy = true
		try {
			if (p.id === 'ollama') {
				const url = addOllamaUrl.trim() || 'http://127.0.0.1:11434'
				await aiSettings.setOllamaUrl(url)
				ollamaUrl = url
			} else {
				await aiSettings.setApiKey(p.id, addKey)
			}
			adding = false
			addKey = ''
			flash(p.id)
		} catch {
			/* surfaced via aiSettings.error */
		} finally {
			busy = false
		}
	}

	onMount(() => {
		void load()
	})
</script>

{#if view}
	<Switch
		checked={view.enabled}
		disabled={busy}
		label="Enable query agent"
		onchange={onEnable}
	/>

	{#if configured.length === 0 && !adding}
		<div class="mt-4">
			<EmptyState
				title="No providers yet"
				description="Add a provider key to use the query agent. Only providers you add appear in the agent panel."
			>
				{#snippet actions()}
					<Button size="sm" disabled={busy} onclick={startAdd}>
						<Icon name="plus" />
						Add provider
					</Button>
				{/snippet}
			</EmptyState>
		</div>
	{:else}
		<div
			class="mt-4 flex flex-col divide-y divide-line overflow-hidden rounded-lg border border-line bg-panel"
		>
			{#each configured as p (p.id)}
				<div class="px-3 py-3">
					<div class="mb-2 flex items-baseline justify-between gap-2">
						<p class="m-0 text-label font-medium text-fg">{p.label}</p>
						<span class="text-caption text-fg-muted">{statusLabel(p)}</span>
					</div>
					{#if p.id === 'ollama'}
						<Field label="Base URL" hint="Default http://127.0.0.1:11434">
							<div class="flex items-center gap-1.5">
								<Input
									bind:value={ollamaUrl}
									inputSize="sm"
									spellcheck="false"
								/>
								<Button
									size="sm"
									disabled={busy}
									onclick={() => void saveOllama()}
								>
									{saved === 'ollama-url' ? 'Saved' : 'Save'}
								</Button>
								<Button
									size="sm"
									variant="ghost"
									disabled={busy}
									onclick={() => void removeProvider(p)}
								>
									Remove
								</Button>
							</div>
						</Field>
					{:else}
						<Field
							label="API key"
							hint={!(drafts[p.id] ?? '').trim()
								? 'Paste a new key to replace the saved one'
								: undefined}
						>
							<div class="flex items-center gap-1.5">
								<PasswordInput
									inputSize="sm"
									autocomplete="off"
									placeholder="Key saved"
									bind:value={
										() => drafts[p.id] ?? '',
										(v) => {
											drafts = { ...drafts, [p.id]: String(v ?? '') }
										}
									}
								/>
								<Button
									size="sm"
									disabled={busy || !(drafts[p.id] ?? '').trim()}
									onclick={() => void saveKey(p.id)}
								>
									{saved === p.id ? 'Saved' : 'Save'}
								</Button>
								<Button
									size="sm"
									variant="ghost"
									disabled={busy}
									onclick={() => void removeProvider(p)}
								>
									Remove
								</Button>
							</div>
						</Field>
					{/if}
				</div>
			{/each}

			{#if adding && addProvider}
				<div class="px-3 py-3">
					<p class="mb-2 text-label font-medium text-fg">Add provider</p>
					<Field label="Provider">
						<Select
							value={addId}
							options={addOptions}
							inputSize="sm"
							onchange={(v) => {
								addId = v
								addKey = ''
							}}
						/>
					</Field>
					<div class="mt-2">
						{#if addProvider.id === 'ollama'}
							<Field label="Base URL" hint="Default http://127.0.0.1:11434">
								<Input
									bind:value={addOllamaUrl}
									inputSize="sm"
									spellcheck="false"
								/>
							</Field>
						{:else}
							<Field label="API key">
								<PasswordInput
									inputSize="sm"
									autocomplete="off"
									placeholder="Paste key"
									bind:value={addKey}
								/>
							</Field>
						{/if}
					</div>
					<div class="mt-2 flex items-center justify-end gap-1.5">
						<Button
							size="sm"
							variant="ghost"
							disabled={busy}
							onclick={cancelAdd}
						>
							Cancel
						</Button>
						<Button
							size="sm"
							disabled={busy || (addProvider.needsKey && !addKey.trim())}
							onclick={() => void confirmAdd()}
						>
							Add
						</Button>
					</div>
				</div>
			{:else if unused.length > 0}
				<div class="px-2 py-2">
					<Button size="sm" variant="ghost" disabled={busy} onclick={startAdd}>
						<Icon name="plus" />
						Add provider
					</Button>
				</div>
			{/if}
		</div>
	{/if}
	<AgentBackendSettings />
	<AgentAccessDefault />
{/if}

{#if error}
	<p class="mt-2 mb-0 text-caption text-danger" role="alert">{error}</p>
{/if}
