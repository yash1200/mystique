/**
 * App-wide chords handled on keydown. In the desktop app the native menu owns
 * settings and zoom (menu.rs), so keydown handles them only in the browser
 * preview. ⌘⇧R has no menu item and always comes through here.
 */
import { isModChord } from '#core/util'

export type AppKeyCommand = 'reset' | 'settings' | 'zoom-in' | 'zoom-out' | 'zoom-reset'

type AppKey = {
	metaKey: boolean
	ctrlKey: boolean
	altKey: boolean
	shiftKey: boolean
	key: string
	code: string
}

export function appKeyCommand(
	event: AppKey,
	context: { mac: boolean; nativeMenu: boolean },
): AppKeyCommand | null {
	if (event.altKey || !isModChord(event, context.mac)) return null
	if (event.shiftKey) return event.code === 'KeyR' ? 'reset' : null
	if (context.nativeMenu) return null
	if (event.key === ',' || event.code === 'Comma') return 'settings'
	if (event.key === '=' || event.key === '+') return 'zoom-in'
	if (event.key === '-' || event.key === '_') return 'zoom-out'
	if (event.key === '0') return 'zoom-reset'
	return null
}
