<script
	lang="ts"
	generics="C extends import('svelte').Component<any, any, any>"
>
	/**
	 * Renders a `lazy()` component, fetching its code on first use. Every
	 * other prop goes to the component.
	 *
	 * `when` holds off the fetch until it is first true (a dialog's `open`);
	 * once loaded the component stays mounted, so it can animate closed.
	 */
	import type { ComponentProps } from 'svelte'
	import Icon from './Icon.svelte'
	import type { LazyComponent } from './lazy-component'

	/** A component without props has `Record<string, never>`; forward nothing then. */
	type Forwarded = string extends keyof ComponentProps<C>
		? unknown
		: ComponentProps<C>

	type Props = {
		component: LazyComponent<C>
		when?: boolean
		/** What to show while the code loads. Dialogs pass 'none'. */
		fallback?: 'spinner' | 'none'
	} & Forwarded

	// Rest props stay a live view of the parent's, so the component tracks
	// each prop on its own.
	let { component, when = true, fallback = 'spinner', ...rest }: Props =
		$props()

	let fetched = $state<C>()
	let failed = $state<string | null>(null)
	const Loaded = $derived(fetched ?? component.current)

	function fetchCode() {
		failed = null
		component.load().then(
			(c) => (fetched = c),
			(e: unknown) => (failed = e instanceof Error ? e.message : String(e)),
		)
	}

	$effect(() => {
		if (when && !Loaded) fetchCode()
	})
</script>

{#if Loaded}
	<Loaded {...rest as ComponentProps<C>} />
{:else if when && failed}
	<div
		class="flex h-full min-h-0 flex-1 flex-col items-center justify-center gap-2 p-6 text-caption text-fg-muted"
		role="alert"
	>
		<p class="m-0">Could not load this view. {failed}</p>
		<button
			type="button"
			class="rounded-md border border-line bg-transparent px-2 py-1 text-caption text-fg-secondary hover:bg-hover hover:text-fg"
			onclick={fetchCode}
		>
			Try again
		</button>
	</div>
{:else if when && fallback === 'spinner'}
	<div
		class="flex h-full min-h-0 flex-1 items-center justify-center gap-2 p-6 text-caption text-fg-muted"
		role="status"
		aria-busy="true"
	>
		<Icon name="refresh" class="size-4 animate-spin" />
		Loading…
	</div>
{/if}
