<script lang="ts">
	import type {
		ConnectionProfile,
		DatabaseMeta,
		EngineCapabilities,
		TableMeta,
	} from '#core/contracts'
	import {
		isProductionDatabase,
		markAction,
		productionLabel,
		ProductionMark,
	} from '#features/connections'
	import {
		ContextMenu,
		Icon,
		IconButton,
		Select,
		type ContextMenuItem,
	} from '#core/ui'
	import { cn } from '#core/util'
	import { defaultCapabilitiesForEngine, presentationFromCaps } from '#engines'
	import { CatalogIcon } from '#features/workspace/catalog'
	import { SaveQueryModal } from '#features/workspace/query'
	import CreateDatabaseForm from './CreateDatabaseForm.svelte'
	import ExplorerSearch from './ExplorerSearch.svelte'
	import ObjectTree from './ObjectTree.svelte'
	export type TableExplorerAction =
		| 'open'
		| 'erd'
		| 'structure'
		| 'export'
		| 'import'
		| 'rename'
		| 'drop'
		| 'truncate'
		| 'duplicate'
		| 'create'

	function defaultCopyName(table: TableMeta): string {
		return `${table.name}_copy`
	}

	type CreateKind = 'database' | 'group' | 'object'

	interface Props {
		engine: string
		databases: DatabaseMeta[]
		selectedDatabase: string
		activeTableId: string | null
		refreshing?: boolean
		readOnly?: boolean
		capabilities?: EngineCapabilities | null
		createBusy?: boolean
		createError?: string | null
		onselectdatabase: (name: string) => void
		onrefresh: () => void
		onrefreshtables?: () => void
		refreshingTables?: boolean
		oncreatedatabase: (name: string) => void
		/** Create group/namespace (e.g. Postgres schema). */
		oncreategroup?: (name: string) => void
		/** Create leaf object (table / collection). Optional group = schema. */
		oncreateobject?: (name: string, group?: string) => void
		onopen: (table: TableMeta) => void
		ontableaction?: (
			action: TableExplorerAction,
			table: TableMeta,
			extra?: string,
		) => void
		/** The connection, for per-database production marks. */
		profile?: Pick<
			ConnectionProfile,
			'name' | 'environment' | 'databaseEnvironments'
		> | null
		/** Mark a database (`environment`) or clear its mark (`null`). */
		onmarkdatabase?: (database: string, environment: string | null) => void
	}

	let {
		engine,
		databases,
		selectedDatabase,
		activeTableId,
		refreshing = false,
		refreshingTables = false,
		readOnly = false,
		capabilities = null,
		createBusy = false,
		createError = null,
		onselectdatabase,
		onrefresh,
		onrefreshtables,
		oncreatedatabase,
		oncreategroup,
		oncreateobject,
		onopen,
		ontableaction,
		profile = null,
		onmarkdatabase,
	}: Props = $props()

	let query = $state('')
	/** `public` / `main` start open; other schemas stay collapsed until clicked. */
	let expanded = $state<Set<string>>(new Set(['public', 'main']))
	/** Empty groups created this session (catalog only lists groups with objects). */
	let localGroups = $state<string[]>([])
	let createKind = $state<CreateKind | null>(null)
	let createName = $state('')
	let createGroupValue = $state('')
	let createLocalError = $state<string | null>(null)

	const caps = $derived(capabilities ?? defaultCapabilitiesForEngine(engine))
	const presentation = $derived(presentationFromCaps(caps))
	const isTree = $derived(presentation.layout === 'schema-tree')
	const noun = $derived(presentation.objectLabelSingular || 'table')
	const groupNoun = $derived(presentation.groupLabel.trim() || 'schema')

	const currentDb = $derived(
		databases.find((d) => d.name === selectedDatabase) ?? databases[0] ?? null,
	)

	const tables = $derived(currentDb?.tables ?? [])

	const productionDb = (name: string) =>
		!!profile && isProductionDatabase(profile, name)

	const dbOptions = $derived(
		databases.map((d) => ({
			value: d.name,
			label: d.name,
			mark: productionDb(d.name)
				? {
						icon: 'shield' as const,
						label: 'Production',
						class: 'text-warning',
					}
				: undefined,
		})),
	)

	const selectedProduction = $derived(productionDb(selectedDatabase))
	/** Mark / unmark the selected database (multi-database engines only). */
	const markToggle = $derived(
		profile && onmarkdatabase && presentation.multiDatabase && selectedDatabase.trim()
			? markAction(profile, selectedDatabase)
			: null,
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
		for (const key of localGroups) {
			if (!map.has(key)) map.set(key, [])
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

	const groupOptions = $derived.by(() => {
		const opts = groups.map((g) => ({ value: g.key, label: g.key }))
		if (isTree && opts.length === 0) {
			return [{ value: 'public', label: 'public' }]
		}
		return opts
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
	const catalogCount = $derived(tables.length)
	const filtering = $derived(query.trim().length > 0)

	const formError = $derived(createLocalError ?? createError)

	const createForm = $derived.by(() => {
		if (createKind === 'database') {
			return {
				title: `Create ${presentation.databaseLabel.toLowerCase()}`,
				description: `Choose a name for the new ${presentation.databaseLabel.toLowerCase()} on this server.`,
				nameLabel: presentation.databaseLabel,
				namePlaceholder: 'my_database',
				confirmLabel: 'Create',
				showGroup: false,
			}
		}
		if (createKind === 'group') {
			return {
				title: `Create ${groupNoun.toLowerCase()}`,
				description: `Create a new ${groupNoun.toLowerCase()} in “${selectedDatabase}”.`,
				nameLabel: groupNoun,
				namePlaceholder: 'app',
				confirmLabel: 'Create',
				showGroup: false,
			}
		}
		if (createKind === 'object') {
			return {
				title: `Create ${noun}`,
				description: isTree
					? `Create a new ${noun} in the selected ${groupNoun.toLowerCase()}.`
					: `Create a new ${noun} in “${selectedDatabase}”.`,
				nameLabel: noun.charAt(0).toUpperCase() + noun.slice(1),
				namePlaceholder: noun === 'collection' ? 'my_collection' : 'my_table',
				confirmLabel: 'Create',
				showGroup: isTree,
			}
		}
		return null
	})

	function toggleGroup(key: string) {
		const next = new Set(expanded)
		if (next.has(key)) next.delete(key)
		else next.add(key)
		expanded = next
	}

	function isExpanded(key: string): boolean {
		// While filtering, open groups so matches are visible.
		if (query.trim()) return true
		return expanded.has(key)
	}

	const allGroupsExpanded = $derived(
		isTree && groups.length > 0 && groups.every((g) => expanded.has(g.key)),
	)

	function toggleAllGroups() {
		if (allGroupsExpanded) expanded = new Set()
		else expanded = new Set(groups.map((g) => g.key))
	}

	// Reveal the schema when a table tab becomes active; user can still collapse it.
	let lastRevealedTableId = $state<string | null>(null)
	$effect(() => {
		const id = activeTableId
		if (!id || id === lastRevealedTableId) return
		const t = tables.find((x) => x.id === id)
		lastRevealedTableId = id
		if (!t) return
		const key = t.schema || 'public'
		if (!expanded.has(key)) {
			const next = new Set(expanded)
			next.add(key)
			expanded = next
		}
	})

	function openCreate(kind: CreateKind, preferredGroup?: string) {
		if (kind === 'database' && !presentation.canCreateDatabase) return
		if (kind === 'group' && !presentation.canCreateGroup) return
		if (kind === 'object' && !presentation.canCreateObject) return
		if ((kind === 'group' || kind === 'object') && readOnly) {
			createLocalError = 'This connection is read-only.'
			return
		}
		createKind = kind
		createName = ''
		createLocalError = null
		if (kind === 'object') {
			const pick =
				preferredGroup ||
				groupOptions.find((g) => g.value === 'public')?.value ||
				groupOptions[0]?.value ||
				'public'
			createGroupValue = pick
		} else {
			createGroupValue = ''
		}
	}

	function cancelCreate() {
		if (createBusy) return
		closeCreate()
	}

	function validateIdent(name: string, label: string): string | null {
		const trimmed = name.trim()
		if (!trimmed) return `Enter a ${label.toLowerCase()} name.`
		if (!/^[A-Za-z_][A-Za-z0-9_]*$/.test(trimmed)) {
			return 'Use letters, numbers, underscore; start with a letter.'
		}
		return null
	}

	function closeCreate() {
		createKind = null
		createName = ''
		createGroupValue = ''
		createLocalError = null
	}

	function submitCreate(name: string, group?: string) {
		const kind = createKind
		if (!kind || createBusy) return
		if ((kind === 'group' || kind === 'object') && readOnly) {
			createLocalError = 'This connection is read-only.'
			return
		}
		if ((kind === 'group' || kind === 'object') && !selectedDatabase.trim()) {
			createLocalError = `Select a ${presentation.databaseLabel.toLowerCase()} first.`
			return
		}
		const label =
			kind === 'database'
				? presentation.databaseLabel
				: kind === 'group'
					? groupNoun
					: noun
		const err = validateIdent(name, label)
		if (err) {
			createLocalError = err
			return
		}
		const trimmed = name.trim()
		if (kind === 'database') {
			if (databases.some((d) => d.name === trimmed)) {
				createLocalError = `A ${presentation.databaseLabel.toLowerCase()} with this name already exists.`
				return
			}
			oncreatedatabase(trimmed)
			closeCreate()
			return
		}
		if (kind === 'group') {
			if (
				groups.some((g) => g.key === trimmed) ||
				localGroups.includes(trimmed)
			) {
				createLocalError = `A ${groupNoun.toLowerCase()} with this name already exists.`
				return
			}
			if (!localGroups.includes(trimmed)) {
				localGroups = [...localGroups, trimmed]
			}
			const next = new Set(expanded)
			next.add(trimmed)
			expanded = next
			oncreategroup?.(trimmed)
			closeCreate()
			return
		}
		const schema = isTree ? group || createGroupValue || undefined : undefined
		const schemaKey = schema || ''
		if (
			tables.some((t) => t.name === trimmed && (t.schema || '') === schemaKey)
		) {
			createLocalError = `A ${noun} with this name already exists.`
			return
		}
		if (!oncreateobject) {
			createLocalError = `Create ${noun} is not available.`
			return
		}
		oncreateobject(trimmed, schema)
		closeCreate()
	}

	// Drop local empty groups once they appear in the live catalog.
	$effect(() => {
		if (localGroups.length === 0) return
		const present = new Set(
			tables.map((t) => t.schema || 'public').filter(Boolean),
		)
		const next = localGroups.filter((g) => !present.has(g))
		if (next.length !== localGroups.length) localGroups = next
	})

	function onDbChange(value: string) {
		query = ''
		expanded = new Set(['public', 'main'])
		localGroups = []
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
		items.push({
			id: 'export',
			label: 'Export data',
			separatorBefore: true,
		})
		if (!isView) {
			items.push({
				id: 'import',
				label: 'Import data',
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
		if (id === 'export') {
			ontableaction('export', table)
			return
		}
		if (id === 'import') {
			ontableaction('import', table)
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
		// The write dialog asks once (type-to-confirm on production).
		if (id === 'drop' || id === 'truncate') {
			ontableaction(id, table)
		}
	}

	function submitName(name: string) {
		const prompt = namePrompt
		namePrompt = null
		if (!prompt || !ontableaction) return
		ontableaction(prompt.action, prompt.table, name)
	}

</script>

<aside
	class="flex h-full w-full min-w-0 flex-col bg-panel"
	aria-label="Database objects"
	aria-busy={refreshing || refreshingTables || undefined}
>
	<header class="shrink-0 border-b border-line px-2.5 pt-2.5 pb-2">
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
				>
					{#snippet leading()}
						{#if selectedProduction && profile}
							<ProductionMark label={productionLabel(profile, selectedDatabase)} />
						{:else}
							<CatalogIcon name="database" class="size-3.5" />
						{/if}
					{/snippet}
				</Select>
			</div>
			<div
				class="flex shrink-0 items-center"
				role="group"
				aria-label="{presentation.databaseLabel} actions"
			>
				<IconButton
					label="Refresh databases"
					size="sm"
					variant="quiet"
					disabled={refreshing}
					aria-busy={refreshing}
					class={refreshing ? 'opacity-45' : ''}
					onclick={onrefresh}
				>
					<Icon name="refresh" class={cn(refreshing && 'animate-spin')} />
				</IconButton>
				{#if markToggle}
					<IconButton
						label="{markToggle.label} (“{selectedDatabase}”)"
						size="sm"
						variant="quiet"
						aria-pressed={selectedProduction}
						class={selectedProduction ? 'text-warning' : ''}
						onclick={() =>
							onmarkdatabase?.(selectedDatabase, markToggle.environment)}
					>
						<Icon name="shield" />
					</IconButton>
				{/if}
				{#if presentation.canCreateDatabase}
					<IconButton
						label="Create {presentation.databaseLabel.toLowerCase()}"
						size="sm"
						variant="quiet"
						disabled={createBusy}
						onclick={() => openCreate('database')}
					>
						<Icon name="plus" />
					</IconButton>
				{/if}
			</div>
		</div>

		<div class="mt-2">
			<ExplorerSearch
				bind:value={query}
				placeholder={presentation.filterPlaceholder}
				label={presentation.filterPlaceholder}
			/>
			{#if filtering}
				<p class="sr-only" aria-live="polite">
					{totalCount} of {catalogCount}
					{presentation.objectLabel}
				</p>
			{/if}
		</div>
	</header>

	<div class="flex shrink-0 items-center gap-1 px-2.5 py-0.5">
		<div class="flex min-w-0 flex-1 items-center gap-1.5">
			<h2
				class="m-0 min-w-0 truncate text-label font-semibold tracking-wider text-fg-muted uppercase"
			>
				{presentation.listTitle}
			</h2>
			<span
				class="shrink-0 rounded-sm bg-input px-1 font-mono text-micro text-fg-muted tabular-nums"
				title={filtering
					? `${totalCount} of ${catalogCount} ${presentation.objectLabel}`
					: `${catalogCount} ${presentation.objectLabel}`}
			>
				{#if filtering}
					{totalCount}/{catalogCount}
				{:else}
					{catalogCount}
				{/if}
			</span>
		</div>
		<div
			class="flex shrink-0 items-center"
			role="group"
			aria-label="{presentation.listTitle} actions"
		>
			{#if isTree && groups.length > 0}
				<IconButton
					label={allGroupsExpanded
						? `Collapse all ${(presentation.groupLabel || 'schema').toLowerCase()}s`
						: `Expand all ${(presentation.groupLabel || 'schema').toLowerCase()}s`}
					size="sm"
					variant="quiet"
					onclick={toggleAllGroups}
				>
					<Icon name={allGroupsExpanded ? 'collapse-all' : 'expand-all'} />
				</IconButton>
			{/if}
			<IconButton
				label="Refresh {presentation.listTitle.toLowerCase()}"
				size="sm"
				variant="quiet"
				disabled={refreshingTables || !currentDb}
				aria-busy={refreshingTables}
				class={refreshingTables ? 'opacity-45' : ''}
				onclick={() => onrefreshtables?.()}
			>
				<Icon name="refresh" class={cn(refreshingTables && 'animate-spin')} />
			</IconButton>
			{#if presentation.canCreateObject}
				<IconButton
					label="Create {noun}"
					size="sm"
					variant="quiet"
					disabled={createBusy || !currentDb || readOnly}
					onclick={() => openCreate('object')}
				>
					<Icon name="plus" />
				</IconButton>
			{/if}
		</div>
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
		busy={refreshingTables}
		onToggleGroup={toggleGroup}
		{onopen}
		onmenu={openTableMenu}
	/>
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

{#if createForm}
	<CreateDatabaseForm
		open={createKind !== null}
		bind:value={createName}
		bind:groupValue={createGroupValue}
		error={formError}
		busy={createBusy}
		title={createForm.title}
		description={createForm.description}
		nameLabel={createForm.nameLabel}
		namePlaceholder={createForm.namePlaceholder}
		confirmLabel={createForm.confirmLabel}
		showGroup={createForm.showGroup}
		groupLabel={groupNoun}
		{groupOptions}
		oncancel={cancelCreate}
		onconfirm={submitCreate}
	/>
{/if}

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
