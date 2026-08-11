import {
	ENGINE_OPTIONS,
	type ConnectionFormDraft,
	type EngineId,
} from '$lib/types/connection'
import {
	connectionKindFromCaps,
	defaultCapabilitiesForEngine,
} from '$lib/types/engineCaps'

function kindFor(engine: string) {
	return connectionKindFromCaps(defaultCapabilitiesForEngine(engine))
}

/** Client-side checks before calling Test connection. */
export function validateDraftForTest(
	draft: ConnectionFormDraft,
): string | null {
	const kind = kindFor(draft.engine)
	if (kind === 'file') {
		if (!draft.path.trim()) return 'SQLite database file path is required.'
		return null
	}
	if (kind === 'uri') {
		if (!draft.uri.trim()) {
			return 'Connection URI is required.'
		}
		return null
	}
	// host / network engines
	if (!draft.host.trim() || !draft.database.trim()) {
		return 'Host and database are required.'
	}
	if (!draft.username.trim()) {
		if (draft.engine === 'mysql' || draft.engine === 'mariadb') {
			return 'Username is required (Docker seed: admin).'
		}
		return 'Username is required (Docker: postgres · Homebrew: your macOS login).'
	}
	return null
}

/** Client-side checks before save / update. */
export function validateDraftForSave(
	draft: ConnectionFormDraft,
	options?: { isEdit?: boolean },
): string | null {
	if (!draft.name.trim()) {
		return 'Give this connection a name.'
	}

	const selected = ENGINE_OPTIONS.find((e) => e.id === draft.engine)
	if (!selected?.available && !options?.isEdit) {
		return 'Selected engine is not available yet.'
	}

	const kind = kindFor(draft.engine)
	if (kind === 'file') {
		if (!draft.path.trim()) return 'SQLite database file path is required.'
		return null
	}
	if (kind === 'uri') {
		if (!draft.uri.trim()) {
			return 'Connection URI is required.'
		}
		return null
	}
	if (!draft.host.trim() || !draft.database.trim()) {
		return 'Host and database are required.'
	}
	return null
}

export function namePlaceholder(engine: EngineId): string {
	return (
		defaultCapabilitiesForEngine(engine).nameExample || 'e.g. local-pg'
	)
}
