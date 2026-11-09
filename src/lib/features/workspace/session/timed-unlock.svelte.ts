/**
 * Timed write unlocks per live engine session, for the countdown marker and
 * the grid's read-only state. Never persisted. The backend decides; when
 * the local countdown ends the marker drops and the backend has already
 * stopped allowing writes.
 */
import type { TimedUnlockStatus } from '#core/contracts'
import {
	endTimedWriteUnlock,
	errorMessage,
	startTimedWriteUnlock,
	timedWriteUnlockStatus,
} from '#core/ipc'
import { showToast } from '#features/toast'
import { deadlineFrom, effectiveReadOnly } from './timed-unlock'

class TimedUnlocks {
	/** Engine session id → local deadline (ms since epoch). */
	#until = $state<Record<string, number>>({})
	now = $state(Date.now())
	#timer: ReturnType<typeof setInterval> | null = null

	remainingMs(sessionId: string | null | undefined): number | null {
		if (!sessionId) return null
		const until = this.#until[sessionId]
		if (until == null) return null
		const left = until - this.now
		return left > 0 ? left : null
	}

	active(sessionId: string | null | undefined): boolean {
		return this.remainingMs(sessionId) != null
	}

	/** Profile read-only and no running unlock on this session. */
	readOnly(profileReadOnly: boolean | null | undefined, sessionId: string | null | undefined) {
		return effectiveReadOnly(
			profileReadOnly,
			sessionId ? this.#until[sessionId] : null,
			this.now,
		)
	}

	start = async (sessionId: string, seconds: number) => {
		try {
			this.#apply(await startTimedWriteUnlock(sessionId, seconds))
		} catch (e) {
			showToast('danger', errorMessage(e))
		}
	}

	end = async (sessionId: string) => {
		this.forget(sessionId)
		try {
			this.#apply(await endTimedWriteUnlock(sessionId))
		} catch {
			// The session is gone: nothing is unlocked.
		}
	}

	/** Ask the backend again, e.g. after the session was reopened. */
	refresh = async (sessionId: string) => {
		if (!(sessionId in this.#until)) return
		try {
			this.#apply(await timedWriteUnlockStatus(sessionId))
		} catch {
			this.forget(sessionId)
		}
	}

	/** A disconnect ends it (the backend already did). */
	forget = (sessionId: string | null | undefined) => {
		if (!sessionId || !(sessionId in this.#until)) return
		const next = { ...this.#until }
		delete next[sessionId]
		this.#until = next
		this.#sync()
	}

	#apply(status: TimedUnlockStatus) {
		this.now = Date.now()
		const deadline = deadlineFrom(status.remainingMs, this.now)
		const next = { ...this.#until }
		if (deadline == null) delete next[status.sessionId]
		else next[status.sessionId] = deadline
		this.#until = next
		this.#sync()
	}

	/** Tick once a second while any unlock runs; drop expired ones. */
	#sync() {
		const running = Object.keys(this.#until).length > 0
		if (running && !this.#timer && typeof setInterval !== 'undefined') {
			this.#timer = setInterval(() => this.#tick(), 1000)
		} else if (!running && this.#timer) {
			clearInterval(this.#timer)
			this.#timer = null
		}
	}

	#tick() {
		this.now = Date.now()
		const live = Object.entries(this.#until).filter(([, until]) => until > this.now)
		if (live.length !== Object.keys(this.#until).length) {
			this.#until = Object.fromEntries(live)
			this.#sync()
		}
	}
}

export const timedUnlocks = new TimedUnlocks()
