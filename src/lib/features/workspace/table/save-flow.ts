/**
 * Grid save decisions. Plain TS so tests can run them.
 *
 * A save runs straight away. The backend asks once on production (one dialog
 * with the SQL that also unlocks the session). "Review before saving: always"
 * shows that same dialog first on every environment.
 */
import { toIpcError } from '#core/ipc'
import type { ApplyTableEditsResult, WriteGuardPrompt } from '#core/contracts'
import type { ReviewBeforeSaving } from '#lib/app'

export function reviewsEverySave(pref: ReviewBeforeSaving): boolean {
	return pref === 'always'
}

export function reviewPrompt(input: {
	production: boolean
	connectionName: string
	statements: string[]
}): WriteGuardPrompt {
	return {
		code: 'writeGuard',
		kind: 'review',
		environment: input.production ? 'production' : 'local',
		connectionName: input.connectionName,
		preview: '',
		statements: input.statements,
		confirmLabel: 'Save',
	}
}

function savedRows(result: ApplyTableEditsResult): number {
	return (result.updated ?? 0) + (result.inserted ?? 0) + (result.deleted ?? 0)
}

export function savedMessage(result: ApplyTableEditsResult): string {
	const rows = savedRows(result)
	if (rows === 0) return 'Saved'
	return rows === 1 ? 'Saved 1 row' : `Saved ${rows} rows`
}

/** Readable rollback error; a stale save names what happened. */
export function undoErrorMessage(err: unknown): string {
	const e = toIpcError(err)
	if (e.kind === 'rollback_stale') return 'Undo refused: the rows changed after this save.'
	return e.message
}

/** `message` is null when the user closed the guard dialog. */
export type UndoOutcome = { ok: true } | { ok: false; message: string | null }

/** Run the undo for one save. Never throws. */
export async function undoSave(
	rollback: () => Promise<void>,
	cancelled: (err: unknown) => boolean,
): Promise<UndoOutcome> {
	try {
		await rollback()
		return { ok: true }
	} catch (err) {
		return { ok: false, message: cancelled(err) ? null : undoErrorMessage(err) }
	}
}
