import type { ConnectionProfile as ConnectionProfileRow, SshAuth } from '../ipc/bindings.gen'

export type {
	DatabaseEnvironment,
	SaveConnectionInput,
	SetDatabaseEnvironmentInput,
	SshAuth,
	SshConfig,
} from '../ipc/bindings.gen'

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
 * The stored profile (`mystique-core` `ConnectionProfile`, generated) with the
 * string fields the UI switches on narrowed to their known values, plus the
 * UI-only `status` overlay (never persisted).
 */
export type ConnectionProfile = Omit<ConnectionProfileRow, 'engine' | 'colorId' | 'environment'> & {
	engine: EngineId
	colorId: ConnectionColorId
	environment: Environment
	/** Ephemeral UI state; defaults to disconnected. */
	status?: ConnectionStatus
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
	/** Edit-only: wipe the stored keychain secret on save. */
	clearStoredPassword?: boolean
	/** Off: ask once per app session instead of saving the password. */
	savePassword: boolean
	ssl: boolean
	sslMode?: 'disable' | 'prefer' | 'require' | 'verify-ca' | 'verify-full'
	caCertPath?: string
	clientCertPath?: string
	clientKeyPath?: string
	sslClientKeyPassphrase?: string
	readOnly: boolean
	environment: Environment
	notes: string
	/** Seconds as typed. Blank uses the app setting; 0 means no timeout. */
	queryTimeout: string
	sshEnabled?: boolean
	sshHost?: string
	sshPort?: number
	sshUser?: string
	sshAuth?: SshAuth
	sshPassword?: string
	sshPrivateKeyPath?: string
	sshPassphrase?: string
	clearStoredSshPassword?: boolean
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
