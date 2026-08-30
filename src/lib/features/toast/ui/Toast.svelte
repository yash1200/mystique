<script lang="ts">
	import { cn, toastSlide } from '$core/util'
	import { appUpdates } from '$features/updates'
	import type { ToastTone } from '../toast.svelte'

	interface Props {
		message: string
		tone?: ToastTone
	}

	let { message, tone = 'info' }: Props = $props()

	const edge: Record<ToastTone, string> = {
		info: 'border-l-info',
		success: 'border-l-success',
		warning: 'border-l-warning',
		danger: 'border-l-danger',
	}
</script>

<div
	class={cn(
		'fixed right-5 z-50 max-w-panel-sm rounded-lg border border-line border-l-[3px] bg-elevated px-3.5 py-3 text-caption text-pretty text-fg shadow-md',
		appUpdates.visible ? 'bottom-36' : 'bottom-5',
		edge[tone],
	)}
	role="status"
	transition:toastSlide|global
>
	{message}
</div>
