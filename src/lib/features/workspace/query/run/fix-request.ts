/**
 * "Fix with AI" from the error banner: what failed, the error, and where it
 * points inside the statement that ran (not inside the whole editor).
 */
import type { AgentFixRequest } from '#core/contracts'

type Diagnostic = { from: number }

/** 1-based line and column of `index` in `text`. */
function lineColumn(text: string, index: number): { line: number; column: number } {
	const before = text.slice(0, Math.max(0, Math.min(index, text.length)))
	const lines = before.split('\n')
	return { line: lines.length, column: (lines.at(-1)?.length ?? 0) + 1 }
}

/**
 * `submitted` is the text that ran and `submittedStart` its editor index;
 * the diagnostic's `from` is an editor index too.
 */
export function fixRequestFor(
	submitted: string,
	submittedStart: number,
	error: string,
	diagnostic?: Diagnostic | null,
): AgentFixRequest {
	const fix: AgentFixRequest = { sql: submitted, error: error.trim() }
	if (diagnostic) {
		const local = diagnostic.from - submittedStart
		if (local >= 0 && local <= submitted.length) {
			Object.assign(fix, lineColumn(submitted, local))
		}
	}
	return fix
}
