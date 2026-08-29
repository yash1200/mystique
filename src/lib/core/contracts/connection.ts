export type EngineId = 'postgres' | 'sqlite' | 'mysql' | 'mongodb' | string

export type ConnectionColorId =
	| 'rose'
	| 'orange'
	| 'amber'
	| 'lime'
	| 'green'
	| 'teal'
	| 'cyan'
	| 'blue'
	| 'indigo'
	| 'violet'
	| 'magenta'
	| 'slate'

export type ConnectionStatus = 'connected' | 'disconnected' | 'error'

export type Environment = 'local' | 'staging' | 'production'

/** Engine-specific fields stored as JSON in SQLite `plugin_config`. */
export type PluginConfig = Record<string, unknown>

/**
 * Connection profile — matches mystique-core `ConnectionProfile` (camelCase).
 * `status` is UI-only (not persisted).
 *
 * Password is returned plaintext for now (local app DB). Keychain later.
 */
export interface ConnectionProfile {
	id: string
	name: string
	engine: EngineId
	colorId: ConnectionColorId
	environment: Environment
	readOnly: boolean
	notes?: string | null
	pluginConfig: PluginConfig
	passwordSet: boolean
	/** Plaintext password when stored (temporary — not keychain). */
	password?: string | null
	lastConnectedAt: string | null
	createdAt: string
	updatedAt: string
	/** Ephemeral UI state; defaults to disconnected. */
	status?: ConnectionStatus
}

/** Payload for create/update — matches backend `SaveConnectionInput`. */
export interface SaveConnectionInput {
	id?: string | null
	name: string
	engine: string
	colorId: string
	environment: string
	readOnly: boolean
	notes?: string | null
	pluginConfig: PluginConfig
	/** Plaintext password to store. Blank on edit keeps the existing secret. */
	password?: string | null
	/** When true, clear the stored password. */
	clearSecret?: boolean
	/** @deprecated Prefer `password`. Kept for IPC compatibility. */
	secretRef?: string | null
}

/** Form draft used by NewConnectionPanel (engine-specific fields optional). */
export interface ConnectionFormDraft {
	name: string
	engine: EngineId
	colorId: ConnectionColorId
	host: string
	port: number
	database: string
	/** SQLite file path (when engine === sqlite). */
	path: string
	/** MongoDB connection URI (required for mongodb engine). */
	uri: string
	username: string
	password: string
	ssl: boolean
	readOnly: boolean
	environment: Environment
	notes: string
}

/** Picker-facing family label. Capability `EngineFamily` is the plugin enum. */
export type ConnectionFamily = 'relational' | 'document'

export interface EngineOption {
	id: EngineId
	name: string
	/** Short card blurb for the engine picker. */
	description: string
	/** Whether the engine is selectable in the UI (plugin may still be FE-only). */
	available: boolean
	queryLanguage: string
	family: ConnectionFamily
	/** Mono badge mark in the picker / EngineBadge. */
	mark: string
	/** Default port for network engines (0 = file / N/A). */
	defaultPort: number
	/** Model view kind label for empty states / copy. */
	modelViewLabel: string
}
