<script lang="ts">
	import type { Snippet } from 'svelte'

	interface Props {
		labelledby?: string
		children?: Snippet
	}

	let { labelledby, children }: Props = $props()
</script>

<div
	class="divide-y divide-line overflow-hidden rounded-lg border border-line bg-surface"
	role="group"
	aria-labelledby={labelledby}
>
	{@render children?.()}
</div>
