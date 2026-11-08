/**
 * When grid chords (⌘N, ⌘D, ⌘R, ⌘Z…) may run. The keys themselves live in the
 * keymap (`#lib/app`); Ctrl stays with the editor on macOS.
 */

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
