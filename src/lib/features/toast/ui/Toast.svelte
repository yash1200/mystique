<script lang="ts">
	import { page } from '$app/state'
	import { Icon, IconButton } from '#core/ui'
	import { cn, toastSlide } from '#core/util'
	import type { ToastAction, ToastTone } from '../toast-policy'

	interface Props {
		message: string
		tone?: ToastTone
		action?: ToastAction
		sticky?: boolean
		onaction?: () => void
		ondismiss?: () => void
	}

	let {
		message,
		tone = 'info',
		action,
		sticky = false,
		onaction,
		ondismiss,
	}: Props = $props()

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
		'fixed right-5 z-50 flex max-w-panel-sm items-start gap-3 rounded-lg border border-line border-l-[3px] bg-elevated px-3.5 py-3 text-caption text-pretty text-fg shadow-md',
		aboveFooter
			? 'bottom-[calc(var(--spacing-control)+theme(spacing.5))]'
			: 'bottom-5',
		edge[tone],
	)}
	role={tone === 'danger' ? 'alert' : 'status'}
	transition:toastSlide|global
>
	<span class="min-w-0 flex-1 py-0.5">{message}</span>
	{#if action}
		<button
			type="button"
			class="shrink-0 rounded px-1.5 py-0.5 font-medium text-accent hover:bg-hover"
			onclick={onaction}
		>
			{action.label}
		</button>
	{/if}
	{#if sticky || action}
		<IconButton label="Dismiss" variant="ghost" size="sm" onclick={ondismiss}>
			<Icon name="close" />
		</IconButton>
	{/if}
</div>
