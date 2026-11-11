<script lang="ts">
	import { cn, formatRelativeTime } from '#core/util'
	import { CatalogIcon } from '#features/workspace/catalog'
	import { previewSql } from '../display'
	import { historyKindLabel } from './history-api'
	import type { QueryHistoryEntry } from './history-types'

	interface Props {
		items: QueryHistoryEntry[]
		query: string
		selectedId: string | null
		hasMore?: boolean
		loadingMore?: boolean
		onselect: (id: string) => void
		onmore?: () => void
	}

	let {
		items,
		query,
		selectedId,
		hasMore = false,
		loadingMore = false,
		onselect,
		onmore,
	}: Props = $props()

	function sourceLabel(source: QueryHistoryEntry['source']): string {
		if (source === 'agent') return 'Agent'
		if (source === 'mcp') return 'MCP'
		return 'UI'
	}
</script>

{#if items.length === 0}
	<p class="m-2 text-caption leading-relaxed text-fg-muted">
		{#if query.trim()}
			No matches for “{query.trim()}”.
		{:else}
			No query history yet.
		{/if}
	</p>
{:else}
	<ul class="m-0 list-none p-0">
	{#each items as item (item.id)}
		{@const active = selectedId === item.id}
		{@const kindLabel = historyKindLabel(item.kind)}
		<li>
			<button
				type="button"
				class={cn(
					'flex w-full items-start gap-1.5 rounded-md border-none px-2 py-1.5 text-left transition-colors',
					active
						? 'bg-accent-soft text-accent-text'
						: 'bg-transparent text-fg hover:bg-hover',
				)}
				onclick={() => onselect(item.id)}
			>
				<span
					class={cn(
						'mt-0.5 grid size-5 shrink-0 place-items-center rounded-sm',
						active ? 'bg-accent/15' : 'bg-field',
					)}
					aria-hidden="true"
				>
					<CatalogIcon name="query" class="size-3" tone="accent" />
				</span>
				<span class="min-w-0 flex-1">
					<span
						class={cn(
							'line-clamp-2 font-mono text-label',
							active ? 'text-accent-text' : 'text-fg',
						)}
					>
						{previewSql(item.sql)}
					</span>
					<span
						class={cn(
							'flex flex-wrap items-center gap-1.5 text-caption',
							active ? 'text-accent-text/70' : 'text-fg-muted',
						)}
					>
						<span
							class={item.status === 'success' ? 'text-accent-text' : 'text-danger'}
						>
							{item.status === 'success' ? 'ok' : 'err'}
						</span>
						<span>·</span>
						<span>{formatRelativeTime(item.ranAt)}</span>
						{#if item.durationMs != null}
							<span>·</span>
							<span>{item.durationMs}ms</span>
						{/if}
						{#if item.affectedRows != null}
							<span>·</span>
							<span>{item.affectedRows} affected</span>
						{/if}
						{#if item.database}
							<span>·</span>
							<span class="max-w-32 truncate">{item.database}</span>
						{/if}
						<span>·</span>
						<span>{sourceLabel(item.source)}</span>
						{#if kindLabel}
							<span>·</span>
							<span class="rounded-sm bg-field px-1 text-fg-secondary">{kindLabel}</span>
						{/if}
					</span>
				</span>
			</button>
		</li>
	{/each}
	</ul>
{/if}
{#if hasMore}
	<button
		type="button"
		class="mx-2 mt-1 rounded-md border-none bg-transparent px-2 py-1.5 text-caption text-fg-muted hover:bg-hover hover:text-fg disabled:opacity-45"
		disabled={loadingMore}
		onclick={() => onmore?.()}
	>
		{loadingMore ? 'Loading…' : 'Older runs'}
	</button>
{/if}
