<script lang="ts">
	import { cn } from '#core/util'

	type Status = 'connected' | 'disconnected' | 'error' | 'running' | 'idle'

	interface Props {
		status: Status
		label?: string
		pulse?: boolean
	}

	let { status, label, pulse = true }: Props = $props()

	const title = $derived(
		label ??
			(
				{
					connected: 'Connected',
					disconnected: 'Disconnected',
					error: 'Error',
					running: 'Running',
					idle: 'Idle',
				} as const
			)[status],
	)

	const colors: Record<Status, string> = {
		connected: 'bg-success',
		disconnected: 'bg-fg-muted',
		error: 'bg-danger',
		running: 'bg-info',
		idle: 'bg-fg-muted',
	}
</script>

<span
	class={cn(
		'size-status shrink-0 rounded-full',
		colors[status],
		pulse &&
			status === 'connected' &&
			'shadow-[0_0_0_3px_color-mix(in_srgb,var(--status-success)_25%,transparent)]',
	)}
	{title}
	aria-label={title}
></span>
