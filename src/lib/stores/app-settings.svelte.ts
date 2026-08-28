export type SettingsSection = 'appearance' | 'workspace' | 'mcp' | 'ai'

class AppSettingsUi {
	open = $state(false)
	section = $state<SettingsSection>('appearance')

	show(section: SettingsSection = 'appearance') {
		this.section = section
		this.open = true
	}

	close() {
		this.open = false
	}
}

export const appSettingsUi = new AppSettingsUi()
