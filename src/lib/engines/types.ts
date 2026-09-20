/**
 * Frontend engine adapter — the mirror of a `crates/plugins/mystique-plugin-*`
 * crate on the Rust side.
 *
 * Everything the UI needs to know about one database that capabilities cannot
 * carry (a form component, a CodeMirror dialect, a badge mark) lives here, and
 * nowhere else. Adding a database is a folder plus one `register()` call.
 *
 * Chrome still renders from `EngineCapabilities`, never from an engine id — the
 * adapter only supplies what a live session cannot.
 */
import type {
	CatalogColumnRow,
	ConnectionFormDraft,
	ConnectionProfile,
	EngineCapabilities,
	PluginConfig,
} from '$core/contracts'
import type { CompletionSource } from '@codemirror/autocomplete'
import type { SQLDialect } from '@codemirror/lang-sql'

export type FormatQueryResult =
	| { ok: true; text: string }
	| { ok: false; error: string; text: string }

export type QueryFormatter = (text: string) => FormatQueryResult

/** Props every engine's connection `Fields` component accepts. */
export interface EngineFieldsProps {
	draft: ConnectionFormDraft
	isEdit?: boolean
	passwordSet?: boolean
	disabled?: boolean
}

export interface EngineConnectionAdapter {
	/** Blank draft for this engine (ports, default db, seed credentials). */
	emptyDraft(): Omit<ConnectionFormDraft, 'name' | 'colorId'>
	/** Placeholder for URI-kind shells (Mongo). */
	uriPlaceholder?: string
	/** Draft → `plugin_config` JSON handed to the Rust connector. */
	toPluginConfig(draft: ConnectionFormDraft): PluginConfig
	/**
	 * Stored profile → `plugin_config` for opening a live session. Defaults to
	 * `toPluginConfig(draftFromProfile(profile))`; engines override when the
	 * connector needs more (Mongo assembles a URI from legacy host fields).
	 */
	sessionPluginConfig?(
		profile: ConnectionProfile,
		merged: PluginConfig,
	): PluginConfig
	/**
	 * Default database carried by the connection URI, when the engine has one
	 * (Mongo's path segment). Undefined for engines where the URI has no db.
	 */
	databaseFromUri?(uri: string): string | null
	/**
	 * Client-side check on the draft; null when it is fine.
	 * `mode` is 'test' before a live connectivity check (credentials matter) or
	 * 'save' before writing the profile (they do not).
	 */
	validate(draft: ConnectionFormDraft, mode: 'test' | 'save'): string | null
}

export interface EngineLanguageAdapter {
	id: 'sql' | 'mql'
	/** Pretty-printer used by the editor's Format action. Never throws. */
	format: QueryFormatter
	/** CodeMirror SQL dialect; undefined for non-SQL engines. */
	sqlDialect?: SQLDialect
	/** Catalog-driven completions for the query editor. */
	completion(rows: CatalogColumnRow[]): CompletionSource | null
	/**
	 * SQL schema namespace for `@codemirror/lang-sql`; undefined for non-SQL.
	 * Returned separately because lang-sql wants schema, not a source.
	 */
	sqlSchema?(rows: CatalogColumnRow[]): {
		schema: unknown
		defaultSchema?: string
	}
}

export interface EngineAdapter {
	id: string
	label: string
	/** Alternate ids that resolve to this adapter (`mariadb` → mysql). */
	aliases?: string[]
	/** Mono mark shown in the picker and `EngineBadge`. */
	mark: string
	/** Card blurb on the "choose a database" step. */
	description: string
	/** Selectable in the UI. A false here hides it from the picker. */
	available: boolean
	defaultPort: number
	connection: EngineConnectionAdapter
	language: EngineLanguageAdapter
	/**
	 * Pre-connect fallback only. A live session's capabilities always win —
	 * see `resolveCapabilities`.
	 */
	defaultCapabilities: EngineCapabilities
}
