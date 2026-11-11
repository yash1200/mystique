<script lang="ts">
	import type { ExplainPlan, PlanNode } from '#core/contracts'
	import { Icon, IconButton, Segmented } from '#core/ui'
	import { SvelteSet } from 'svelte/reactivity'
	import { selfCostShare } from './explain-cost'

	interface Props {
		plan: ExplainPlan
		/** Back to the results. */
		onclose?: () => void
	}

	let { plan, onclose }: Props = $props()

	const VIEWS = [
		{ value: 'tree', label: 'Tree' },
		{ value: 'raw', label: 'Raw' },
	]

	let tab = $state<'tree' | 'raw'>('tree')
	const collapsed = new SvelteSet<string>()

	const warnings = $derived(plan.warnings ?? [])

	function toggle(path: string) {
		if (collapsed.has(path)) collapsed.delete(path)
		else collapsed.add(path)
	}

	function formatNum(value: number): string {
		if (!Number.isFinite(value)) return ''
		if (Number.isInteger(value)) return String(value)
		return value.toFixed(2)
	}
</script>

{#snippet node(item: PlanNode, path: string)}
	{@const children = item.children ?? []}
	{@const share = selfCostShare(
		item.cost,
		children.map((child) => child.cost),
	)}
	{@const open = !collapsed.has(path)}
	<div class="min-w-0">
		<div class="flex min-w-0 items-start gap-1.5 py-1">
			{#if children.length > 0}
				<button
					type="button"
					class="mt-0.5 grid size-4 shrink-0 place-items-center rounded-xs border-none bg-transparent text-fg-muted hover:bg-hover hover:text-fg [&_svg]:size-3"
					aria-expanded={open}
					aria-label={open ? 'Collapse plan node' : 'Expand plan node'}
					onclick={() => toggle(path)}
				>
					<Icon name={open ? 'chevron-down' : 'chevron-right'} />
				</button>
			{:else}
				<span class="mt-0.5 size-4 shrink-0"></span>
			{/if}
			<div class="min-w-0 flex-1">
				<div class="truncate font-mono text-body text-fg" title={item.name}>
					{item.name}
				</div>
				<div
					class="mt-0.5 flex flex-wrap items-center gap-x-2 gap-y-0.5 font-mono text-caption text-fg-muted"
				>
					{#if item.rows != null}
						<span>rows {formatNum(item.rows)}</span>
					{/if}
					{#if item.cost != null}
						<span>cost {formatNum(item.cost)}</span>
					{/if}
					{#each item.fields ?? [] as field, i (`${i}:${field.label}`)}
						<span>{field.label} {field.value}</span>
					{/each}
				</div>
				{#if share != null}
					<div class="mt-1 flex max-w-56 items-center gap-2">
						<div
							class="h-1.5 min-w-0 flex-1 overflow-hidden rounded-full bg-line"
							title="Share of this node's cost that is not in its children"
						>
							<div
								class="h-full rounded-full bg-accent"
								style="width: {Math.round(share * 100)}%"
							></div>
						</div>
						<span
							class="shrink-0 font-mono text-caption tabular-nums text-fg-muted"
						>
							self {Math.round(share * 100)}%
						</span>
					</div>
				{/if}
			</div>
		</div>
		{#if open && children.length > 0}
			<div class="ml-3 border-l border-line pl-2">
				{#each children as child, index (`${path}/${index}`)}
					{@render node(child, `${path}/${index}`)}
				{/each}
			</div>
		{/if}
	</div>
{/snippet}

<div class="flex min-h-0 min-w-0 flex-1 flex-col bg-app">
	<div class="flex h-10 shrink-0 items-center gap-2 border-b border-line bg-surface pr-1.5 pl-3">
		<span class="text-label font-medium text-fg-secondary">Query plan</span>
		<Segmented size="sm" label="Plan view" value={tab} options={VIEWS} onchange={(v) => (tab = v as 'tree' | 'raw')} />
		{#if onclose}
			<IconButton label="Back to results" variant="quiet" size="sm" class="ml-auto" onclick={onclose}>
				<Icon name="close" />
			</IconButton>
		{/if}
	</div>
	{#if warnings.length > 0}
		<ul class="m-0 shrink-0 list-none space-y-1 border-b border-line bg-warn-soft px-3.5 py-2">
			{#each warnings as warning, i (`${i}:${warning}`)}
				<li class="text-label text-fg">{warning}</li>
			{/each}
		</ul>
	{/if}
	<div class="min-h-0 flex-1 overflow-auto px-3.5 py-2.5">
		{#if tab === 'tree'}
			{@render node(plan.root, '0')}
		{:else}
			<pre
				class="whitespace-pre-wrap wrap-break-word font-mono text-caption text-fg">{plan.raw}</pre>
		{/if}
	</div>
</div>
