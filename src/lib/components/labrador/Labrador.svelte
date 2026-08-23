<script lang="ts">
	import { labradorSprites, SPRITE_CELL } from '$illustrations'
	import { cn } from '$lib/utils/cn'
	import { onMount } from 'svelte'
	import Sprite from './Sprite.svelte'

	type Phase = 'sit' | 'happy' | 'stand' | 'walk' | 'sitDown' | 'sleep'

	interface Props {
		/** Rendered sprite height in px. */
		height: number
		/** Curled up asleep instead of sitting. */
		asleep?: boolean
		/**
		 * Let the dog get up and wander to a new spot along the bottom of the
		 * parent, which must be `position: relative`. When false it just sits
		 * (and occasionally waves) in place.
		 */
		roam?: boolean
		class?: string
	}

	let { height, asleep = false, roam = false, class: className = '' }: Props = $props()

	const spriteW = $derived(Math.round((SPRITE_CELL.w * height) / SPRITE_CELL.h))
	/** Walking speed in px/s, relative to the dog's size. */
	const speed = $derived(height * 0.45)

	let phase = $state<Phase>('sit')
	/** Bumped to restart a one-shot strip that is being replayed. */
	let take = $state(0)
	let reduced = $state(false)
	let stage = $state<HTMLDivElement | null>(null)
	let x = $state(0)
	let flip = $state(false)
	let walkMs = $state(0)
	let sitLoops = 0
	let sitsUntilRoam = 3
	let walkTimer: ReturnType<typeof setTimeout> | undefined
	let pendingX = 0

	const rand = (lo: number, hi: number) => lo + Math.random() * (hi - lo)

	onMount(() => {
		reduced = window.matchMedia('(prefers-reduced-motion: reduce)').matches
		if (roam && stage) x = Math.max(0, stage.clientWidth - spriteW + 4)
		return () => clearTimeout(walkTimer)
	})

	$effect(() => {
		phase = asleep ? 'sleep' : 'sit'
	})

	function sit() {
		flip = false
		phase = 'sit'
		take += 1
	}

	function onSitEnd() {
		sitLoops += 1
		if (roam && stage && sitLoops >= sitsUntilRoam) {
			sitLoops = 0
			sitsUntilRoam = Math.round(rand(3, 7))
			startRoam(stage.clientWidth)
			return
		}
		if (Math.random() < 0.15) {
			phase = 'happy'
			take += 1
			return
		}
		take += 1 // replay the wag
	}

	function startRoam(width: number) {
		const max = Math.max(0, width - spriteW)
		if (max < spriteW) {
			take += 1
			return
		}
		let target = rand(0, max)
		// Make sure the trip is worth getting up for.
		if (Math.abs(target - x) < spriteW) target = x < max / 2 ? max : 0
		flip = target < x
		walkMs = Math.round((Math.abs(target - x) / speed) * 1000)
		phase = 'stand'
		take += 1
		pendingX = target
	}

	function onStandEnd() {
		phase = 'walk'
		x = pendingX
		walkTimer = setTimeout(() => {
			phase = 'sitDown'
			take += 1
		}, walkMs)
	}
</script>

{#snippet dog()}
	{#if reduced}
		<Sprite sheet={asleep ? labradorSprites.sleep : labradorSprites.sitWag} {height} playing={false} />
	{:else if phase === 'sleep'}
		<Sprite sheet={labradorSprites.sleep} {height} />
	{:else if phase === 'walk'}
		<Sprite sheet={labradorSprites.walk} {height} {flip} />
	{:else}
		{#key take}
			{#if phase === 'sit'}
				<Sprite sheet={labradorSprites.sitWag} {height} loop={false} onend={onSitEnd} />
			{:else if phase === 'happy'}
				<Sprite sheet={labradorSprites.happy} {height} loop={false} onend={sit} />
			{:else if phase === 'stand'}
				<Sprite sheet={labradorSprites.standUp} {height} {flip} loop={false} onend={onStandEnd} />
			{:else if phase === 'sitDown'}
				<Sprite sheet={labradorSprites.sitDown} {height} {flip} loop={false} onend={sit} />
			{/if}
		{/key}
	{/if}
{/snippet}

{#if roam}
	<div
		bind:this={stage}
		class={cn('pointer-events-none absolute inset-0 overflow-hidden', className)}
		aria-hidden="true"
	>
		<div
			class="absolute bottom-0"
			style:left="{x}px"
			style:transition={phase === 'walk' ? `left ${walkMs}ms linear` : 'none'}
		>
			{@render dog()}
		</div>
	</div>
{:else}
	<div class={cn('pointer-events-none', className)} aria-hidden="true">
		{@render dog()}
	</div>
{/if}
