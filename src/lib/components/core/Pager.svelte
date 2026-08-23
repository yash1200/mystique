<script lang="ts">
	import Icon from './Icon.svelte'
	import IconButton from './IconButton.svelte'

	interface Props {
		/** 0-based page index. */
		page: number
		totalPages: number
		disabled?: boolean
		/** When false, show an em dash instead of page numbers. */
		active?: boolean
		onprev: () => void
		onnext: () => void
		ariaLabel?: string
	}

	let {
		page,
		totalPages,
		disabled = false,
		active = true,
		onprev,
		onnext,
		ariaLabel = 'Pagination',
	}: Props = $props()

	const label = $derived(
		active && totalPages > 0 ? `${page + 1}/${totalPages}` : '—',
	)
</script>

<div class="flex items-center gap-1.5" role="group" aria-label={ariaLabel}>
	<IconButton
		label="Previous page"
		variant="quiet"
		size="sm"
		disabled={disabled || !active || page <= 0}
		onclick={onprev}
	>
		<Icon name="chevron-left" />
	</IconButton>
	<span
		class="min-w-12 shrink-0 px-1.5 text-center font-mono text-caption text-fg-secondary tabular-nums whitespace-nowrap"
	>
		{label}
	</span>
	<IconButton
		label="Next page"
		variant="quiet"
		size="sm"
		disabled={disabled || !active || page >= totalPages - 1}
		onclick={onnext}
	>
		<Icon name="chevron-right" />
	</IconButton>
</div>
