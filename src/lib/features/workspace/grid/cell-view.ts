import { isLargeCell, largeCellText } from './large-cell'
import type { CellViewMode } from './types'
export const CELL_VIEW_MODES: { id: CellViewMode; label: string }[] = [
	{ id: 'text', label: 'Text' },
	{ id: 'json', label: 'JSON' },
	{ id: 'html', label: 'HTML' },
]

/** String shown / edited in the expanded cell modal. */
export function valueToEditorText(value: unknown): string {
	if (isLargeCell(value)) return largeCellText(value)
	if (value === null || value === undefined) return ''
	if (typeof value === 'string') return value
	if (typeof value === 'boolean') return value ? 'true' : 'false'
	if (typeof value === 'number' || typeof value === 'bigint') {
		return String(value)
	}
	try {
		return JSON.stringify(value)
	} catch {
		return String(value)
	}
}

export function looksLikeJson(raw: string): boolean {
	const t = raw.trim()
	if (!t || (t[0] !== '{' && t[0] !== '[' && t[0] !== '"')) return false
	try {
		JSON.parse(t)
		return true
	} catch {
		return false
	}
}

export function looksLikeHtml(raw: string): boolean {
	const t = raw.trim()
	if (!t.startsWith('<')) return false
	return /<\/?[a-z][\s\S]*>/i.test(t)
}

export function prettyJson(raw: string): string | null {
	try {
		return JSON.stringify(JSON.parse(raw), null, 2)
	} catch {
		return null
	}
}

/** Compact JSON for Text view and for persisting a JSON cell. */
export function minifyJson(raw: string): string | null {
	const t = raw.trim()
	if (!t || t.toUpperCase() === 'NULL') return null
	try {
		return JSON.stringify(JSON.parse(t))
	} catch {
		return null
	}
}

export function jsonParseError(raw: string): string | null {
	const t = raw.trim()
	if (!t || t.toUpperCase() === 'NULL') return null
	try {
		JSON.parse(t)
		return null
	} catch (e) {
		return e instanceof Error ? e.message : 'Invalid JSON'
	}
}

export function detectCellViewMode(
	raw: string,
	colType?: string,
): CellViewMode {
	if (looksLikeJson(raw)) return 'json'
	if (looksLikeHtml(raw)) return 'html'
	const ty = (colType ?? '').toLowerCase()
	if (/\bjsonb?\b/.test(ty)) return 'json'
	if (ty.includes('html') || ty.includes('xml')) return 'html'
	return 'text'
}
