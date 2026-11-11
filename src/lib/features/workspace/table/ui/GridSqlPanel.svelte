<script lang="ts">
	import { Icon, IconButton } from '#core/ui'

	interface Props {
		statements: string[]
		error?: string | null
		onclose: () => void
	}

	let { statements, error = null, onclose }: Props = $props()
</script>

<aside
	class="flex w-80 shrink-0 flex-col border-l border-line bg-surface"
	aria-label="Pending SQL"
>
	<header class="flex shrink-0 items-center justify-between border-b border-line px-2.5 py-1.5">
		<span class="text-label font-semibold text-fg-muted">Pending SQL</span>
		<IconButton label="Hide SQL" size="sm" variant="quiet" onclick={onclose}>
			<Icon name="close" />
		</IconButton>
	</header>
	<div class="min-h-0 flex-1 overflow-auto px-2.5 py-2">
		{#if error}
			<p class="m-0 text-caption text-danger">{error}</p>
		{:else if statements.length === 0}
			<p class="m-0 text-caption text-fg-muted">No pending changes.</p>
		{:else}
			<pre
				class="m-0 font-mono text-caption leading-relaxed whitespace-pre-wrap text-fg">{statements.join(
					';\n',
				)};</pre>
		{/if}
	</div>
</aside>
