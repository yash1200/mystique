<script lang="ts">
	import { Icon, IconButton, Spinner } from '#core/ui'
	import { highlightCode, tokenClass, type SyntaxToken } from '#core/util'

	interface Props {
		text: string
		busy: boolean
		copied: boolean
		oncopy: () => void
		onclose: () => void
	}

	let { text, busy, copied, oncopy, onclose }: Props = $props()

	/** Highlighted tokens, split at newlines so each line gets a number. */
	const lines = $derived.by(() => {
		const rows: SyntaxToken[][] = [[]]
		for (const token of highlightCode('dbml', text.replace(/\n+$/, ''))) {
			const parts = token.text.split('\n')
			parts.forEach((part, i) => {
				if (i > 0) rows.push([])
				if (part) rows[rows.length - 1]!.push({ kind: token.kind, text: part })
			})
		}
		return rows
	})
</script>

<aside
	class="flex w-[min(24rem,38vw)] shrink-0 flex-col border-l border-line bg-surface"
	aria-label="DBML"
>
	<header class="flex h-11 shrink-0 items-center gap-2 border-b border-line pr-1.5 pl-3">
		<span class="flex text-fg-muted [&_svg]:size-4" aria-hidden="true"><Icon name="code" /></span>
		<h2 class="m-0 text-label font-semibold text-fg">DBML</h2>
		<span class="badge badge-sm border border-line-strong text-fg-muted">
			{busy ? 'Generating' : 'Snapshot'}
		</span>
		<span class="flex-1"></span>
		<IconButton
			label={copied ? 'Copied' : 'Copy DBML'}
			size="sm"
			variant="quiet"
			disabled={!text}
			onclick={oncopy}
		>
			<Icon name={copied ? 'check' : 'copy'} />
		</IconButton>
		<IconButton label="Close DBML" size="sm" variant="quiet" onclick={onclose}>
			<Icon name="close" />
		</IconButton>
	</header>

	{#if busy && !text}
		<div class="flex flex-1 items-center justify-center gap-2 text-caption text-fg-muted">
			<Spinner size="sm" />Generating DBML…
		</div>
	{:else}
		<div class="dbml min-h-0 flex-1 overflow-auto bg-app py-2.5" role="region" aria-label="DBML snapshot" tabindex="-1">
			<pre class="m-0 w-max min-w-full font-mono text-caption leading-[1.7] select-text"><code
					>{#each lines as row, n (n)}<span class="flex"><span
								class="sticky left-0 w-10 shrink-0 bg-app pr-3 text-right text-fg-muted/60 tabular-nums select-none"
								aria-hidden="true">{n + 1}</span
							><span class="pr-4 text-fg">{#each row as token, i (i)}<span class={tokenClass(token.kind)}
										>{token.text}</span
									>{/each}</span
							></span
						>{/each}</code
				></pre>
		</div>
	{/if}
</aside>

<style>
	.dbml {
		scrollbar-width: thin;
		scrollbar-color: var(--line-2) transparent;
	}
</style>
