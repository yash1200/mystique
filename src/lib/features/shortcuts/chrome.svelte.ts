/** App-wide keyboard-shortcuts dialog. Open from the Help menu or the rail. */
import type { KeyLabelStyle } from './data'

const PET_LINES = [
	'Psst. You found the secret one.',
	'Tail status: wagging.',
	'That’s the spot.',
	'Okay, one more. Then queries.',
	'Best DBA ever.',
]

class AppShortcutsUi {
	open = $state(false)
	/** Kept while the app runs, so the dialog reopens the way it was left. */
	labels = $state<KeyLabelStyle>('glyphs')
	/** The hidden ⌘⇧D shortcut: every pet counts. */
	pets = $state(0)

	/** What Mystique says after the latest pet. */
	get petLine(): string {
		if (this.pets === 0) return PET_LINES[0]!
		return `${PET_LINES[this.pets % PET_LINES.length]} (${this.pets})`
	}

	show() {
		this.open = true
	}

	close() {
		this.open = false
	}

	toggle() {
		this.open = !this.open
	}

	pet() {
		this.pets += 1
	}
}

export const appShortcutsUi = new AppShortcutsUi()
