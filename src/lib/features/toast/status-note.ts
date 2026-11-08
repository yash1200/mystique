/**
 * A one-line note in the status footer, for things worth seeing that do
 * not need a popup. Plain TS so tests can drive it with fake timers.
 */

export type StatusNoteTone = 'info' | 'warning'

export type StatusNote = {
	id: number
	tone: StatusNoteTone
	message: string
}

/** Long enough to read a short sentence. */
export const STATUS_NOTE_MS = 4000

/** Holds the latest note and clears it after `STATUS_NOTE_MS`. */
export class StatusNoteClock {
	#timer: ReturnType<typeof setTimeout> | null = null
	#seq = 0
	#onChange: (note: StatusNote | null) => void

	constructor(onChange: (note: StatusNote | null) => void) {
		this.#onChange = onChange
	}

	/** A newer note replaces the current one and restarts the clock. */
	show(message: string, tone: StatusNoteTone = 'info') {
		this.#stop()
		this.#onChange({ id: ++this.#seq, tone, message })
		this.#timer = setTimeout(() => this.clear(), STATUS_NOTE_MS)
	}

	clear() {
		this.#stop()
		this.#onChange(null)
	}

	#stop() {
		if (this.#timer === null) return
		clearTimeout(this.#timer)
		this.#timer = null
	}
}
