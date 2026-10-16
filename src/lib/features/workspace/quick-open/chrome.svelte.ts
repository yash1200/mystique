class QuickOpenController {
	open = $state(false)
	query = $state('')
	returnFocus: HTMLElement | null = null

	show() {
		const active = document.activeElement
		this.returnFocus = active instanceof HTMLElement ? active : null
		this.query = ''
		this.open = true
	}

	hide(restore = true) {
		const back = this.returnFocus
		this.returnFocus = null
		this.open = false
		this.query = ''
		if (restore && back?.isConnected) back.focus()
	}

	toggle() {
		if (this.open) this.hide(true)
		else this.show()
	}
}

export const quickOpen = new QuickOpenController()
