/**
 * Public surface of the engines zone. Features import from here; nothing
 * outside `engines/` reaches into an individual engine folder.
 */
export type {
	EngineAdapter,
	EngineConnectionAdapter,
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
	selectableEngines,
} from './registry'

export {
	connectionKindFromCaps,
	familyFromCaps,
	mergeCapabilities,
	modelTabTitle,
	modelViewLabelFromCaps,
	presentationFromCaps,
	queryLangFromCaps,
} from './capabilities'

export type { SqlCompleteConfig } from './sql/complete'
