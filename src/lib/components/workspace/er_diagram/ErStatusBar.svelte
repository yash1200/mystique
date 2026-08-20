<script lang="ts">
	import { Button, IconButton, Input, StatusBar } from '$lib/components/core'
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
		saving: boolean
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
		onLoadLatest: () => void
		onSave: () => void
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
		saving,
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
		onLoadLatest,
		onSave,
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
		{/if}
		<IconButton
			label={showGrid ? 'Hide grid' : 'Show grid'}
			size="sm"
			variant="ghost"
			active={showGrid}
			onclick={onToggleGrid}
		>
			<svg class="size-3.5" viewBox="0 0 16 16" fill="none" aria-hidden="true">
				<rect
					x="2"
					y="2"
					width="12"
					height="12"
					rx="1.2"
					stroke="currentColor"
					stroke-width="1.3"
				/>
				<path
					d="M2 6.5h12M2 10.5h12M6.5 2v12M10.5 2v12"
					stroke="currentColor"
					stroke-width="1.3"
				/>
			</svg>
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
		{#if !readOnly}
			<Button variant="ghost" size="sm" onclick={onLoadLatest}>Load last</Button
			>
			<Button variant="primary" size="sm" onclick={onSave} disabled={saving}>
				{saving ? 'Saving…' : 'Save'}
			</Button>
		{/if}
	{/snippet}
</StatusBar>
