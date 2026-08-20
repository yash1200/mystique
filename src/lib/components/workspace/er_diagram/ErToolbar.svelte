<script lang="ts">
	import { Badge, Button, IconButton, Input } from '$lib/components/core'

	interface Props {
		embedded: boolean
		diagramName: string
		title: string
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
		saving: boolean
		canZoomOut: boolean
		canZoomIn: boolean
		zoomLabel: string
		onAutoLayout: () => void
		onZoomOut: () => void
		onZoomIn: () => void
		onResetZoom: () => void
		onFitZoom: () => void
		onToggleGrid: () => void
		onToggleDbmlEditor: () => void
		onToggleDbml: () => void
		onLoadLatest: () => void
		onSave: () => void
	}

	let {
		embedded,
		diagramName = $bindable(''),
		title,
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
		saving,
		canZoomOut,
		canZoomIn,
		zoomLabel,
		onAutoLayout,
		onZoomOut,
		onZoomIn,
		onResetZoom,
		onFitZoom,
		onToggleGrid,
		onToggleDbmlEditor,
		onToggleDbml,
		onLoadLatest,
		onSave,
	}: Props = $props()
</script>

<div
	class="flex h-topbar shrink-0 items-center gap-3 border-b border-line bg-panel px-3"
>
	<div class="flex min-w-0 shrink-0 items-center gap-2">
		<span
			class="max-w-48 truncate text-body font-semibold text-fg sm:max-w-[18rem]"
		>
			{embedded ? diagramName || 'Live schema' : `ER · ${title}`}
		</span>
		{#if embedded}
			<Badge tone="info" size="sm">Live</Badge>
		{/if}
		{#if dirty}
			<Badge tone="warning" size="sm">Unsaved</Badge>
		{/if}
		{#if hasDiagram}
			<Badge tone="neutral" size="sm">
				{nodeCount}
				{entityWord}{nodeCount === 1 ? '' : 's'}
				{#if edgeCount > 0}
					· {edgeCount} link{edgeCount === 1 ? '' : 's'}
				{/if}
			</Badge>
		{/if}
		{#if selectionCount > 0}
			<Badge tone="neutral" size="sm">{selectionCount} selected</Badge>
		{/if}
		{#if arrowTool}
			<Badge tone="accent" size="sm">
				{arrowTool === 'broken' ? 'Broken arrow' : 'Normal arrow'}
				{#if linkFrom}
					· pick target
				{/if}
			</Badge>
		{/if}
	</div>

	{#if !readOnly}
		<div class="min-w-32 flex-1">
			<Input
				bind:value={diagramName}
				class="w-full"
				inputSize="sm"
				aria-label="Diagram name"
			/>
		</div>
	{:else}
		<div class="min-w-0 flex-1"></div>
	{/if}

	<div class="flex shrink-0 items-center gap-2">
		<Button
			variant="secondary"
			size="sm"
			onclick={onAutoLayout}
			disabled={!hasNodes}
		>
			Auto-layout
		</Button>
		<div class="flex items-center gap-1">
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
				class="h-control-sm rounded-md border border-line bg-input px-2 font-mono text-caption text-fg hover:border-line-strong"
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
			<Button
				variant="ghost"
				size="sm"
				onclick={onFitZoom}
				disabled={!hasNodes}
			>
				Fit
			</Button>
		</div>
		<Button
			variant={showGrid ? 'secondary' : 'ghost'}
			size="sm"
			onclick={onToggleGrid}
			aria-pressed={showGrid}
		>
			{showGrid ? 'Grid on' : 'Grid'}
		</Button>
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
		{#if !readOnly}
			<Button variant="secondary" size="sm" onclick={onLoadLatest}
				>Load last</Button
			>
			<Button variant="primary" size="sm" onclick={onSave} disabled={saving}>
				{saving ? 'Saving…' : 'Save'}
			</Button>
		{/if}
	</div>
</div>
