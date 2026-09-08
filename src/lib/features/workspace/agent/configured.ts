/** Which catalog providers the user has actually set up. */
import type { AgentProviderInfo, AiSettingsView } from './types'

export function isProviderConfigured(
	view: AiSettingsView | null | undefined,
	id: string,
): boolean {
	if (!view) return false
	if (id === 'ollama') return !!view.ollamaConfigured
	return view.keys.some((k) => k.provider === id && k.hasKey)
}

export function configuredProviders(
	view: AiSettingsView | null | undefined,
): AgentProviderInfo[] {
	if (!view) return []
	return view.providers.filter((p) => isProviderConfigured(view, p.id))
}

export function availableToAdd(
	view: AiSettingsView | null | undefined,
): AgentProviderInfo[] {
	if (!view) return []
	return view.providers.filter((p) => !isProviderConfigured(view, p.id))
}

/** First configured provider matching a candidate, else the first configured one. */
export function pickConfiguredProvider(
	view: AiSettingsView | null | undefined,
	...candidates: Array<string | null | undefined>
): AgentProviderInfo | null {
	const ready = configuredProviders(view)
	if (ready.length === 0) return null
	for (const id of candidates) {
		if (!id) continue
		const found = ready.find((p) => p.id === id)
		if (found) return found
	}
	return ready[0] ?? null
}

/** Snap a stored or default selection onto a provider the user has actually set up. */
export function resolveAgentProvider(
	view: AiSettingsView | null | undefined,
	current: { provider: string; model: string },
	preferred?: { provider: string; model: string } | null,
): { provider: string; model: string } {
	const pick = pickConfiguredProvider(
		view,
		preferred?.provider,
		current.provider,
	)
	if (!pick) return { provider: '', model: '' }
	if (preferred && preferred.provider === pick.id) {
		return {
			provider: pick.id,
			model: preferred.model.trim() || pick.defaultModel,
		}
	}
	if (current.provider === pick.id && current.model.trim()) {
		return { provider: pick.id, model: current.model }
	}
	return { provider: pick.id, model: pick.defaultModel }
}
