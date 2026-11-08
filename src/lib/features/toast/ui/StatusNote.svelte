<script lang="ts">
	import { cn, fadeRise } from '#core/util'
	import { appStatusNote } from '../status-note.svelte'

	interface Props {
		class?: string
	}

	let { class: className = '' }: Props = $props()

	const note = $derived(appStatusNote.current)
</script>

<!-- Always mounted so screen readers hear each new note. -->
<span
	class={cn('flex min-w-0 items-center', className)}
	role="status"
	aria-live="polite"
>
	{#if note}
		{#key note.id}
			<span
				class={cn(
					'truncate text-caption',
					note.tone === 'warning' ? 'text-warning' : 'text-fg-secondary',
				)}
				title={note.message}
				in:fadeRise
			>
				{note.message}
			</span>
		{/key}
	{/if}
</span>
