/** App-wide keyboard-shortcuts dialog. Open from the Help menu or the rail. */

class AppShortcutsUi {
	open = $state(false)

	show() {
		this.open = true
	}

	close() {
		this.open = false
	}

	toggle() {
		this.open = !this.open
	}
}

export const appShortcutsUi = new AppShortcutsUi()
