/**
 * Engine adapter registry — the frontend twin of
 * `ConnectorRegistry::bootstrap()` in `mystique-core`.
 *
 * Adding a database: create `engines/<engine>/` (emptyDraft, toPluginConfig,
 * language), then one `register()` call. Connection chrome is Host/File/URI
 * in `engines/ui/` from `connectionKind`. Nothing else should learn the engine name.
 */
import type {
	ConnectionFormDraft,
	EngineCapabilities,
	EngineId,
	EngineOption,
} from '#core/contracts'
import {
	applySupportDepth,
	mergeCapabilities,
	modelViewLabelFromCaps,
	relationalCapabilities,
} from './capabilities'
import { mongodb } from './mongodb'
import { mysql } from './mysql'
import { postgres } from './postgres'
import { pickerProducts, productById, type EngineProduct } from './products'
import { sqlite } from './sqlite'
import type { EngineAdapter } from './types'

const ADAPTERS = new Map<string, EngineAdapter>()
const ORDERED: EngineAdapter[] = []

export function register(adapter: EngineAdapter): void {
	if (!ADAPTERS.has(adapter.id)) ORDERED.push(adapter)
	for (const id of [adapter.id, ...(adapter.aliases ?? [])]) {
		ADAPTERS.set(id.toLowerCase(), adapter)
	}
}

/**
 * Last-resort adapter for an engine the frontend has never heard of. The
 * backend plugin still drives every capability; this only keeps the UI alive
 * until someone writes a real folder for it.
 */
const genericSql: EngineAdapter = {
	...postgres,
	id: 'sql',
	label: 'SQL',
	aliases: [],
	mark: 'SQL',
	description: 'Generic SQL engine.',
	available: false,
	language: { ...postgres.language, sqlDialect: undefined },
	defaultCapabilities: relationalCapabilities({
		engine: 'sql',
		sqlDialect: 'sql',
	}),
}

register(postgres)
register(mysql)
register(mongodb)
register(sqlite)

function overlayAdapter(
	runtime: EngineAdapter,
	product: EngineProduct,
): EngineAdapter {
	return {
		...runtime,
		id: product.id,
		label: product.label,
		mark: product.mark,
		description: product.description,
		available: product.onPicker,
		defaultPort: product.defaultPort,
		aliases: [],
		defaultCapabilities: applySupportDepth({
			...runtime.defaultCapabilities,
			engine: product.id,
			supportDepth: product.supportDepth,
		}),
		connection: {
			...runtime.connection,
			emptyDraft: () => {
				const draft = runtime.connection.emptyDraft()
				return {
					...draft,
					engine: product.id,
					port: product.defaultPort > 0 ? product.defaultPort : draft.port,
				}
			},
		},
	}
}

export function adapterFor(engine: string): EngineAdapter {
	const id = String(engine).trim().toLowerCase()
	const product = productById(id)
	if (product) {
		const runtime = ADAPTERS.get(product.runtime)
		if (runtime) {
			return product.id === runtime.id
				? runtime
				: overlayAdapter(runtime, product)
		}
	}
	return ADAPTERS.get(id) ?? genericSql
}

/** Engines offered on the Home “choose database” step. */
export function engineOptions(): EngineOption[] {
	return pickerProducts().map((product) => {
		const adapter = adapterFor(product.id)
		return {
			id: product.id as EngineId,
			name: product.label,
			description: product.description,
			available: true,
			queryLanguage: adapter.defaultCapabilities.queryLanguage,
			family:
				adapter.defaultCapabilities.family === 'DOCUMENT'
					? 'document'
					: 'relational',
			mark: product.mark,
			defaultPort: product.defaultPort,
			modelViewLabel: modelViewLabelFromCaps(adapter.defaultCapabilities),
		}
	})
}

export function engineLabel(engine: string): string {
	const id = String(engine).trim()
	return productById(id)?.label ?? ADAPTERS.get(id.toLowerCase())?.label ?? id
}

/** Fallback when no live session yet (browser / pre-connect). */
export function defaultCapabilitiesForEngine(
	engine: string,
): EngineCapabilities {
	return adapterFor(engine).defaultCapabilities
}

/** A live session's capabilities always win over adapter defaults. */
export function resolveCapabilities(
	engine: string,
	caps?: EngineCapabilities | null,
): EngineCapabilities {
	return applySupportDepth(caps ?? defaultCapabilitiesForEngine(engine))
}

/** Merge raw plugin JSON onto the adapter's defaults. */
export function normalizeCapabilities(
	raw: unknown,
	engine: string,
): EngineCapabilities {
	return mergeCapabilities(raw, defaultCapabilitiesForEngine(engine))
}

/** Blank connection draft for an engine. */
export function emptyDraft(engine: EngineId = 'postgres'): ConnectionFormDraft {
	return {
		name: '',
		colorId: 'teal',
		...adapterFor(engine).connection.emptyDraft(),
		engine,
		sshEnabled: false,
		sshHost: '',
		sshPort: 22,
		sshUser: '',
		sshAuth: 'password',
		sshPassword: '',
		sshPrivateKeyPath: '',
		sshPassphrase: '',
		clearStoredSshPassword: false,
	}
}

/** Format query text for the given engine. Never throws. */
export function formatQuery(engine: string, text: string) {
	try {
		return adapterFor(engine).language.format(text)
	} catch (e) {
		const msg = e instanceof Error ? e.message : String(e)
		return { ok: false as const, error: msg || 'Format failed.', text }
	}
}

/**
 * Default database carried by a connection URI, when the engine has one.
 * Empty string for engines whose URI has no database path.
 */
export function databaseFromUri(engine: string, uri: string): string {
	return adapterFor(engine).connection.databaseFromUri?.(uri) ?? ''
}
