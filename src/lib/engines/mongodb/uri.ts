/** mongodb:// URI parsing and assembly (legacy host-field profiles). */
import type { PluginConfig } from '$core/contracts'

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
