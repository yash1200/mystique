<script lang="ts">
	import { Draggable } from '$lib/components/core'
	import type { WorkspaceTab } from '$lib/types/workspace'
	import { cn } from '$lib/utils/cn'
	import { flip } from 'svelte/animate'

	interface Props {
		tabs: WorkspaceTab[]
		activeTabId: string | null
		connectionColor?: string | null
		production?: boolean
		/** Only tabs for the active connection (parent filters). */
		onactivate: (id: string) => void
		onclose: (id: string) => void
		/** Open / focus Query canvas for this connection. */
		onopenquery?: () => void
		/** Insert `movedId` before `beforeId`, or at the end when `beforeId` is null. */
		onreorder?: (movedId: string, beforeId: string | null) => void
	}

	let {
		tabs,
		activeTabId,
		connectionColor = null,
		production = false,
		onactivate,
		onclose,
		onopenquery,
		onreorder,
	}: Props = $props()

	const flipMs =
		typeof window !== 'undefined' &&
		window.matchMedia('(prefers-reduced-motion: reduce)').matches
			? 0
			: 140

	let tablistEl = $state<HTMLDivElement | undefined>(undefined)

	const tabIds = $derived(tabs.map((t) => t.id))
	const canReorder = $derived(!!onreorder && tabs.length > 1)

	function tabById(id: string): WorkspaceTab | undefined {
		return tabs.find((t) => t.id === id)
	}
</script>

{#snippet kindMark(kind: WorkspaceTab['kind'])}
	{#if kind === 'query'}
		<span class="text-accent">◇</span>
		{' '}
	{:else if kind === 'model'}
		<span class="text-info">⬡</span>
		{' '}
	{:else if kind === 'structure'}
		<span class="text-fg-muted">▣</span>
		{' '}
	{/if}
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
							'min-w-0 flex-1 truncate border-none bg-transparent px-2.5 text-left text-label font-medium leading-none',
							canReorder &&
								(drag.draggingId ? 'cursor-grabbing' : 'cursor-grab'),
						)}
						title={dirty ? `${tab.title} (unsaved changes)` : tab.title}
						onclick={() => {
							if (drag.ateClick()) return
							onactivate(tab.id)
						}}
					>
						{@render kindMark(tab.kind)}
						{tab.title}
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
							? `Unsaved — close ${tab.title}`
							: `Close ${tab.title}`}
						title={dirty ? 'Unsaved changes — click to close' : 'Close'}
						onclick={(e) => {
							e.stopPropagation()
							onclose(tab.id)
						}}
					>
						<!-- Dirty: yellow dot; hover this control to reveal × -->
						{#if dirty}
							<span
								class="text-label leading-none group-hover/close:hidden"
								aria-hidden="true">●</span
							>
							<span
								class="hidden leading-none group-hover/close:inline"
								aria-hidden="true">×</span
							>
						{:else}
							<span class="leading-none" aria-hidden="true">×</span>
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
					class="min-w-0 flex-1 self-center truncate px-2.5 text-left text-label font-medium leading-none"
				>
					{@render kindMark(tab.kind)}
					{tab.title}
				</div>
				{#if tab.dirty}
					<span class="mr-1.5 self-center text-label leading-none text-warning"
						>●</span
					>
				{:else}
					<span
						class="mr-1.5 self-center text-body-lg leading-none text-fg-muted"
						>×</span
					>
				{/if}
			</div>
		{/if}
	{/snippet}
</Draggable>
