<script lang="ts">
	import type { Snippet } from 'svelte'

	interface Props {
		label: string
		description?: string
		htmlFor?: string
		children?: Snippet
	}

	let { label, description, htmlFor, children }: Props = $props()
</script>

<div class="flex items-center justify-between gap-4 px-3 py-2.5">
	<div class="min-w-0 flex-1">
		{#if htmlFor}
			<label class="m-0 text-label font-medium text-fg" for={htmlFor}>{label}</label>
		{:else}
			<p class="m-0 text-label font-medium text-fg">{label}</p>
		{/if}
		{#if description}
			<p class="mt-0.5 mb-0 text-caption leading-relaxed text-fg-muted">
				{description}
			</p>
		{/if}
	</div>
	<div class="shrink-0">
		{@render children?.()}
	</div>
</div>
