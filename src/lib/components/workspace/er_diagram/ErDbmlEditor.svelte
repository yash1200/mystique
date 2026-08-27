<script lang="ts">
	import { Button, Icon, IconButton, Pill, Textarea } from '$lib/components/core'

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

<header
	class="flex shrink-0 items-center gap-2 border-b border-line bg-elevated px-2.5 py-2"
>
	<span
		class="grid size-6 shrink-0 place-items-center rounded-md bg-input"
		aria-hidden="true"
	>
		<Icon name="code-slash" class="size-3.5 text-info" />
	</span>
	<div class="flex min-w-0 flex-1 items-center gap-1.5">
		<h2 class="m-0 min-w-0 truncate font-mono text-label font-semibold text-fg">
			DBML
		</h2>
		<Pill tone="neutral" uppercase>Editor</Pill>
	</div>
	<IconButton label="Close DBML editor" size="sm" variant="quiet" onclick={onclose}>
		<Icon name="close" />
	</IconButton>
</header>
<div class="flex min-h-0 flex-1 flex-col">
	<div
		class="flex shrink-0 items-center gap-1.5 border-b border-line bg-elevated px-2.5 py-1.5"
	>
		<Button variant="primary" size="sm" onclick={onapply}>Apply</Button>
		<Button variant="ghost" size="sm" onclick={onfromcanvas}>
			From canvas
		</Button>
		<Button variant="ghost" size="sm" onclick={oncopy}>
			{copied ? 'Copied' : 'Copy'}
		</Button>
	</div>
	<Textarea
		class="!min-h-0 flex-1 rounded-none border-0 bg-input font-mono text-caption leading-relaxed"
		bind:value={text}
		resize="none"
		spellcheck={false}
		aria-label="DBML source"
		placeholder={'Table users {\n  id integer [primary key]\n}'}
	/>
	{#if parseError}
		<p
			class="shrink-0 border-t border-danger/40 bg-danger/10 px-2.5 py-1.5 text-caption text-danger"
			role="alert"
		>
			{parseError}
		</p>
	{/if}
	<p class="shrink-0 border-t border-line px-2.5 py-1.5 text-caption text-fg-muted">
		Write DBML, then Apply to rebuild the canvas.
		<a
			class="text-accent hover:underline"
			href="https://dbml.dbdiagram.io/docs"
			target="_blank"
			rel="noreferrer">Docs</a
		>
	</p>
</div>
