/**
 * Debounced async run where only the newest result lands. Used for live SQL
 * previews: each edit reschedules, and a slow older plan never overwrites a
 * newer one.
 */
export type LatestRun = {
	schedule: () => void
	cancel: () => void
}

export function latestRun<T>(opts: {
	delayMs: number
	run: () => Promise<T>
	onResult: (value: T) => void
	onError: (err: unknown) => void
}): LatestRun {
	let timer: ReturnType<typeof setTimeout> | null = null
	let seq = 0

	const clear = () => {
		if (timer) clearTimeout(timer)
		timer = null
	}

	return {
		schedule() {
			clear()
			const mine = ++seq
			timer = setTimeout(async () => {
				timer = null
				try {
					const value = await opts.run()
					if (mine === seq) opts.onResult(value)
				} catch (err) {
					if (mine === seq) opts.onError(err)
				}
			}, opts.delayMs)
		},
		cancel() {
			clear()
			seq++
		},
	}
}
