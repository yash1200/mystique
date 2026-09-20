import type { ConnectionFormDraft, ParsedConnectionUrl } from '$core/contracts'

/** Apply a parsed URL onto a host-kind draft. Shared by every Host form. */
export function applyParsedHostUrl(
	draft: ConnectionFormDraft,
	parsed: ParsedConnectionUrl,
): void {
	const cfg = parsed.pluginConfig ?? {}
	const h = cfg.host
	if (typeof h === 'string' && h.trim()) draft.host = h.trim()
	const p = cfg.port
	if (typeof p === 'number' && Number.isFinite(p)) draft.port = p
	else if (typeof p === 'string' && p.trim()) {
		const n = Number(p)
		if (Number.isFinite(n)) draft.port = n
	}
	const db = cfg.database
	if (typeof db === 'string') draft.database = db
	const user = cfg.username ?? cfg.user
	if (typeof user === 'string') draft.username = user
	if (typeof cfg.ssl === 'boolean') draft.ssl = cfg.ssl
	else if (cfg.sslMode === 'require') draft.ssl = true
	if (typeof parsed.password === 'string') draft.password = parsed.password
}
