/**
 * Public surface of the engines zone. Features import from here; nothing
 * outside `engines/` reaches into an individual engine folder.
 */

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

export { runtimeFor } from './products'

export {
	connectionKindFromCaps,
	familyFromCaps,
	modelTabTitle,
	presentationFromCaps,
	queryLangFromCaps,
} from './capabilities'

export {
	catalogSignature,
	sqlCatalogCompletion,
	sqlSchemaFromColumns,
} from './sql/complete'

export { sslModeFromConfig } from './ssl'

export { default as EngineFields } from './ui/EngineFields.svelte'

export { OPEN_URL_IMPORT } from './commands'
