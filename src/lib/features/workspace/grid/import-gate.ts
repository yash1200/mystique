/**
 * Import reloads the table, which would drop pending grid edits. When the
 * table's tab has edits, the user picks: save first, discard, or cancel.
 */
import type { SaveOutcome } from '#features/workspace/shell'

export type ImportGateChoice = 'save' | 'discard' | 'cancel'

export type ImportGateResult = { proceed: boolean; error: string | null }

export async function resolveImportGate(
	choice: ImportGateChoice,
	deps: {
		save: () => Promise<SaveOutcome>
		discard: () => void | Promise<void>
	},
): Promise<ImportGateResult> {
	if (choice === 'cancel') return { proceed: false, error: null }
	if (choice === 'discard') {
		await deps.discard()
		return { proceed: true, error: null }
	}
	const outcome = await deps.save()
	return outcome.ok
		? { proceed: true, error: null }
		: { proceed: false, error: outcome.error }
}
