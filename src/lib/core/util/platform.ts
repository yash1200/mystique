/** Host platform checks. Used for modifier-key labels and OS-shaped defaults. */
export function isMacPlatform(): boolean {
	if (typeof navigator === 'undefined') return false
	return /Mac|iPhone|iPad/.test(navigator.platform)
}

/**
 * The app's command modifier is held: ⌘ on macOS, Ctrl elsewhere.
 * On macOS a Ctrl chord belongs to the editor, so it never counts.
 */
export function isModChord(
	event: { metaKey: boolean; ctrlKey: boolean },
	mac: boolean,
): boolean {
	if (mac) return event.metaKey && !event.ctrlKey
	return event.ctrlKey && !event.metaKey
}
