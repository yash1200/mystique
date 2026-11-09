/**
 * The cell modal's binary view: hex dump rows, base64, and image sniffing.
 * Loaded with the modal's hex tab, not with the grid.
 */
export { binaryBytes, formatBytes, toHex } from './binary'

export function toBase64(bytes: Uint8Array): string {
	let binary = ''
	const chunk = 0x8000
	for (let i = 0; i < bytes.length; i += chunk) {
		binary += String.fromCharCode(...bytes.subarray(i, i + chunk))
	}
	return btoa(binary)
}

export type HexRow = { offset: string; hex: string; ascii: string }

/** 16 bytes a row: offset, hex pairs (split at 8), printable ASCII. */
export function hexDump(bytes: Uint8Array, maxRows = Number.POSITIVE_INFINITY): HexRow[] {
	const rows: HexRow[] = []
	for (let at = 0; at < bytes.length && rows.length < maxRows; at += 16) {
		const line = bytes.subarray(at, at + 16)
		const pairs: string[] = []
		let ascii = ''
		for (let i = 0; i < 16; i++) {
			const b = line[i]
			pairs.push(b == null ? '  ' : b.toString(16).padStart(2, '0'))
			ascii += b == null ? '' : b >= 0x20 && b < 0x7f ? String.fromCharCode(b) : '.'
		}
		rows.push({
			offset: at.toString(16).padStart(8, '0'),
			hex: `${pairs.slice(0, 8).join(' ')}  ${pairs.slice(8).join(' ')}`,
			ascii,
		})
	}
	return rows
}

export type ImageMime = 'image/png' | 'image/jpeg' | 'image/gif' | 'image/webp'

/** Magic numbers of the image formats the modal previews. */
export function imageMime(bytes: Uint8Array): ImageMime | null {
	const at = (i: number) => bytes[i]
	if (
		bytes.length >= 8 &&
		at(0) === 0x89 &&
		at(1) === 0x50 &&
		at(2) === 0x4e &&
		at(3) === 0x47 &&
		at(4) === 0x0d &&
		at(5) === 0x0a &&
		at(6) === 0x1a &&
		at(7) === 0x0a
	) {
		return 'image/png'
	}
	if (bytes.length >= 3 && at(0) === 0xff && at(1) === 0xd8 && at(2) === 0xff) {
		return 'image/jpeg'
	}
	if (
		bytes.length >= 6 &&
		at(0) === 0x47 &&
		at(1) === 0x49 &&
		at(2) === 0x46 &&
		at(3) === 0x38 &&
		(at(4) === 0x37 || at(4) === 0x39) &&
		at(5) === 0x61
	) {
		return 'image/gif'
	}
	if (
		bytes.length >= 12 &&
		at(0) === 0x52 &&
		at(1) === 0x49 &&
		at(2) === 0x46 &&
		at(3) === 0x46 &&
		at(8) === 0x57 &&
		at(9) === 0x45 &&
		at(10) === 0x42 &&
		at(11) === 0x50
	) {
		return 'image/webp'
	}
	return null
}
