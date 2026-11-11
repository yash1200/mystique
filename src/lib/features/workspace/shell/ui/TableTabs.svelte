<script lang="ts">
	import type { CatalogObjectKind } from '#core/contracts'
	import { ContextMenu, Draggable, Icon, type ContextMenuItem } from '#core/ui'
	import { cn } from '#core/util'
	import {
		CatalogIcon,
		iconNameForTab,
		iconToneForName,
	} from '#features/workspace/catalog'
	import { tick } from 'svelte'
	import { flip } from 'svelte/animate'
	import type { WorkspaceTab } from '#features/workspace/session'

	interface Props {
		tabs: WorkspaceTab[]
		activeTabId: string | null
		connectionColor?: string | null
		production?: boolean
		/** Only tabs for the active connection (parent filters). */
		onactivate: (id: string) => void
		onclose: (id: string) => void
		/** Close several tabs in order (parent handles dirty prompts). */
		onclosemany?: (ids: string[]) => void
		/** Open / focus Query canvas for this connection. */
		onopenquery?: () => void
		/** Insert `movedId` before `beforeId`, or at the end when `beforeId` is null. */
		onreorder?: (movedId: string, beforeId: string | null) => void
		/** Live catalog kind so table tabs show table / view / collection marks. */
		objectKindById?: Record<string, CatalogObjectKind>
	}

	let {
		tabs,
		activeTabId,
		connectionColor = null,
		production = false,
		onactivate,
		onclose,
		onclosemany,
		onopenquery,
		onreorder,
		objectKindById = {},
	}: Props = $props()

	const flipMs =
		typeof window !== 'undefined' &&
		window.matchMedia('(prefers-reduced-motion: reduce)').matches
			? 0
			: 140

	let tablistEl = $state<HTMLDivElement | undefined>(undefined)

	$effect(() => {
		const id = activeTabId
		const root = tablistEl
		if (!id || !root) return
		void tick().then(() => {
			requestAnimationFrame(() => {
				const el = root.querySelector(`[data-drag-id="${CSS.escape(id)}"]`)
				if (!(el instanceof HTMLElement)) return
				el.scrollIntoView({
					inline: 'nearest',
					block: 'nearest',
					behavior: 'auto',
				})
			})
		})
	})

	const tabIds = $derived(tabs.map((t) => t.id))
	const canReorder = $derived(!!onreorder && tabs.length > 1)

	function tabById(id: string): WorkspaceTab | undefined {
		return tabs.find((t) => t.id === id)
	}

	let ctxMenu = $state<{
		clientX: number
		clientY: number
		tabId: string
	} | null>(null)

	const ctxTab = $derived(
		ctxMenu ? (tabs.find((t) => t.id === ctxMenu!.tabId) ?? null) : null,
	)
	const ctxIndex = $derived(
		ctxMenu ? tabs.findIndex((t) => t.id === ctxMenu!.tabId) : -1,
	)

	const ctxMenuItems = $derived.by((): ContextMenuItem[] => {
		if (!ctxMenu || ctxIndex < 0) return []
		const last = tabs.length - 1
		return [
			{ id: 'close', label: 'Close' },
			{
				id: 'close-others',
				label: 'Close others',
				disabled: tabs.length < 2,
			},
			{ id: 'close-all', label: 'Close all' },
			{
				id: 'close-right',
				label: 'Close tabs to the right',
				disabled: ctxIndex < 0 || ctxIndex >= last,
			},
			{
				id: 'close-left',
				label: 'Close tabs to the left',
				disabled: ctxIndex <= 0,
			},
			{
				id: 'copy-name',
				label: 'Copy entity name',
				icon: 'copy',
				separatorBefore: true,
				disabled: !ctxTab || !entityName(ctxTab),
			},
		]
	})

	function tabLabel(tab: WorkspaceTab): string {
		if (tab.kind === 'structure' || tab.kind === 'model') return entityName(tab)
		return tab.title
	}

	function entityName(tab: WorkspaceTab): string {
		if (tab.kind === 'table' || tab.kind === 'structure') {
			const fromId = tab.tableId?.split('.').pop()?.trim()
			if (fromId) return fromId
			return tab.title.replace(/^Structure\s*[·.]\s*/, '').trim()
		}
		if (tab.kind === 'model') {
			const fromId = tab.tableId?.split('.').pop()?.trim()
			if (fromId) return fromId
			const db = tab.modelDatabase?.trim()
			if (db) return db
			return (
				tab.title
					.replace(/^ER\s*[·.]\s*/, '')
					.replace(/^Model\s*[·.]\s*/, '')
					.replace(/^ER diagram$/i, '')
					.trim() || tab.title.trim()
			)
		}
		return tab.title.trim()
	}

	function closeMany(ids: string[]) {
		if (!ids.length) return
		if (onclosemany) onclosemany(ids)
		else ids.forEach((id) => onclose(id))
	}

	function onCtxPick(id: string) {
		const ctx = ctxMenu
		ctxMenu = null
		if (!ctx) return
		const index = tabs.findIndex((t) => t.id === ctx.tabId)
		if (index < 0) return
		const tab = tabs[index]!
		if (id === 'close') {
			onclose(tab.id)
			return
		}
		if (id === 'close-others') {
			closeMany(tabs.filter((t) => t.id !== tab.id).map((t) => t.id))
			return
		}
		if (id === 'close-all') {
			closeMany(tabs.map((t) => t.id))
			return
		}
		if (id === 'close-right') {
			closeMany(tabs.slice(index + 1).map((t) => t.id))
			return
		}
		if (id === 'close-left') {
			closeMany(tabs.slice(0, index).map((t) => t.id))
			return
		}
		if (id === 'copy-name') {
			const name = entityName(tab)
			if (!name) return
			void navigator.clipboard.writeText(name).catch(() => {})
		}
	}

	function onTabContextMenu(tabId: string, e: MouseEvent) {
		e.preventDefault()
		e.stopPropagation()
		ctxMenu = { clientX: e.clientX, clientY: e.clientY, tabId }
	}
</script>

{#snippet kindMark(tab: WorkspaceTab)}
	{@const name = iconNameForTab(
		tab.kind,
		tab.tableId ? objectKindById[tab.tableId] : null,
	)}
	<CatalogIcon {name} class="size-3" tone={iconToneForName(name)} />
{/snippet}

<Draggable
	ids={tabIds}
	enabled={canReorder}
	axis="x"
	ignore="[data-tab-close]"
	scrollRoot={tablistEl}
	previewClass={production ? 'bg-warning/[0.08]' : ''}
	onpickup={onactivate}
	{onreorder}
>
	{#snippet children(drag)}
		<div
			bind:this={tablistEl}
			class={cn(
				'scrollbar-none flex h-control-sm shrink-0 items-stretch overflow-x-auto overscroll-x-contain border-b border-line bg-panel',
				production && 'bg-warning/6',
				drag.draggingId && 'cursor-grabbing select-none',
			)}
			style={connectionColor
				? `--connection-color: ${connectionColor}`
				: undefined}
			role="tablist"
			aria-label="Workspace tabs"
			onwheel={(e) => {
				const el = e.currentTarget
				if (el.scrollWidth <= el.clientWidth) return
				if (Math.abs(e.deltaY) <= Math.abs(e.deltaX)) return
				el.scrollLeft += e.deltaY
				e.preventDefault()
			}}
		>
			{#each tabs as tab (tab.id)}
				{@const active = activeTabId === tab.id}
				{@const dirty = !!tab.dirty}
				{@const dragging = drag.draggingId === tab.id}
				{@const edge = drag.dropEdge(tab.id)}
				<div
					data-drag-id={tab.id}
					animate:flip={{ duration: flipMs }}
					class={cn(
						'group/tab relative flex max-w-56 shrink-0 items-stretch border-r border-line',
						canReorder && !drag.draggingId && 'cursor-grab',
						dragging && 'pointer-events-none cursor-grabbing opacity-35',
						active
							? 'bg-app-wash text-fg'
							: 'bg-transparent text-fg-secondary hover:bg-hover hover:text-fg',
					)}
					role="presentation"
					onpointerdown={(e) => drag.start(e, tab.id)}
					oncontextmenu={(e) => onTabContextMenu(tab.id, e)}
				>
					{#if connectionColor}
						<span
							class="absolute inset-x-0 top-0 h-0.5 bg-conn"
							class:opacity-40={!active}
							aria-hidden="true"
						></span>
					{/if}
					{#if edge === 'before' || edge === 'after'}
						<span
							class={cn(
								'pointer-events-none absolute inset-y-0.5 z-10 w-0.5 rounded-full bg-accent',
								edge === 'before' ? '-left-px' : '-right-px',
							)}
							aria-hidden="true"
						></span>
					{/if}
					<button
						type="button"
						role="tab"
						aria-selected={active}
						draggable="false"
						class={cn(
							'flex min-w-0 flex-1 items-center gap-1 border-none bg-transparent px-2.5 text-left text-label font-medium leading-none',
							canReorder &&
								(drag.draggingId ? 'cursor-grabbing' : 'cursor-grab'),
						)}
						title={dirty
							? `${tab.kind === 'structure' ? `Structure: ${tabLabel(tab)}` : tab.title} (unsaved changes)`
							: tab.kind === 'structure'
								? `Structure: ${tabLabel(tab)}`
								: tab.title}
						onclick={() => {
							if (drag.ateClick()) return
							onactivate(tab.id)
						}}
					>
						{@render kindMark(tab)}
						<span class="min-w-0 truncate leading-normal">{tabLabel(tab)}</span>
					</button>
					<button
						type="button"
						data-tab-close
						draggable="false"
						class={cn(
							'group/close mr-1 flex size-6 shrink-0 self-center items-center justify-center rounded-sm border-none bg-transparent text-body-lg font-medium',
							'hover:bg-hover hover:text-fg',
							// Clean inactive tabs: show close only when the tab row is hovered
							!dirty && !active && 'opacity-0 group-hover/tab:opacity-100',
							dirty ? 'text-warning' : 'text-fg-muted',
						)}
						aria-label={dirty
							? `Unsaved, close ${tabLabel(tab)}`
							: `Close ${tabLabel(tab)}`}
						title={dirty ? 'Unsaved changes — click to close' : 'Close'}
						onclick={(e) => {
							e.stopPropagation()
							onclose(tab.id)
						}}
					>
						<!-- Dirty: yellow dot; hover this control to reveal close -->
						{#if dirty}
							<span
								class="text-label leading-none group-hover/close:hidden"
								aria-hidden="true">●</span
							>
							<Icon
								name="close"
								class="hidden size-3.5 group-hover/close:block"
							/>
						{:else}
							<Icon name="close" class="size-3.5" />
						{/if}
					</button>
				</div>
			{/each}

			{#if onopenquery}
				<button
					type="button"
					class="flex shrink-0 items-center border-none bg-transparent px-2 text-caption text-fg-muted hover:bg-hover hover:text-fg"
					title="New query tab (⌘/Ctrl+T)"
					aria-label="New query tab"
					onclick={onopenquery}
				>
					+ Query
				</button>
			{/if}
		</div>
	{/snippet}

	{#snippet preview({ id })}
		{@const tab = tabById(id)}
		{#if tab}
			<div
				class="relative flex h-full min-w-0 flex-1 items-stretch text-fg"
				style={connectionColor
					? `--connection-color: ${connectionColor}`
					: undefined}
			>
				{#if connectionColor}
					<span class="absolute inset-x-0 top-0 h-0.5 bg-conn"></span>
				{/if}
				<div
					class="flex min-w-0 flex-1 items-center gap-1 self-center px-2.5 text-left text-label font-medium leading-none"
				>
					{@render kindMark(tab)}
					<span class="min-w-0 truncate leading-normal">{tabLabel(tab)}</span>
				</div>
				{#if tab.dirty}
					<span class="mr-1.5 self-center text-label leading-none text-warning"
						>●</span
					>
				{:else}
					<Icon
						name="close"
						class="mr-1.5 size-3.5 shrink-0 self-center text-fg-muted"
					/>
				{/if}
			</div>
		{/if}
	{/snippet}
</Draggable>

{#if ctxMenu}
	<ContextMenu
		x={ctxMenu.clientX}
		y={ctxMenu.clientY}
		items={ctxMenuItems}
		ariaLabel="Tab actions"
		onpick={onCtxPick}
		onclose={() => (ctxMenu = null)}
	/>
{/if}
