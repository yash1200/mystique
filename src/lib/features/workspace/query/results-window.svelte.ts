/**
 * The mounted row window of the results grid.
 * The window only moves when the viewport nears its edge. Everyday scrolling
 * is native compositing of already-painted rows, so the pane does not go
 * blank while Svelte mounts the next slice.
 */
import { computeResultWindow, scrollDirection } from '#features/workspace/data-grid'
import { untrack } from 'svelte'

export class ResultRowWindow {
	/** First mounted local row (inclusive). */
	start = $state(0)
	/** Last mounted local row (exclusive). Seeded so the first paint is not empty. */
	end = $state(128)
	/**
	 * Cached viewport height. Read from the ResizeObserver, never from the
	 * scroll handler: `clientHeight` there forces a synchronous layout of the
	 * rows mounted by the previous scroll event, which is a long frame right
	 * in the middle of the gesture.
	 */
	viewH = 360
	/** +1 scrolling down, -1 scrolling up. Not $state — must not retrigger effects. */
	#dir: 1 | -1 = 1
	#lastTop = 0
	/** Movement against the direction needed before the buffer flips sides. */
	#accum = 0
	readonly #total: () => number
	readonly #rowH: () => number
	readonly #headerH: number

	constructor(opts: { total: () => number; rowH: () => number; headerH: number }) {
		this.#total = opts.total
		this.#rowH = opts.rowH
		this.#headerH = opts.headerH
	}

	#apply(scrollTop: number, dir: 1 | -1) {
		const next = computeResultWindow({
			totalRows: this.#total(),
			scrollTop,
			viewportH: this.viewH,
			rowH: this.#rowH(),
			headerH: this.#headerH,
			scrollDir: dir,
			current: { start: this.start, end: this.end },
		})
		if (next.start !== this.start) this.start = next.start
		if (next.end !== this.end) this.end = next.end
	}

	#onScroll(el: HTMLElement) {
		const top = el.scrollTop
		const next = scrollDirection(top - this.#lastTop, this.#dir, this.#accum, this.#rowH())
		this.#dir = next.dir
		this.#accum = next.accum
		this.#lastTop = top
		// Apply in the scroll event — not rAF — so the next compositor frame
		// already has rows for the new position.
		this.#apply(top, this.#dir)
	}

	/** Follow `el`'s scroll and size. Returns the detach. */
	attach(el: HTMLElement): () => void {
		const onScroll = () => this.#onScroll(el)
		el.addEventListener('scroll', onScroll, { passive: true })
		const measure = () => {
			this.viewH = el.clientHeight
			untrack(() => this.#apply(el.scrollTop, this.#dir))
		}
		untrack(measure)
		const ro = typeof ResizeObserver === 'undefined' ? null : new ResizeObserver(measure)
		ro?.observe(el)
		return () => {
			el.removeEventListener('scroll', onScroll)
			ro?.disconnect()
		}
	}

	/** New data: back to the top. */
	reset(el: HTMLElement | null) {
		this.#dir = 1
		this.#lastTop = 0
		if (el) {
			el.scrollTop = 0
			this.viewH = el.clientHeight
		}
		this.#accum = 0
		this.#apply(0, 1)
	}

	/** The grid scrolled `el` itself (scroll into view). */
	scrolled(el: HTMLElement) {
		this.viewH = el.clientHeight
		this.#lastTop = el.scrollTop
		this.#apply(el.scrollTop, this.#dir)
	}
}
