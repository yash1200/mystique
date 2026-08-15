<script lang="ts">
	import {
		getMcpStatus,
		mcpClientSnippet,
		rotateMcpToken,
		setMcpEnabled,
		type McpStatus,
	} from '$lib/api/mcp'
	import { isTauri } from '$lib/api/tauri'
	import { cn } from '$lib/utils/cn'
	import AppSettings from './AppSettings.svelte'
	import Button from './Button.svelte'
	import IconButton from './IconButton.svelte'
	import Switch from './Switch.svelte'

	interface Props {
		/** Unused — kept so existing callers compile. Modal is centered. */
		placement?: 'top-end' | 'right-end'
		buttonSize?: 'sm' | 'md'
	}

	let { buttonSize = 'sm' }: Props = $props()

	type Section = 'appearance' | 'mcp'

	let open = $state(false)
	let section = $state<Section>('appearance')
	let mcp = $state<McpStatus>({ enabled: false, running: false })
	let mcpBusy = $state(false)
	let mcpError = $state<string | null>(null)
	let copied = $state<string | null>(null)

	async function loadMcp() {
		if (!isTauri()) return
		try {
			mcp = await getMcpStatus()
			mcpError = null
		} catch (e) {
			mcpError = e instanceof Error ? e.message : String(e)
		}
	}

	function toggle() {
		open = !open
		if (open) {
			section = 'appearance'
			void loadMcp()
		}
	}

	function close() {
		open = false
	}

	async function onEnable(on: boolean) {
		if (!isTauri()) return
		mcpBusy = true
		mcpError = null
		try {
			mcp = await setMcpEnabled(on)
		} catch (err) {
			mcpError = err instanceof Error ? err.message : String(err)
			await loadMcp()
		} finally {
			mcpBusy = false
		}
	}

	async function onRotate() {
		if (!isTauri()) return
		mcpBusy = true
		mcpError = null
		try {
			mcp = await rotateMcpToken()
		} catch (err) {
			mcpError = err instanceof Error ? err.message : String(err)
		} finally {
			mcpBusy = false
		}
	}

	async function copy(text: string, key: string) {
		try {
			await navigator.clipboard.writeText(text)
			copied = key
			setTimeout(() => {
				if (copied === key) copied = null
			}, 1600)
		} catch {
			mcpError = 'Could not copy — select the text instead.'
		}
	}

	const snippet = $derived(mcpClientSnippet(mcp))
</script>

<IconButton
	label="Settings"
	variant="ghost"
	size={buttonSize}
	active={open}
	onclick={toggle}
	aria-expanded={open}
	aria-haspopup="dialog"
>
	<svg class="size-3.5" viewBox="0 0 16 16" fill="none" aria-hidden="true">
		<path
			d="M6.6 1.85h2.8l.3 1.45c.42.16.81.38 1.16.66l1.48-.38 1.4 2.42-1.18.9c.07.3.11.62.11 1.1 0 .37-.04.69-.11 1.01l1.18.9-1.4 2.42-1.48-.38a4.6 4.6 0 0 1-1.16.66l-.3 1.45H6.6l-.3-1.45a4.6 4.6 0 0 1-1.16-.66l-1.48.38-1.4-2.42 1.18-.9A4.4 4.4 0 0 1 3.33 8c0-.37.04-.69.11-1.01l-1.18-.9 1.4-2.42 1.48.38c.35-.28.74-.5 1.16-.66l.3-1.45Z"
			stroke="currentColor"
			stroke-width="1.2"
			stroke-linejoin="round"
		/>
		<circle cx="8" cy="8" r="1.65" stroke="currentColor" stroke-width="1.2" />
	</svg>
</IconButton>

{#if open}
	<div
		class="fixed inset-0 z-50 flex items-center justify-center bg-black/45 p-4"
		role="presentation"
		onclick={(e) => {
			if (e.target === e.currentTarget) close()
		}}
		onkeydown={(e) => {
			if (e.key === 'Escape') close()
		}}
	>
		<div
			class="flex h-[min(36rem,calc(100vh-2rem))] w-full max-w-2xl overflow-hidden rounded-lg border border-line bg-elevated shadow-lg"
			role="dialog"
			aria-modal="true"
			aria-labelledby="settings-title"
		>
			<nav
				class="flex w-40 shrink-0 flex-col gap-0.5 border-r border-line bg-panel p-2"
				aria-label="Settings sections"
			>
				<p
					id="settings-title"
					class="mb-1.5 px-2 pt-1 text-caption font-semibold tracking-wider text-fg-muted uppercase"
				>
					Settings
				</p>
				<button
					type="button"
					class={cn(
						'rounded-md border-none px-2 py-1.5 text-left text-label transition-colors',
						section === 'appearance'
							? 'bg-accent-muted text-accent'
							: 'bg-transparent text-fg-secondary hover:bg-hover hover:text-fg',
					)}
					onclick={() => (section = 'appearance')}
				>
					Appearance
				</button>
				<button
					type="button"
					class={cn(
						'rounded-md border-none px-2 py-1.5 text-left text-label transition-colors',
						section === 'mcp'
							? 'bg-accent-muted text-accent'
							: 'bg-transparent text-fg-secondary hover:bg-hover hover:text-fg',
					)}
					onclick={() => {
						section = 'mcp'
						void loadMcp()
					}}
				>
					MCP
				</button>
			</nav>

			<div class="min-h-0 min-w-0 flex-1 overflow-auto p-4">
				<div class="mb-3 flex items-start justify-between gap-2">
					<h2 class="m-0 text-body font-semibold text-fg">
						{section === 'appearance' ? 'Appearance' : 'MCP server'}
					</h2>
					<IconButton
						label="Close settings"
						variant="ghost"
						size="sm"
						onclick={close}
					>
						<svg
							class="size-3.5"
							viewBox="0 0 16 16"
							fill="none"
							aria-hidden="true"
						>
							<path
								d="M4 4l8 8M12 4l-8 8"
								stroke="currentColor"
								stroke-width="1.4"
								stroke-linecap="round"
							/>
						</svg>
					</IconButton>
				</div>

				{#if section === 'appearance'}
					<AppSettings />
				{:else}
					<p class="mt-0 mb-3 text-label text-fg-muted">
						While Mystique DB is running, external LLMs can read schema (DBML),
						list tables, run read-only queries, and save queries. Writes are
						blocked.
					</p>
					{#if !isTauri()}
						<p class="m-0 text-caption text-fg-muted">
							MCP is available in the desktop app.
						</p>
					{:else}
						<Switch
							checked={mcp.enabled}
							disabled={mcpBusy}
							label="Enable MCP on localhost"
							onchange={onEnable}
						/>
						{#if mcp.running && mcp.url}
							<div class="mt-3 flex flex-col gap-2">
								<div>
									<p
										class="m-0 mb-1 text-caption font-medium text-fg-secondary"
									>
										URL
									</p>
									<div class="flex items-center gap-1.5">
										<code
											class="min-w-0 flex-1 truncate rounded-md border border-line bg-panel px-2 py-1 font-mono text-caption text-fg"
											>{mcp.url}</code
										>
										<Button
											size="sm"
											onclick={() => void copy(mcp.url ?? '', 'url')}
										>
											{copied === 'url' ? 'Copied' : 'Copy'}
										</Button>
									</div>
								</div>
								<div>
									<p
										class="m-0 mb-1 text-caption font-medium text-fg-secondary"
									>
										Token
									</p>
									<div class="flex items-center gap-1.5">
										<code
											class="min-w-0 flex-1 truncate rounded-md border border-line bg-panel px-2 py-1 font-mono text-caption text-fg"
											>{mcp.token}</code
										>
										<Button
											size="sm"
											onclick={() => void copy(mcp.token ?? '', 'token')}
										>
											{copied === 'token' ? 'Copied' : 'Copy'}
										</Button>
									</div>
								</div>
								<div>
									<p
										class="m-0 mb-1 text-caption font-medium text-fg-secondary"
									>
										Client config
									</p>
									<pre
										class="m-0 max-h-40 overflow-auto rounded-md border border-line bg-panel p-2 font-mono text-caption text-fg">{snippet}</pre>
									<div class="mt-1.5 flex flex-wrap gap-1.5">
										<Button
											size="sm"
											onclick={() => void copy(snippet, 'snippet')}
										>
											{copied === 'snippet' ? 'Copied' : 'Copy config'}
										</Button>
										<Button
											size="sm"
											variant="ghost"
											disabled={mcpBusy}
											onclick={() => void onRotate()}
										>
											New token
										</Button>
									</div>
								</div>
								<p class="m-0 text-caption text-fg-muted">
									Turning the app off stops the server.
								</p>
							</div>
						{:else if mcp.enabled && !mcp.running}
							<p class="mt-2 mb-0 text-caption text-fg-muted">
								Enabled, but the server is not running yet.
							</p>
						{/if}
					{/if}
					{#if mcpError}
						<p class="mt-2 mb-0 text-caption text-danger" role="alert">
							{mcpError}
						</p>
					{/if}
				{/if}
			</div>
		</div>
	</div>
{/if}

<svelte:window
	onkeydown={(e) => {
		if (e.key === 'Escape' && open) close()
	}}
/>
