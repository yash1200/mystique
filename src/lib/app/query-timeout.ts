/**
 * Query timeout: the profile's value wins, then the app setting, then the
 * default. 0 means no timeout (for DDL and migrations). Seconds in the UI,
 * milliseconds on the wire.
 */

export const QUERY_TIMEOUT_DEFAULT_SECS = 120
/** One day. The backend clamps to the same ceiling. */
export const QUERY_TIMEOUT_MAX_SECS = 86_400

/** A whole number of seconds in range, or null when the value is not one. */
export function parseTimeoutSecs(raw: unknown): number | null {
	if (raw === null || raw === undefined) return null
	const text = String(raw).trim()
	if (!/^\d+$/.test(text)) return null
	const secs = Number(text)
	return secs <= QUERY_TIMEOUT_MAX_SECS ? secs : null
}

/** Milliseconds to send. 0 is "no timeout", and the backend keeps it. */
export function resolveQueryTimeoutMs(
	profileSecs: number | null | undefined,
	globalSecs: number | null | undefined,
): number {
	const secs =
		parseTimeoutSecs(profileSecs) ??
		parseTimeoutSecs(globalSecs) ??
		QUERY_TIMEOUT_DEFAULT_SECS
	return secs * 1000
}

/** Short label: "120 s" or "no timeout". */
export function timeoutLabel(secs: number): string {
	return secs === 0 ? 'no timeout' : `${secs} s`
}
