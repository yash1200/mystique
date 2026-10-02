<script lang="ts">
	import { cn } from '#core/util'
	import { labradorRest, labradorSit } from '#illustrations'
	import { onMount } from 'svelte'

	/** Late night: the dog is asleep. Otherwise it sits and watches. */
	let src = $state(labradorSit)
	let loaf = $state(false)

	onMount(() => {
		const hour = new Date().getHours()
		if (hour >= 20 || hour < 6) {
			src = labradorRest
			loaf = true
		}
	})
</script>

<div
	class="pointer-events-none absolute inset-0 z-0 overflow-hidden"
	aria-hidden="true"
>
	<img
		{src}
		alt=""
		width={loaf ? 240 : 176}
		height={loaf ? 138 : 192}
		draggable="false"
		class={cn(
			'home-labrador absolute select-none',
			loaf ? 'right-3 bottom-2 w-60' : '-right-1 -bottom-1 w-44',
		)}
	/>
</div>

<style>
	.home-labrador {
		opacity: 0.24;
		animation: home-labrador-in var(--duration-toast) var(--ease-out) both;
	}

	:global([data-theme='light']) .home-labrador {
		opacity: 0.18;
		animation-name: home-labrador-in-light;
	}

	@keyframes home-labrador-in {
		from {
			opacity: 0;
			transform: translateY(0.5rem);
		}
		to {
			opacity: 0.24;
			transform: none;
		}
	}

	@keyframes home-labrador-in-light {
		from {
			opacity: 0;
			transform: translateY(0.5rem);
		}
		to {
			opacity: 0.18;
			transform: none;
		}
	}

	@media (prefers-reduced-motion: reduce) {
		.home-labrador,
		:global([data-theme='light']) .home-labrador {
			animation: none;
		}
	}
</style>
