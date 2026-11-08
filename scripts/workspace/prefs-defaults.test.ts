import { describe, expect, it } from 'vitest'
import {
	INSPECT_ON_ROW_CLICK_DEFAULT,
	parseBoolPref,
} from '../../src/lib/app/prefs-defaults'

describe('inspect on row click', () => {
	it('is off for users who never toggled it', () => {
		expect(INSPECT_ON_ROW_CLICK_DEFAULT).toBe(false)
		expect(parseBoolPref(null, INSPECT_ON_ROW_CLICK_DEFAULT)).toBe(false)
	})

	it('keeps an explicit choice', () => {
		expect(parseBoolPref('1', INSPECT_ON_ROW_CLICK_DEFAULT)).toBe(true)
		expect(parseBoolPref('true', INSPECT_ON_ROW_CLICK_DEFAULT)).toBe(true)
		expect(parseBoolPref('0', true)).toBe(false)
	})
})
