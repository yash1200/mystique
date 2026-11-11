<script lang="ts">
	import { page } from '$app/state'
	import { Icon, IconButton, type IconName } from '#core/ui'
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

	/** Clear the workspace status footer (`h-statusbar`, 34px). */
	const aboveFooter = $derived(
		page.route.id === '/workspace' ||
			page.url.pathname.startsWith('/workspace'),
	)

	const glyph: Record<ToastTone, { name: IconName; ink: string }> = {
		info: { name: 'info', ink: 'text-info' },
		success: { name: 'check', ink: 'text-accent-text' },
		warning: { name: 'shield-alert', ink: 'text-warning' },
		danger: { name: 'alert-circle', ink: 'text-danger' },
	}
</script>

<div
	class={cn(
		'fixed right-5 z-50 flex max-w-panel-sm items-center gap-2.5 rounded-lg bg-raised py-2.5 pl-3.5 text-body font-medium text-pretty text-fg shadow-lg',
		sticky || action ? 'pr-2' : 'pr-3.5',
		aboveFooter ? 'bottom-[3.375rem]' : 'bottom-5',
	)}
	role={tone === 'danger' ? 'alert' : 'status'}
	transition:toastSlide|global
>
	<Icon name={glyph[tone].name} class={cn('size-3.5', glyph[tone].ink)} />
	<span class="min-w-0 flex-1 py-0.5">{message}</span>
	{#if action}
		<button
			type="button"
			class="h-control-sm shrink-0 rounded-[7px] px-2.5 text-label font-medium text-fg-secondary transition-colors hover:bg-hover hover:text-fg"
			onclick={onaction}
		>
			{action.label}
		</button>
	{/if}
	{#if sticky || action}
		<IconButton label="Dismiss" variant="quiet" size="sm" onclick={ondismiss}>
			<Icon name="close" />
		</IconButton>
	{/if}
</div>
