<script lang="ts">
	import { page } from '$app/state'
	import { cn, toastSlide } from '$core/util'
	import type { ToastTone } from '../toast.svelte'

	interface Props {
		message: string
		tone?: ToastTone
	}

	let { message, tone = 'info' }: Props = $props()

	/** Workspace status footer is `h-statusbar` (= `--spacing-control`). */
	const aboveFooter = $derived(
		page.route.id === '/workspace' ||
			page.url.pathname.startsWith('/workspace'),
	)

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
		aboveFooter
			? 'bottom-[calc(var(--spacing-control)+theme(spacing.5))]'
			: 'bottom-5',
		edge[tone],
	)}
	role="status"
	transition:toastSlide|global
>
	{message}
</div>
