<script lang="ts">
	import { commands } from '#core/commands'
	import {
		ContextMenu,
		Icon,
		IconButton,
		Input,
		Mascot,
		Resizable,
		type ContextMenuItem,
	} from '#core/ui'
	import { cn } from '#core/util'
	import { SettingsModal } from '#features/settings'
	import { appShortcutsUi } from '#features/shortcuts'
	import { showToast } from '#features/toast'
	import {
		appChrome,
		appPrefs,
		cmd,
		HOME_SIDEBAR_WIDTH_DEFAULT,
		HOME_SIDEBAR_WIDTH_MAX,
		HOME_SIDEBAR_WIDTH_MIN,
	} from '#lib/app'
	import { onMount } from 'svelte'
	import { getHome, type ConnectionMenuAction } from '../home.svelte'
	import { groupConnections, isListSort, isTreatQuery, SORT_OPTIONS } from '../list'
	import ConnectionListItem from './ConnectionListItem.svelte'

	const FILTER_ID = 'home-connection-filter'

	const ENV_DOT: Record<string, string> = {
		production: 'bg-warning',
		staging: 'bg-violet',
		local: 'bg-fg-muted',
	}

	const home = getHome()

	let query = $state('')
	let sortMenu = $state<{ x: number; y: number } | null>(null)
	let connMenu = $state<{ id: string; x: number; y: number } | null>(null)
	let logoClicks = $state(0)
	let zoom = $state(false)

	const q = $derived(query.trim())
	const groups = $derived(
		groupConnections(home.connections, q, appPrefs.connectionListSort),
	)
	const shown = $derived(groups.reduce((n, g) => n + g.items.length, 0))
	const total = $derived(home.connections.length)
	const live = $derived(
		home.connections.filter((c) => c.status === 'connected').length,
	)
	const treat = $derived(isTreatQuery(q))
	const guarding = $derived(home.selected?.environment === 'production')

	const sortItems = $derived<ContextMenuItem[]>(
		SORT_OPTIONS.map((opt) => ({
			id: opt.id,
			label: opt.label,
			checked: appPrefs.connectionListSort === opt.id,
		})),
	)

	const connMenuItems = $derived.by((): ContextMenuItem[] => {
		const current = home.find(connMenu?.id)
		const connecting = !!current && home.connectingId === current.id
		return [
			{ id: 'view', label: 'View' },
			{
				id: 'connect',
				label: connecting ? 'Connecting…' : 'Connect',
				disabled: connecting,
			},
			{
				id: 'favorite',
				label: current?.favorite ? 'Remove from favourites' : 'Add to favourites',
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

	function pickConnAction(action: string) {
		const id = connMenu?.id
		connMenu = null
		if (id) home.act(action as ConnectionMenuAction, id)
	}

	function toggleSort(e: MouseEvent) {
		if (sortMenu) {
			sortMenu = null
			return
		}
		const rect = (e.currentTarget as HTMLElement).getBoundingClientRect()
		sortMenu = { x: rect.left, y: rect.bottom + 4 }
	}

	function pickSort(id: string) {
		if (isListSort(id)) appPrefs.setConnectionListSort(id)
		sortMenu = null
	}

	/** Every third click: zoomies. */
	function petLogo() {
		logoClicks += 1
		if (logoClicks % 3 !== 0) return
		zoom = false
		requestAnimationFrame(() => (zoom = true))
		setTimeout(() => (zoom = false), 950)
		showToast('success', 'Zoomies unlocked. Mystique is a very good dog.')
	}

	onMount(() =>
		commands.register([
			{
				...cmd.homeFocusFilter,
				run: () => document.getElementById(FILTER_ID)?.focus(),
			},
		]),
	)
</script>

<Resizable
	size={appPrefs.homeSidebarWidth}
	defaultSize={HOME_SIDEBAR_WIDTH_DEFAULT}
	min={HOME_SIDEBAR_WIDTH_MIN}
	max={HOME_SIDEBAR_WIDTH_MAX}
	label="Resize connection list"
	class="h-full border-r border-line bg-side"
	onresize={(n) => appPrefs.setHomeSidebarWidth(n, false)}
	oncommit={(n) => appPrefs.setHomeSidebarWidth(n, true)}
>
	<aside class="flex h-full min-h-0 flex-col" aria-label="Connections">
		<header class="flex h-topbar shrink-0 items-center gap-2.5 pr-3 pl-3.5">
			<button
				type="button"
				class="flex rounded-md border-none bg-transparent p-0"
				aria-label="Mystique"
				onclick={petLogo}
			>
				<Mascot mood={guarding ? 'alert' : 'idle'} {zoom} playful />
			</button>
			<span class="min-w-0 flex-1 truncate text-body-lg font-semibold tracking-tight">
				Mystique
			</span>
			<IconButton
				label="Hide sidebar"
				variant="quiet"
				size="sm"
				onclick={() => (appChrome.homeSidebarOpen = false)}
			>
				<Icon name="sidebar" />
			</IconButton>
		</header>

		<div class="relative mx-3 mt-1 mb-2.5 shrink-0">
			<label class="sr-only" for={FILTER_ID}>Filter connections</label>
			<Icon
				name="search"
				class="pointer-events-none absolute top-1/2 left-3 -translate-y-1/2 text-fg-muted"
			/>
			<Input
				id={FILTER_ID}
				type="search"
				placeholder="Filter connections"
				bind:value={query}
				inset="both"
				class="[&::-webkit-search-cancel-button]:hidden"
				onkeydown={(e) => {
					if (e.key !== 'Escape') return
					if (query) {
						e.stopPropagation()
						query = ''
					} else {
						e.currentTarget.blur()
					}
				}}
			/>
			<span class="absolute top-1/2 right-2 flex -translate-y-1/2">
				{#if query}
					<button
						type="button"
						class="grid size-5 place-items-center rounded-xs border-none bg-transparent text-fg-muted transition-colors hover:bg-hover hover:text-fg"
						aria-label="Clear filter"
						onclick={() => (query = '')}
					>
						<Icon name="close" class="size-3" />
					</button>
				{:else}
					<kbd aria-hidden="true">/</kbd>
				{/if}
			</span>
		</div>

		<div class="flex shrink-0 items-center gap-0.5 pt-1.5 pr-3 pb-1 pl-5">
			<h2 class="m-0 min-w-0 flex-1 truncate label-caps">
				Connections
				<span class="ml-1 font-mono tracking-normal">{total}</span>
			</h2>
			<IconButton
				label="Order connections"
				variant="quiet"
				size="sm"
				active={sortMenu != null}
				aria-haspopup="menu"
				aria-expanded={sortMenu != null}
				onclick={toggleSort}
			>
				<Icon name="sort" />
			</IconButton>
			<IconButton
				label="Refresh connections"
				variant="quiet"
				size="sm"
				disabled={home.refreshing}
				aria-busy={home.refreshing}
				onclick={() => void home.refresh()}
			>
				<Icon name="refresh" class={cn(home.refreshing && 'animate-spin')} />
			</IconButton>
			<IconButton
				label="New connection"
				variant="accent"
				size="sm"
				onclick={home.create}
			>
				<Icon name="plus" />
			</IconButton>
		</div>

		<nav
			class="flex min-h-0 flex-1 flex-col gap-1.5 overflow-auto px-2 pb-3"
			aria-label="Saved connections"
			aria-busy={home.refreshing || undefined}
		>
			{#each groups as group (group.env)}
				<section class="flex flex-col gap-px" aria-label={group.label}>
					<h3 class="m-0 flex items-center gap-2 px-3 pt-2.5 pb-1">
						<span class={cn('size-1.5 rounded-full', ENV_DOT[group.env])}></span>
						<span class="text-[0.71875rem] font-medium text-fg-muted">{group.label}</span>
					</h3>
					{#each group.items as conn (conn.id)}
						<ConnectionListItem
							connection={conn}
							selected={home.selectedId === conn.id}
							connecting={home.connectingId === conn.id}
							query={q}
							onselect={() => home.select(conn.id)}
							onopen={() => void home.connect(conn.id)}
							oncontextmenu={(e) => openConnMenu(e, conn.id)}
							onfavorite={() => void home.toggleFavorite(conn.id)}
						/>
					{/each}
				</section>
			{:else}
				<div
					class="mx-2 mt-4 flex animate-rise flex-col items-center gap-2.5 rounded-lg border border-dashed border-line-strong px-4 py-5 text-center"
				>
					<Mascot mood={treat ? 'happy' : 'sleep'} size={48} />
					{#if total === 0}
						<p class="m-0 font-medium">No connections yet</p>
						<p class="m-0 text-label text-fg-muted">Create one to get started.</p>
					{:else if treat}
						<p class="m-0 font-medium">Here’s a treat.</p>
						<p class="m-0 text-label text-fg-muted">
							Mystique approves. Now go find a real connection.
						</p>
					{:else}
						<p class="m-0 font-medium">Nothing matches “{q}”</p>
						<p class="m-0 text-label text-fg-muted">Try a name, engine or host.</p>
					{/if}
				</div>
			{/each}
		</nav>

		<footer
			class="flex h-12 shrink-0 items-center gap-1 border-t border-line pr-2.5 pl-4"
			aria-label="Connection list status"
		>
			<span class="min-w-0 flex-1 truncate text-label text-fg-muted">
				{#if q}
					{shown} of {total}
				{:else if live > 0}
					{live} connected · {total} saved
				{:else}
					{total} saved locally
				{/if}
			</span>
			<IconButton
				label="Keyboard shortcuts"
				variant="quiet"
				size="sm"
				active={appShortcutsUi.open}
				aria-haspopup="dialog"
				aria-expanded={appShortcutsUi.open}
				onclick={() => appShortcutsUi.toggle()}
			>
				<Icon name="keyboard" />
			</IconButton>
			<SettingsModal buttonSize="sm" buttonVariant="quiet" />
		</footer>
	</aside>
</Resizable>

{#if sortMenu}
	<ContextMenu
		x={sortMenu.x}
		y={sortMenu.y}
		items={sortItems}
		ariaLabel="Order connections"
		onpick={pickSort}
		onclose={() => (sortMenu = null)}
	/>
{/if}

{#if connMenu}
	<ContextMenu
		x={connMenu.x}
		y={connMenu.y}
		items={connMenuItems}
		ariaLabel="Connection actions"
		onpick={pickConnAction}
		onclose={() => (connMenu = null)}
	/>
{/if}
