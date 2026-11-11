import { currentUiScale, onUiScaleChange } from '#core/util'
import type { SettingsSection } from './prefs'

export type { SettingsSection }

class AppSettingsUi {
	open = $state(false)
	section = $state<SettingsSection>('appearance')
	/** The UI scale on screen; ⌘+ / ⌘− change it from anywhere. */
	scale = $state(currentUiScale())

	show(section: SettingsSection = 'appearance') {
		this.section = section
		this.open = true
	}

	close() {
		this.open = false
	}
}

export const appSettingsUi = new AppSettingsUi()

onUiScaleChange((scale) => (appSettingsUi.scale = scale))
