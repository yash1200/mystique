<script lang="ts">
	import type { Snippet } from 'svelte'
	import { appToast } from '../toast.svelte'
	import Toast from './Toast.svelte'

	interface Props {
		children: Snippet
	}

	let { children }: Props = $props()

	const toast = $derived(appToast.current)
</script>

{@render children()}
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
