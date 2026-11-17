import { readFileSync } from 'node:fs'
import { join } from 'node:path'
import { describe, expect, it } from 'vitest'
// @ts-expect-error: plain .mjs script, no types
import { isNewer, setVersion, VERSION_FILES } from '../release.mjs'

const ROOT = join(__dirname, '..', '..')

describe('release version', () => {
	it('ranks x.y.z numerically', () => {
		expect(isNewer('1.0.9', '1.0.8')).toBe(true)
		expect(isNewer('1.0.10', '1.0.9')).toBe(true)
		expect(isNewer('1.1.0', '1.0.99')).toBe(true)
		expect(isNewer('1.0.8', '1.0.8')).toBe(false)
		expect(isNewer('1.0.7', '1.0.8')).toBe(false)
	})

	it('finds the version field in every real version file', () => {
		const found = VERSION_FILES.map(
			({ file, pattern }: { file: string; pattern: RegExp }) =>
				setVersion(readFileSync(join(ROOT, file), 'utf8'), pattern, '9.9.9'),
		)
		expect(found.every((e: { text: string } | null) => e?.text.includes('9.9.9'))).toBe(true)
		expect(new Set(found.map((e: { current: string } | null) => e?.current)).size).toBe(1)
	})
})
