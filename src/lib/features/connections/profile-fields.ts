/** Profile fields with no engine logic: query timeout and the read-only hint. */
import { parseTimeoutSecs, QUERY_TIMEOUT_MAX_SECS } from '../../app/query-timeout'

/** A production profile that can write gets a hint to turn read-only on. */
export function suggestReadOnly(draft: {
	environment: string
	readOnly: boolean
}): boolean {
	return draft.environment === 'production' && !draft.readOnly
}

/** Form text for the stored timeout. Blank means "use the app setting". */
export function timeoutText(secs: number | null | undefined): string {
	return secs == null ? '' : String(secs)
}

/** Stored timeout from the form text. Blank (or invalid) is null. */
export function timeoutFromText(text: string | undefined): number | null {
	return parseTimeoutSecs(text)
}

export function timeoutError(text: string | undefined): string | null {
	const value = text?.trim() ?? ''
	if (!value || parseTimeoutSecs(value) !== null) return null
	return `Query timeout is whole seconds from 0 to ${QUERY_TIMEOUT_MAX_SECS}, or blank.`
}
