import type {
	ConnectionProfile,
	SaveConnectionInput,
	SetDatabaseEnvironmentInput,
} from '#core/contracts'
import { invoke } from '#core/ipc'
import { asColorId, asEnvironment } from './model'

/** Normalize a backend payload into a typed UI profile. */
function normalizeProfile(raw: ConnectionProfile): ConnectionProfile {
	return {
		...raw,
		colorId: asColorId(String(raw.colorId ?? 'slate')),
		environment: asEnvironment(String(raw.environment ?? 'local')),
		pluginConfig: (raw.pluginConfig ?? {}) as ConnectionProfile['pluginConfig'],
		notes: raw.notes ?? null,
		password: null,
		passwordSet: Boolean(raw.passwordSet),
		savePassword: raw.savePassword !== false,
		favorite: Boolean(raw.favorite),
		lastConnectedAt: raw.lastConnectedAt ?? null,
		status: raw.status ?? 'disconnected',
	}
}

export async function listProfiles(): Promise<ConnectionProfile[]> {
	const rows = await invoke<ConnectionProfile[]>('list_profiles')
	return rows.map(normalizeProfile)
}

export async function saveProfile(
	input: SaveConnectionInput,
): Promise<ConnectionProfile> {
	const row = await invoke<ConnectionProfile>('save_profile', { input })
	return normalizeProfile(row)
}

/** Mark one database with an environment; `environment: null` clears it. */
export async function setProfileDatabaseEnvironment(
	input: SetDatabaseEnvironmentInput,
): Promise<ConnectionProfile> {
	const row = await invoke<ConnectionProfile>('set_profile_database_environment', {
		input,
	})
	return normalizeProfile(row)
}

export async function deleteProfile(id: string): Promise<void> {
	if (!id?.trim()) {
		throw new Error('Connection id is required to delete.')
	}
	// Backend returns true on success (empty () can be awkward for some clients).
	await invoke<boolean>('delete_profile', { id })
}

export async function touchProfileConnected(
	id: string,
): Promise<ConnectionProfile> {
	const row = await invoke<ConnectionProfile>('touch_profile_connected', {
		id,
	})
	return normalizeProfile(row)
}
