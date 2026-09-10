/** True when Backspace/typing should edit the focused control. */

const NON_TEXT_INPUT = new Set([
	'button',
	'submit',
	'reset',
	'checkbox',
	'radio',
	'file',
	'color',
	'range',
	'hidden',
])

export function isWritableField(target: EventTarget | null): boolean {
	if (!(target instanceof HTMLElement)) return false
	if (target.isContentEditable && target.contentEditable !== 'false') {
		return true
	}
	const el = target.closest('input, textarea, select')
	if (!(el instanceof HTMLElement)) return false
	if ('disabled' in el && Boolean((el as HTMLInputElement).disabled)) {
		return false
	}
	if ('readOnly' in el && Boolean((el as HTMLInputElement).readOnly)) {
		return false
	}
	if (el.tagName === 'INPUT') {
		const type = ((el as HTMLInputElement).type || 'text').toLowerCase()
		if (NON_TEXT_INPUT.has(type)) return false
	}
	return true
}

/** WKWebView / Chromium treat Backspace outside a text field as history.back(). */
export function blockHistoryBackspace(e: KeyboardEvent) {
	if (e.defaultPrevented) return
	if (e.key !== 'Backspace') return
	if (e.metaKey || e.ctrlKey || e.altKey) return
	if (isWritableField(e.target)) return
	e.preventDefault()
}
