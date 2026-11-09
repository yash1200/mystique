/**
 * Timed write unlock on a read-only connection (pure helpers). The backend
 * owns the clock and refuses writes after expiry; the UI only counts down
 * from the remaining time it was given.
 */

/** The windows the backend accepts. */
export const TIMED_UNLOCK_CHOICES = [
	{ seconds: 60, label: 'Allow writes for 1 minute' },
	{ seconds: 300, label: 'Allow writes for 5 minutes' },
] as const

/** `4:05`; a minute or more never shows hours here (the cap is 5 minutes). */
export function formatCountdown(remainingMs: number): string {
	const total = Math.max(0, Math.ceil(remainingMs / 1000))
	const minutes = Math.floor(total / 60)
	const seconds = total % 60
	return `${minutes}:${String(seconds).padStart(2, '0')}`
}

/** Local deadline from the backend's remaining time. */
export function deadlineFrom(remainingMs: number | null | undefined, now: number): number | null {
	if (remainingMs == null || remainingMs <= 0) return null
	return now + remainingMs
}

/** Writes are blocked: the profile is read-only and no unlock is running. */
export function effectiveReadOnly(
	profileReadOnly: boolean | null | undefined,
	deadline: number | null | undefined,
	now: number,
): boolean {
	if (!profileReadOnly) return false
	return !(deadline != null && deadline > now)
}
