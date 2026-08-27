<script lang="ts">
	import { Button, Icon, IconButton, StatusBar } from '$lib/components/core'
	import { cn } from '$lib/utils/cn'

	interface Props {
		dirty: boolean
		readOnly: boolean
		entityWord: string
		hasDiagram: boolean
		nodeCount: number
		edgeCount: number
		selectionCount: number
		arrowTool: 'normal' | 'broken' | null
		linkFrom: boolean
		hasNodes: boolean
		showGrid: boolean
		dbmlSourceMode: boolean
		showDbml: boolean
		canZoomOut: boolean
		canZoomIn: boolean
		zoomLabel: string
		refreshing?: boolean
		toolMsg: string | null
		onAutoLayout: () => void
		onRefresh?: () => void
		onZoomOut: () => void
		onZoomIn: () => void
		onResetZoom: () => void
		onToggleGrid: () => void
		onToggleDbmlEditor: () => void
		onToggleDbml: () => void
	}

	let {
		dirty,
		readOnly,
		entityWord,
		hasDiagram,
		nodeCount,
		edgeCount,
		selectionCount,
		arrowTool,
		linkFrom,
		hasNodes,
		showGrid,
		dbmlSourceMode,
		showDbml,
		canZoomOut,
		canZoomIn,
		zoomLabel,
		refreshing = false,
		toolMsg,
		onAutoLayout,
		onRefresh,
		onZoomOut,
		onZoomIn,
		onResetZoom,
		onToggleGrid,
		onToggleDbmlEditor,
		onToggleDbml,
	}: Props = $props()
</script>

<StatusBar class="!pl-1" ariaLabel="ER diagram status">
	{#snippet left()}
		<Button
			variant="ghost"
			size="sm"
			class="!px-1.5"
			onclick={onAutoLayout}
			disabled={!hasNodes}
			title="Arrange tables on the canvas"
		>
			<Icon name="expand-all" class="size-3.5" />
			Layout
		</Button>
		{#if hasDiagram}
			<span class="truncate font-mono tabular-nums">
				{nodeCount}
				{entityWord}{nodeCount === 1 ? '' : 's'}
				{#if edgeCount > 0}
					· {edgeCount} link{edgeCount === 1 ? '' : 's'}
				{/if}
			</span>
		{:else}
			<span class="truncate font-mono tabular-nums text-fg-muted">No diagram</span>
		{/if}
		{#if selectionCount > 0}
			<span class="shrink-0 text-fg-muted">{selectionCount} selected</span>
		{/if}
		{#if arrowTool}
			<span class="shrink-0 text-accent">
				{arrowTool === 'broken' ? 'Broken arrow' : 'Normal arrow'}
				{#if linkFrom}
					· pick target
				{/if}
			</span>
		{/if}
		{#if dirty && !readOnly}
			<span
				class="shrink-0 rounded-sm border border-warning/45 bg-warning/15 px-1 py-px text-micro font-medium text-warning"
			>
				Unsaved
			</span>
		{/if}
		{#if toolMsg}
			<span class="min-w-0 truncate text-fg-muted" role="status">{toolMsg}</span>
		{/if}
	{/snippet}
	{#snippet center()}
		<div class="flex items-center gap-1.5" role="group" aria-label="Canvas zoom">
			<IconButton
				label="Zoom out"
				size="sm"
				variant="quiet"
				onclick={onZoomOut}
				disabled={!canZoomOut}
			>
				<span class="text-title leading-none">−</span>
			</IconButton>
			<button
				type="button"
				class="min-w-12 shrink-0 border-none bg-transparent px-1.5 text-center font-mono text-caption text-fg-secondary tabular-nums whitespace-nowrap hover:text-fg"
				onclick={onResetZoom}
				title="Reset zoom to 100%"
				aria-label="Reset zoom to 100%"
			>
				{zoomLabel}
			</button>
			<IconButton
				label="Zoom in"
				size="sm"
				variant="quiet"
				onclick={onZoomIn}
				disabled={!canZoomIn}
			>
				<span class="text-title leading-none">+</span>
			</IconButton>
		</div>
	{/snippet}
	{#snippet right()}
		{#if onRefresh}
			<IconButton
				label={refreshing ? 'Refreshing diagram' : 'Refresh diagram'}
				size="sm"
				variant="quiet"
				onclick={onRefresh}
				disabled={refreshing}
				aria-busy={refreshing}
				class={refreshing ? 'opacity-45' : ''}
			>
				<Icon name="refresh" class={cn(refreshing && 'animate-spin')} />
			</IconButton>
		{/if}
		<IconButton
			label={showGrid ? 'Hide grid' : 'Show grid'}
			size="sm"
			variant="quiet"
			active={showGrid}
			onclick={onToggleGrid}
		>
			<Icon name="grid" />
		</IconButton>
		{#if !readOnly}
			<IconButton
				label={dbmlSourceMode ? 'Close DBML editor' : 'Open DBML editor'}
				size="sm"
				variant="quiet"
				active={dbmlSourceMode}
				onclick={onToggleDbmlEditor}
			>
				<Icon name="code-slash" />
			</IconButton>
		{/if}
		{#if !dbmlSourceMode}
			<IconButton
				label={showDbml ? 'Hide DBML snapshot' : 'Show DBML snapshot'}
				size="sm"
				variant="quiet"
				active={showDbml}
				onclick={onToggleDbml}
			>
				<Icon name="code" />
			</IconButton>
		{/if}
	{/snippet}
</StatusBar>
