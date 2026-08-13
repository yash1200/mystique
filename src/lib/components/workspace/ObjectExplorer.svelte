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

	type CreateKind = 'database' | 'group' | 'object'

	interface Props {
		engine: string
		databases: DatabaseMeta[]
		selectedDatabase: string
		activeTableId: string | null
		refreshing?: boolean
		modelLoading?: boolean
		readOnly?: boolean
		capabilities?: EngineCapabilities | null
		createBusy?: boolean
		createError?: string | null
		onselectdatabase: (name: string) => void
		onrefresh: () => void
		oncreatedatabase: (name: string) => void
		/** Create group/namespace (e.g. Postgres schema). */
		oncreategroup?: (name: string) => void
		/** Create leaf object (table / collection). Optional group = schema. */
		oncreateobject?: (name: string, group?: string) => void
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
		createBusy = false,
		createError = null,
		onselectdatabase,
		onrefresh,
		oncreatedatabase,
		oncreategroup,
		oncreateobject,
		onopen,
		onopenmodel,
		ontableaction,
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
	let createMenuOpen = $state(false)
	let createMenuRoot = $state<HTMLDivElement | null>(null)

	const caps = $derived(capabilities ?? defaultCapabilitiesForEngine(engine))
	const presentation = $derived(presentationFromCaps(caps))
	const isTree = $derived(presentation.layout === 'schema-tree')
	const noun = $derived(presentation.objectLabelSingular || 'table')
	const groupNoun = $derived(presentation.groupLabel.trim() || 'schema')

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
		// Keep the active table's schema open.
		if (activeTableId) {
			const hit = filtered.some(
				(t) => t.id === activeTableId && (t.schema || 'public') === key,
			)
			if (hit) return true
		}
		return expanded.has(key)
	}

	/** Show + when any create action is allowed by plugin caps (not FE engine checks). */
	const canCreateAnything = $derived(
		presentation.canCreateDatabase ||
			presentation.canCreateGroup ||
			presentation.canCreateObject,
	)

	type CreateMenuEntry = {
		id: CreateKind
		label: string
		disabled?: boolean
		separatorBefore?: boolean
	}

	const createMenuItems = $derived.by((): CreateMenuEntry[] => {
		const items: CreateMenuEntry[] = []
		if (presentation.canCreateDatabase) {
			items.push({
				id: 'database',
				label: `Create ${presentation.databaseLabel.toLowerCase()}`,
				disabled: createBusy,
			})
		}
		if (presentation.canCreateGroup) {
			items.push({
				id: 'group',
				label: `Create ${groupNoun.toLowerCase()}`,
				separatorBefore: items.length > 0,
				// Mutating ops: disable when profile is read-only or no DB context.
				disabled: createBusy || readOnly || !selectedDatabase,
			})
		}
		if (presentation.canCreateObject) {
			items.push({
				id: 'object',
				label: `Create ${noun}`,
				separatorBefore: items.length > 0 && !presentation.canCreateGroup,
				// Need a database context (picker value or loaded catalog entry).
				disabled:
					createBusy ||
					readOnly ||
					!(selectedDatabase.trim() || currentDb?.name),
			})
		}
		return items
	})

	function toggleCreateMenu() {
		if (!canCreateAnything || createBusy) return
		createMenuOpen = !createMenuOpen
	}

	function onCreateMenuPick(id: CreateKind) {
		createMenuOpen = false
		openCreate(id)
	}

	function openCreate(kind: CreateKind, preferredGroup?: string) {
		if (kind === 'database' && !presentation.canCreateDatabase) return
		if (kind === 'group' && !presentation.canCreateGroup) return
		if (kind === 'object' && !presentation.canCreateObject) return
		if ((kind === 'group' || kind === 'object') && readOnly) {
			createLocalError = 'This connection is read-only.'
			return
		}
		createMenuOpen = false
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

	// Close create dropdown on outside click / Escape.
	$effect(() => {
		if (!createMenuOpen) return
		const onDoc = (e: PointerEvent) => {
			if (e.target instanceof Node && createMenuRoot?.contains(e.target)) {
				return
			}
			createMenuOpen = false
		}
		const onKey = (e: KeyboardEvent) => {
			if (e.key === 'Escape') createMenuOpen = false
		}
		const tid = window.setTimeout(() => {
			document.addEventListener('pointerdown', onDoc, true)
		}, 0)
		window.addEventListener('keydown', onKey)
		return () => {
			clearTimeout(tid)
			document.removeEventListener('pointerdown', onDoc, true)
			window.removeEventListener('keydown', onKey)
		}
	})

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
	let confirmPrompt = $state<{
		action: 'drop' | 'truncate'
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
			{#if canCreateAnything}
				<div class="relative" bind:this={createMenuRoot}>
					<IconButton
						label="Create"
						size="sm"
						variant="ghost"
						disabled={createBusy}
						aria-haspopup="menu"
						aria-expanded={createMenuOpen}
						onclick={(e) => {
							e.stopPropagation()
							toggleCreateMenu()
						}}
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
					{#if createMenuOpen}
						<div
							class="absolute top-full right-0 z-50 mt-1 min-w-44 rounded-md border border-line bg-elevated py-1 shadow-md"
							role="menu"
							tabindex="-1"
							aria-label="Create"
							onpointerdown={(e) => e.stopPropagation()}
						>
							{#each createMenuItems as item (item.id)}
								{#if item.separatorBefore}
									<div class="my-1 border-t border-line" role="separator"></div>
								{/if}
								<button
									type="button"
									role="menuitem"
									disabled={item.disabled}
									class={cn(
										'flex w-full cursor-pointer items-center border-none bg-transparent px-2.5 py-1.5 text-left text-label text-fg hover:bg-hover',
										item.disabled &&
											'cursor-not-allowed opacity-45 hover:bg-transparent',
									)}
									onclick={() => {
										if (item.disabled) return
										onCreateMenuPick(item.id)
									}}
								>
									{item.label}
								</button>
							{/each}
						</div>
					{/if}
				</div>
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
		canCreateObject={presentation.canCreateObject && !readOnly}
		objectLabelSingular={noun}
		oncreatein={(groupKey) => openCreate('object', groupKey)}
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
