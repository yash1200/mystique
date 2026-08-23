<script lang="ts">
	import { SPRITE_CELL, type SpriteSheet } from '$illustrations'

	interface Props {
		sheet: SpriteSheet
		/** Rendered cell height in px; width follows the cell aspect ratio. */
		height: number
		loop?: boolean
		playing?: boolean
		/** Mirror horizontally (the walk/stand/sit-down strips face right). */
		flip?: boolean
		class?: string
		/** Fires once when a non-looping strip reaches its last frame. */
		onend?: () => void
	}

	let {
		sheet,
		height,
		loop = true,
		playing = true,
		flip = false,
		class: className = '',
		onend,
	}: Props = $props()

	let frame = $state(0)

	// Integer cell width keeps background-position on whole pixels (no seams).
	const w = $derived(Math.round((SPRITE_CELL.w * height) / SPRITE_CELL.h))

	$effect(() => {
		void sheet
		frame = 0
		if (!playing) return
		const id = setInterval(() => {
			if (frame < sheet.frames - 1) frame += 1
			else if (loop) frame = 0
			else {
				clearInterval(id)
				onend?.()
			}
		}, 1000 / sheet.fps)
		return () => clearInterval(id)
	})
</script>

<div
	class={className}
	aria-hidden="true"
	style:width="{w}px"
	style:height="{height}px"
	style:background-image="url({sheet.src})"
	style:background-size="{w * sheet.frames}px {height}px"
	style:background-position="{-frame * w}px 0"
	style:background-repeat="no-repeat"
	style:transform={flip ? 'scaleX(-1)' : undefined}
></div>
