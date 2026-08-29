<script lang="ts">
	import { cn, toastSlide } from '$core/util'
	type Tone = 'info' | 'success' | 'warning' | 'danger'

	interface Props {
		message: string
		tone?: Tone
	}

	let { message, tone = 'info' }: Props = $props()

	const edge: Record<Tone, string> = {
		info: 'border-l-info',
		success: 'border-l-success',
		warning: 'border-l-warning',
		danger: 'border-l-danger',
	}
</script>

<div
	class={cn(
		'fixed right-5 bottom-5 z-50 max-w-panel-sm rounded-lg border border-line border-l-[3px] bg-elevated px-3.5 py-3 text-caption text-pretty text-fg shadow-md',
		edge[tone],
	)}
	role="status"
	transition:toastSlide|global
>
	{message}
</div>
