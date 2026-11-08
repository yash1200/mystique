import type { SourcePosition } from '#core/contracts'

export type QueryDiagnostic = {
	message: string
	from: number
	to: number
	line: number
	column: number
}

const editorEpochs = new Map<string, number>()

/** Bumps when the editor document changes, including after a tab remount. */
export function noteEditorEdit(tabId: string): void {
	editorEpochs.set(tabId, (editorEpochs.get(tabId) ?? 0) + 1)
}

export function editorEpoch(tabId: string): number {
	return editorEpochs.get(tabId) ?? 0
}

/** Byte offset in `text` as a JavaScript string index. */
export function utf16Index(text: string, byteOffset: number): number {
	const encoder = new TextEncoder()
	let bytes = 0
	let units = 0
	for (const ch of text) {
		const width = encoder.encode(ch).length
		if (bytes >= byteOffset) return units
		if (bytes + width > byteOffset) return units
		bytes += width
		units += ch.length
	}
	return units
}

export function tokenEnd(text: string, from: number): number {
	if (from >= text.length) return text.length
	const match = /^[A-Za-z0-9_]+/.exec(text.slice(from))
	if (match && match[0].length > 0) return from + match[0].length
	return Math.min(text.length, from + 1)
}

/**
 * Map a backend offset in the submitted text onto the editor document.
 * `anchor` is the editor index where that submitted text begins.
 */
export function placeDiagnostic(
	doc: string,
	anchor: number,
	position: SourcePosition,
	message: string,
): QueryDiagnostic | null {
	if (!doc) return null
	const origin = Math.min(Math.max(0, anchor), doc.length)
	const local = utf16Index(doc.slice(origin), position.offset)
	const from = Math.min(doc.length, origin + local)
	let to = tokenEnd(doc, from)
	if (to <= from) to = Math.min(doc.length, from + 1)
	if (from >= doc.length || to <= from) return null
	const line = doc.slice(0, from).split('\n').length
	const lastBreak = doc.lastIndexOf('\n', Math.max(0, from - 1))
	const column = from - (lastBreak + 1) + 1
	return { message, from, to, line, column }
}
