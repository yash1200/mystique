/** mongodb:// URI parsing and assembly (legacy host-field profiles). */
import type { PluginConfig } from '#core/contracts'

/** Userinfo pulled off a Mongo URI. The password is not left in `uri`. */
export function splitMongoUserinfo(uri: string): {
	uri: string
	username: string
	password: string
} {
	const trimmed = uri.trim()
	const scheme = trimmed.startsWith('mongodb+srv://')
		? 'mongodb+srv://'
		: trimmed.startsWith('mongodb://')
			? 'mongodb://'
			: null
	if (!scheme) return { uri: trimmed, username: '', password: '' }
	const rest = trimmed.slice(scheme.length)
	const at = rest.lastIndexOf('@')
	if (at <= 0) return { uri: trimmed, username: '', password: '' }
	const userinfo = rest.slice(0, at)
	const hostpart = rest.slice(at + 1)
	if (!hostpart) return { uri: trimmed, username: '', password: '' }
	const colon = userinfo.indexOf(':')
	const userEnc = colon >= 0 ? userinfo.slice(0, colon) : userinfo
	const passEnc = colon >= 0 ? userinfo.slice(colon + 1) : ''
	return {
		uri: `${scheme}${hostpart}`,
		username: decodeURIComponentSafe(userEnc),
		password: decodeURIComponentSafe(passEnc),
	}
}

function decodeURIComponentSafe(value: string): string {
	try {
		return decodeURIComponent(value)
	} catch {
		return value
	}
}

/** Extract `/dbname` from a mongodb:// or mongodb+srv:// URI (before `?` / `#`). */
export function databaseFromMongoUri(uri: string): string | null {
	const trimmed = uri.trim()
	const rest = trimmed.startsWith('mongodb+srv://')
		? trimmed.slice('mongodb+srv://'.length)
		: trimmed.startsWith('mongodb://')
			? trimmed.slice('mongodb://'.length)
			: null
	if (rest == null) return null
	const at = rest.lastIndexOf('@')
	const hostPath = at >= 0 ? rest.slice(at + 1) : rest
	const slash = hostPath.indexOf('/')
	if (slash < 0) return null
	const path = hostPath.slice(slash + 1)
	const name = path.split(/[?#]/)[0]?.trim() ?? ''
	return name.length > 0 ? name : null
}

/**
 * Build a mongodb:// URI from discrete host/user fields (legacy profiles).
 * Returns null when there is nothing useful to assemble.
 */
export function buildMongoUriFromFields(
	cfg: PluginConfig,
	password?: string | null,
): string | null {
	const existing = String(cfg.uri ?? '').trim()
	if (existing) return existing

	const host = String(cfg.host ?? '').trim() || '127.0.0.1'
	const portRaw = cfg.port
	const port =
		typeof portRaw === 'number'
			? portRaw
			: typeof portRaw === 'string' && portRaw.trim()
				? Number(portRaw)
				: 27017
	const user = String(cfg.username ?? '').trim()
	const pass = (password ?? String(cfg.password ?? '')).trim()
	const database = String(cfg.database ?? '').trim()
	const authSource =
		String(cfg.authSource ?? cfg.auth_source ?? '').trim() ||
		(user ? 'admin' : '')

	let authority = host
	if (port && !Number.isNaN(port)) authority = `${host}:${port}`
	if (user) {
		const encUser = encodeURIComponent(user)
		const encPass = pass ? `:${encodeURIComponent(pass)}` : ''
		authority = `${encUser}${encPass}@${authority}`
	}
	const path = database ? `/${database}` : '/'
	const qs = authSource ? `?authSource=${encodeURIComponent(authSource)}` : ''
	return `mongodb://${authority}${path}${qs}`
}
