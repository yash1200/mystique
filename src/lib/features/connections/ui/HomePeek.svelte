<script lang="ts">
	import { Mascot } from '#core/ui'
	import { getHome } from '../home.svelte'

	/** Hover the corner and Mystique peeks up; click for a one-liner. */
	const LINES = [
		'Woof. Need a query?',
		'I only fetch rows.',
		'SELECT * FROM treats;',
		'Who’s a good DBA?',
		'Prod? I’ll stay very still.',
	]
	const BUBBLE_MS = 2600

	const home = getHome()

	let line = $state(-1)
	let timer: ReturnType<typeof setTimeout> | undefined

	const petted = $derived(line >= 0)
	const mood = $derived(
		petted || home.happy
			? 'happy'
			: home.selected?.environment === 'production'
				? 'alert'
				: 'idle',
	)

	function pet() {
		line += 1
		clearTimeout(timer)
		timer = setTimeout(() => (line = -1), BUBBLE_MS)
	}

	$effect(() => () => clearTimeout(timer))
</script>

<div
	class="pointer-events-none absolute right-10 bottom-0 z-10 flex h-[9.375rem] w-28 flex-col items-end justify-end"
>
	{#if petted}
		<span
			class="mr-1 mb-2 animate-rise rounded-[12px_12px_4px_12px] bg-raised px-3 py-2 text-[0.78125rem] whitespace-nowrap text-fg shadow-md"
			role="status"
		>
			{LINES[line % LINES.length]}
		</span>
	{/if}
	<button
		type="button"
		class="peek pointer-events-auto relative h-[4.125rem] w-28 overflow-hidden rounded-t-xl border-none bg-transparent p-0"
		class:up={petted || home.happy}
		aria-label="Pet Mystique"
		onclick={pet}
	>
		<span class="pup absolute top-2.5 left-3.5">
			<Mascot {mood} size={84} playful />
		</span>
		<span class="paw left-[1.875rem]"></span>
		<span class="paw left-[3.875rem]"></span>
	</button>
</div>

<style>
	.pup,
	.paw {
		transform: translateY(1.5rem);
		transition: transform 0.5s var(--ease-spring);
	}
	.paw {
		position: absolute;
		bottom: -5px;
		width: 20px;
		height: 13px;
		border-radius: 10px 10px 4px 4px;
		background: var(--fur);
		transform: translateY(8px);
	}
	.peek:hover .pup,
	.peek:focus-visible .pup,
	.up .pup,
	.peek:hover .paw,
	.peek:focus-visible .paw,
	.up .paw {
		transform: translateY(0);
	}
</style>
