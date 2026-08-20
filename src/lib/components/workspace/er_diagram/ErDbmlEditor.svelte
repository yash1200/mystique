<script lang="ts">
	import { Button } from '$lib/components/core'

	interface Props {
		text: string
		parseError: string | null
		copied: boolean
		onclose: () => void
		onapply: () => void
		onfromcanvas: () => void
		oncopy: () => void
	}

	let {
		text = $bindable(''),
		parseError,
		copied,
		onclose,
		onapply,
		onfromcanvas,
		oncopy,
	}: Props = $props()
</script>

<div
	class="flex h-control shrink-0 items-center justify-between gap-2 border-b border-line px-3"
>
	<div class="min-w-0">
		<p class="text-label font-semibold tracking-wider text-fg-muted uppercase">
			DBML editor
		</p>
		<p class="truncate text-caption text-fg-muted">Text → diagram</p>
	</div>
	<Button variant="ghost" size="sm" onclick={onclose}>Close</Button>
</div>
<div class="flex min-h-0 flex-1 flex-col gap-2 p-3">
	<div class="flex flex-wrap gap-1.5">
		<Button variant="primary" size="sm" onclick={onapply}>Apply to canvas</Button>
		<Button variant="secondary" size="sm" onclick={onfromcanvas}>
			From canvas
		</Button>
		<Button variant="ghost" size="sm" onclick={oncopy}>
			{copied ? 'Copied' : 'Copy'}
		</Button>
	</div>
	<textarea
		class="min-h-0 w-full flex-1 resize-none rounded-md border border-line bg-input p-2.5 font-mono text-caption leading-relaxed text-fg placeholder:text-fg-muted focus:border-focus focus:outline-none focus:ring-2 focus:ring-accent-muted"
		bind:value={text}
		spellcheck="false"
		aria-label="DBML source"
		placeholder="Table users &#123;&#10;  id integer [primary key]&#10;&#125;"
	></textarea>
	{#if parseError}
		<p
			class="rounded-md border border-danger/40 bg-danger/10 px-2 py-1.5 text-caption text-danger"
			role="alert"
		>
			{parseError}
		</p>
	{/if}
	<p class="text-caption text-fg-muted">
		Write DBML, then <strong class="text-fg-secondary">Apply</strong>. The
		snapshot panel stays hidden in this mode.
		<a
			class="text-accent hover:underline"
			href="https://dbml.dbdiagram.io/docs"
			target="_blank"
			rel="noreferrer">Docs</a
		>
	</p>
</div>
