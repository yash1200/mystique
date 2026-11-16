<script lang="ts">
	import { page } from '$app/state'
	import { cn } from '#core/util'
	import type { Snippet } from 'svelte'
	import { appToast } from '../toast.svelte'
	import Toast from './Toast.svelte'

	interface Props {
		children: Snippet
		/** Lasting cards in the bottom-right corner (the update notice), above the toast. */
		corner?: Snippet
	}

	let { children, corner }: Props = $props()

	const toast = $derived(appToast.current)

	/** Clear the workspace status footer (`h-statusbar`, 34px). */
	const aboveFooter = $derived(
		page.route.id === '/workspace' ||
			page.url.pathname.startsWith('/workspace'),
	)
</script>

{@render children()}
<div
	class={cn(
		'pointer-events-none fixed right-5 z-50 flex flex-col items-end gap-2 [&>*]:pointer-events-auto',
		aboveFooter ? 'bottom-[3.375rem]' : 'bottom-5',
	)}
>
	{@render corner?.()}
	{#if toast}
		{#key toast.id}
			<Toast
				message={toast.message}
				tone={toast.kind}
				action={toast.action}
				sticky={toast.sticky}
				onaction={() => void appToast.runAction()}
				ondismiss={appToast.dismiss}
			/>
		{/key}
	{/if}
</div>
