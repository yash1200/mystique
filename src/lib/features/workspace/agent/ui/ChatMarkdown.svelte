<script lang="ts">
	import { cn } from '#core/util'
	import { parseChatMarkdown } from '../markdown'
	import { renderMath } from '../math'
	import type {
		MdAlertKind,
		MdAlign,
		MdBlock,
		MdInline,
	} from '../markdown-types'
	import ChatCodeBlock from './ChatCodeBlock.svelte'

	interface Props {
		source?: string
		blocks?: MdBlock[]
		engine: string
		/** Tighter type + spacing for user bubbles. */
		compact?: boolean
		onApply?: (sql: string) => void
	}

	let {
		source = '',
		blocks,
		engine,
		compact = false,
		onApply,
	}: Props = $props()

	const text = $derived(String(source ?? ''))
	const tree = $derived.by((): MdBlock[] => {
		if (blocks) return blocks
		try {
			return parseChatMarkdown(text)
		} catch {
			return text.trim()
				? [{ type: 'p', children: [{ type: 'text', value: text }] }]
				: []
		}
	})

	const alertTone: Record<
		MdAlertKind,
		{ label: string; box: string }
	> = {
		note: { label: 'Note', box: 'border-info/35 bg-info/12' },
		tip: { label: 'Tip', box: 'border-accent-text/35 bg-accent-soft' },
		important: { label: 'Important', box: 'border-accent/35 bg-accent/10' },
		warning: { label: 'Warning', box: 'border-warning/35 bg-warning/12' },
		caution: { label: 'Caution', box: 'border-danger/35 bg-danger/12' },
	}

	function alignClass(align: MdAlign | null | undefined): string {
		if (align === 'center') return 'text-center'
		if (align === 'right') return 'text-right'
		return 'text-left'
	}
</script>

{#snippet inlineNodes(nodes: MdInline[])}
	{#each nodes ?? [] as node, i (i)}
		{#if node.type === 'text'}{node.value}{:else if node.type === 'break'}<br
			/>{:else if node.type === 'code'}<code
				class="rounded-sm bg-field px-1 py-px font-mono text-caption text-syntax-name"
				>{node.value}</code
			>{:else if node.type === 'strong'}<strong class="font-semibold text-fg"
				>{@render inlineNodes(node.children)}</strong
			>{:else if node.type === 'em'}<em class="italic"
				>{@render inlineNodes(node.children)}</em
			>{:else if node.type === 'del'}<s class="text-fg-secondary"
				>{@render inlineNodes(node.children)}</s
			>{:else if node.type === 'link' && node.href}<a
				href={node.href}
				title={node.title}
				target="_blank"
				rel="external noreferrer"
				class="text-accent-text underline-offset-2 hover:underline"
				>{@render inlineNodes(node.children)}</a
			>{:else if node.type === 'link'}{@render inlineNodes(
				node.children,
			)}{:else if node.type === 'image' && node.src}<img
				src={node.src}
				alt={node.alt}
				title={node.title}
				referrerpolicy="no-referrer"
				class="my-1 max-h-80 max-w-full rounded-md"
			/>{:else if node.type === 'image'}{node.alt}{:else if node.type ===
				'math'}<span class="italic text-fg">{renderMath(node.value)}</span
			>{/if}
	{/each}
{/snippet}

{#snippet blockTree(items: MdBlock[])}
	{#each items ?? [] as block, i (i)}
		{#if block.type === 'p'}
			<p
				class={cn(
					'm-0 min-w-0 wrap-break-word text-pretty text-fg',
					compact ? 'text-label leading-relaxed' : 'text-body leading-relaxed',
				)}
			>
				{@render inlineNodes(block.children)}
			</p>
		{:else if block.type === 'h'}
			{#if block.level === 1}
				<h3 class="m-0 text-body-lg font-semibold tracking-tight text-fg">
					{@render inlineNodes(block.children)}
				</h3>
			{:else if block.level === 2}
				<h4 class="m-0 text-body font-semibold text-fg">
					{@render inlineNodes(block.children)}
				</h4>
			{:else if block.level === 3}
				<h5 class="m-0 text-label font-semibold tracking-tight text-fg">
					{@render inlineNodes(block.children)}
				</h5>
			{:else}
				<h6 class="m-0 text-caption font-semibold tracking-wide text-fg">
					{@render inlineNodes(block.children)}
				</h6>
			{/if}
		{:else if block.type === 'code'}
			<ChatCodeBlock
				code={block.value}
				lang={block.lang}
				{engine}
				{compact}
				{onApply}
			/>
		{:else if block.type === 'ul' || block.type === 'ol'}
			{@const tasky = block.items.some((it) => it.checked !== undefined)}
			<svelte:element
				this={block.type === 'ol' ? 'ol' : 'ul'}
				class={cn(
					'm-0 space-y-1 p-0',
					tasky
						? 'list-none pl-0'
						: block.type === 'ol'
							? 'list-decimal pl-4 marker:text-fg-muted'
							: 'list-disc pl-4 marker:text-fg-muted',
				)}
				start={block.type === 'ol' ? block.start : undefined}
			>
				{#each block.items as item, j (j)}
					<li class="min-w-0 pl-0.5">
						{#if item.checked !== undefined}
							<label class="flex items-start gap-2">
								<input
									type="checkbox"
									class="mt-1 size-3.5 shrink-0 accent-accent"
									checked={item.checked}
									disabled
								/>
								<div class="min-w-0 flex-1">
									{@render blockTree(item.children)}
								</div>
							</label>
						{:else}
							{@render blockTree(item.children)}
						{/if}
					</li>
				{/each}
			</svelte:element>
		{:else if block.type === 'blockquote'}
			<blockquote
				class="m-0 border-l border-line-strong pl-3 text-fg-secondary"
			>
				{@render blockTree(block.children)}
			</blockquote>
		{:else if block.type === 'alert'}
			<div
				class={cn(
					'm-0 rounded-md border px-3 py-2',
					alertTone[block.kind].box,
				)}
			>
				<p class="m-0 mb-1 text-caption font-semibold tracking-wide text-fg">
					{alertTone[block.kind].label}
				</p>
				<div class="flex min-w-0 flex-col gap-2">
					{@render blockTree(block.children)}
				</div>
			</div>
		{:else if block.type === 'table'}
			<div class="overflow-x-auto rounded-lg border border-line">
				<table class="w-full border-collapse text-left">
					<thead class="bg-raised">
						<tr>
							{#each block.head as cell, c (c)}
								<th
									class={cn(
										'border-b border-line px-2 py-1 text-caption font-semibold text-fg-secondary',
										alignClass(block.align?.[c]),
									)}
								>
									{@render inlineNodes(cell)}
								</th>
							{/each}
						</tr>
					</thead>
					<tbody>
						{#each block.rows as row, r (r)}
							<tr class="odd:bg-surface even:bg-raised">
								{#each row as cell, c (c)}
									<td
										class={cn(
											'border-t border-line px-2 py-1 font-mono text-caption text-fg',
											alignClass(block.align?.[c]),
										)}
									>
										{@render inlineNodes(cell)}
									</td>
								{/each}
							</tr>
						{/each}
					</tbody>
				</table>
			</div>
		{:else if block.type === 'math'}
			<p
				class="m-0 py-0.5 text-center italic text-fg"
			>
				{renderMath(block.value)}
			</p>
		{:else if block.type === 'hr'}
			<hr class="m-1 border-0 border-t border-line" />
		{/if}
	{/each}
{/snippet}

<div class={cn('flex min-w-0 flex-col', compact ? 'gap-1.5' : 'gap-2.5')}>
	{#if tree.length}
		{@render blockTree(tree)}
	{:else if text.trim()}
		<p
			class={cn(
				'm-0 min-w-0 whitespace-pre-wrap wrap-break-word text-fg',
				compact ? 'text-label leading-relaxed' : 'text-body leading-relaxed',
			)}
		>
			{text}
		</p>
	{/if}
</div>
