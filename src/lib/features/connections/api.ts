import type { ConnectionProfile, SaveConnectionInput } from '$core/contracts'
import { isTauri } from '$core/ipc'
import { invoke } from '@tauri-apps/api/core'
import { asColorId, asEnvironment } from './model'

const LS_KEY = 'mystique.profiles.v1'
const LS_KEY_LEGACY = 'mystique-db.profiles.v1'

/** Normalize backend / local payload into a typed UI profile. */
export function normalizeProfile(raw: ConnectionProfile): ConnectionProfile {
	const password =
		raw.password != null && String(raw.password).length > 0
			? String(raw.password)
			: null
	return {
		...raw,
		colorId: asColorId(String(raw.colorId ?? 'slate')),
		environment: asEnvironment(String(raw.environment ?? 'local')),
		pluginConfig: (raw.pluginConfig ?? {}) as ConnectionProfile['pluginConfig'],
		notes: raw.notes ?? null,
		password,
		passwordSet: raw.passwordSet || !!password,
		favorite: Boolean(raw.favorite),
		lastConnectedAt: raw.lastConnectedAt ?? null,
		status: raw.status ?? 'disconnected',
	}
}

function nowIso(): string {
	return new Date().toISOString()
}

function readLocal(): ConnectionProfile[] {
	try {
		const raw =
			localStorage.getItem(LS_KEY) ?? localStorage.getItem(LS_KEY_LEGACY)
		if (!raw) return []
		const parsed = JSON.parse(raw) as ConnectionProfile[]
		if (!Array.isArray(parsed)) return []
		return parsed.map(normalizeProfile)
	} catch {
		return []
	}
}

function writeLocal(rows: ConnectionProfile[]) {
	localStorage.setItem(LS_KEY, JSON.stringify(rows))
	localStorage.removeItem(LS_KEY_LEGACY)
}

function localSave(input: SaveConnectionInput): ConnectionProfile {
	const rows = readLocal()
	const ts = nowIso()
	const id =
		(input.id && String(input.id).trim()) ||
		String(
			Math.max(0, ...rows.map((r) => Number.parseInt(String(r.id), 10) || 0)) +
				1,
		)

	const existing = rows.find((r) => r.id === id)

	let password: string | null | undefined = existing?.password ?? null
	if (input.clearSecret) {
		password = null
	} else if (input.password != null && String(input.password).length > 0) {
		password = String(input.password)
	} else if (
		input.secretRef != null &&
		String(input.secretRef).length > 0 &&
		!String(input.secretRef).startsWith('mystique-db/connection') &&
		!String(input.secretRef).startsWith('mystique/conn/')
	) {
		password = String(input.secretRef)
	}

	const passwordSet = !!password

	const profile: ConnectionProfile = {
		id,
		name: input.name,
		engine: input.engine,
		colorId: asColorId(input.colorId),
		environment: asEnvironment(input.environment),
		readOnly: input.readOnly,
		notes: input.notes ?? null,
		pluginConfig: input.pluginConfig ?? {},
		passwordSet,
		password: password || null,
		favorite: input.favorite ?? existing?.favorite ?? false,
		lastConnectedAt: existing?.lastConnectedAt ?? null,
		createdAt: existing?.createdAt ?? ts,
		updatedAt: ts,
		status: existing?.status ?? 'disconnected',
	}

	const next = existing
		? rows.map((r) => (r.id === id ? profile : r))
		: [...rows, profile]
	writeLocal(next)
	return normalizeProfile(profile)
}

export async function listProfiles(): Promise<ConnectionProfile[]> {
	if (!isTauri()) {
		return readLocal()
	}
	const rows = await invoke<ConnectionProfile[]>('list_profiles')
	return rows.map(normalizeProfile)
}

export async function saveProfile(
	input: SaveConnectionInput,
): Promise<ConnectionProfile> {
	if (!isTauri()) {
		return localSave(input)
	}
	const row = await invoke<ConnectionProfile>('save_profile', { input })
	return normalizeProfile(row)
}

export async function deleteProfile(id: string): Promise<void> {
	if (!id?.trim()) {
		throw new Error('Connection id is required to delete.')
	}
	if (!isTauri()) {
		writeLocal(readLocal().filter((r) => r.id !== id))
		return
	}
	// Backend returns true on success (empty () can be awkward for some clients).
	await invoke<boolean>('delete_profile', { id })
}

export async function touchProfileConnected(
	id: string,
): Promise<ConnectionProfile> {
	if (!isTauri()) {
		const rows = readLocal()
		const ts = nowIso()
		const next = rows.map((r) =>
			r.id === id
				? {
						...r,
						lastConnectedAt: ts,
						updatedAt: ts,
						status: 'connected' as const,
					}
				: r,
		)
		writeLocal(next)
		const row = next.find((r) => r.id === id)
		if (!row) throw new Error(`Profile not found: ${id}`)
		return normalizeProfile(row)
	}
	const row = await invoke<ConnectionProfile>('touch_profile_connected', {
		id,
	})
	return normalizeProfile(row)
}
