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

export { runtimeFor } from './products'

export {
	adminOpsApply,
	connectionKindFromCaps,
	familyFromCaps,
	modelTabTitle,
	objectKindRank,
	objectKindSpec,
	presentationFromCaps,
	queryLangFromCaps,
} from './capabilities'

export type { SqlCompleteConfig, SqlSnippetKind } from './sql/complete'
// Completions live in each engine's `editor()`, so CodeMirror stays out of this index.
export { catalogSignature } from './sql/signature'
export { findCloser, isDigit, isIdentByte, isIdentStart } from './sql/scan'
export { quoteIdent, type IdentQuote } from './sql/quote'

// Statement models for run-at-cursor: SQL (same rules as `mystique-sql`) and the Mongo shell.
export {
	splitSqlStatements,
	sqlSplitDialectFor,
	type SqlSplitDialect,
	type StatementSpan,
} from './sql/split'
export { splitMongoStatements } from './mongodb/statements'

// Header tooltip provenance of a SQL result column.
export {
	resultColumnSources,
	resultColumnTitle,
	type ResultColumnSource,
} from './sql/result-source'

export { sslModeFromConfig } from './ssl'

export { default as EngineFields } from './ui/EngineFields.svelte'

export { OPEN_URL_IMPORT } from './commands'
