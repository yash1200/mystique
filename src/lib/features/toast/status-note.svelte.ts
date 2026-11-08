/** App-wide status-footer note. Footers render `StatusNote`. */

import {
	StatusNoteClock,
	type StatusNote,
	type StatusNoteTone,
} from './status-note'

class AppStatusNote {
	current = $state<StatusNote | null>(null)
	#clock = new StatusNoteClock((note) => {
		this.current = note
	})

	show = (message: string, tone: StatusNoteTone = 'info') => {
		this.#clock.show(message, tone)
	}

	clear = () => {
		this.#clock.clear()
	}
}

export const appStatusNote = new AppStatusNote()

/** A short note in the status footer instead of a toast. */
export function showStatus(message: string, tone: StatusNoteTone = 'info') {
	appStatusNote.show(message, tone)
}
