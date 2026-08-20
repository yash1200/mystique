/** Relative timestamps and one-line SQL previews for query lists. */

export function formatRelativeTime(iso: string): string {
	const t = new Date(iso).getTime()
	if (Number.isNaN(t)) return iso
	const diff = Date.now() - t
	const mins = Math.floor(diff / 60000)
	if (mins < 1) return 'just now'
	if (mins < 60) return `${mins}m ago`
	const hours = Math.floor(mins / 60)
	if (hours < 24) return `${hours}h ago`
	const days = Math.floor(hours / 24)
	if (days < 14) return `${days}d ago`
	return new Intl.DateTimeFormat(undefined, { dateStyle: 'medium' }).format(
		new Date(iso),
	)
}

export function previewSql(sql: string, max = 72): string {
	const one = sql.replace(/\s+/g, ' ').trim()
	if (one.length <= max) return one
	return `${one.slice(0, max - 1)}…`
}
