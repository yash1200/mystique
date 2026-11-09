/**
 * Catalog completions that may wait for the catalog, and never answer late.
 *
 * The query tab starts loading the catalog columns when it opens (see
 * `prefetchQueryEditor`). If the user asks for completions before they
 * arrive, the source waits for that load (up to {@link CATALOG_WAIT_MS})
 * instead of showing keywords only. The answer is dropped when, by then:
 * - the tab's catalog changed (another database or session),
 * - the document or the caret moved, or
 * - CodeMirror aborted the request.
 */
import type { CatalogColumnRow } from '#core/contracts'
import type { CompletionContext, CompletionSource } from '@codemirror/autocomplete'

export const CATALOG_WAIT_MS = 1500

/** The catalog the tab completes from, read at the moment of the request. */
export type CatalogFeed = {
	/** Session + database. A different key means a different catalog. */
	key: string
	rows: CatalogColumnRow[]
	/** The load in flight for `key`, if any. */
	pending: Promise<CatalogColumnRow[]> | null
}

type MakeSource = (rows: CatalogColumnRow[]) => CompletionSource | null

/**
 * `direct` answers from the rows the language was built with. While those
 * are empty and a load is running, the request waits for it and answers
 * from `make(loadedRows)`.
 */
export function gatedCompletion(
	direct: CompletionSource | null,
	make: MakeSource,
	feed: () => CatalogFeed | null,
	waitMs = CATALOG_WAIT_MS,
): CompletionSource {
	return (context) => {
		const now = feed()
		if (!now || now.rows.length > 0 || !now.pending) {
			return direct ? direct(context) : null
		}
		const key = now.key
		return settle(now.pending, waitMs).then((rows) => {
			if (!rows || rows.length === 0) return null
			if (isStale(context, key, feed)) return null
			const source = make(rows)
			return source ? source(context) : null
		})
	}
}

/** True when an answer for `context` would no longer fit the editor. */
export function isStale(
	context: CompletionContext,
	key: string,
	feed: () => CatalogFeed | null,
): boolean {
	if (context.aborted) return true
	if (feed()?.key !== key) return true
	const view = context.view
	if (!view) return false
	return (
		view.state.doc !== context.state.doc ||
		view.state.selection.main.head !== context.pos
	)
}

/** The load's rows, or null when it fails or takes longer than `ms`. */
function settle(
	load: Promise<CatalogColumnRow[]>,
	ms: number,
): Promise<CatalogColumnRow[] | null> {
	return new Promise((resolve) => {
		const timer = setTimeout(() => resolve(null), ms)
		load.then(
			(rows) => {
				clearTimeout(timer)
				resolve(rows)
			},
			() => {
				clearTimeout(timer)
				resolve(null)
			},
		)
	})
}
