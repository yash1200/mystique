/** Debounced tab persist. Disconnect must cancel the timer before tabs are dropped. */

export function createPersistScheduler(
	persist: (connectionId: string) => void | Promise<void>,
) {
	const timers = new Map<string, ReturnType<typeof setTimeout>>()
	return {
		schedule(connectionId: string, delay = 350) {
			const prev = timers.get(connectionId)
			if (prev) clearTimeout(prev)
			timers.set(
				connectionId,
				setTimeout(() => {
					timers.delete(connectionId)
					void persist(connectionId)
				}, delay),
			)
		},
		/** Stop a pending save. Returns whether a timer was waiting. */
		cancel(connectionId: string): boolean {
			const prev = timers.get(connectionId)
			if (!prev) return false
			clearTimeout(prev)
			timers.delete(connectionId)
			return true
		},
		pending(connectionId: string): boolean {
			return timers.has(connectionId)
		},
	}
}

/** Tabs that belong to a connection. Snapshot this before clearing memory. */
export function tabsForConnection<T extends { connectionId: string }>(
	tabs: T[],
	connectionId: string,
): T[] {
	return tabs.filter((tab) => tab.connectionId === connectionId)
}
