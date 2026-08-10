<script lang="ts">
	import { listProfiles } from '$lib/api/profiles'
	import { Button, EngineBadge, Input, Pill } from '$lib/components/ui'
	import type { ConnectionProfile } from '$lib/types/connection'
	import {
		colorHex,
		connectionEndpoint,
		connectionInitial,
		engineLabel,
	} from '$lib/types/connection'
	import { cn } from '$lib/utils/cn'
	import { onMount } from 'svelte'

	interface Props {
		openIds: string[]
		busy?: boolean
		error?: string | null
		onclose: () => void
		onpick: (profile: ConnectionProfile) => void
		onmanage: () => void
	}

	let {
		openIds,
		busy = false,
		error = null,
		onclose,
		onpick,
		onmanage,
	}: Props = $props()

	let query = $state('')
	let loading = $state(true)
	let loadError = $state<string | null>(null)
	let profiles = $state<ConnectionProfile[]>([])

	const available = $derived(profiles.filter((p) => !openIds.includes(p.id)))

	const filtered = $derived.by(() => {
		const q = query.trim().toLowerCase()
		if (!q) return available
		return available.filter((c) => {
			const cfg = c.pluginConfig ?? {}
			return (
				c.name.toLowerCase().includes(q) ||
				String(c.engine).toLowerCase().includes(q) ||
				String(cfg.host ?? '')
					.toLowerCase()
					.includes(q) ||
				String(cfg.database ?? '')
					.toLowerCase()
					.includes(q) ||
				String(cfg.path ?? '')
					.toLowerCase()
					.includes(q)
			)
		})
	})

	onMount(() => {
		const onKey = (e: KeyboardEvent) => {
			if (e.key === 'Escape' && !busy) onclose()
		}
		window.addEventListener('keydown', onKey)
		void (async () => {
			loading = true
			loadError = null
			try {
				profiles = await listProfiles()
			} catch (e) {
				loadError = e instanceof Error ? e.message : String(e)
			} finally {
				loading = false
			}
		})()
		return () => window.removeEventListener('keydown', onKey)
	})
</script>

<div class="fixed inset-0 z-50 flex items-center justify-center p-4">
	<button
		type="button"
		class="absolute inset-0 border-none bg-app/70"
		aria-label="Dismiss"
		onclick={onclose}
		disabled={busy}
	></button>
	<div
		class="relative z-10 flex max-h-[min(32rem,80vh)] w-full max-w-md flex-col overflow-hidden rounded-xl border border-line bg-elevated shadow-md"
		role="dialog"
		aria-modal="true"
		aria-labelledby="open-conn-title"
	>
		<header class="shrink-0 border-b border-line px-4 py-3">
			<h2 id="open-conn-title" class="m-0 text-title font-semibold text-fg">
				Open connection
			</h2>
			<p class="mt-1 text-caption text-fg-muted">
				Add another saved database to this workspace.
			</p>
		</header>

		<div class="shrink-0 px-4 py-2">
			<Input
				type="search"
				placeholder="Filter by name, engine, host…"
				bind:value={query}
				inputSize="sm"
				disabled={busy}
				autofocus
			/>
		</div>

		<div class="min-h-0 flex-1 overflow-auto px-2 pb-2">
			{#if loading}
				<p class="px-2 py-4 text-caption text-fg-muted">Loading…</p>
			{:else if loadError}
				<p class="px-2 py-4 text-caption text-danger">{loadError}</p>
			{:else if available.length === 0}
				<div class="px-2 py-6 text-center">
					<p class="text-body font-medium text-fg">
						{profiles.length === 0
							? 'No saved connections yet'
							: 'Every saved connection is already open'}
					</p>
					<p class="mt-1 text-caption text-fg-muted">
						Create a new profile on the home screen, then come back.
					</p>
				</div>
			{:else if filtered.length === 0}
				<p class="px-2 py-4 text-caption text-fg-muted">
					No matches for “{query}”.
				</p>
			{:else}
				<ul class="m-0 flex list-none flex-col gap-1 p-0">
					{#each filtered as conn (conn.id)}
						{@const hex = colorHex(conn.colorId)}
						{@const initial = connectionInitial(conn.name, String(conn.engine))}
						<li>
							<button
								type="button"
								class={cn(
									'flex w-full items-center gap-2.5 rounded-lg border border-transparent px-2 py-2 text-left',
									'hover:border-line hover:bg-hover disabled:opacity-55',
								)}
								style="--connection-color: {hex}"
								disabled={busy}
								onclick={() => onpick(conn)}
							>
								<span
									class="relative grid size-8 shrink-0 place-items-center overflow-hidden rounded-md border border-line bg-elevated text-body font-semibold"
									aria-hidden="true"
								>
									<span class="absolute inset-x-0 top-0 h-0.5 bg-conn"></span>
									<span style="color: var(--connection-color)">{initial}</span>
								</span>
								<div class="min-w-0 flex-1">
									<div class="flex items-center gap-1.5">
										<span class="truncate text-body font-medium text-fg"
											>{conn.name}</span
										>
										{#if conn.environment === 'production'}
											<Pill tone="warning" uppercase>PROD</Pill>
										{/if}
										{#if conn.readOnly}
											<Pill uppercase>RO</Pill>
										{/if}
									</div>
									<div
										class="mt-0.5 flex items-center gap-1.5 font-mono text-caption text-fg-muted"
									>
										<EngineBadge engine={conn.engine} />
										<span class="truncate">{connectionEndpoint(conn)}</span>
									</div>
								</div>
								<span class="sr-only">{engineLabel(conn.engine)}</span>
							</button>
						</li>
					{/each}
				</ul>
			{/if}
			{#if error}
				<p class="px-2 pt-1 text-caption text-danger">{error}</p>
			{/if}
		</div>

		<footer
			class="flex shrink-0 items-center justify-between gap-2 border-t border-line px-4 py-2.5"
		>
			<Button variant="ghost" size="sm" onclick={onmanage} disabled={busy}>
				New connection…
			</Button>
			<Button variant="ghost" size="sm" onclick={onclose} disabled={busy}>
				Cancel
			</Button>
		</footer>
	</div>
</div>
