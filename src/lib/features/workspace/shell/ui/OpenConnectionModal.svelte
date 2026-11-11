<script lang="ts">
	import type { ConnectionProfile } from '#core/contracts'
	import { Button, Icon, Input, Modal, Pill } from '#core/ui'
	import { cn } from '#core/util'
	import { engineLabel } from '#engines'
	import {
		ConnectionTile,
		colorHex,
		connectionProfiles,
		connectionTarget,
	} from '#features/connections'
	import { onMount, tick } from 'svelte'

	interface Props {
		openIds: string[]
		busy?: boolean
		error?: string | null
		onclose: () => void
		onpick: (profile: ConnectionProfile) => void
	}

	let { openIds, busy = false, error = null, onclose, onpick }: Props = $props()

	const LIST_ID = 'open-connection-list'

	let query = $state('')
	/** The highlighted row; arrow keys move it, Enter opens it. */
	let cursor = $state(0)
	let listEl = $state<HTMLUListElement | null>(null)

	const loading = $derived(connectionProfiles.loading && !connectionProfiles.items.length)
	const loadError = $derived(connectionProfiles.error)
	const profiles = $derived(connectionProfiles.items)
	const available = $derived(profiles.filter((p) => !openIds.includes(p.id)))

	const filtered = $derived.by(() => {
		const q = query.trim().toLowerCase()
		const rows = !q
			? available
			: available.filter((c) => {
					const cfg = c.pluginConfig ?? {}
					return [c.name, String(c.engine), cfg.host, cfg.database, cfg.path].some(
						(field) => String(field ?? '').toLowerCase().includes(q),
					)
				})
		return [...rows].sort((a, b) => {
			if (a.favorite !== b.favorite) return a.favorite ? -1 : 1
			return (
				a.name.localeCompare(b.name, undefined, { numeric: true, sensitivity: 'base' }) ||
				a.id.localeCompare(b.id)
			)
		})
	})

	const current = $derived(filtered[Math.min(cursor, filtered.length - 1)] ?? null)

	function open(profile: ConnectionProfile | null = current) {
		if (profile && !busy) onpick(profile)
	}

	async function move(delta: number) {
		if (!filtered.length) return
		cursor = (Math.min(cursor, filtered.length - 1) + delta + filtered.length) % filtered.length
		await tick()
		listEl?.querySelector<HTMLElement>('[aria-selected="true"]')?.scrollIntoView({ block: 'nearest' })
	}

	function onKey(e: KeyboardEvent) {
		if (e.key === 'ArrowDown' || e.key === 'ArrowUp') {
			e.preventDefault()
			void move(e.key === 'ArrowDown' ? 1 : -1)
		} else if (e.key === 'Enter') {
			e.preventDefault()
			open()
		}
	}

	onMount(() => {
		void connectionProfiles.ensureLoaded()
	})
</script>

<Modal
	open
	{onclose}
	icon="plug"
	tone="accent"
	title="Open connection"
	description="Add another saved database to this workspace."
	note="↑ ↓ to move · Enter to open"
	size="md"
	dismissible={!busy}
>
	<div class="relative">
		<span class="pointer-events-none absolute top-1/2 left-3 flex -translate-y-1/2 text-fg-muted [&_svg]:size-3.5">
			<Icon name="search" />
		</span>
		<Input
			type="search"
			inset="start"
			placeholder="Filter by name, engine, host…"
			aria-label="Filter connections"
			aria-controls={LIST_ID}
			bind:value={query}
			disabled={busy}
			autofocus
			class="[&::-webkit-search-cancel-button]:hidden"
			oninput={() => (cursor = 0)}
			onkeydown={onKey}
		/>
	</div>

	<div class="-mx-2 flex min-h-[16.25rem] flex-col">
		{#if loading}
			<p class="m-0 px-3 py-10 text-center text-fg-muted">Loading…</p>
		{:else if loadError}
			<p class="m-0 px-3 py-10 text-center text-danger">{loadError}</p>
		{:else if available.length === 0}
			<div class="px-3 py-10 text-center">
				<p class="m-0 font-medium">
					{profiles.length === 0 ? 'No saved connections yet' : 'Every saved connection is already open'}
				</p>
				<p class="m-0 mt-1 text-label text-fg-muted">
					Create a profile on the home screen, then come back.
				</p>
			</div>
		{:else if filtered.length === 0}
			<p class="m-0 px-3 py-10 text-center text-fg-muted">No saved connection matches “{query.trim()}”.</p>
		{:else}
			<ul
				bind:this={listEl}
				id={LIST_ID}
				class="m-0 flex max-h-[20rem] list-none flex-col gap-0.5 overflow-auto p-0"
				role="listbox"
				aria-label="Saved connections"
			>
				{#each filtered as conn, i (conn.id)}
					{@const on = current?.id === conn.id}
					<li role="option" aria-selected={on}>
						<button
							type="button"
							tabindex="-1"
							class={cn(
								'flex w-full items-center gap-3 rounded-[10px] border-none p-2.5 text-left text-fg',
								'transition-[background-color,box-shadow] duration-150',
								on
									? 'bg-accent-soft shadow-[inset_0_0_0_1px_color-mix(in_oklab,var(--accent)_30%,transparent)]'
									: 'bg-transparent hover:bg-hover',
							)}
							disabled={busy}
							onclick={() => (cursor = i)}
							ondblclick={() => open(conn)}
						>
							<ConnectionTile engine={conn.engine} color={colorHex(conn.colorId)} size="md" />
							<span class="flex min-w-0 flex-1 flex-col gap-0.5">
								<span class="flex min-w-0 items-center gap-2">
									<span class="min-w-0 truncate font-medium">{conn.name}</span>
									{#if conn.environment === 'production'}
										<Pill tone="warning" uppercase>Prod</Pill>
									{/if}
									{#if conn.readOnly}
										<Pill tone="info" uppercase>RO</Pill>
									{/if}
									{#if conn.favorite}
										<span class="flex text-warning [&_svg]:size-3"><Icon name="star-filled" /></span>
									{/if}
								</span>
								<span class="truncate font-mono text-[0.71875rem] text-fg-muted">
									{engineLabel(conn.engine)} · {connectionTarget(conn)}
								</span>
							</span>
							{#if on}<kbd aria-hidden="true">↵</kbd>{/if}
						</button>
					</li>
				{/each}
			</ul>
		{/if}
	</div>
	{#if error}
		<p class="m-0 text-label text-danger" role="alert">{error}</p>
	{/if}

	{#snippet footer()}
		<Button onclick={onclose} disabled={busy}>Cancel</Button>
		<Button variant="primary" disabled={busy || !current} onclick={() => open()}>
			{busy ? 'Opening…' : 'Open'}
		</Button>
	{/snippet}
</Modal>
