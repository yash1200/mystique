/**
 * Deletes that wait out an Undo window before reaching the backend.
 * Plain TS so tests can drive it with fake timers.
 */
export class DeferredDeletes {
	#pending = new Map<string, ReturnType<typeof setTimeout>>()
	#commit: (id: string) => void | Promise<void>
	#delayMs: number

	constructor(commit: (id: string) => void | Promise<void>, delayMs: number) {
		this.#commit = commit
		this.#delayMs = delayMs
	}

	has(id: string): boolean {
		return this.#pending.has(id)
	}

	/** Commit `id` after the delay unless it is cancelled first. */
	schedule(id: string) {
		this.cancel(id)
		this.#pending.set(
			id,
			setTimeout(() => void this.#run(id), this.#delayMs),
		)
	}

	/** Undo. False when the delete already ran or was never scheduled. */
	cancel(id: string): boolean {
		const timer = this.#pending.get(id)
		if (timer === undefined) return false
		clearTimeout(timer)
		this.#pending.delete(id)
		return true
	}

	/** Commit everything now, e.g. when the page is going away. */
	flush(): Promise<void[]> {
		return Promise.all([...this.#pending.keys()].map((id) => this.#run(id)))
	}

	async #run(id: string) {
		const timer = this.#pending.get(id)
		if (timer === undefined) return
		clearTimeout(timer)
		this.#pending.delete(id)
		await this.#commit(id)
	}
}

/** Put an item back where it was, unless it is already there. */
export function restoreAt<T extends { id: string }>(
	list: readonly T[],
	item: T,
	index: number,
): T[] {
	if (list.some((x) => String(x.id) === String(item.id))) return [...list]
	const next = [...list]
	next.splice(Math.max(0, Math.min(index, next.length)), 0, item)
	return next
}
