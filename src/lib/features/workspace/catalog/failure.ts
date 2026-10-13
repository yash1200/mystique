/** One catalog failure toast per connection and message. */

let lastCatalogFailure: string | null = null

export function recordCatalogFailure(
	connectionId: string,
	message: string,
): boolean {
	const sig = `${connectionId}\0${message}`
	if (lastCatalogFailure === sig) return false
	lastCatalogFailure = sig
	return true
}

export function clearCatalogFailure(connectionId: string) {
	if (lastCatalogFailure?.startsWith(`${connectionId}\0`)) {
		lastCatalogFailure = null
	}
}
