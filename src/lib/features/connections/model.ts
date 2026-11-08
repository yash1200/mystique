/**
 * Connection profile ↔ form draft, and the colour palette for connection
 * chrome. Everything engine-specific is delegated to `#engines`.
 */
import type {
	ConnectionColorId,
	ConnectionFormDraft,
	ConnectionProfile,
	Environment,
	PluginConfig,
	SaveConnectionInput,
} from '#core/contracts'
import {
	adapterFor,
	connectionKindFromCaps,
	defaultCapabilitiesForEngine,
	sslModeFromConfig,
} from '#engines'
import { timeoutFromText, timeoutText } from './profile-fields'

export const CONNECTION_COLORS: {
	id: ConnectionColorId
	name: string
	hex: string
}[] = [
	{ id: 'rose', name: 'Rose', hex: '#e56b78' },
	{ id: 'orange', name: 'Orange', hex: '#e8945c' },
	{ id: 'amber', name: 'Amber', hex: '#e0b04a' },
	{ id: 'lime', name: 'Lime', hex: '#9dce5c' },
	{ id: 'green', name: 'Green', hex: '#4ecb88' },
	{ id: 'teal', name: 'Teal', hex: '#3ec9c0' },
	{ id: 'cyan', name: 'Cyan', hex: '#45b4e0' },
	{ id: 'blue', name: 'Blue', hex: '#5a8ee8' },
	{ id: 'indigo', name: 'Indigo', hex: '#7a7aef' },
	{ id: 'violet', name: 'Violet', hex: '#9b6de8' },
	{ id: 'magenta', name: 'Magenta', hex: '#d46ad0' },
	{ id: 'slate', name: 'Slate', hex: '#9a9388' },
]

export function colorHex(id: string): string {
	return CONNECTION_COLORS.find((c) => c.id === id)?.hex ?? '#9a9388'
}

/** First letter/digit of a connection name for rail and picker marks. */
export function connectionInitial(name: string, engine: string): string {
	const ch = name.trim().match(/\p{L}|\p{N}/u)?.[0]
	if (ch) return ch.toLocaleUpperCase()
	const engineCh = String(engine)
		.trim()
		.match(/\p{L}|\p{N}/u)?.[0]
	return engineCh ? engineCh.toLocaleUpperCase() : '?'
}

export function asColorId(id: string): ConnectionColorId {
	return (CONNECTION_COLORS.find((c) => c.id === id)?.id ??
		'slate') as ConnectionColorId
}

export function asEnvironment(env: string): Environment {
	if (env === 'staging' || env === 'production' || env === 'local') return env
	return 'local'
}

export function configStr(
	config: PluginConfig,
	key: string,
	fallback = '',
): string {
	const v = config[key]
	if (v == null) return fallback
	return String(v)
}

export function configNum(
	config: PluginConfig,
	key: string,
	fallback = 0,
): number {
	const v = config[key]
	if (typeof v === 'number' && !Number.isNaN(v)) return v
	if (typeof v === 'string' && v.trim() !== '') {
		const n = Number(v)
		if (!Number.isNaN(n)) return n
	}
	return fallback
}

export function configBool(
	config: PluginConfig,
	key: string,
	fallback = false,
): boolean {
	const v = config[key]
	if (typeof v === 'boolean') return v
	return fallback
}

/** Short endpoint label for the sidebar (host:port, path, or uri). */
export function connectionEndpoint(profile: ConnectionProfile): string {
	const c = profile.pluginConfig ?? {}
	const path = configStr(c, 'path')
	if (path) return path
	const uri = configStr(c, 'uri')
	if (uri) return uri
	const host = configStr(c, 'host')
	const port = configNum(c, 'port', 0)
	if (host && port) return `${host}:${port}`
	if (host) return host
	return '—'
}

export function draftFromProfile(
	profile: ConnectionProfile,
): ConnectionFormDraft {
	const c = profile.pluginConfig ?? {}
	const eng = String(profile.engine)
	const adapter = adapterFor(eng)
	const caps = adapter.defaultCapabilities
	const uri = configStr(c, 'uri')
	let database = configStr(c, 'database')
	if (!database && caps.preferDatabaseFromUri) {
		database = adapter.connection.databaseFromUri?.(uri) ?? ''
	}
	return {
		name: profile.name,
		engine: profile.engine,
		colorId: asColorId(profile.colorId),
		host: configStr(
			c,
			'host',
			connectionKindFromCaps(caps) === 'host' ? '127.0.0.1' : '',
		),
		port: configNum(c, 'port', adapter.defaultPort || 5432),
		database,
		path: configStr(c, 'path'),
		uri,
		username: configStr(c, 'username'),
		password: '',
		clearStoredPassword: false,
		ssl:
			configBool(c, 'ssl', false) ||
			sslModeFromConfig(false, configStr(c, 'sslMode')) !== 'disable',
		sslMode: sslModeFromConfig(
			configBool(c, 'ssl', false),
			configStr(c, 'sslMode'),
		),
		caCertPath: configStr(c, 'caCertPath'),
		clientCertPath: configStr(c, 'clientCertPath'),
		clientKeyPath: configStr(c, 'clientKeyPath'),
		sslClientKeyPassphrase: '',
		readOnly: profile.readOnly,
		environment: asEnvironment(profile.environment),
		notes: profile.notes ?? '',
		queryTimeout: timeoutText(profile.queryTimeoutSecs),
		sshEnabled: Boolean(profile.ssh?.enabled),
		sshHost: profile.ssh?.host ?? '',
		sshPort: profile.ssh?.port || 22,
		sshUser: profile.ssh?.user ?? '',
		sshAuth: profile.ssh?.auth ?? 'password',
		sshPassword: '',
		sshPrivateKeyPath: profile.ssh?.privateKeyPath ?? '',
		sshPassphrase: '',
		clearStoredSshPassword: false,
	}
}

/** Build engine `pluginConfig` JSON from a form draft (shared by save + test). */
export function pluginConfigFromDraft(
	draft: ConnectionFormDraft,
): PluginConfig {
	return adapterFor(draft.engine).connection.toPluginConfig(draft)
}

/**
 * Plugin config used when testing / opening a live session. Merges stored
 * profile fields with draft-derived defaults, then lets the adapter finish the
 * job (Mongo assembles a URI from legacy host fields).
 */
export function pluginConfigForSession(
	profile: ConnectionProfile,
): PluginConfig {
	const adapter = adapterFor(profile.engine)
	const fromDraft = pluginConfigFromDraft(draftFromProfile(profile))
	const stored = (profile.pluginConfig ?? {}) as PluginConfig
	const merged: PluginConfig = { ...stored, ...fromDraft }
	return adapter.connection.sessionPluginConfig?.(profile, merged) ?? merged
}

/** Build SaveConnectionInput from the form draft (engine-aware plugin_config). */
export function saveInputFromDraft(
	draft: ConnectionFormDraft,
	options?: {
		id?: string
		existingPasswordSet?: boolean
		clearPassword?: boolean
	},
): SaveConnectionInput {
	const pluginConfig = pluginConfigFromDraft(draft)

	const input: SaveConnectionInput = {
		id: options?.id ?? null,
		name: draft.name.trim(),
		engine: String(draft.engine),
		colorId: draft.colorId,
		environment: draft.environment,
		readOnly: draft.readOnly,
		notes: draft.notes.trim() || null,
		pluginConfig,
		queryTimeoutSecs: timeoutFromText(draft.queryTimeout),
	}

	const typedPassword = (draft.password ?? '').trim()
	if (
		(options?.clearPassword || draft.clearStoredPassword) &&
		typedPassword.length === 0
	) {
		input.clearSecret = true
	} else if (typedPassword.length > 0) {
		input.password = typedPassword
	}

	const tlsPass = (draft.sslClientKeyPassphrase ?? '').trim()
	if (tlsPass) input.tlsKeyPassphrase = tlsPass

	const kind = connectionKindFromCaps(
		adapterFor(draft.engine).defaultCapabilities,
	)
	if (kind === 'file' || !draft.sshEnabled) {
		input.sshConfig = {
			enabled: false,
			host: '',
			port: 22,
			user: '',
			auth: 'password',
			privateKeyPath: '',
		}
		if (draft.clearStoredSshPassword) input.clearSshSecret = true
	} else {
		input.sshConfig = {
			enabled: true,
			host: draft.sshHost?.trim() ?? '',
			port: Number(draft.sshPort) || 22,
			user: draft.sshUser?.trim() ?? '',
			auth: draft.sshAuth ?? 'password',
			privateKeyPath: draft.sshPrivateKeyPath?.trim() ?? '',
		}
		if (draft.clearStoredSshPassword && !draft.sshPassword && !draft.sshPassphrase) {
			input.clearSshSecret = true
		} else if (draft.sshAuth === 'password' && draft.sshPassword) {
			input.sshPassword = draft.sshPassword
		} else if (draft.sshAuth === 'key' && draft.sshPassphrase) {
			input.sshPassword = draft.sshPassphrase
		}
	}

	return input
}

/** Connection-name placeholder comes from the engine's capabilities. */
export function namePlaceholder(engine: string): string {
	return defaultCapabilitiesForEngine(engine).nameExample || 'e.g. local-pg'
}
