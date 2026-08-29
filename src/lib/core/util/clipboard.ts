/** Copy plain text to the system clipboard. */
import type { SaveResult } from './files'

export async function copyTextToClipboard(text: string): Promise<SaveResult> {
	try {
		if (navigator.clipboard?.writeText) {
			await navigator.clipboard.writeText(text)
			return { ok: true, path: 'clipboard' }
		}
		// Fallback
		const ta = document.createElement('textarea')
		ta.value = text
		ta.style.position = 'fixed'
		ta.style.left = '-9999px'
		document.body.appendChild(ta)
		ta.select()
		const ok = document.execCommand('copy')
		ta.remove()
		if (!ok) {
			return { ok: false, cancelled: false, error: 'Clipboard copy failed.' }
		}
		return { ok: true, path: 'clipboard' }
	} catch (e) {
		return {
			ok: false,
			cancelled: false,
			error: e instanceof Error ? e.message : String(e),
		}
	}
}
