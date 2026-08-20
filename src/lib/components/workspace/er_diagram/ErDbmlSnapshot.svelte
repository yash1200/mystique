<script lang="ts">
	import { Button } from '$lib/components/core'

	interface Props {
		text: string
		busy: boolean
		copied: boolean
		oncopy: () => void
	}

	let { text, busy, copied, oncopy }: Props = $props()
</script>

<aside
	class="flex w-[min(22rem,36vw)] shrink-0 flex-col border-l border-line bg-panel"
	aria-label="DBML text view"
>
	<div
		class="flex h-control shrink-0 items-center justify-between gap-2 border-b border-line px-3"
	>
		<div class="min-w-0">
			<p class="text-label font-semibold tracking-wider text-fg-muted uppercase">
				DBML
			</p>
			<p class="truncate text-caption text-fg-muted">
				{busy ? 'Generating…' : 'Snapshot · not live while dragging'}
			</p>
		</div>
		<Button variant="ghost" size="sm" onclick={oncopy} disabled={busy && !text}>
			{copied ? 'Copied' : 'Copy'}
		</Button>
	</div>
	<div class="flex min-h-0 flex-1 flex-col bg-input">
		{#if busy && !text}
			<p class="p-3 text-caption text-fg-muted">Generating DBML…</p>
		{:else}
			<textarea
				class="min-h-0 w-full flex-1 resize-none border-0 bg-transparent p-3 font-mono text-caption leading-relaxed text-fg focus:outline-none"
				readonly
				wrap="off"
				spellcheck="false"
				aria-label="DBML snapshot"
				value={text}
			></textarea>
		{/if}
	</div>
	<p class="shrink-0 border-t border-line px-3 py-2 text-caption text-fg-muted">
		Rebuilds when the schema changes, not on drag.
		<a
			class="text-accent hover:underline"
			href="https://dbml.dbdiagram.io/docs"
			target="_blank"
			rel="noreferrer">DBML docs</a
		>.
	</p>
</aside>
