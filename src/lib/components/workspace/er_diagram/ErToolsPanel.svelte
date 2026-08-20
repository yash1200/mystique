<script lang="ts">
	import { Button, Input } from '$lib/components/core'
	import type { ErEdge, ErNode } from '$lib/types/er'
	import { cn } from '$lib/utils/cn'
	import { isBrokenEdge } from './erConstants'

	interface Props {
		entityWord: string
		nodeLabel: string
		tableQuery: string
		searchWrap: HTMLDivElement | null
		navNodes: ErNode[]
		showGrid: boolean
		showDbml: boolean
		canUndo: boolean
		canRedo: boolean
		arrowTool: 'normal' | 'broken' | null
		linkFrom: boolean
		selectedEdgeId: string | null
		toolMsg: string | null
		edges: ErEdge[]
		hasNodes: boolean
		onfocus: (id: string) => void
		onaddtable: () => void
		onundo: () => void
		onredo: () => void
		ontogglegrid: () => void
		onopendbmleditor: () => void
		ontoggledbml: () => void
		onautolayout: () => void
		onsetarrow: (kind: 'normal' | 'broken' | null) => void
		oncancellink: () => void
		onstraighten: () => void
		onselectedge: (id: string) => void
		ondeleteedge: (id: string) => void
	}

	let {
		entityWord,
		nodeLabel,
		tableQuery = $bindable(''),
		searchWrap = $bindable<HTMLDivElement | null>(null),
		navNodes,
		showGrid,
		showDbml,
		canUndo,
		canRedo,
		arrowTool,
		linkFrom,
		selectedEdgeId,
		toolMsg,
		edges,
		hasNodes,
		onfocus,
		onaddtable,
		onundo,
		onredo,
		ontogglegrid,
		onopendbmleditor,
		ontoggledbml,
		onautolayout,
		onsetarrow,
		oncancellink,
		onstraighten,
		onselectedge,
		ondeleteedge,
	}: Props = $props()
</script>

<div class="flex min-h-0 flex-1 flex-col gap-2 overflow-y-auto p-3">
	<p class="text-label font-semibold tracking-wider text-fg-muted uppercase">
		Tools
	</p>
	<div bind:this={searchWrap}>
		<Input
			bind:value={tableQuery}
			inputSize="sm"
			placeholder={`Find ${entityWord}`}
			aria-label={`Find ${entityWord}`}
		/>
	</div>
	{#if tableQuery.trim()}
		<ul class="m-0 max-h-36 list-none space-y-0.5 overflow-auto p-0">
			{#each navNodes as node (node.id)}
				<li>
					<button
						type="button"
						class="flex w-full items-center justify-between gap-2 rounded-md px-2 py-1 text-left font-mono text-caption text-fg-secondary hover:bg-hover"
						onclick={() => onfocus(node.id)}
					>
						<span class="min-w-0 truncate">{node.name}</span>
						<span class="text-micro text-fg-muted">{node.columns.length}</span>
					</button>
				</li>
			{:else}
				<li class="px-2 py-1 text-caption text-fg-muted">No matches.</li>
			{/each}
		</ul>
	{/if}
	<Button
		variant="primary"
		fullWidth
		onclick={(e: MouseEvent) => {
			e.preventDefault()
			e.stopPropagation()
			onaddtable()
		}}
	>
		+ Add {nodeLabel.toLowerCase()}
	</Button>

	<div class="grid grid-cols-2 gap-2">
		<Button
			variant="secondary"
			size="sm"
			fullWidth
			onclick={onundo}
			disabled={!canUndo}
		>
			Undo
		</Button>
		<Button
			variant="secondary"
			size="sm"
			fullWidth
			onclick={onredo}
			disabled={!canRedo}
		>
			Redo
		</Button>
	</div>

	<button
		type="button"
		class={cn(
			'flex w-full items-center justify-between rounded-lg border px-3 py-2 text-left text-caption font-medium transition-colors',
			showGrid
				? 'border-accent bg-accent-muted text-fg'
				: 'border-line bg-input text-fg-secondary hover:border-line-strong',
		)}
		onclick={ontogglegrid}
		aria-pressed={showGrid}
	>
		<span>Grid</span>
		<span class="font-mono text-micro tracking-wide uppercase">
			{showGrid ? 'ON' : 'OFF'}
		</span>
	</button>

	<button
		type="button"
		class={cn(
			'flex w-full items-center justify-between rounded-lg border px-3 py-2 text-left text-caption font-medium transition-colors',
			'border-line bg-input text-fg-secondary hover:border-line-strong',
		)}
		onclick={onopendbmleditor}
	>
		<span>DBML editor</span>
		<span class="font-mono text-micro tracking-wide uppercase">Text</span>
	</button>

	<button
		type="button"
		class={cn(
			'flex w-full items-center justify-between rounded-lg border px-3 py-2 text-left text-caption font-medium transition-colors',
			showDbml
				? 'border-accent bg-accent-muted text-fg'
				: 'border-line bg-input text-fg-secondary hover:border-line-strong',
		)}
		onclick={ontoggledbml}
		aria-pressed={showDbml}
	>
		<span>DBML snapshot</span>
		<span class="font-mono text-micro tracking-wide uppercase">
			{showDbml ? 'ON' : 'OFF'}
		</span>
	</button>

	<Button
		variant="secondary"
		size="sm"
		fullWidth
		onclick={onautolayout}
		disabled={!hasNodes}
	>
		Auto-layout
	</Button>

	<p class="mt-2 text-label font-semibold tracking-wider text-fg-muted uppercase">
		Arrows
	</p>
	<div class="grid grid-cols-2 gap-2">
		<button
			type="button"
			class={cn(
				'flex flex-col items-center gap-1 rounded-lg border px-2 py-3 transition-colors',
				arrowTool === 'normal'
					? 'border-accent bg-accent-muted'
					: 'border-line bg-input hover:border-line-strong',
			)}
			onclick={() => onsetarrow(arrowTool === 'normal' ? null : 'normal')}
		>
			<svg width="36" height="14" class="text-fg" aria-hidden="true">
				<defs>
					<marker
						id="tn"
						markerWidth="6"
						markerHeight="6"
						refX="5"
						refY="3"
						orient="auto"
					>
						<path d="M0,0 L6,3 L0,6 Z" fill="currentColor" />
					</marker>
				</defs>
				<line
					x1="2"
					y1="7"
					x2="28"
					y2="7"
					stroke="currentColor"
					stroke-width="2"
					marker-end="url(#tn)"
				/>
			</svg>
			<span class="text-caption font-medium text-fg">Normal</span>
		</button>
		<button
			type="button"
			class={cn(
				'flex flex-col items-center gap-1 rounded-lg border px-2 py-3 transition-colors',
				arrowTool === 'broken'
					? 'border-accent bg-accent-muted'
					: 'border-line bg-input hover:border-line-strong',
			)}
			onclick={() => onsetarrow(arrowTool === 'broken' ? null : 'broken')}
		>
			<svg width="36" height="14" class="text-fg" aria-hidden="true">
				<defs>
					<marker
						id="tb"
						markerWidth="6"
						markerHeight="6"
						refX="5"
						refY="3"
						orient="auto"
					>
						<path d="M0,0 L6,3 L0,6 Z" fill="currentColor" />
					</marker>
				</defs>
				<line
					x1="2"
					y1="7"
					x2="28"
					y2="7"
					stroke="currentColor"
					stroke-width="2"
					stroke-dasharray="4 3"
					marker-end="url(#tb)"
				/>
			</svg>
			<span class="text-caption font-medium text-fg">Broken</span>
		</button>
	</div>

	<ol class="m-0 list-decimal space-y-1 pl-4 text-caption text-fg-muted">
		<li>DBML editor → write text → Apply</li>
		<li>Or draw tables / 90° links on canvas</li>
		<li>Show DBML to copy a snapshot</li>
		<li>Ctrl+Z undo · / find · Shift+Ctrl+L layout</li>
	</ol>

	{#if linkFrom}
		<Button variant="secondary" fullWidth onclick={oncancellink}
			>Cancel link</Button
		>
	{/if}
	{#if selectedEdgeId}
		<Button variant="ghost" fullWidth onclick={onstraighten}>
			Straighten selected link
		</Button>
	{/if}

	{#if toolMsg}
		<p
			class="rounded-md border border-line bg-input px-2 py-1.5 text-caption text-fg-secondary"
			role="status"
		>
			{toolMsg}
		</p>
	{/if}

	{#if edges.length > 0}
		<div class="border-t border-line pt-2">
			<p class="mb-1 text-label font-semibold text-fg-muted uppercase">Links</p>
			<ul class="m-0 max-h-36 list-none space-y-1 overflow-auto p-0">
				{#each edges as edge (edge.id)}
					<li
						class={cn(
							'flex w-full items-start justify-between gap-1 rounded-md border px-1 py-0.5 text-caption',
							selectedEdgeId === edge.id
								? 'border-accent bg-accent-muted'
								: 'border-line bg-input',
						)}
					>
						<button
							type="button"
							class="min-w-0 flex-1 truncate border-none bg-transparent px-1 py-1 text-left text-fg-secondary"
							onclick={() => onselectedge(edge.id)}
						>
							{isBrokenEdge(edge.kind) ? '┄' : '─'}
							{edge.label}
						</button>
						<button
							type="button"
							class="shrink-0 border-none bg-transparent px-1 py-1 text-danger"
							aria-label="Delete relationship"
							onclick={() => ondeleteedge(edge.id)}
						>
							×
						</button>
					</li>
				{/each}
			</ul>
		</div>
	{/if}
</div>
