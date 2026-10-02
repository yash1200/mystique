<script lang="ts">
	import type { ConnectionProfile } from '#core/contracts'
	import {
		Button,
		ContextMenu,
		Icon,
		IconButton,
		Input,
		Resizable,
		StatusBar,
		type ContextMenuItem,
	} from '#core/ui'
	import { SettingsModal } from '#features/settings'
	import { appShortcutsUi } from '#features/shortcuts'
	import type { ConnectionListSort } from '#lib/app'
	import {
		appPrefs,
		HOME_SIDEBAR_WIDTH_DEFAULT,
		HOME_SIDEBAR_WIDTH_MAX,
		HOME_SIDEBAR_WIDTH_MIN,
	} from '#lib/app'
	import BrandMark from './BrandMark.svelte'
	import ConnectionListItem from './ConnectionListItem.svelte'

	export type ConnectionMenuAction =
		| 'view'
		| 'connect'
		| 'edit'
		| 'duplicate'
		| 'delete'
		| 'copy-url'
		| 'favorite'

	interface Props {
		connections: ConnectionProfile[]
		selectedId: string | null
		creating: boolean
		onselect: (id: string) => void
		onnew: () => void
		onconnect: (id: string) => void
		onaction: (action: ConnectionMenuAction, id: string) => void
		onrefresh: () => void
		refreshing: boolean
		connectingId: string | null
	}

	let {
		connections,
		selectedId,
		creating,
		onselect,
		onnew,
		onconnect,
		onaction,
		onrefresh,
		refreshing,
		connectingId,
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

	let query = $state('')
	let sortMenu = $state<{ x: number; y: number } | null>(null)
	let connMenu = $state<{ id: string; x: number; y: number } | null>(null)

	const connMenuItems = $derived.by((): ContextMenuItem[] => {
		const connecting = connMenu != null && connectingId === connMenu.id
		const current = connections.find((c) => c.id === connMenu?.id)
		return [
			{ id: 'view', label: 'View' },
			{
				id: 'connect',
				label: connecting ? 'Connecting…' : 'Connect',
				disabled: connecting,
			},
			{
				id: 'favorite',
				label: current?.favorite
					? 'Remove from favourites'
					: 'Add to favourites',
			},
			{ id: 'edit', label: 'Edit', separatorBefore: true },
			{ id: 'duplicate', label: 'Duplicate' },
			{ id: 'copy-url', label: 'Copy connection URL' },
			{ id: 'delete', label: 'Delete', danger: true, separatorBefore: true },
		]
	})

	function openConnMenu(e: MouseEvent, id: string) {
		e.preventDefault()
		e.stopPropagation()
		sortMenu = null
		connMenu = { id, x: e.clientX, y: e.clientY }
	}

	function onConnMenuPick(action: string) {
		const id = connMenu?.id
		connMenu = null
		if (!id) return
		if (
			action === 'view' ||
			action === 'connect' ||
			action === 'edit' ||
			action === 'duplicate' ||
			action === 'delete' ||
			action === 'copy-url' ||
			action === 'favorite'
		) {
			onaction(action, id)
		}
	}

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
		if (a.favorite !== b.favorite) return a.favorite ? -1 : 1
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

	const widthPx = $derived(appPrefs.homeSidebarWidth)
</script>

<Resizable
	size={widthPx}
	defaultSize={HOME_SIDEBAR_WIDTH_DEFAULT}
	min={HOME_SIDEBAR_WIDTH_MIN}
	max={HOME_SIDEBAR_WIDTH_MAX}
	label="Resize connection list"
	class="h-full border-r border-line bg-panel"
	onresize={(n) => appPrefs.setHomeSidebarWidth(n, false)}
	oncommit={(n) => appPrefs.setHomeSidebarWidth(n, true)}
>
	<aside class="flex h-full min-h-0 flex-col" aria-label="Connections">
		<header class="shrink-0 border-b border-line px-3 pt-3">
			<div class="flex items-center gap-2.5 pb-2.5">
				<BrandMark size="sm" />
				<p
					class="m-0 min-w-0 truncate text-body-lg font-semibold tracking-tight text-fg"
				>
					Mystique
				</p>
			</div>

			<div class="pb-3">
				<div class="relative">
					<label class="sr-only" for="home-connection-filter"
						>Filter connections</label
					>
					<Icon
						name="search"
						class="pointer-events-none absolute top-1/2 left-2.5 size-3.5 -translate-y-1/2 text-fg-muted"
					/>
					<Input
						id="home-connection-filter"
						type="search"
						placeholder="Filter…"
						bind:value={query}
						inputSize="md"
						inset={query.length > 0 ? 'both' : 'start'}
						class="[appearance:textfield] [&::-webkit-search-cancel-button]:hidden [&::-webkit-search-decoration]:hidden"
						onkeydown={(e) => {
							if (e.key !== 'Escape' || !query) return
							e.stopPropagation()
							query = ''
						}}
					/>
					{#if query.length > 0}
						<button
							type="button"
							class="absolute top-1/2 right-1 grid size-6 -translate-y-1/2 place-items-center rounded-sm border-none bg-transparent text-fg-muted transition-colors duration-150 hover:bg-hover hover:text-fg focus-visible:outline-focus focus-visible:outline-2 focus-visible:outline-offset-1 enabled:active:scale-[0.97]"
							aria-label="Clear filter"
							onclick={() => (query = '')}
						>
							<Icon name="close" class="size-3" />
						</button>
					{/if}
				</div>
			</div>
		</header>

		<div class="flex items-center gap-2 px-3 pt-2 pb-1.5">
			<h2
				class="m-0 min-w-0 flex-1 truncate text-label font-semibold uppercase tracking-wider text-fg-muted"
			>
				Connections
			</h2>
			<div
				class="flex shrink-0 items-center gap-1"
				role="group"
				aria-label="Connection list actions"
			>
				<IconButton
					label="Refresh connections"
					variant="quiet"
					size="sm"
					disabled={refreshing}
					aria-busy={refreshing}
					class={refreshing ? 'opacity-45' : ''}
					onclick={onrefresh}
				>
					<Icon name="refresh" class={refreshing ? 'animate-spin' : ''} />
				</IconButton>
				<div class="relative">
					<IconButton
						label="Order connections"
						variant="quiet"
						size="sm"
						active={sortMenu != null}
						onclick={toggleSort}
						aria-expanded={sortMenu != null}
						aria-haspopup="menu"
					>
						<Icon name="sort" />
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
				<Button
					variant="primary"
					size="sm"
					aria-pressed={creating}
					onclick={onnew}
				>
					<Icon name="plus" class="size-3.5" />
					New
				</Button>
			</div>
		</div>

		<div
			class="flex flex-1 flex-col gap-1 overflow-auto px-2 pb-2"
			role="listbox"
			aria-label="Saved connections"
			aria-busy={refreshing || undefined}
		>
			{#if visible.length === 0}
				<div class="flex flex-col items-center px-4 py-8 text-center">
					<span
						class="mb-2 grid size-8 place-items-center rounded-lg bg-input"
						aria-hidden="true"
					>
						<Icon
							name={connections.length === 0 ? 'plus' : 'search'}
							class="size-4 text-fg-muted"
						/>
					</span>
					{#if connections.length === 0}
						<p class="m-0 text-label font-medium text-fg">No connections yet</p>
						<p class="mt-0.5 text-caption leading-relaxed text-fg-muted">
							Create one to get started.
						</p>
					{:else}
						<p class="m-0 text-label font-medium text-fg">No matches</p>
						<p class="mt-0.5 text-caption leading-relaxed text-fg-muted">
							Nothing matches “{query.trim()}”.
						</p>
					{/if}
				</div>
			{:else}
				{#each visible as conn (conn.id)}
					<div
						role="option"
						aria-selected={selectedId === conn.id && !creating}
					>
						<ConnectionListItem
							connection={conn}
							selected={selectedId === conn.id && !creating}
							connecting={connectingId === conn.id}
							query={query.trim()}
							onclick={() => onselect(conn.id)}
							ondblclick={() => onconnect(conn.id)}
							oncontextmenu={(e) => openConnMenu(e, conn.id)}
							onfavorite={() => onaction('favorite', conn.id)}
						/>
					</div>
				{/each}
			{/if}
		</div>

		<StatusBar ariaLabel="Connection list status">
			{#snippet left()}
				<span class="flex min-w-0 items-center truncate">
					{#if query.trim()}
						{visible.length} of {connections.length}
					{:else}
						{connections.length} saved
					{/if}
				</span>
			{/snippet}
			{#snippet right()}
				<IconButton
					label="Keyboard shortcuts"
					variant="quiet"
					size="sm"
					active={appShortcutsUi.open}
					onclick={() => appShortcutsUi.toggle()}
					aria-expanded={appShortcutsUi.open}
					aria-haspopup="dialog"
				>
					<Icon name="help" />
				</IconButton>
				<SettingsModal buttonSize="sm" buttonVariant="quiet" />
			{/snippet}
		</StatusBar>
	</aside>
</Resizable>

{#if connMenu}
	<ContextMenu
		x={connMenu.x}
		y={connMenu.y}
		items={connMenuItems}
		ariaLabel="Connection actions"
		onpick={onConnMenuPick}
		onclose={() => (connMenu = null)}
	/>
{/if}


