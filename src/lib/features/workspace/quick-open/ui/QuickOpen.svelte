<script lang="ts">
	import { EmptyHint, Icon, pushDialog } from '#core/ui'
	import { focusTrap } from '#core/ui/focus-trap.js'
	import { cn, modalPanel, overlayFade } from '#core/util'
	import { adapterFor, engineLabel, resolveCapabilities } from '#engines'
	import { colorHex, ConnectionTile, connectionProfiles } from '#features/connections'
	import { isMacPlatform, keyCap } from '#features/shortcuts'
	import { CatalogIcon, catalogStore, type CatalogIconName } from '#features/workspace/catalog'
	import type { IconName } from '#icons'
	import { historyStore, savedStore } from '#features/workspace/query'
	import { workspaceSession } from '#features/workspace/session'
	import { tick, untrack } from 'svelte'
	import { runQuickOpenItem } from '../actions'
	import { quickOpen } from '../chrome.svelte'
	import { SearchGeneration } from '../generation'
	import { warmSessionCatalogs } from '../warm'
	import {
		collectQuickOpenItems,
		groupQuickOpenItems,
		QUICK_OPEN_PER_GROUP,
		quickOpenKindLabel,
		rankQuickOpenItems,
		type QuickOpenItem,
		type QuickOpenProfile,
		type QuickOpenSource,
	} from '../items'

	const generation = new SearchGeneration()
	let active = $state(0)
	let listEl = $state<HTMLDivElement | null>(null)
	let inputEl = $state<HTMLInputElement | null>(null)

	$effect(() => {
		if (!quickOpen.open) return
		return pushDialog({ close: () => quickOpen.hide(true), dismissible: () => true })
	})

	const source = $derived.by((): QuickOpenSource => {
		if (!quickOpen.open) {
			return {
				activeConnectionId: null,
				profiles: [],
				catalogs: [],
				saved: [],
				history: [],
				inWorkspace: false,
				supportsMetrics: false,
			}
		}
		const activeId = workspaceSession.activeConnectionId
		const sessions = workspaceSession.sessions
		const activeSession = sessions.find(
			(session) => session.profile.id === activeId,
		)
		const profiles = connectionProfiles.items.map(toProfile)
		const orderedIds = [
			...(activeId ? [activeId] : []),
			...sessions
				.map((session) => session.profile.id)
				.filter((id) => id !== activeId),
		]
		const catalogs: QuickOpenSource['catalogs'] = []
		for (const id of orderedIds) {
			const session = sessions.find((row) => row.profile.id === id)
			if (!session) continue
			const profile =
				profiles.find((row) => row.id === id) ?? toProfile(session.profile)
			const databases = catalogStore.databases(id)
			catalogs.push({
				connectionId: id,
				name: profile.name,
				colorId: profile.colorId,
				mark: profile.mark,
				tables: databases.flatMap((database) => database.tables),
			})
		}
		return {
			activeConnectionId: activeId,
			profiles,
			inWorkspace: sessions.length > 0,
			supportsMetrics: activeSession
				? resolveCapabilities(
						String(activeSession.profile.engine),
						activeSession.capabilities,
					).supportsServerMetrics
				: false,
			catalogs,
			saved: savedStore.of(activeId).items.flatMap((query) => {
				const connectionId = query.connectionId || activeId
				if (!connectionId) return []
				return [
					{
						id: query.id,
						connectionId,
						name: query.name,
						body: query.sql,
					},
				]
			}),
			history: historyStore.of(activeId).entries.map((entry) => ({
				id: entry.id,
				connectionId: entry.connectionId ?? null,
				title: historyTitle(entry.sql),
				body: entry.sql,
				connectionName: entry.connectionName ?? '',
			})),
		}
	})

	$effect(() => {
		if (!quickOpen.open) return
		const el = inputEl
		void connectionProfiles.ensureLoaded()
		void tick().then(() => el?.focus())
	})

	// Open sessions whose catalog never loaded (never shown this run) load
	// it now, two at a time, so their tables can be found. Closing stops
	// the queue; loads already running still fill the store.
	$effect(() => {
		if (!quickOpen.open) return
		const ids = untrack(() => workspaceSession.sessions.map((s) => s.profile.id))
		const stop = new AbortController()
		void warmSessionCatalogs(ids, stop.signal)
		return () => stop.abort()
	})

	const groups = $derived.by(() => {
		if (!quickOpen.open) return []
		const searching = quickOpen.query.trim().length > 0
		const items = collectQuickOpenItems(source)
		if (!searching) return groupQuickOpenItems(rankQuickOpenItems(items, '', 50))
		return groupQuickOpenItems(
			rankQuickOpenItems(items, quickOpen.query, items.length),
			QUICK_OPEN_PER_GROUP,
			60,
		)
	})
	/** Rows in display order; arrow keys and indexes follow it. */
	const ranked = $derived(groups.flatMap((group) => group.items))
	const startOf = $derived.by(() => {
		const starts: number[] = []
		let at = 0
		for (const group of groups) {
			starts.push(at)
			at += group.items.length
		}
		return starts
	})
	const cursor = $derived(
		ranked.length === 0 ? 0 : Math.min(active, ranked.length - 1),
	)

	function onQueryInput() {
		const token = generation.next()
		queueMicrotask(() => {
			if (!generation.isCurrent(token)) return
			active = 0
		})
	}

	$effect(() => {
		const index = cursor
		const root = listEl
		if (!root) return
		const row = root.querySelector(`[data-index="${index}"]`)
		if (row instanceof HTMLElement) {
			row.scrollIntoView({ block: 'nearest' })
		}
	})

	function close(restore: boolean) {
		quickOpen.hide(restore)
	}

	function onBackdrop() {
		close(true)
	}

	function onKeydown(event: KeyboardEvent) {
		if (!quickOpen.open) return
		if (event.key === 'Escape') {
			event.preventDefault()
			event.stopImmediatePropagation()
			close(true)
			return
		}
		if (event.key === 'ArrowDown') {
			event.preventDefault()
			if (ranked.length === 0) return
			active = (cursor + 1) % ranked.length
			return
		}
		if (event.key === 'ArrowUp') {
			event.preventDefault()
			if (ranked.length === 0) return
			active = (cursor - 1 + ranked.length) % ranked.length
			return
		}
		if (event.key === 'Home') {
			event.preventDefault()
			active = 0
			return
		}
		if (event.key === 'End') {
			event.preventDefault()
			active = Math.max(0, ranked.length - 1)
			return
		}
		if (event.key === 'Enter') {
			event.preventDefault()
			const item = ranked[cursor]
			if (item) void choose(item)
		}
	}

	async function choose(item: QuickOpenItem) {
		close(false)
		await runQuickOpenItem(item)
	}

	const mac = isMacPlatform()

	/** `Mod+Shift+[` → one cap per key. */
	function caps(chord: string): string[] {
		return chord.split('+').map((key) => keyCap(key, 'glyphs', mac))
	}

	const COMMAND_ICONS: Record<string, IconName> = {
		'new-query': 'plus',
		'prev-tab': 'chevron-left',
		'next-tab': 'chevron-right',
		'focus-filter': 'search',
		'toggle-rail': 'sidebar',
		metrics: 'metrics',
	}

	function itemIcon(item: QuickOpenItem): IconName {
		if (item.kind === 'command') return COMMAND_ICONS[item.command ?? ''] ?? 'bolt'
		if (item.kind === 'saved') return 'bookmark'
		return 'history'
	}

	function catalogIcon(kind: string | undefined): CatalogIconName {
		if (kind === 'view') return 'view'
		if (kind === 'collection') return 'collection'
		return 'table'
	}

	function historyTitle(sql: string): string {
		const line = sql.replace(/\s+/g, ' ').trim()
		if (line.length <= 80) return line || 'Query'
		return `${line.slice(0, 79)}…`
	}

	function toProfile(profile: {
		id: string
		name: string
		engine: string
		colorId: string
		environment: string
	}): QuickOpenProfile {
		return {
			id: profile.id,
			name: profile.name,
			engine: String(profile.engine),
			colorId: profile.colorId,
			environment: profile.environment,
			mark: adapterFor(String(profile.engine)).mark,
			engineLabel: engineLabel(String(profile.engine)),
		}
	}
</script>

<svelte:window onkeydowncapture={onKeydown} />

{#if quickOpen.open}
	<div
		class="fixed inset-0 z-60 flex items-start justify-center px-4 pt-[12vh]"
		role="presentation"
	>
		<button
			type="button"
			class="absolute inset-0 border-none bg-scrim"
			aria-label="Dismiss quick open"
			transition:overlayFade|global
			onclick={onBackdrop}
		></button>
		<div
			class="relative z-10 flex max-h-[min(34rem,72vh)] w-full max-w-[38rem] flex-col overflow-hidden rounded-xl border border-line bg-raised shadow-lg"
			role="dialog"
			aria-modal="true"
			aria-label="Quick open"
			transition:modalPanel|global
			use:focusTrap={() => quickOpen.open}
		>
			<div class="flex h-13 shrink-0 items-center gap-3 border-b border-line px-4">
				<span class="flex text-fg-muted [&_svg]:size-4" aria-hidden="true"><Icon name="search" /></span>
				<input
					bind:this={inputEl}
					bind:value={quickOpen.query}
					class="h-full min-w-0 flex-1 border-none bg-transparent p-0 text-body-lg text-fg outline-none placeholder:text-fg-muted focus-visible:outline-none"
					role="combobox"
					aria-autocomplete="list"
					aria-expanded="true"
					aria-controls="quick-open-list"
					aria-activedescendant={ranked[cursor] ? `quick-open-opt-${cursor}` : undefined}
					placeholder="Search connections, tables, queries and commands…"
					autocomplete="off"
					spellcheck="false"
					oninput={onQueryInput}
				/>
				<kbd aria-hidden="true">esc</kbd>
			</div>

			<div
				bind:this={listEl}
				id="quick-open-list"
				class="min-h-0 flex-1 overflow-y-auto px-2 pb-2"
				role="listbox"
				aria-label="Quick open results"
			>
				{#if ranked.length === 0}
					<EmptyHint
						icon="search"
						title="No matches"
						description={quickOpen.query.trim()
							? `Nothing matches “${quickOpen.query.trim()}”.`
							: 'Open a connection to search its tables.'}
					/>
				{:else}
					{#each groups as group, g (group.id)}
						<div role="group" aria-labelledby="quick-open-group-{g}">
							<div id="quick-open-group-{g}" class="label-caps flex items-center gap-2 px-2.5 pt-3 pb-1.5">
								{#if group.colorId}
									<span
										class="size-2 shrink-0 rounded-full"
										style:background={colorHex(group.colorId)}
										aria-hidden="true"
									></span>
								{/if}
								<span class="truncate">{group.label}</span>
							</div>
							{#each group.items as item, i (item.id)}
								{@const index = (startOf[g] ?? 0) + i}
								{@const on = index === cursor}
								<button
									type="button"
									id="quick-open-opt-{index}"
									data-index={index}
									role="option"
									tabindex="-1"
									aria-selected={on}
									class={cn(
										'flex h-10 w-full items-center gap-3 rounded-[9px] border-none px-2.5 text-left text-fg transition-colors duration-100',
										on ? 'bg-hover' : 'bg-transparent',
									)}
									onpointermove={() => (active = index)}
									onclick={() => void choose(item)}
								>
									{#if item.kind === 'connection' && item.engine && item.colorId}
										<ConnectionTile engine={item.engine} color={colorHex(item.colorId)} />
									{:else}
										<span
											class={cn(
												'grid size-7 shrink-0 place-items-center rounded-md bg-hover shadow-[inset_0_0_0_1px_var(--line)] [&_svg]:size-3.5',
												on ? 'text-accent-text' : 'text-fg-secondary',
											)}
											aria-hidden="true"
										>
											{#if item.kind === 'catalog'}
												<CatalogIcon name={catalogIcon(item.objectKind)} active={on} />
											{:else}
												<Icon name={itemIcon(item)} />
											{/if}
										</span>
									{/if}
									<span class="flex min-w-0 flex-1 items-baseline gap-2">
										<span class={cn('min-w-0 truncate text-body', item.kind === 'catalog' && 'font-mono')}>
											{item.title}
										</span>
										{#if item.detail && !item.keys}
											<span class="min-w-0 flex-1 truncate text-label text-fg-muted">{item.detail}</span>
										{/if}
									</span>
									{#if item.keys}
										<span class="flex shrink-0 gap-1" aria-label={item.keys}>
											{#each caps(item.keys) as cap, c (c)}<kbd>{cap}</kbd>{/each}
										</span>
									{:else if on}
										<kbd class="shrink-0" aria-hidden="true">↵</kbd>
									{:else}
										<span class="shrink-0 text-caption text-fg-muted">{quickOpenKindLabel(item)}</span>
									{/if}
								</button>
							{/each}
						</div>
					{/each}
				{/if}
			</div>

			<footer class="flex h-9 shrink-0 items-center gap-4 border-t border-line bg-surface px-4 text-caption text-fg-muted">
				<span class="flex items-center gap-1.5"><kbd>↑</kbd><kbd>↓</kbd> move</span>
				<span class="flex items-center gap-1.5"><kbd>↵</kbd> open</span>
				<span class="flex items-center gap-1.5"><kbd>esc</kbd> close</span>
				<span class="ml-auto tabular-nums">
					{ranked.length}
					{ranked.length === 1 ? 'result' : 'results'}
				</span>
			</footer>
		</div>
	</div>
{/if}
