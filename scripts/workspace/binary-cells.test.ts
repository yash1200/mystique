import { describe, expect, test } from 'vitest'

import {
	binaryBytes,
	binaryColumnType,
	binaryPreview,
	binarySize,
	formatBytes,
	hexDump,
	imageMime,
	isBinaryValue,
	toBase64,
	toHex,
} from '../../src/lib/features/workspace/data-grid/binary'

const PNG = '\\x89504e470d0a1a0a0000000d49484452'

describe('binary cells', () => {
	test('binary column types', () => {
		for (const t of ['bytea', 'BLOB', 'longblob', 'varbinary(16)', 'binary(16)']) {
			expect(binaryColumnType(t), t).toBe(true)
		}
		for (const t of ['text', 'bytea[]', 'varchar(16)', '', null]) {
			expect(binaryColumnType(t), String(t)).toBe(false)
		}
	})

	test('a hex value is binary in a binary column or with no type', () => {
		expect(isBinaryValue(PNG, 'bytea')).toBe(true)
		expect(isBinaryValue(PNG, null)).toBe(true)
		expect(isBinaryValue(PNG, 'text')).toBe(false)
		expect(isBinaryValue('plain text', 'bytea')).toBe(true)
		expect(isBinaryValue('plain text', null)).toBe(false)
		expect(isBinaryValue(42, 'bytea')).toBe(false)
	})

	test('the grid preview is short hex and a size', () => {
		expect(binaryPreview(PNG, 'bytea')).toBe('0x89504E470D0A1A0A… (16 B)')
		expect(binaryPreview('\\x0102', 'bytea')).toBe('0x0102 (2 B)')
		expect(binaryPreview('hi', 'bytea')).toBe('0x6869 (2 B)')
		expect(binaryPreview('text', 'text')).toBeNull()
		expect(binaryPreview(null, 'bytea')).toBeNull()
		// A large cell: size from the full length, not the preview.
		const large = { $large: true, preview: '\\x89504e47', totalChars: 2 + 2 * 12_600 }
		expect(binarySize(large)).toBe(12_600)
		expect(binaryPreview(large, 'bytea')).toBe('0x89504E47… (12.3 KB)')
	})

	test('sizes', () => {
		expect(formatBytes(0)).toBe('0 B')
		expect(formatBytes(1023)).toBe('1023 B')
		expect(formatBytes(1536)).toBe('1.5 KB')
		expect(formatBytes(5 * 1024 * 1024)).toBe('5.0 MB')
	})

	test('hex dump, hex and base64 copies', () => {
		const bytes = binaryBytes(PNG)
		expect(bytes.length).toBe(16)
		expect(toHex(bytes)).toBe(PNG.slice(2))
		expect(toBase64(binaryBytes('\\x4d616e'))).toBe('TWFu')
		const rows = hexDump(binaryBytes('\\x48656c6c6f00ff'))
		expect(rows).toHaveLength(1)
		expect(rows[0]?.offset).toBe('00000000')
		expect(rows[0]?.hex.trimEnd()).toBe('48 65 6c 6c 6f 00 ff')
		expect(rows[0]?.ascii).toBe('Hello..')
		const full = hexDump(binaryBytes(`\\x${'41'.repeat(17)}`))
		expect(full.map((r) => r.offset)).toEqual(['00000000', '00000010'])
		expect(full[0]?.hex).toBe('41 41 41 41 41 41 41 41  41 41 41 41 41 41 41 41')
		expect(hexDump(new Uint8Array(40), 2)).toHaveLength(2)
	})

	test('image magic numbers', () => {
		expect(imageMime(binaryBytes(PNG))).toBe('image/png')
		expect(imageMime(binaryBytes('\\xffd8ffe0'))).toBe('image/jpeg')
		expect(imageMime(binaryBytes('\\x474946383961'))).toBe('image/gif')
		expect(imageMime(binaryBytes('\\x52494646000000005745425056503820'))).toBe('image/webp')
		expect(imageMime(binaryBytes('\\x00010203'))).toBeNull()
	})
})
