import { describe, expect, it } from 'vitest'
import { toastLifetime } from '../../src/lib/features/toast/toast-policy'

describe('toastLifetime', () => {
	it('keeps errors until dismissed', () => {
		expect(toastLifetime('danger')).toBeNull()
		expect(toastLifetime('danger', { sticky: false })).toBe(3600)
	})

	it('gives an action time to be clicked', () => {
		const action = { label: 'Undo', run: () => {} }
		expect(toastLifetime('success', { action })).toBe(8000)
		expect(toastLifetime('info')).toBe(3600)
	})
})
