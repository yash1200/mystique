/**
 * Public surface of the engines zone. Features import from here; nothing
 * outside `engines/` reaches into an individual engine folder.
 */
export type {
	EngineAdapter,
	EngineConnectionAdapter,
	EngineEditorSupport,
	EngineFieldsProps,
	EngineLanguageAdapter,
	FormatQueryResult,
	QueryFormatter,
} from './types'

export {
	adapterFor,
	databaseFromUri,
	defaultCapabilitiesForEngine,
	emptyDraft,
	engineLabel,
	engineOptions,
	formatQuery,
	normalizeCapabilities,
	register,
	resolveCapabilities,
} from './registry'

export {
	ENGINE_PRODUCTS,
	pickerProducts,
	productById,
	runtimeFor,
	type EngineProduct,
} from './products'

export {
	applySupportDepth,
	connectionKindFromCaps,
	familyFromCaps,
	mergeCapabilities,
	modelTabTitle,
	modelViewLabelFromCaps,
	parseSupportDepth,
	presentationFromCaps,
	queryLangFromCaps,
} from './capabilities'

export type { SqlCompleteConfig, SqlSnippetKind } from './sql/complete'
// Completions live in each engine's `editor()`, so CodeMirror stays out of this index.
export { catalogSignature } from './sql/signature'

export {
	emptySslDraft,
	sslModeFromConfig,
	sslPluginFields,
	SSL_MODE_OPTIONS,
	type SslMode,
} from './ssl'

export { default as EngineFields } from './ui/EngineFields.svelte'

export { OPEN_URL_IMPORT } from './commands'
