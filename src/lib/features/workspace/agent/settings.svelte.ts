/** AI provider settings. Shared by the settings modal and the agent pane. */

import {
	getAiSettings,
	setAiApiKey,
	setAiEnabled,
	setAiOllamaUrl,
} from './api'
import type { AiSettingsView } from './types'

class AiSettingsStore {
	view = $state<AiSettingsView | null>(null)
	loading = $state(false)
	error = $state<string | null>(null)
	#inflight: Promise<AiSettingsView | null> | null = null

	load = async (force = false) => {
		if (!force && this.view) return this.view
		if (!force && this.#inflight) return this.#inflight
		this.loading = true
		this.error = null
		const task = getAiSettings()
			.then((view) => {
				this.view = view
				return view
			})
			.catch((e) => {
				this.error = e instanceof Error ? e.message : String(e)
				throw e
			})
			.finally(() => {
				this.loading = false
				this.#inflight = null
			})
		this.#inflight = task
		return task
	}

	setEnabled = async (on: boolean) => {
		this.view = await setAiEnabled(on)
		return this.view
	}

	setApiKey = async (provider: string, key: string) => {
		this.view = await setAiApiKey(provider, key)
		return this.view
	}

	setOllamaUrl = async (url: string) => {
		this.view = await setAiOllamaUrl(url)
		return this.view
	}
}

export const aiSettings = new AiSettingsStore()
