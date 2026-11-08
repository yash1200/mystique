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
 * List/get never include a password. `passwordSet` means one lives in the OS keychain.
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
	/** Write-only on save. Never populated by list/get. */
	password?: string | null
	favorite: boolean
	/** Seconds. Null or missing uses the app setting; 0 means no timeout. */
	queryTimeoutSecs?: number | null
	lastConnectedAt: string | null
	createdAt: string
	updatedAt: string
	/** Ephemeral UI state; defaults to disconnected. */
	status?: ConnectionStatus
	/** SSH bastion (transport). Missing on old profiles. */
	ssh?: SshConfig
	sshPasswordSet?: boolean
}

export type SshAuth = 'password' | 'key' | 'agent'

export interface SshConfig {
	enabled: boolean
	host: string
	port: number
	user: string
	auth: SshAuth
	privateKeyPath: string
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
	/** Copy the keychain secret from this profile (duplicate). */
	copySecretFrom?: string | null
	/** @deprecated Prefer `password`. Kept for IPC compatibility. */
	secretRef?: string | null
	/** Omit on update to leave the stored flag unchanged. */
	favorite?: boolean
	sshConfig?: SshConfig
	sshPassword?: string | null
	clearSshSecret?: boolean
	tlsKeyPassphrase?: string | null
	/** Saved as sent on update too. Null uses the app setting; 0 means none. */
	queryTimeoutSecs?: number | null
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
