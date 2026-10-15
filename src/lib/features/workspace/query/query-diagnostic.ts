import type { SourcePosition } from '#core/contracts'

export type QueryDiagnostic = {
	message: string
	from: number
	to: number
	line: number
	column: number
}

export type QueryFault = {
	kind?: string
	message: string
	position?: SourcePosition | null
	statementIndex?: number | null
}

function isPosition(value: unknown): value is SourcePosition {
	if (!value || typeof value !== 'object') return false
	const pos = value as SourcePosition
	return (
		typeof pos.offset === 'number' &&
		typeof pos.line === 'number' &&
		typeof pos.column === 'number'
	)
}

/** Tauri rejects query commands with this object. Older strings still parse. */
export function readQueryFault(err: unknown): QueryFault {
	if (typeof err === 'string') return faultFromText(err)
	if (err instanceof Error) return faultFromText(err.message)
	if (err && typeof err === 'object' && 'message' in err) {
		const record = err as {
			kind?: unknown
			message?: unknown
			position?: unknown
			statementIndex?: unknown
		}
		const message =
			typeof record.message === 'string' ? record.message : String(err)
		return {
			kind: typeof record.kind === 'string' ? record.kind : undefined,
			message,
			position: isPosition(record.position) ? record.position : null,
			statementIndex:
				typeof record.statementIndex === 'number'
					? record.statementIndex
					: null,
		}
	}
	return { message: String(err) }
}

function faultFromText(message: string): QueryFault {
	const trimmed = message.trim()
	if (trimmed.startsWith('{')) {
		try {
			return readQueryFault(JSON.parse(trimmed) as unknown)
		} catch {
			/* plain text */
		}
	}
	return { message }
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
