/**
 * Query-canvas language helpers. Execution policy lives in the engine plugins.
 */
import type { EngineCapabilities } from '#core/contracts'
import { queryLangFromCaps, resolveCapabilities } from '#engines'

export function queryLanguageFor(
	engine: string,
	caps?: EngineCapabilities | null,
): 'sql' | 'mql' {
	return queryLangFromCaps(resolveCapabilities(engine, caps))
}

/** Empty — new query tabs should not show sample SQL/MQL hints. */
export function queryPlaceholder(_engine: string): string {
	return ''
}
