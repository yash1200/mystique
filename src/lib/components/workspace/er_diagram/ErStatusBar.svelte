<script lang="ts">
	import { Button, Icon, IconButton, Input, StatusBar } from '$lib/components/core'
	import { cn } from '$lib/utils/cn'

	interface Props {
		diagramName: string
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
		diagramName = $bindable(''),
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

<StatusBar ariaLabel="ER diagram status">
	{#snippet left()}
		{#if hasDiagram}
			<span class="truncate font-mono tabular-nums">
				{nodeCount}
				{entityWord}{nodeCount === 1 ? '' : 's'}
				{#if edgeCount > 0}
					· {edgeCount} link{edgeCount === 1 ? '' : 's'}
				{/if}
			</span>
		{:else}
			<span>No diagram</span>
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
		{#if !readOnly}
			<Input
				bind:value={diagramName}
				class="max-w-40"
				inputSize="sm"
				aria-label="Diagram name"
			/>
		{/if}
		{#if toolMsg}
			<span class="min-w-0 truncate text-fg-muted" role="status">{toolMsg}</span
			>
		{/if}
	{/snippet}
	{#snippet center()}
		<div class="flex items-center gap-0.5">
			<IconButton
				label="Zoom out"
				size="sm"
				variant="ghost"
				onclick={onZoomOut}
				disabled={!canZoomOut}
			>
				−
			</IconButton>
			<button
				type="button"
				class="h-control-sm rounded-md border border-line bg-input px-1.5 font-mono text-caption text-fg hover:border-line-strong"
				onclick={onResetZoom}
				title="Reset zoom to 100%"
				aria-label="Reset zoom to 100%"
			>
				{zoomLabel}
			</button>
			<IconButton
				label="Zoom in"
				size="sm"
				variant="ghost"
				onclick={onZoomIn}
				disabled={!canZoomIn}
			>
				+
			</IconButton>
		</div>
	{/snippet}
	{#snippet right()}
		<Button
			variant="ghost"
			size="sm"
			onclick={onAutoLayout}
			disabled={!hasNodes}
		>
			Auto-layout
		</Button>
		{#if onRefresh}
			<IconButton
				label={refreshing ? 'Refreshing diagram' : 'Refresh diagram'}
				size="sm"
				variant="ghost"
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
			variant="ghost"
			active={showGrid}
			onclick={onToggleGrid}
		>
			<Icon name="grid" />
		</IconButton>
		{#if !readOnly}
			<Button
				variant={dbmlSourceMode ? 'secondary' : 'ghost'}
				size="sm"
				onclick={onToggleDbmlEditor}
				aria-pressed={dbmlSourceMode}
			>
				{dbmlSourceMode ? 'Close editor' : 'DBML editor'}
			</Button>
		{/if}
		{#if !dbmlSourceMode}
			<Button
				variant={showDbml ? 'secondary' : 'ghost'}
				size="sm"
				onclick={onToggleDbml}
				aria-pressed={showDbml}
			>
				{showDbml ? 'Hide DBML' : 'Show DBML'}
			</Button>
		{/if}
	{/snippet}
</StatusBar>
