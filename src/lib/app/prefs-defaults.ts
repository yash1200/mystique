/** Pref defaults and parsing. Plain TS so tests can import it. */

/** Off: a row click selects; the inspector opens on demand. */
export const INSPECT_ON_ROW_CLICK_DEFAULT = false

/**
 * Prefs are written only when the user toggles them, so an unset key
 * follows the current default.
 */
export function parseBoolPref(raw: string | null, fallback: boolean): boolean {
	if (raw == null) return fallback
	return raw === '1' || raw === 'true'
}
