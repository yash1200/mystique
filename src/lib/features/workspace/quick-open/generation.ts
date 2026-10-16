/** Drops a search result that lost the race to a newer keystroke. */

export class SearchGeneration {
	#token = 0

	next(): number {
		this.#token += 1
		return this.#token
	}

	isCurrent(token: number): boolean {
		return token === this.#token
	}
}
