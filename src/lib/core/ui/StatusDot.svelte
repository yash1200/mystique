<script lang="ts">
	import { cn } from '#core/util'

	type Status = 'connected' | 'disconnected' | 'error' | 'running' | 'idle'

	interface Props {
		status: Status
		label?: string
		/** Halo on a live connection. */
		pulse?: boolean
		class?: string
	}

	let { status, label, pulse = true, class: className = '' }: Props = $props()

	const titles: Record<Status, string> = {
		connected: 'Connected',
		disconnected: 'Disconnected',
		error: 'Error',
		running: 'Running',
		idle: 'Idle',
	}

	const colors: Record<Status, string> = {
		connected: 'bg-accent',
		disconnected: 'bg-fg-muted opacity-60',
		error: 'bg-danger',
		running: 'bg-info',
		idle: 'bg-fg-muted opacity-60',
	}

	const title = $derived(label ?? titles[status])
</script>

<span
	class={cn(
		'inline-block size-status shrink-0 rounded-full',
		colors[status],
		pulse && status === 'connected' && 'dot-live',
		className,
	)}
	role="img"
	{title}
	aria-label={title}
></span>
