/**
 * Shared UI motion — tokens match docs/DESIGN_LANGUAGE.md.
 * Curves are the animate-skill set (not CSS built-ins).
 *
 * Svelte transitions drive enter/exit (WAAPI via the `css` hook).
 * Hover/press stay on CSS transitions in components.
 */

export type MotionParams = {
	delay?: number
	duration?: number
	/** `transform-origin` for trigger-anchored surfaces. */
	origin?: string
	/** Starting scale. Never 0 — real objects don't appear from nothing. */
	start?: number
}

type TransitionConfig = {
	delay?: number
	duration: number
	easing: (t: number) => number
	css: (t: number) => string
}

/** CSS `cubic-bezier(0.23, 1, 0.32, 1)` — strong ease-out for UI enter/exit. */
export const easeOut = cubicBezier(0.23, 1, 0.32, 1)

/** CSS `ease` — toast personality (Sonner-style, slightly softer). */
const easeCss = cubicBezier(0.25, 0.1, 0.25, 1)

export const DURATION = {
	press: 160,
	popover: 200,
	modal: 250,
	toast: 400,
} as const

export function reducedMotion(): boolean {
	return (
		typeof matchMedia === 'function' &&
		matchMedia('(prefers-reduced-motion: reduce)').matches
	)
}

/**
 * Backdrop / overlay scrim. Opacity only.
 * Pair with `modalPanel` on the dialog sibling — don't put this on a
 * wrapper that also contains the dialog, or the panel fades twice.
 */
export function overlayFade(
	_node: Element,
	params?: MotionParams,
): TransitionConfig {
	const reduce = reducedMotion()
	return {
		delay: params?.delay,
		duration: reduce ? 140 : (params?.duration ?? DURATION.modal),
		easing: easeOut,
		css: (t) => `opacity:${t}`,
	}
}

/**
 * Centered modal surface. Scale + opacity. Origin stays centered.
 */
export function modalPanel(
	_node: Element,
	params?: MotionParams,
): TransitionConfig {
	const reduce = reducedMotion()
	const start = params?.start ?? 0.96
	return {
		delay: params?.delay,
		duration: reduce ? 140 : (params?.duration ?? DURATION.modal),
		easing: easeOut,
		css: (t) => {
			const s = reduce ? 1 : start + (1 - start) * t
			return `opacity:${t};transform:scale(${s})`
		},
	}
}

/**
 * Dropdown, select menu, context menu. Scales out of its trigger.
 */
export function popover(node: Element, params?: MotionParams): TransitionConfig {
	const reduce = reducedMotion()
	const start = params?.start ?? 0.95
	if (params?.origin && node instanceof HTMLElement) {
		node.style.transformOrigin = params.origin
	}
	return {
		delay: params?.delay,
		duration: reduce ? 120 : (params?.duration ?? DURATION.popover),
		easing: easeOut,
		css: (t) => {
			const s = reduce ? 1 : start + (1 - start) * t
			return `opacity:${t};transform:scale(${s})`
		},
	}
}

/**
 * Toast: enter/exit the same bottom edge. Slightly slower, `ease` not ease-out.
 */
export function toastSlide(
	_node: Element,
	params?: MotionParams,
): TransitionConfig {
	const reduce = reducedMotion()
	return {
		delay: params?.delay,
		duration: reduce ? 180 : (params?.duration ?? DURATION.toast),
		easing: easeCss,
		css: (t) => {
			const y = reduce ? 0 : (1 - t) * 100
			return `opacity:${t};transform:translateY(${y}%)`
		},
	}
}

/**
 * Unit cubic-bezier. Matches CSS `cubic-bezier(p1x, p1y, p2x, p2y)`.
 */
function cubicBezier(
	p1x: number,
	p1y: number,
	p2x: number,
	p2y: number,
): (t: number) => number {
	const cx = 3 * p1x
	const bx = 3 * (p2x - p1x) - cx
	const ax = 1 - cx - bx
	const cy = 3 * p1y
	const by = 3 * (p2y - p1y) - cy
	const ay = 1 - cy - by

	const sampleX = (t: number) => ((ax * t + bx) * t + cx) * t
	const sampleY = (t: number) => ((ay * t + by) * t + cy) * t
	const sampleDX = (t: number) => (3 * ax * t + 2 * bx) * t + cx

	function solveX(x: number): number {
		let t = x
		for (let i = 0; i < 8; i++) {
			const err = sampleX(t) - x
			const d = sampleDX(t)
			if (Math.abs(err) < 1e-6 || Math.abs(d) < 1e-6) return t
			t -= err / d
		}
		let t0 = 0
		let t1 = 1
		t = x
		while (t1 - t0 > 1e-6) {
			const x2 = sampleX(t)
			if (Math.abs(x2 - x) < 1e-6) return t
			if (x > x2) t0 = t
			else t1 = t
			t = (t0 + t1) / 2
		}
		return t
	}

	return (t: number) => sampleY(solveX(t))
}
