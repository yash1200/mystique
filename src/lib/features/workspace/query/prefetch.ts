/**
 * Warm what the first completion needs as soon as a query tab opens: the
 * catalog columns of the tab's database, and the editor's language code.
 * Both would otherwise start only after the editor chunk has loaded and
 * mounted. CodeMirror stays out of the startup bundle (dynamic import).
 */
import type { EngineCapabilities } from '#core/contracts'
import { catalogColumnsCache } from '#features/workspace/catalog'
import { queryLanguageFor } from './policy'

export type QueryPrefetchTarget = {
	engine: string
	capabilities?: EngineCapabilities | null
	sessionId: string | null | undefined
	database: string | null | undefined
}

export function prefetchQueryEditor(target: QueryPrefetchTarget): void {
	if (target.sessionId) {
		catalogColumnsCache.load(target.sessionId, target.database || null).catch(() => {
			// The canvas loads again and reports; a warm-up stays quiet.
		})
	}
	const language = queryLanguageFor(target.engine, target.capabilities)
	import('./editor/language')
		.then(({ loadLanguage }) => loadLanguage(language, target.engine))
		.catch(() => {})
}
