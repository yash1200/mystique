<script lang="ts">
	import {
		Button,
		ContextMenu,
		IconButton,
		Input,
		Select,
		StatusBar,
		type ContextMenuItem,
	} from '$lib/components/ui'
	import {
		defaultCapabilitiesForEngine,
		presentationFromCaps,
		type EngineCapabilities,
	} from '$lib/types/engineCaps'
	import type { DatabaseMeta, TableMeta } from '$lib/types/workspace'
	import { cn } from '$lib/utils/cn'
	import CreateDatabaseForm from './explorer/CreateDatabaseForm.svelte'
	import ObjectTree from './explorer/ObjectTree.svelte'
	import SaveQueryModal from './SaveQueryModal.svelte'

	export type TableExplorerAction =
		| 'open'
		| 'erd'
		| 'structure'
		| 'rename'
		| 'drop'
		| 'truncate'
		| 'duplicate'
		| 'create'

	function defaultCopyName(table: TableMeta): string {
		return `${table.name}_copy`
	}

	interface Props {
		engine: string
		databases: DatabaseMeta[]
		selectedDatabase: string
		activeTableId: string | null
		refreshing?: boolean
		modelLoading?: boolean
		readOnly?: boolean
		capabilities?: EngineCapabilities | null
		onselectdatabase: (name: string) => void
		onrefresh: () => void
		oncreatedatabase: (name: string) => void
		onopen: (table: TableMeta) => void
		onopenmodel?: () => void
		ontableaction?: (
			action: TableExplorerAction,
			table: TableMeta,
			extra?: string,
		) => void
	}

	let {
		engine,
		databases,
		selectedDatabase,
		activeTableId,
		refreshing = false,
		modelLoading = false,
		readOnly = false,
		capabilities = null,
		onselectdatabase,
		onrefresh,
		oncreatedatabase,
		onopen,
		onopenmodel,
		ontableaction,
	}: Props = $props()

	let query = $state('')
	/** `public` / `main` start open; other schemas stay collapsed until clicked. */
	let expanded = $state<Set<string>>(new Set(['public', 'main']))
	let creating = $state(false)
	let newDbName = $state('')
	let createError = $state<string | null>(null)

	const caps = $derived(capabilities ?? defaultCapabilitiesForEngine(engine))
	const presentation = $derived(presentationFromCaps(caps))
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

	function submitCreate(name: string) {
		const trimmed = name.trim()
		if (!trimmed) {
			createError = 'Enter a database name.'
			return
		}
		if (!/^[A-Za-z_][A-Za-z0-9_]*$/.test(trimmed)) {
			createError = 'Use letters, numbers, underscore; start with a letter.'
			return
		}
		if (databases.some((d) => d.name === trimmed)) {
			createError = 'A database with this name already exists.'
			return
		}
		oncreatedatabase(trimmed)
		creating = false
		newDbName = ''
		createError = null
	}

	function onDbChange(value: string) {
		query = ''
		expanded = new Set()
		onselectdatabase(value)
	}

	let tableMenu = $state<{
		table: TableMeta
		x: number
		y: number
	} | null>(null)
	let namePrompt = $state<{
		action: 'rename' | 'duplicate'
		table: TableMeta
	} | null>(null)
	let confirmPrompt = $state<{
		action: 'drop' | 'truncate'
		table: TableMeta
	} | null>(null)

	const noun = $derived(caps.explorer.objectLabelSingular || 'table')
	const admin = $derived(caps.tableAdmin)
	const showModel = $derived(caps.supportsModelView || caps.supportsErDiagram)

	const tableMenuItems = $derived.by((): ContextMenuItem[] => {
		const t = tableMenu?.table
		if (!t) return []
		const isView = t.kind === 'view'
		// Plugin config is the only feature gate. Do not grey out collection
		// actions — read-only is enforced when the plugin runs the op.
		const items: ContextMenuItem[] = [{ id: 'open', label: 'Open' }]
		if (admin.viewStructure) {
			items.push({
				id: 'structure',
				label: 'View structure',
			})
		}
		if (showModel) {
			items.push({
				id: 'erd',
				label: caps.supportsErDiagram ? 'View ERD' : 'View model',
			})
		}
		if (admin.rename) {
			items.push({
				id: 'rename',
				label: 'Rename',
				separatorBefore: true,
			})
		}
		if (admin.duplicate && !isView) {
			items.push({
				id: 'duplicate',
				label: 'Duplicate',
				separatorBefore: !admin.rename,
			})
		}
		if (admin.createStatement) {
			items.push({
				id: 'create',
				label: 'View Create',
				separatorBefore: true,
			})
		}
		if (admin.truncate && !isView) {
			items.push({
				id: 'truncate',
				label: 'Truncate',
				separatorBefore: true,
			})
		}
		if (admin.drop) {
			items.push({
				id: 'drop',
				label: 'Drop',
				danger: true,
			})
		}
		return items
	})

	function openTableMenu(table: TableMeta, e: MouseEvent) {
		tableMenu = { table, x: e.clientX, y: e.clientY }
	}

	function onMenuPick(id: string) {
		const ctx = tableMenu
		tableMenu = null
		if (!ctx || !ontableaction) return
		const { table } = ctx
		if (id === 'open') {
			ontableaction('open', table)
			return
		}
		if (id === 'erd') {
			ontableaction('erd', table)
			return
		}
		if (id === 'structure') {
			ontableaction('structure', table)
			return
		}
		if (id === 'create') {
			ontableaction('create', table)
			return
		}
		if (id === 'rename') {
			namePrompt = { action: 'rename', table }
			return
		}
		if (id === 'duplicate') {
			namePrompt = { action: 'duplicate', table }
			return
		}
		if (id === 'drop' || id === 'truncate') {
			confirmPrompt = { action: id, table }
		}
	}

	function submitName(name: string) {
		const prompt = namePrompt
		namePrompt = null
		if (!prompt || !ontableaction) return
		ontableaction(prompt.action, prompt.table, name)
	}

	function confirmDanger() {
		const prompt = confirmPrompt
		confirmPrompt = null
		if (!prompt || !ontableaction) return
		ontableaction(prompt.action, prompt.table)
	}
</script>

<aside
	class="flex h-full w-full min-w-0 flex-col bg-panel"
	aria-label="Database objects"
>
	<div class="shrink-0 space-y-1.5 border-b border-line px-2 py-2">
		<div class="flex items-center gap-1">
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
			{#if onopenmodel && showModel}
				<IconButton
					label={caps.supportsErDiagram
						? 'ER diagram for this database'
						: 'Model for this database'}
					size="sm"
					variant="ghost"
					disabled={modelLoading || !currentDb}
					onclick={() => onopenmodel?.()}
				>
					<svg
						class={cn('size-3.5', modelLoading && 'opacity-50')}
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
				</IconButton>
			{/if}
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

		<Input
			type="search"
			placeholder={presentation.filterPlaceholder}
			bind:value={query}
			inputSize="sm"
		/>
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
		onmenu={openTableMenu}
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

{#if tableMenu}
	<ContextMenu
		x={tableMenu.x}
		y={tableMenu.y}
		items={tableMenuItems}
		ariaLabel="{tableMenu.table.name} actions"
		onpick={onMenuPick}
		onclose={() => {
			tableMenu = null
		}}
	/>
{/if}

<CreateDatabaseForm
	open={creating}
	bind:value={newDbName}
	error={createError}
	databaseLabel={presentation.databaseLabel}
	oncancel={cancelCreate}
	onconfirm={submitCreate}
/>

<SaveQueryModal
	open={!!namePrompt}
	initialName={namePrompt
		? namePrompt.action === 'duplicate'
			? defaultCopyName(namePrompt.table)
			: namePrompt.table.name
		: ''}
	title={namePrompt?.action === 'duplicate'
		? `Duplicate ${noun}`
		: `Rename ${noun}`}
	description={namePrompt?.action === 'duplicate'
		? `Create a copy of “${namePrompt.table.name}” on this database.`
		: `Rename “${namePrompt?.table.name ?? ''}” on this database.`}
	confirmLabel={namePrompt?.action === 'duplicate' ? 'Duplicate' : 'Rename'}
	onconfirm={submitName}
	oncancel={() => {
		namePrompt = null
	}}
/>

{#if confirmPrompt}
	<!-- svelte-ignore a11y_no_static_element_interactions -->
	<div
		class="fixed inset-0 z-50 flex items-center justify-center bg-black/45 p-4"
		role="presentation"
		onclick={(e) => {
			if (e.target === e.currentTarget) confirmPrompt = null
		}}
		onkeydown={(e) => {
			if (e.key === 'Escape') confirmPrompt = null
		}}
	>
		<div
			class="w-full max-w-md rounded-lg border border-line bg-elevated p-4 shadow-lg"
			role="dialog"
			aria-modal="true"
			aria-labelledby="table-confirm-title"
		>
			<h2 id="table-confirm-title" class="m-0 text-body font-semibold text-fg">
				{confirmPrompt.action === 'drop' ? 'Drop' : 'Truncate'}
				{noun}
			</h2>
			<p class="mt-2 mb-0 text-label leading-relaxed text-fg-secondary">
				{confirmPrompt.action === 'drop'
					? `Drop “${confirmPrompt.table.name}”? This runs on the database and cannot be undone.`
					: `Truncate “${confirmPrompt.table.name}”? This deletes all rows on the database and cannot be undone.`}
			</p>
			<div class="mt-4 flex flex-wrap items-center justify-end gap-2">
				<Button
					variant="ghost"
					size="sm"
					onclick={() => {
						confirmPrompt = null
					}}>Cancel</Button
				>
				<Button variant="danger" size="sm" onclick={confirmDanger}>
					{confirmPrompt.action === 'drop' ? 'Drop' : 'Truncate'}
				</Button>
			</div>
		</div>
	</div>
{/if}
