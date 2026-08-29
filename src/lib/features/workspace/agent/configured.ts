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
