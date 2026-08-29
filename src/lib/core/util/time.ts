/** Relative timestamps for lists and status bars. */

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

/** Relative clock for status-bar “last fetched” (table + structure). */
export function fetchedLabel(ts: number, at: number): string {
	const sec = Math.max(0, Math.floor((at - ts) / 1000))
	if (sec < 60) return 'Just now'
	const min = Math.floor(sec / 60)
	if (min < 60) return `${min}m ago`
	const hr = Math.floor(min / 60)
	if (hr < 24) return `${hr}h ago`
	return new Date(ts).toLocaleDateString(undefined, {
		month: 'short',
		day: 'numeric',
	})
}
