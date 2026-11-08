/** App chords Quick Open owns. Letter keys use `code` so the layout does not matter. */

import { isModChord } from '#core/util'

export type AppChord =
	| { type: 'quick-open' }
	| { type: 'toggle-sidebar' }
	| { type: 'focus-explorer-filter' }
	| { type: 'focus-tab'; index: number }
	| { type: 'cycle-connection'; delta: -1 | 1 }

export type ChordEvent = {
	metaKey: boolean
	ctrlKey: boolean
	altKey: boolean
	shiftKey: boolean
	code: string
	repeat: boolean
	isComposing?: boolean
}

export type ChordContext = {
	/** On macOS only ⌘ counts; Ctrl chords stay with the editor. */
	mac: boolean
	/** Focus is in a text editor (CodeMirror, input, textarea). */
	inEditor?: boolean
}

export function appChord(event: ChordEvent, context: ChordContext): AppChord | null {
	if (event.repeat || event.isComposing) return null
	if (event.altKey) return null
	if (!isModChord(event, context.mac)) return null
	if (event.shiftKey) {
		if (event.code === 'BracketLeft')
			return { type: 'cycle-connection', delta: -1 }
		if (event.code === 'BracketRight')
			return { type: 'cycle-connection', delta: 1 }
		return null
	}
	if (event.code === 'KeyK') return { type: 'quick-open' }
	if (event.code === 'KeyB') return { type: 'toggle-sidebar' }
	// ⌘L is select-line in the editor.
	if (event.code === 'KeyL') {
		return context.inEditor ? null : { type: 'focus-explorer-filter' }
	}
	if (event.code.startsWith('Digit')) {
		const n = Number(event.code.slice(5))
		if (n >= 1 && n <= 9) return { type: 'focus-tab', index: n - 1 }
	}
	return null
}
