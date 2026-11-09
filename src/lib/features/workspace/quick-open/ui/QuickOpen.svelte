<script lang="ts">
	import { pushDialog } from '#core/ui'
	import { focusTrap } from '#core/ui/focus-trap.js'
	import { adapterFor, engineLabel, resolveCapabilities } from '#engines'
	import { colorHex, connectionProfiles } from '#features/connections'
	import { catalogStore } from '#features/workspace/catalog'
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
		class="fixed inset-0 z-60 flex items-start justify-center px-4 pt-[10vh]"
		role="presentation"
	>
		<button
			type="button"
			class="absolute inset-0 border-none bg-black/45"
			aria-label="Dismiss quick open"
			onclick={onBackdrop}
		></button>
		<div
			class="relative z-10 flex max-h-[min(32rem,70vh)] w-full max-w-xl flex-col overflow-hidden rounded-lg border border-line bg-elevated shadow-lg"
			role="dialog"
			aria-modal="true"
			aria-label="Quick open"
			use:focusTrap={() => quickOpen.open}
		>
			<div class="border-b border-line p-2">
				<input
					bind:this={inputEl}
					bind:value={quickOpen.query}
					class="h-control w-full rounded-md border border-line bg-input px-2.5 text-body text-fg placeholder:text-fg-muted focus:outline-none focus-visible:border-focus focus-visible:ring-2 focus-visible:ring-accent-muted"
					role="combobox"
					aria-autocomplete="list"
					aria-expanded="true"
					aria-controls="quick-open-list"
					aria-activedescendant={ranked[cursor]
						? `quick-open-opt-${cursor}`
						: undefined}
					placeholder="Connections, tables, queries"
					autocomplete="off"
					spellcheck="false"
					oninput={onQueryInput}
				/>
			</div>
			<div
				bind:this={listEl}
				id="quick-open-list"
				class="min-h-0 flex-1 overflow-y-auto p-1"
				role="listbox"
				aria-label="Quick open results"
			>
				{#if ranked.length === 0}
					<p class="px-2 py-6 text-center text-label text-fg-muted">
						No matches
					</p>
				{:else}
					{#each groups as group, g (group.id)}
						<div
							role="group"
							aria-labelledby="quick-open-group-{g}"
							class={g > 0 ? 'mt-1 border-t border-line pt-1' : ''}
						>
							<div
								id="quick-open-group-{g}"
								class="flex items-center gap-1.5 px-2 pb-0.5 pt-1 text-micro font-semibold uppercase tracking-wider text-fg-muted"
							>
								{#if group.colorId}
									<span
										class="size-1.5 shrink-0 rounded-full"
										style="background: {colorHex(group.colorId)}"
										aria-hidden="true"
									></span>
								{/if}
								<span class="truncate">{group.label}</span>
							</div>
							{#each group.items as item, i (item.id)}
								{@const index = (startOf[g] ?? 0) + i}
								<button
									type="button"
									id="quick-open-opt-{index}"
									data-index={index}
									role="option"
									tabindex="-1"
									aria-selected={index === cursor}
									class="flex w-full items-center gap-2 rounded-md px-2 py-1.5 text-left {index ===
									cursor
										? 'bg-accent-muted'
										: 'hover:bg-hover'}"
									onpointerenter={() => (active = index)}
									onclick={() => void choose(item)}
								>
									{#if item.colorId}
										<span
											class="size-2 shrink-0 rounded-full"
											style="background: {colorHex(item.colorId)}"
											aria-hidden="true"
										></span>
									{:else}
										<span class="size-2 shrink-0" aria-hidden="true"></span>
									{/if}
									<span
										class="w-8 shrink-0 truncate text-caption font-medium text-fg-muted"
										>{item.mark}</span
									>
									<span class="min-w-0 flex-1">
										<span class="block truncate text-label text-fg"
											>{item.title}</span
										>
										<span class="block truncate text-caption text-fg-muted"
											>{item.detail}</span
										>
									</span>
									<span class="shrink-0 text-caption text-fg-muted"
										>{quickOpenKindLabel(item)}</span
									>
								</button>
							{/each}
						</div>
					{/each}
				{/if}
			</div>
			<p class="border-t border-line px-3 py-1.5 text-caption text-fg-muted">
				↑↓ move · Enter open · Esc close
			</p>
		</div>
	</div>
{/if}
