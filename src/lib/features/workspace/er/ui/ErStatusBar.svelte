<script lang="ts">
	import { Button, Icon, IconButton, StatusBar } from '#core/ui'
	import { cn } from '#core/util'
	import { StatusNote } from '#features/toast'
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

	const step =
		'grid h-full w-7 place-items-center border-none bg-transparent text-body leading-none text-fg-secondary transition-colors enabled:hover:bg-hover enabled:hover:text-fg disabled:opacity-40'
</script>

<StatusBar class="pl-1.5!" ariaLabel="ER diagram status">
	{#snippet left()}
		<Button
			variant="secondary"
			size="sm"
			onclick={onAutoLayout}
			disabled={!hasNodes}
			title="Arrange the tables on the canvas"
		>
			<Icon name="layout" />Auto layout
		</Button>
		<span class="ml-1 min-w-0 truncate font-mono text-fg-muted tabular-nums">
			{#if hasDiagram}
				{nodeCount}
				{entityWord}{nodeCount === 1 ? '' : 's'}
				{#if edgeCount > 0}· {edgeCount} link{edgeCount === 1 ? '' : 's'}{/if}
				{#if selectionCount > 0}· {selectionCount} selected{/if}
			{:else}
				No diagram
			{/if}
		</span>
		<StatusNote />
	{/snippet}
	{#snippet center()}
		<div
			class="flex h-6.5 items-center overflow-hidden rounded-lg border border-line bg-field"
			role="group"
			aria-label="Canvas zoom"
		>
			<button type="button" class={step} aria-label="Zoom out" title="Zoom out" onclick={onZoomOut} disabled={!canZoomOut}>
				−
			</button>
			<button
				type="button"
				class="h-full min-w-13 border-x border-y-0 border-line bg-transparent px-2 font-mono text-caption font-medium text-fg tabular-nums hover:bg-hover"
				onclick={onResetZoom}
				title="Reset zoom to 100%"
				aria-label="Zoom {zoomLabel}. Reset to 100%"
			>
				{zoomLabel}
			</button>
			<button type="button" class={step} aria-label="Zoom in" title="Zoom in" onclick={onZoomIn} disabled={!canZoomIn}>
				+
			</button>
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
			<Icon name="hash" />
		</IconButton>
		<IconButton
			label={exportingPng ? 'Downloading PNG…' : 'Download PNG'}
			size="sm"
			variant="quiet"
			onclick={onDownloadPng}
			disabled={!hasNodes || exportingPng}
			aria-busy={exportingPng}
		>
			<Icon name="download" />
		</IconButton>
		<IconButton
			label={showDbml ? 'Hide DBML' : 'Show DBML'}
			size="sm"
			variant="quiet"
			active={showDbml}
			onclick={onToggleDbml}
		>
			<Icon name="code" />
		</IconButton>
	{/snippet}
</StatusBar>
