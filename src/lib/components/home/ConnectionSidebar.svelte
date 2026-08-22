<script lang="ts">
	import {
		ContextMenu,
		IconButton,
		Input,
		type ContextMenuItem,
	} from '$lib/components/core'
	import {
		appPrefs,
		type ConnectionListSort,
	} from '$lib/stores/app-prefs.svelte'
	import type { ConnectionProfile } from '$lib/types/connection'
	import BrandMark from './BrandMark.svelte'
	import ConnectionListItem from './ConnectionListItem.svelte'
	import KeyboardShortcutsModal from './KeyboardShortcutsModal.svelte'
	import { SettingsModal } from './settings'

	interface Props {
		connections: ConnectionProfile[]
		selectedId: string | null
		creating: boolean
		onselect: (id: string) => void
		onnew: () => void
		onconnect: (id: string) => void
		onrefresh: () => void
		refreshing?: boolean
		connectingId?: string | null
	}

	let {
		connections,
		selectedId,
		creating,
		onselect,
		onnew,
		onconnect,
		onrefresh,
		refreshing = false,
		connectingId = null,
	}: Props = $props()

	const SORT_OPTIONS: { id: ConnectionListSort; label: string }[] = [
		{ id: 'name', label: 'Name' },
		{ id: 'created', label: 'Created' },
		{ id: 'recent', label: 'Recently used' },
		{ id: 'engine', label: 'Connection type' },
		{ id: 'environment', label: 'Environment' },
	]

	const ENV_RANK: Record<string, number> = {
		production: 0,
		staging: 1,
		local: 2,
	}

	let shortcutsOpen = $state(false)
	let query = $state('')
	let sortMenu = $state<{ x: number; y: number } | null>(null)

	function byName(a: ConnectionProfile, b: ConnectionProfile): number {
		return (
			a.name.localeCompare(b.name, undefined, {
				numeric: true,
				sensitivity: 'base',
			}) || a.id.localeCompare(b.id)
		)
	}

	function compareConnections(
		a: ConnectionProfile,
		b: ConnectionProfile,
		sort: ConnectionListSort,
	): number {
		let n = 0
		if (sort === 'name') {
			n = byName(a, b)
		} else if (sort === 'created') {
			const ta = Date.parse(a.createdAt)
			const tb = Date.parse(b.createdAt)
			n = (Number.isFinite(tb) ? tb : 0) - (Number.isFinite(ta) ? ta : 0)
		} else if (sort === 'recent') {
			const ta = a.lastConnectedAt ? Date.parse(a.lastConnectedAt) : NaN
			const tb = b.lastConnectedAt ? Date.parse(b.lastConnectedAt) : NaN
			n = (Number.isFinite(tb) ? tb : 0) - (Number.isFinite(ta) ? ta : 0)
		} else if (sort === 'engine') {
			n = String(a.engine).localeCompare(String(b.engine), undefined, {
				sensitivity: 'base',
			})
		} else {
			n = (ENV_RANK[a.environment] ?? 99) - (ENV_RANK[b.environment] ?? 99)
		}
		if (n !== 0) return n
		return byName(a, b)
	}

	const sortItems = $derived.by((): ContextMenuItem[] =>
		SORT_OPTIONS.map((opt) => ({
			id: opt.id,
			label: opt.label,
			checked: appPrefs.connectionListSort === opt.id,
		})),
	)

	function toggleSort(e: MouseEvent) {
		if (sortMenu) {
			sortMenu = null
			return
		}
		const el = e.currentTarget
		if (!(el instanceof HTMLElement)) return
		const rect = el.getBoundingClientRect()
		sortMenu = { x: rect.right, y: rect.bottom }
	}

	function setSort(id: string) {
		if (
			id === 'name' ||
			id === 'created' ||
			id === 'recent' ||
			id === 'engine' ||
			id === 'environment'
		) {
			appPrefs.setConnectionListSort(id)
		}
		sortMenu = null
	}

	const visible = $derived.by(() => {
		const q = query.trim().toLowerCase()
		const rows = !q
			? connections
			: connections.filter((c) => {
					const cfg = c.pluginConfig ?? {}
					const host = String(cfg.host ?? '').toLowerCase()
					const database = String(cfg.database ?? '').toLowerCase()
					const path = String(cfg.path ?? '').toLowerCase()
					return (
						c.name.toLowerCase().includes(q) ||
						host.includes(q) ||
						database.includes(q) ||
						path.includes(q) ||
						String(c.engine).toLowerCase().includes(q)
					)
				})
		return [...rows].sort((a, b) =>
			compareConnections(a, b, appPrefs.connectionListSort),
		)
	})
</script>

<aside
	class="flex h-full w-sidebar min-w-60 max-w-panel-sm flex-col border-r border-line bg-panel"
	aria-label="Connections"
>
	<header class="px-4 pt-4 pb-3">
		<div class="flex items-center gap-3">
			<BrandMark />
			<div>
				<div class="text-body-lg font-semibold tracking-tight text-fg">
					Mystique <span class="font-mono text-label font-medium text-fg-muted"
						>DB</span
					>
				</div>
			</div>
		</div>
	</header>

	<div class="flex items-center gap-2 px-3 pb-3">
		<div class="min-w-0 flex-1">
			<span class="sr-only">Filter connections</span>
			<Input
				type="search"
				placeholder="Filter…"
				bind:value={query}
				inputSize="md"
			/>
		</div>
		<IconButton
			label="New connection"
			variant="accent"
			size="md"
			active={creating}
			onclick={onnew}
		>
			<svg class="size-5" viewBox="0 0 16 16" fill="none" aria-hidden="true">
				<path
					d="M8 3.25v9.5M3.25 8h9.5"
					stroke="currentColor"
					stroke-width="1.6"
					stroke-linecap="round"
				/>
			</svg>
		</IconButton>
	</div>

	<div class="flex items-center justify-between gap-2 px-3 pb-1.5">
		<h2
			class="m-0 min-w-0 truncate text-label font-semibold uppercase tracking-wider text-fg-muted"
		>
			Connections
		</h2>
		<div class="relative flex shrink-0 items-center gap-0.5">
			<IconButton
				label="Refresh connections"
				variant="ghost"
				size="sm"
				disabled={refreshing}
				aria-busy={refreshing}
				class={refreshing ? 'opacity-45' : ''}
				onclick={onrefresh}
			>
				<svg
					class="size-3.5"
					viewBox="0 0 16 16"
					fill="none"
					aria-hidden="true"
				>
					<path
						d="M12.7 6.2A4.85 4.85 0 1 0 13.1 9"
						stroke="currentColor"
						stroke-width="1.3"
						stroke-linecap="round"
					/>
					<path
						d="M12.15 3.15v3.15h-3.1"
						stroke="currentColor"
						stroke-width="1.3"
						stroke-linecap="round"
						stroke-linejoin="round"
					/>
				</svg>
			</IconButton>
			<IconButton
				label="Order connections"
				variant="ghost"
				size="sm"
				active={sortMenu != null}
				onclick={toggleSort}
				aria-expanded={sortMenu != null}
				aria-haspopup="menu"
			>
				<svg
					class="size-3.5"
					viewBox="0 0 16 16"
					fill="none"
					aria-hidden="true"
				>
					<path
						d="M3 4.25h10M4.25 8h7.5M5.75 11.75h4.5"
						stroke="currentColor"
						stroke-width="1.3"
						stroke-linecap="round"
					/>
				</svg>
			</IconButton>
			{#if sortMenu}
				<ContextMenu
					x={sortMenu.x}
					y={sortMenu.y}
					items={sortItems}
					ariaLabel="Order connections"
					onpick={setSort}
					onclose={() => (sortMenu = null)}
				/>
			{/if}
		</div>
	</div>

	<div
		class="flex flex-1 flex-col gap-1.5 overflow-auto px-2 pb-2"
		role="listbox"
		aria-label="Saved connections"
	>
		{#if visible.length === 0}
			<p class="m-4 text-label leading-relaxed text-fg-muted">
				{#if connections.length === 0}
					No connections yet. Create one to get started.
				{:else}
					No matches for “{query}”.
				{/if}
			</p>
		{:else}
			{#each visible as conn (conn.id)}
				<div role="option" aria-selected={selectedId === conn.id && !creating}>
					<ConnectionListItem
						connection={conn}
						selected={selectedId === conn.id && !creating}
						connecting={connectingId === conn.id}
						onclick={() => onselect(conn.id)}
						ondblclick={() => onconnect(conn.id)}
					/>
				</div>
			{/each}
		{/if}
	</div>

	<footer
		class="relative flex items-center justify-between gap-2 border-t border-line px-3 py-2"
	>
		<span class="min-w-0 truncate text-caption text-fg-muted">
			{connections.length} saved
		</span>
		<div class="flex shrink-0 items-center gap-0.5">
			<IconButton
				label="Keyboard shortcuts"
				variant="ghost"
				size="sm"
				active={shortcutsOpen}
				onclick={() => (shortcutsOpen = !shortcutsOpen)}
				aria-expanded={shortcutsOpen}
				aria-haspopup="dialog"
			>
				<svg
					class="size-3.5"
					viewBox="0 0 16 16"
					fill="none"
					aria-hidden="true"
				>
					<circle
						cx="8"
						cy="8"
						r="5.75"
						stroke="currentColor"
						stroke-width="1.2"
					/>
					<path
						d="M6.35 6.15c0-.95.75-1.65 1.65-1.65.9 0 1.65.7 1.65 1.6 0 .75-.45 1.15-1.05 1.5-.4.25-.6.5-.6 1.05"
						stroke="currentColor"
						stroke-width="1.2"
						stroke-linecap="round"
					/>
					<circle cx="8" cy="11.15" r="0.7" fill="currentColor" />
				</svg>
			</IconButton>
			<SettingsModal buttonSize="sm" />
		</div>
	</footer>
</aside>

<KeyboardShortcutsModal
	open={shortcutsOpen}
	onclose={() => (shortcutsOpen = false)}
/>
