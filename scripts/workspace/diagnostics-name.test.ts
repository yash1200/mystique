import { describe, expect, it } from 'vitest'
import { defaultDiagnosticsName } from '../../src/lib/features/settings/api'

describe('diagnostics bundle name', () => {
	it('stamps local date and time with zero padding', () => {
		const at = new Date(2026, 0, 5, 7, 3, 9)
		expect(defaultDiagnosticsName(at)).toBe(
			'mystique-diagnostics-20260105-070309.tar.gz',
		)
	})
})
