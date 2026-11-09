import type { Component } from 'svelte'

type AnyComponent = Component<any, any, any>

/**
 * A component whose code is fetched the first time it is needed. Make one
 * per component at module scope, next to the dynamic `import()`, and render
 * it with `<Lazy component={…} props={…} />`.
 */
export interface LazyComponent<C extends AnyComponent = AnyComponent> {
	/** The component once its code has loaded. */
	readonly current: C | undefined
	/** Fetch the code. Repeated calls share one request; a failed one can be retried. */
	load(): Promise<C>
}

export function lazy<C extends AnyComponent>(
	importer: () => Promise<{ default: C }>,
): LazyComponent<C> {
	let current: C | undefined
	let pending: Promise<C> | undefined
	return {
		get current() {
			return current
		},
		load() {
			pending ??= importer().then(
				(m) => (current = m.default),
				(e: unknown) => {
					pending = undefined
					throw e
				},
			)
			return pending
		},
	}
}
