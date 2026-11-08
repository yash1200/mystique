import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import {
	STATUS_NOTE_MS,
	StatusNoteClock,
	type StatusNote,
} from '../../src/lib/features/toast/status-note'

describe('StatusNoteClock', () => {
	let current: StatusNote | null = null
	let clock: StatusNoteClock

	beforeEach(() => {
		vi.useFakeTimers()
		current = null
		clock = new StatusNoteClock((note) => {
			current = note
		})
	})
	afterEach(() => vi.useRealTimers())

	it('shows a note and clears it on its own', () => {
		clock.show('Cell copied')
		expect(current).toMatchObject({ tone: 'info', message: 'Cell copied' })
		vi.advanceTimersByTime(STATUS_NOTE_MS - 1)
		expect(current).not.toBeNull()
		vi.advanceTimersByTime(1)
		expect(current).toBeNull()
	})

	it('a newer note replaces the old one and restarts the clock', () => {
		clock.show('first')
		vi.advanceTimersByTime(STATUS_NOTE_MS - 100)
		clock.show('Connection is read-only', 'warning')
		vi.advanceTimersByTime(200)
		expect(current).toMatchObject({
			tone: 'warning',
			message: 'Connection is read-only',
		})
		vi.advanceTimersByTime(STATUS_NOTE_MS)
		expect(current).toBeNull()
	})

	it('gives each note a new id so the same text shows again', () => {
		clock.show('Cell copied')
		const first = current?.id
		clock.show('Cell copied')
		expect(current?.id).not.toBe(first)
	})
})
