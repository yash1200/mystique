/** Large-document thresholds. See `setup.ts` for what the mode turns off. */

/** About 1 MB of SQL. */
export const LARGE_DOC_CHARS = 1_000_000
export const LARGE_DOC_LINES = 20_000
/** Leave the mode only below this share of both limits. */
const EXIT_SHARE = 0.9

/**
 * Whether a document of this size runs in large-document mode. `was` is the
 * current mode; it gives the hysteresis.
 */
export function isLargeDocument(chars: number, lines: number, was: boolean): boolean {
	if (chars > LARGE_DOC_CHARS || lines > LARGE_DOC_LINES) return true
	if (!was) return false
	return chars > LARGE_DOC_CHARS * EXIT_SHARE || lines > LARGE_DOC_LINES * EXIT_SHARE
}
