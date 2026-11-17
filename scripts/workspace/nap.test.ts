import { describe, expect, it } from 'vitest'
import { isNapTime } from '../../src/lib/core/ui/nap'

const at = (h: number, m = 0) => new Date(2026, 0, 1, h, m)

describe('isNapTime', () => {
	it('naps 2–4 PM and 9 PM–7 AM', () => {
		for (const [h, m, nap] of [
			[13, 59, false],
			[14, 0, true],
			[15, 59, true],
			[16, 0, false],
			[20, 59, false],
			[21, 0, true],
			[0, 0, true],
			[6, 59, true],
			[7, 0, false],
			[10, 0, false],
		] as const) {
			expect(isNapTime(at(h, m)), `${h}:${m}`).toBe(nap)
		}
	})
})
