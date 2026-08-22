<script lang="ts">
	import type { MdBlock } from '$lib/utils/chatMarkdown'
	import { parseChatMarkdown } from '$lib/utils/chatMarkdown'
	import { cn } from '$lib/utils/cn'
	import ChatCodeBlock from './ChatCodeBlock.svelte'
	import ChatMdInline from './ChatMdInline.svelte'

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

	const tree = $derived(blocks ?? parseChatMarkdown(source))
</script>

<div class={cn('flex min-w-0 flex-col', compact ? 'gap-1.5' : 'gap-2')}>
	{#each tree as block, i (i)}
		{#if block.type === 'p'}
			<p
				class={cn(
					'm-0 min-w-0 wrap-break-word text-fg',
					compact ? 'text-label' : 'text-body',
				)}
			>
				<ChatMdInline nodes={block.children} />
			</p>
		{:else if block.type === 'h'}
			{#if block.level === 1}
				<h3 class="m-0 text-body-lg font-semibold tracking-tight text-fg">
					<ChatMdInline nodes={block.children} />
				</h3>
			{:else if block.level === 2}
				<h4 class="m-0 text-body font-semibold text-fg">
					<ChatMdInline nodes={block.children} />
				</h4>
			{:else}
				<h5
					class="m-0 text-label font-semibold tracking-wide text-fg-secondary uppercase"
				>
					<ChatMdInline nodes={block.children} />
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
						<ChatMarkdown
							blocks={item.children.length ? item.children : []}
							{engine}
							{compact}
							{onApply}
						/>
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
						<ChatMarkdown
							blocks={item.children.length ? item.children : []}
							{engine}
							{compact}
							{onApply}
						/>
					</li>
				{/each}
			</ol>
		{:else if block.type === 'blockquote'}
			<blockquote
				class="m-0 border-l border-line-strong pl-2.5 text-fg-secondary"
			>
				<ChatMarkdown blocks={block.children} {engine} {compact} {onApply} />
			</blockquote>
		{:else if block.type === 'table'}
			<div class="overflow-x-auto rounded-md border border-line">
				<table class="w-full border-collapse text-left">
					<thead class="bg-elevated">
						<tr>
							{#each block.head as cell, c (c)}
								<th
									class="border-b border-line px-2 py-1 text-caption font-semibold text-fg-secondary"
								>
									<ChatMdInline nodes={cell} />
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
										<ChatMdInline nodes={cell} />
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
</div>
