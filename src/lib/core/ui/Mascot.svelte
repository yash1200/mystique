<script lang="ts" module>
	/**
	 * idle blinks; alert pricks the ears (a production connection is in
	 * view); happy closes the eyes and shows the tongue (a test passed, a
	 * pet); sleep closes the eyes; tilt is curious (something is running).
	 */
	export type MascotMood = 'idle' | 'alert' | 'happy' | 'sleep' | 'tilt'
</script>

<script lang="ts">
	import { cn } from '#core/util'

	interface Props {
		mood?: MascotMood
		size?: number
		/** One spin, for the zoomies. Toggle it off and on to replay. */
		zoom?: boolean
		/** Ears wiggle while the pointer is on the dog. */
		playful?: boolean
		/** Wag (a few swings) — being petted. */
		wag?: boolean
		/** Collar colour; the accent when unset. */
		collar?: string
		class?: string
	}

	let {
		mood = 'idle',
		size = 26,
		zoom = false,
		playful = false,
		wag = false,
		collar,
		class: className = '',
	}: Props = $props()
</script>

<svg
	class={cn(
		'dog',
		mood !== 'idle' && mood,
		zoom && 'zoom',
		playful && 'playful',
		wag && 'wag',
		className,
	)}
	viewBox="0 0 64 64"
	width={size}
	height={size}
	style:--collar={collar}
	aria-hidden="true"
>
	<path class="ear ear-l" d="M17 17c-7 1-11 11-9 21 1 5 6 6 9 2l4-15z" />
	<path class="ear ear-r" d="M47 17c7 1 11 11 9 21-1 5-6 6-9 2l-4-15z" />
	<ellipse class="fur" cx="32" cy="31" rx="17" ry="18" />
	<ellipse class="muzzle" cx="32" cy="41" rx="10" ry="8" />
	<circle class="ink eye" cx="25" cy="29" r="2.3" />
	<circle class="ink eye" cx="39" cy="29" r="2.3" />
	<path class="line closed" d="M22.5 29.5q2.5 2 5 0M36.5 29.5q2.5 2 5 0" />
	<path class="line grin" d="M22.5 30q2.5-3 5 0M36.5 30q2.5-3 5 0" />
	<ellipse class="ink" cx="32" cy="37" rx="3.6" ry="2.6" />
	<path class="line" d="M32 39.5v2.5M28.5 43q3.5 2.4 7 0" />
	<path class="tongue" d="M30.5 43.6h3v2.6a1.5 1.5 0 0 1-3 0z" />
	<path class="collar" d="M19 49q13 7 26 0" />
	<circle class="tag" cx="32" cy="53.6" r="2.4" />
	<text class="zz" x="48" y="14">z</text>
</svg>

<style>
	.dog {
		flex: none;
		overflow: visible;
		transition: transform var(--duration-spring) var(--ease-spring);
	}
	.fur {
		fill: var(--fur);
	}
	.ear {
		fill: var(--ear);
		transform-box: fill-box;
		transition: transform 0.4s var(--ease-spring);
	}
	.ear-l {
		transform-origin: 80% 8%;
	}
	.ear-r {
		transform-origin: 20% 8%;
	}
	.muzzle {
		fill: var(--muzzle);
	}
	.ink {
		fill: var(--ink);
	}
	.line {
		fill: none;
		stroke: var(--ink);
		stroke-width: 1.7;
		stroke-linecap: round;
	}
	.collar {
		fill: none;
		stroke: var(--collar, var(--accent));
		stroke-width: 3.2;
		stroke-linecap: round;
		transition: stroke 0.3s;
	}
	.tag {
		fill: var(--mascot-tag);
	}
	.tongue {
		fill: var(--mascot-tongue);
	}
	.eye {
		transform-box: fill-box;
		transform-origin: center;
		animation: mq-blink 5s infinite;
	}
	.closed,
	.grin,
	.tongue,
	.zz {
		display: none;
	}
	.zz {
		fill: var(--text-3);
		font: 700 10px var(--mds-font-ui);
		animation: mq-z 2.4s infinite;
	}

	.sleep .eye,
	.happy .eye {
		display: none;
	}
	.sleep .zz,
	.sleep .closed,
	.happy .grin,
	.happy .tongue {
		display: inline;
	}
	.alert .ear-l {
		transform: rotate(20deg);
	}
	.alert .ear-r {
		transform: rotate(-20deg);
	}
	.tilt {
		transform: rotate(-12deg);
	}
	.zoom {
		animation: mq-zoom 0.9s cubic-bezier(0.34, 1.3, 0.64, 1);
	}
	.wag {
		animation: mq-wag 0.45s ease-in-out 4;
	}
	.wag .ear-l {
		animation: mq-ear-l 0.45s 4;
	}
	.wag .ear-r {
		animation: mq-ear-r 0.45s 4;
	}
	.playful:hover .ear-l {
		animation: mq-ear-l 0.45s 2;
	}
	.playful:hover .ear-r {
		animation: mq-ear-r 0.45s 2;
	}

	@keyframes mq-blink {
		0%,
		94%,
		100% {
			transform: scaleY(1);
		}
		97% {
			transform: scaleY(0.1);
		}
	}
	@keyframes mq-ear-l {
		50% {
			transform: rotate(16deg);
		}
	}
	@keyframes mq-ear-r {
		50% {
			transform: rotate(-16deg);
		}
	}
	@keyframes mq-z {
		0% {
			opacity: 0;
			transform: translate(0, 0);
		}
		30% {
			opacity: 1;
		}
		100% {
			opacity: 0;
			transform: translate(6px, -10px);
		}
	}
	@keyframes mq-wag {
		0%,
		100% {
			transform: rotate(0);
		}
		25% {
			transform: rotate(-8deg);
		}
		75% {
			transform: rotate(8deg);
		}
	}
	@keyframes mq-zoom {
		0% {
			transform: rotate(0);
		}
		30% {
			transform: rotate(-30deg) scale(1.2);
		}
		100% {
			transform: rotate(360deg) scale(1);
		}
	}
</style>
