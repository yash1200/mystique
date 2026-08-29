/** Saved connection profiles. One list for home, workspace boot, and the picker. */

import type { ConnectionProfile, SaveConnectionInput } from '$core/contracts'
import {
	deleteProfile,
	listProfiles,
	saveProfile,
	touchProfileConnected,
} from './api'

class ConnectionProfilesStore {
	items = $state<ConnectionProfile[]>([])
	loading = $state(false)
	error = $state<string | null>(null)
	#loaded = false
	#inflight: Promise<void> | null = null

	profile = (id: string): ConnectionProfile | undefined =>
		this.items.find((p) => p.id === id)

	refresh = async () => {
		this.loading = true
		this.error = null
		try {
			this.items = await listProfiles()
			this.#loaded = true
		} catch (e) {
			this.error = e instanceof Error ? e.message : String(e)
			throw e
		} finally {
			this.loading = false
		}
	}

	ensureLoaded = async () => {
		if (this.#loaded) return
		if (this.#inflight) return this.#inflight
		this.#inflight = this.refresh().finally(() => {
			this.#inflight = null
		})
		return this.#inflight
	}

	save = async (input: SaveConnectionInput) => {
		const saved = await saveProfile(input)
		const idx = this.items.findIndex((p) => p.id === saved.id)
		this.items =
			idx >= 0
				? this.items.map((p) => (p.id === saved.id ? saved : p))
				: [...this.items, saved]
		this.#loaded = true
		return saved
	}

	remove = async (id: string) => {
		await deleteProfile(id)
		this.items = this.items.filter((p) => p.id !== id)
	}

	touchConnected = async (id: string) => {
		const row = await touchProfileConnected(id)
		this.items = this.items.map((p) => (p.id === id ? row : p))
		return row
	}

	toggleFavorite = async (id: string) => {
		const profile = this.profile(id)
		if (!profile) throw new Error(`Profile not found: ${id}`)
		return this.save({
			id: profile.id,
			name: profile.name,
			engine: String(profile.engine),
			colorId: profile.colorId,
			environment: profile.environment,
			readOnly: profile.readOnly,
			notes: profile.notes ?? null,
			pluginConfig: profile.pluginConfig ?? {},
			favorite: !profile.favorite,
		})
	}

	upsert = (row: ConnectionProfile) => {
		const idx = this.items.findIndex((p) => p.id === row.id)
		this.items =
			idx >= 0
				? this.items.map((p) => (p.id === row.id ? row : p))
				: [...this.items, row]
	}
}

export const connectionProfiles = new ConnectionProfilesStore()
