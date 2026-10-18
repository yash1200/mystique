/**
 * Grid accelerators. On macOS, Ctrl stays with the editor (Ctrl+T / Ctrl+O).
 */

export type AccelPlatform = 'mac' | 'other'

export function accelPlatform(platform: string): AccelPlatform {
	return /Mac|iPhone|iPad/i.test(platform) ? 'mac' : 'other'
}

export function isGridAccel(
	event: { metaKey: boolean; ctrlKey: boolean; altKey: boolean; shiftKey: boolean },
	platform: AccelPlatform,
): boolean {
	if (event.altKey || event.shiftKey) return false
	if (platform === 'mac') return event.metaKey && !event.ctrlKey
	return (event.ctrlKey || event.metaKey) && !(event.ctrlKey && event.metaKey)
}

export function gridShortcutBlocked(input: {
	modalOpen: boolean
	inCellEditor: boolean
	tagName?: string
	contentEditable?: boolean
	inDialog?: boolean
}): boolean {
	if (input.modalOpen || input.inDialog) return true
	const tag = (input.tagName ?? '').toUpperCase()
	const typing =
		tag === 'INPUT' ||
		tag === 'TEXTAREA' ||
		tag === 'SELECT' ||
		input.contentEditable === true
	if (!typing) return false
	return !input.inCellEditor
}
