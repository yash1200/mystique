<script lang="ts">
	import {
		getAiSettings,
		setAiApiKey,
		setAiEnabled,
		setAiOllamaUrl,
		type AiSettingsView,
	} from '$lib/api/agent'
	import { isTauri } from '$lib/api/tauri'
	import {
		Button,
		Field,
		Input,
		PasswordInput,
		Switch,
	} from '$lib/components/core'
	import { onMount } from 'svelte'

	let view = $state<AiSettingsView | null>(null)
	let drafts = $state<Record<string, string>>({})
	let ollamaUrl = $state('')
	let busy = $state(false)
	let error = $state<string | null>(null)
	let saved = $state<string | null>(null)

	const desktop = isTauri()

	function flash(key: string) {
		saved = key
		setTimeout(() => {
			if (saved === key) saved = null
		}, 1400)
	}

	async function load() {
		if (!desktop) return
		try {
			view = await getAiSettings()
			ollamaUrl = view.ollamaBaseUrl
			error = null
		} catch (e) {
			error = e instanceof Error ? e.message : String(e)
		}
	}

	async function onEnable(on: boolean) {
		if (!desktop) return
		busy = true
		error = null
		try {
			view = await setAiEnabled(on)
		} catch (e) {
			error = e instanceof Error ? e.message : String(e)
			await load()
		} finally {
			busy = false
		}
	}

	async function saveKey(provider: string) {
		if (!desktop) return
		busy = true
		error = null
		try {
			view = await setAiApiKey(provider, drafts[provider] ?? '')
			drafts = { ...drafts, [provider]: '' }
			flash(provider)
		} catch (e) {
			error = e instanceof Error ? e.message : String(e)
		} finally {
			busy = false
		}
	}

	async function clearKey(provider: string) {
		if (!desktop) return
		busy = true
		error = null
		try {
			view = await setAiApiKey(provider, '')
			drafts = { ...drafts, [provider]: '' }
			flash(provider)
		} catch (e) {
			error = e instanceof Error ? e.message : String(e)
		} finally {
			busy = false
		}
	}

	async function saveOllama() {
		if (!desktop) return
		busy = true
		error = null
		try {
			view = await setAiOllamaUrl(ollamaUrl)
			flash('ollama-url')
		} catch (e) {
			error = e instanceof Error ? e.message : String(e)
		} finally {
			busy = false
		}
	}

	onMount(() => {
		void load()
	})
</script>

<p class="mt-0 mb-3 text-label text-fg-muted">
	Keys are encrypted in the local app database. The query-tab agent uses them to
	draft SQL or MQL against the open connection.
</p>

{#if !desktop}
	<p class="m-0 text-caption text-fg-muted">
		AI settings are available in the desktop app.
	</p>
{:else if view}
	<Switch
		checked={view.enabled}
		disabled={busy}
		label="Enable query agent"
		onchange={onEnable}
	/>

	<div class="mt-4 flex flex-col gap-3">
		{#each view.providers as p (p.id)}
			{@const has = view.keys.find((k) => k.provider === p.id)?.hasKey}
			<div class="rounded-md border border-line bg-panel px-3 py-2.5">
				<div class="mb-1.5 flex items-baseline justify-between gap-2">
					<p class="m-0 text-label font-medium text-fg">{p.label}</p>
					<span class="text-caption text-fg-muted">
						{p.needsKey ? (has ? 'Key saved' : 'No key') : 'Local'}
					</span>
				</div>
				{#if p.id === 'ollama'}
					<Field label="Base URL" hint="Default http://127.0.0.1:11434">
						<div class="flex items-center gap-1.5">
							<Input bind:value={ollamaUrl} inputSize="sm" spellcheck="false" />
							<Button
								size="sm"
								disabled={busy}
								onclick={() => void saveOllama()}
							>
								{saved === 'ollama-url' ? 'Saved' : 'Save'}
							</Button>
						</div>
					</Field>
				{:else}
					<Field
						label="API key"
						hint={has && !(drafts[p.id] ?? '').trim()
							? 'Paste a new key to replace the saved one'
							: undefined}
					>
						<div class="flex items-center gap-1.5">
							<PasswordInput
								inputSize="sm"
								autocomplete="off"
								placeholder={has ? 'Key saved' : 'Paste key'}
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
							{#if has}
								<Button
									size="sm"
									variant="ghost"
									disabled={busy}
									onclick={() => void clearKey(p.id)}
								>
									Clear
								</Button>
							{/if}
						</div>
					</Field>
				{/if}
			</div>
		{/each}
	</div>
{/if}

{#if error}
	<p class="mt-2 mb-0 text-caption text-danger" role="alert">{error}</p>
{/if}
