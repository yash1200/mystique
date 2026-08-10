<script lang="ts">
	import { IconButton, Input, Select, StatusBar } from '$lib/components/ui'
	import type { DatabaseMeta, TableMeta } from '$lib/types/workspace'
	import { catalogPresentation } from '$lib/types/workspace'
	import { cn } from '$lib/utils/cn'
	import CreateDatabaseForm from './explorer/CreateDatabaseForm.svelte'
	import ObjectTree from './explorer/ObjectTree.svelte'

	interface Props {
		engine: string
		databases: DatabaseMeta[]
		selectedDatabase: string
		activeTableId: string | null
		refreshing?: boolean
		modelLoading?: boolean
		onselectdatabase: (name: string) => void
		onrefresh: () => void
		oncreatedatabase: (name: string) => void
		onopen: (table: TableMeta) => void
		onopenmodel?: () => void
	}

	let {
		engine,
		databases,
		selectedDatabase,
		activeTableId,
		refreshing = false,
		modelLoading = false,
		onselectdatabase,
		onrefresh,
		oncreatedatabase,
		onopen,
		onopenmodel,
	}: Props = $props()

	let query = $state('')
	/** Schema groups start closed; membership means expanded. */
	let expanded = $state<Set<string>>(new Set())
	let creating = $state(false)
	let newDbName = $state('')
	let createError = $state<string | null>(null)

	const presentation = $derived(catalogPresentation(engine))
	const isTree = $derived(presentation.layout === 'schema-tree')

	const currentDb = $derived(
		databases.find((d) => d.name === selectedDatabase) ?? databases[0] ?? null,
	)

	const tables = $derived(currentDb?.tables ?? [])

	const dbOptions = $derived(
		databases.map((d) => ({
			value: d.name,
			label: d.name,
		})),
	)

	const filtered = $derived.by(() => {
		const q = query.trim().toLowerCase()
		if (!q) return tables
		return tables.filter(
			(t) =>
				t.name.toLowerCase().includes(q) ||
				t.schema.toLowerCase().includes(q) ||
				t.id.toLowerCase().includes(q) ||
				t.kind.toLowerCase().includes(q),
		)
	})

	const groups = $derived.by(() => {
		if (!isTree) return [] as { key: string; items: TableMeta[] }[]
		const map = new Map<string, TableMeta[]>()
		for (const t of filtered) {
			const key = t.schema || 'public'
			const list = map.get(key) ?? []
			list.push(t)
			map.set(key, list)
		}
		return [...map.entries()]
			.sort(([a], [b]) => a.localeCompare(b))
			.map(([key, items]) => ({
				key,
				items: [...items].sort((a, b) => {
					const rank = (k: string) =>
						k === 'table' ? 0 : k === 'collection' ? 1 : 2
					const d = rank(a.kind) - rank(b.kind)
					return d !== 0 ? d : a.name.localeCompare(b.name)
				}),
			}))
	})

	const flatItems = $derived.by(() => {
		if (isTree) return [] as TableMeta[]
		return [...filtered].sort((a, b) => {
			const rank = (k: string) => (k === 'table' ? 0 : k === 'view' ? 1 : 2)
			const d = rank(a.kind) - rank(b.kind)
			return d !== 0 ? d : a.name.localeCompare(b.name)
		})
	})

	const totalCount = $derived(filtered.length)

	function toggleGroup(key: string) {
		const next = new Set(expanded)
		if (next.has(key)) next.delete(key)
		else next.add(key)
		expanded = next
	}

	function isExpanded(key: string): boolean {
		// While filtering, open groups so matches are visible.
		if (query.trim()) return true
		// Keep the active table's schema open.
		if (activeTableId) {
			const hit = filtered.some(
				(t) => t.id === activeTableId && (t.schema || 'public') === key,
			)
			if (hit) return true
		}
		return expanded.has(key)
	}

	function openCreate() {
		if (!presentation.canCreateDatabase) return
		creating = true
		newDbName = ''
		createError = null
	}

	function cancelCreate() {
		creating = false
		newDbName = ''
		createError = null
	}

	function submitCreate(e: Event) {
		e.preventDefault()
		const name = newDbName.trim()
		if (!name) {
			createError = 'Enter a database name.'
			return
		}
		if (!/^[A-Za-z_][A-Za-z0-9_]*$/.test(name)) {
			createError = 'Use letters, numbers, underscore; start with a letter.'
			return
		}
		if (databases.some((d) => d.name === name)) {
			createError = 'A database with this name already exists.'
			return
		}
		oncreatedatabase(name)
		creating = false
		newDbName = ''
		createError = null
	}

	function onDbChange(value: string) {
		query = ''
		expanded = new Set()
		onselectdatabase(value)
	}
</script>

<aside
	class="flex h-full w-60 shrink-0 flex-col border-r border-line bg-panel"
	aria-label="Database objects"
>
	<div class="shrink-0 space-y-2 border-b border-line px-2 py-2">
		<div class="flex items-center gap-1.5">
			<div class="min-w-0 flex-1">
				<span class="sr-only">{presentation.databaseLabel}</span>
				<Select
					id="workspace-database"
					value={selectedDatabase}
					options={dbOptions}
					placeholder="Select database…"
					inputSize="sm"
					disabled={refreshing || dbOptions.length === 0}
					onchange={onDbChange}
				/>
			</div>
			<IconButton
				label="Refresh databases"
				size="sm"
				variant="ghost"
				disabled={refreshing}
				onclick={onrefresh}
			>
				<svg
					class={cn('size-3.5', refreshing && 'animate-spin')}
					viewBox="0 0 16 16"
					fill="none"
					aria-hidden="true"
				>
					<path
						d="M13.5 8A5.5 5.5 0 1 1 11.2 3.4"
						stroke="currentColor"
						stroke-width="1.4"
						stroke-linecap="round"
					/>
					<path
						d="M13.5 2.5v3.2h-3.2"
						stroke="currentColor"
						stroke-width="1.4"
						stroke-linecap="round"
						stroke-linejoin="round"
					/>
				</svg>
			</IconButton>
			{#if presentation.canCreateDatabase}
				<IconButton
					label="Create database"
					size="sm"
					variant="ghost"
					onclick={openCreate}
				>
					<svg
						class="size-3.5"
						viewBox="0 0 16 16"
						fill="none"
						aria-hidden="true"
					>
						<path
							d="M8 3.25v9.5M3.25 8h9.5"
							stroke="currentColor"
							stroke-width="1.4"
							stroke-linecap="round"
						/>
					</svg>
				</IconButton>
			{/if}
		</div>

		{#if creating}
			<CreateDatabaseForm
				bind:value={newDbName}
				error={createError}
				oncancel={cancelCreate}
				onsubmit={submitCreate}
			/>
		{/if}

		<Input
			type="search"
			placeholder={presentation.filterPlaceholder}
			bind:value={query}
			inputSize="sm"
		/>

		{#if onopenmodel}
			<button
				type="button"
				class={cn(
					'flex w-full items-center justify-center gap-1.5 rounded-md border px-2 py-1.5 text-label font-medium transition-colors',
					'border-line bg-elevated text-fg hover:border-accent hover:bg-accent-muted hover:text-accent',
					'disabled:cursor-not-allowed disabled:opacity-60',
				)}
				disabled={modelLoading || !currentDb}
				title="Show full ER / model diagram for this database"
				onclick={() => onopenmodel?.()}
			>
				<svg
					class="size-3.5 shrink-0"
					viewBox="0 0 16 16"
					fill="none"
					aria-hidden="true"
				>
					<rect
						x="1.5"
						y="2"
						width="5"
						height="4.5"
						rx="0.8"
						stroke="currentColor"
						stroke-width="1.2"
					/>
					<rect
						x="9.5"
						y="2"
						width="5"
						height="4.5"
						rx="0.8"
						stroke="currentColor"
						stroke-width="1.2"
					/>
					<rect
						x="5.5"
						y="9.5"
						width="5"
						height="4.5"
						rx="0.8"
						stroke="currentColor"
						stroke-width="1.2"
					/>
					<path
						d="M4 6.5v1.5M12 6.5v1.5M8 9.5V8"
						stroke="currentColor"
						stroke-width="1.2"
						stroke-linecap="round"
					/>
				</svg>
				{#if modelLoading}
					Building diagram…
				{:else}
					ER diagram
				{/if}
			</button>
		{/if}
	</div>

	<ObjectTree
		objectLabel={presentation.objectLabel}
		groupLabel={presentation.groupLabel}
		{isTree}
		{selectedDatabase}
		{currentDb}
		{totalCount}
		{query}
		{activeTableId}
		{groups}
		{flatItems}
		{isExpanded}
		onToggleGroup={toggleGroup}
		{onopen}
	/>

	<StatusBar ariaLabel="Catalog status">
		{#snippet left()}
			<span class="min-w-0 truncate">
				{#if currentDb}
					<span class="font-mono text-fg-secondary">{currentDb.name}</span>
					<span aria-hidden="true"> · </span>
				{/if}
				{totalCount}
				{presentation.objectLabel}
				{#if isTree && groups.length > 0}
					· {groups.length}
					{groups.length === 1
						? presentation.groupLabel.toLowerCase()
						: `${presentation.groupLabel.toLowerCase()}s`}
				{/if}
			</span>
		{/snippet}
	</StatusBar>
</aside>
