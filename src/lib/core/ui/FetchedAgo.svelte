<script lang="ts">
	import { cn, fetchedLabel } from '#core/util'

	/**
	 * "Fetched 2m ago" for a status bar. Keeps its own 15 s clock, so a
	 * caller passes only the timestamp; the full date is in the tooltip.
	 */
	interface Props {
		at: number
		/** The verb before the age: Fetched, Collected. */
		prefix?: string
		class?: string
	}

	let { at, prefix = 'Fetched', class: className = '' }: Props = $props()

	let now = $state(Date.now())

	$effect(() => {
		const id = window.setInterval(() => (now = Date.now()), 15_000)
		return () => window.clearInterval(id)
	})

	const age = $derived(fetchedLabel(at, now))
</script>

<span
	class={cn('min-w-0 truncate text-[0.71875rem] tabular-nums', className)}
	title={new Date(at).toLocaleString()}
>
	{prefix}
	{age === 'Just now' ? 'just now' : age}
</span>
