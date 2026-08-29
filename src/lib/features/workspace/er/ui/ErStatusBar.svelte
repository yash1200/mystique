<script lang="ts">
	import { Button, Icon, IconButton, StatusBar } from '$core/ui'
	import { cn } from '$core/util'
	interface Props {
		entityWord: string
		hasDiagram: boolean
		nodeCount: number
		edgeCount: number
		selectionCount: number
		hasNodes: boolean
		showGrid: boolean
		showDbml: boolean
		canZoomOut: boolean
		canZoomIn: boolean
		zoomLabel: string
		refreshing?: boolean
		exportingPng?: boolean
		onAutoLayout: () => void
		onDownloadPng: () => void
		onRefresh?: () => void
		onZoomOut: () => void
		onZoomIn: () => void
		onResetZoom: () => void
		onToggleGrid: () => void
		onToggleDbml: () => void
	}

	let {
		entityWord,
		hasDiagram,
		nodeCount,
		edgeCount,
		selectionCount,
		hasNodes,
		showGrid,
		showDbml,
		canZoomOut,
		canZoomIn,
		zoomLabel,
		refreshing = false,
		exportingPng = false,
		onAutoLayout,
		onDownloadPng,
		onRefresh,
		onZoomOut,
		onZoomIn,
		onResetZoom,
		onToggleGrid,
		onToggleDbml,
	}: Props = $props()
</script>

<StatusBar class="pl-1!" ariaLabel="ER diagram status">
	{#snippet left()}
		<Button
			variant="ghost"
			size="sm"
			class="px-1.5!"
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
			<span class="truncate font-mono tabular-nums text-fg-muted"
				>No diagram</span
			>
		{/if}
		{#if selectionCount > 0}
			<span class="shrink-0 text-fg-muted">{selectionCount} selected</span>
		{/if}
	{/snippet}
	{#snippet center()}
		<div
			class="flex items-center gap-1.5"
			role="group"
			aria-label="Canvas zoom"
		>
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
			label={exportingPng ? 'Downloading PNG…' : 'Download PNG'}
			size="sm"
			variant="quiet"
			onclick={onDownloadPng}
			disabled={!hasNodes || exportingPng}
			aria-busy={exportingPng}
			class={exportingPng ? 'opacity-45' : ''}
		>
			<Icon name="download" />
		</IconButton>
		<IconButton
			label={showGrid ? 'Hide grid' : 'Show grid'}
			size="sm"
			variant="quiet"
			active={showGrid}
			onclick={onToggleGrid}
		>
			<Icon name="grid" />
		</IconButton>
		<IconButton
			label={showDbml ? 'Hide DBML snapshot' : 'Show DBML snapshot'}
			size="sm"
			variant="quiet"
			active={showDbml}
			onclick={onToggleDbml}
		>
			<Icon name="code" />
		</IconButton>
	{/snippet}
</StatusBar>
