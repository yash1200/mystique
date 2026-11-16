/** App-wide About dialog. Opened from the native menu. */
class AppAboutUi {
	open = $state(false)

	show() {
		this.open = true
	}

	close() {
		this.open = false
	}
}

export const appAboutUi = new AppAboutUi()
