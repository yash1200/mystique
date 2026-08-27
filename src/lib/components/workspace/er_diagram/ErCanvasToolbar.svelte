<script lang="ts">
	import { Button, Icon } from '$lib/components/core'
	import { cn } from '$lib/utils/cn'

	interface Props {
		nodeLabel: string
		canUndo: boolean
		canRedo: boolean
		arrowTool: 'normal' | 'broken' | null
		linkFrom: boolean
		selectedEdgeId: string | null
		onaddtable: () => void
		onundo: () => void
		onredo: () => void
		onsetarrow: (kind: 'normal' | 'broken' | null) => void
		oncancellink: () => void
		onstraighten: () => void
	}

	let {
		nodeLabel,
		canUndo,
		canRedo,
		arrowTool,
		linkFrom,
		selectedEdgeId,
		onaddtable,
		onundo,
		onredo,
		onsetarrow,
		oncancellink,
		onstraighten,
	}: Props = $props()
</script>

<div
	class="flex shrink-0 flex-wrap items-center gap-2 border-b border-line bg-elevated px-2.5 py-1.5"
>
	<div class="flex min-w-0 flex-1 items-center gap-1.5">
		<Button
			variant="primary"
			size="sm"
			onclick={(e: MouseEvent) => {
				e.preventDefault()
				e.stopPropagation()
				onaddtable()
			}}
		>
			<Icon name="plus" class="size-3.5" />
			Add {nodeLabel.toLowerCase()}
		</Button>
		<span class="mx-0.5 h-4 w-px shrink-0 bg-line" aria-hidden="true"></span>
		<Button variant="ghost" size="sm" onclick={onundo} disabled={!canUndo}>
			Undo
		</Button>
		<Button variant="ghost" size="sm" onclick={onredo} disabled={!canRedo}>
			Redo
		</Button>
		{#if linkFrom}
			<span class="mx-0.5 h-4 w-px shrink-0 bg-line" aria-hidden="true"></span>
			<span class="truncate text-caption text-accent">Pick a target column</span>
			<Button variant="ghost" size="sm" onclick={oncancellink}>Cancel</Button>
		{/if}
		{#if selectedEdgeId}
			<span class="mx-0.5 h-4 w-px shrink-0 bg-line" aria-hidden="true"></span>
			<Button variant="ghost" size="sm" onclick={onstraighten}>
				Straighten
			</Button>
		{/if}
	</div>
	<div class="flex shrink-0 items-center gap-1.5">
		<Button
			variant="ghost"
			size="sm"
			class={arrowTool === 'normal'
				? 'border-accent/40 bg-accent-muted text-accent hover:border-accent/50 hover:bg-accent-muted'
				: ''}
			aria-pressed={arrowTool === 'normal'}
			onclick={() => onsetarrow(arrowTool === 'normal' ? null : 'normal')}
			title="Draw a solid column link"
		>
			<svg width="22" height="10" class="text-current" aria-hidden="true">
				<defs>
					<marker
						id="er-tb-normal"
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
					x1="1"
					y1="5"
					x2="16"
					y2="5"
					stroke="currentColor"
					stroke-width="1.75"
					marker-end="url(#er-tb-normal)"
				/>
			</svg>
			Normal
		</Button>
		<Button
			variant="ghost"
			size="sm"
			class={cn(
				arrowTool === 'broken'
					? 'border-accent/40 bg-accent-muted text-accent hover:border-accent/50 hover:bg-accent-muted'
					: '',
			)}
			aria-pressed={arrowTool === 'broken'}
			onclick={() => onsetarrow(arrowTool === 'broken' ? null : 'broken')}
			title="Draw a dashed column link"
		>
			<svg width="22" height="10" class="text-current" aria-hidden="true">
				<defs>
					<marker
						id="er-tb-broken"
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
					x1="1"
					y1="5"
					x2="16"
					y2="5"
					stroke="currentColor"
					stroke-width="1.75"
					stroke-dasharray="3 2.5"
					marker-end="url(#er-tb-broken)"
				/>
			</svg>
			Broken
		</Button>
	</div>
</div>
