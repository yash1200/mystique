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
