/**
 * Form-level checks. Anything engine-specific (which fields matter, what the
 * seed credentials are) is the adapter's `connection.validate`.
 */
import type { ConnectionFormDraft } from '$core/contracts'
import { adapterFor, connectionKindFromCaps, engineOptions } from '$engines'

function validateSsh(draft: ConnectionFormDraft): string | null {
	const kind = connectionKindFromCaps(
		adapterFor(draft.engine).defaultCapabilities,
	)
	if (kind === 'file' || !draft.sshEnabled) return null
	if (!draft.sshHost?.trim() || !draft.sshUser?.trim()) {
		return 'SSH tunnel needs a bastion host and user.'
	}
	if (draft.sshAuth === 'key' && !draft.sshPrivateKeyPath?.trim()) {
		return 'SSH private key path is required.'
	}
	return null
}

/** Client-side checks before calling Test connection. */
export function validateDraftForTest(
	draft: ConnectionFormDraft,
): string | null {
	const ssh = validateSsh(draft)
	if (ssh) return ssh
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
	const ssh = validateSsh(draft)
	if (ssh) return ssh
	return adapterFor(draft.engine).connection.validate(draft, 'save')
}
