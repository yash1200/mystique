<!--
	The one production marker: a shield in the warning colour, with the word
	in its tooltip and for screen readers. Used wherever an environment pill
	used to be, and on production databases in the explorer.
-->
<script lang="ts">
	import { Icon } from '#core/ui'
	import { cn } from '#core/util'

	interface Props {
		/** Tooltip and accessible name. */
		label?: string
		class?: string
	}

	let { label = 'Production', class: className = '' }: Props = $props()
</script>

<span
	class={cn('inline-flex shrink-0 items-center text-warning', className)}
	title={label}
	role="img"
	aria-label={label}
>
	<Icon name="shield" class="size-3.5" />
</span>
