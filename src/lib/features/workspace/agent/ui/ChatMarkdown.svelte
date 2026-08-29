<script lang="ts">
	import { cn } from '$core/util'
	import { parseChatMarkdown } from '../markdown'
	import type { MdBlock, MdInline } from '../markdown-types'
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
</script>

{#snippet inlineNodes(nodes: MdInline[])}
	{#each nodes ?? [] as node, i (i)}
		{#if node.type === 'text'}{node.value}{:else if node.type === 'break'}<br
			/>{:else if node.type === 'code'}<code
				class="rounded-sm bg-input px-1 py-px font-mono text-caption text-syntax-name"
				>{node.value}</code
			>{:else if node.type === 'strong'}<strong class="font-semibold text-fg"
				>{@render inlineNodes(node.children)}</strong
			>{:else if node.type === 'em'}<em class="italic"
				>{@render inlineNodes(node.children)}</em
			>{:else if node.type === 'del'}<s class="text-fg-secondary"
				>{@render inlineNodes(node.children)}</s
			>{:else if node.type === 'link' && node.href}<a
				href={node.href}
				target="_blank"
				rel="external noreferrer"
				class="text-accent underline-offset-2 hover:underline"
				>{@render inlineNodes(node.children)}</a
			>{:else if node.type === 'link'}{@render inlineNodes(node.children)}{/if}
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
			{:else}
				<h5 class="m-0 text-label font-semibold tracking-tight text-fg">
					{@render inlineNodes(block.children)}
				</h5>
			{/if}
		{:else if block.type === 'code'}
			<ChatCodeBlock
				code={block.value}
				lang={block.lang}
				{engine}
				{compact}
				{onApply}
			/>
		{:else if block.type === 'ul'}
			<ul class="m-0 list-disc space-y-1 p-0 pl-4 marker:text-fg-muted">
				{#each block.items as item, j (j)}
					<li class="min-w-0 pl-0.5">
						{@render blockTree(item.children)}
					</li>
				{/each}
			</ul>
		{:else if block.type === 'ol'}
			<ol
				class="m-0 list-decimal space-y-1 p-0 pl-4 marker:text-fg-muted"
				start={block.start}
			>
				{#each block.items as item, j (j)}
					<li class="min-w-0 pl-0.5">
						{@render blockTree(item.children)}
					</li>
				{/each}
			</ol>
		{:else if block.type === 'blockquote'}
			<blockquote
				class="m-0 border-l border-line-strong pl-3 text-fg-secondary"
			>
				{@render blockTree(block.children)}
			</blockquote>
		{:else if block.type === 'table'}
			<div class="overflow-x-auto rounded-lg border border-line">
				<table class="w-full border-collapse text-left">
					<thead class="bg-elevated">
						<tr>
							{#each block.head as cell, c (c)}
								<th
									class="border-b border-line px-2 py-1 text-caption font-semibold text-fg-secondary"
								>
									{@render inlineNodes(cell)}
								</th>
							{/each}
						</tr>
					</thead>
					<tbody>
						{#each block.rows as row, r (r)}
							<tr class="odd:bg-panel even:bg-elevated">
								{#each row as cell, c (c)}
									<td
										class="border-t border-line px-2 py-1 font-mono text-caption text-fg"
									>
										{@render inlineNodes(cell)}
									</td>
								{/each}
							</tr>
						{/each}
					</tbody>
				</table>
			</div>
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
