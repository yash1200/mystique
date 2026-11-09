/**
 * Binary cells (bytea, BLOB, VARBINARY). Plugins send bytes as `\x` + hex;
 * Postgres sends bytes that happen to be UTF-8 as text. The grid shows a
 * compact `0x89504E47… (12.3 KB)` preview; the cell modal shows a hex dump,
 * copies as hex or base64, and previews PNG / JPEG / GIF / WebP images.
 */
import { isLargeCell } from './large-cell'

const BINARY_TYPES = [
	'bytea',
	'blob',
	'tinyblob',
	'mediumblob',
	'longblob',
	'binary',
	'varbinary',
	'image',
	'raw',
	'long raw',
	'bindata',
]

const HEX_TEXT = /^\\x(?:[0-9a-fA-F]{2})*$/

/** Bytes the grid preview shows before the ellipsis. */
const PREVIEW_BYTES = 8

/** Column type is a byte string. Arrays (`bytea[]`) are not. */
export function binaryColumnType(type: string | null | undefined): boolean {
	const t = (type ?? '').trim().toLowerCase()
	if (!t || t.endsWith('[]')) return false
	const base = t.split('(')[0]?.trim() ?? ''
	return BINARY_TYPES.includes(base)
}

/**
 * A value the grid should treat as bytes: a string in a binary column, or a
 * `\x…` hex string when the type is unknown. Large-cell previews count.
 */
export function isBinaryValue(value: unknown, type: string | null | undefined): boolean {
	const text = isLargeCell(value) ? value.preview : value
	if (typeof text !== 'string') return false
	if (binaryColumnType(type)) return true
	if ((type ?? '').trim()) return false
	return text.length >= 4 && HEX_TEXT.test(text)
}

/** Decode `\x…` hex, or a binary column's text (UTF-8). Odd hex is cut. */
export function binaryBytes(text: string): Uint8Array {
	if (text.startsWith('\\x') && /^[0-9a-fA-F]*$/.test(text.slice(2))) {
		const hex = text.slice(2)
		const out = new Uint8Array(Math.floor(hex.length / 2))
		for (let i = 0; i < out.length; i++) {
			out[i] = parseInt(hex.slice(i * 2, i * 2 + 2), 16)
		}
		return out
	}
	return new TextEncoder().encode(text)
}

/** Byte size of a binary value, without decoding it. */
export function binarySize(value: unknown): number | null {
	if (isLargeCell(value)) {
		return value.preview.startsWith('\\x')
			? Math.max(0, Math.floor((value.totalChars - 2) / 2))
			: null
	}
	if (typeof value !== 'string') return null
	if (value.startsWith('\\x') && HEX_TEXT.test(value)) return (value.length - 2) / 2
	return new TextEncoder().encode(value).length
}

export function formatBytes(n: number): string {
	if (n < 1024) return `${n} B`
	const units = ['KB', 'MB', 'GB', 'TB']
	let size = n / 1024
	let unit = 0
	while (size >= 1024 && unit < units.length - 1) {
		size /= 1024
		unit += 1
	}
	return `${size >= 100 ? Math.round(size) : size.toFixed(1)} ${units[unit]}`
}

/** `0x89504E470D0A1A0A… (12.3 KB)`, or null when the value is not binary. */
export function binaryPreview(value: unknown, type: string | null | undefined): string | null {
	if (value == null || !isBinaryValue(value, type)) return null
	const text = isLargeCell(value) ? value.preview : (value as string)
	const size = binarySize(value) ?? 0
	let head: string
	if (text.startsWith('\\x')) {
		head = text.slice(2, 2 + PREVIEW_BYTES * 2)
	} else {
		head = toHex(binaryBytes(text.slice(0, PREVIEW_BYTES * 4)).subarray(0, PREVIEW_BYTES))
	}
	const more = size > PREVIEW_BYTES ? '…' : ''
	return `0x${head.toUpperCase()}${more} (${formatBytes(size)})`
}

export function toHex(bytes: Uint8Array): string {
	let out = ''
	for (const b of bytes) out += b.toString(16).padStart(2, '0')
	return out
}
