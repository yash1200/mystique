/** Host platform checks. Used for modifier-key labels and OS-shaped defaults. */
export function isMacPlatform(): boolean {
	if (typeof navigator === 'undefined') return false
	return /Mac|iPhone|iPad/.test(navigator.platform)
}
