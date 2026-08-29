/**
 * Form-level checks. Anything engine-specific (which fields matter, what the
 * seed credentials are) is the adapter's `connection.validate`.
 */
import type { ConnectionFormDraft } from '$core/contracts'
import { adapterFor, engineOptions } from '$engines'

/** Client-side checks before calling Test connection. */
export function validateDraftForTest(
	draft: ConnectionFormDraft,
): string | null {
	return adapterFor(draft.engine).connection.validate(draft, 'test')
}

/** Client-side checks before save / update. Credentials may still be blank. */
export function validateDraftForSave(
	draft: ConnectionFormDraft,
	options?: { isEdit?: boolean },
): string | null {
	if (!draft.name.trim()) {
		return 'Give this connection a name.'
	}
	const selected = engineOptions().find((e) => e.id === draft.engine)
	if (!selected?.available && !options?.isEdit) {
		return 'Selected engine is not available yet.'
	}
	return adapterFor(draft.engine).connection.validate(draft, 'save')
}
