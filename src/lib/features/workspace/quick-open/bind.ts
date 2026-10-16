import { appChrome } from '#lib/app'
import { cycleConnection, focusExplorerFilter, focusTabIndex } from './actions'
import { quickOpen } from './chrome.svelte'
import { appChord } from './keys'

/** Capture-phase chords for Quick Open and the §8.1 shortcuts it owns. */
export function mountAppChords(): () => void {
	const onKey = (event: KeyboardEvent) => {
		const chord = appChord(event)
		if (!chord) return
		if (quickOpen.open && chord.type !== 'quick-open') return
		event.preventDefault()
		event.stopImmediatePropagation()
		if (chord.type === 'quick-open') {
			quickOpen.toggle()
			return
		}
		if (chord.type === 'toggle-sidebar') {
			appChrome.toggle(window.location.pathname)
			return
		}
		if (chord.type === 'focus-explorer-filter') {
			focusExplorerFilter()
			return
		}
		if (chord.type === 'focus-tab') {
			focusTabIndex(chord.index)
			return
		}
		cycleConnection(chord.delta)
	}
	window.addEventListener('keydown', onKey, true)
	return () => window.removeEventListener('keydown', onKey, true)
}
