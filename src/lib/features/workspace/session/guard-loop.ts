/**
 * Retry loop for writes the backend guard stopped. Plain TS so tests can
 * drive it with fake `ask` / `unlock`.
 */
import type { WriteGuardPrompt } from '#core/contracts'
import { writeGuardPrompt } from '#core/ipc'

export type WriteGuardDecision = { ok: true; typedName?: string } | { ok: false }

export type GuardOptions = {
	/** Sent on the first try, e.g. after the caller's own dialog. */
	confirmObjectName?: string
	/** Confirm button label, e.g. "Save". */
	confirmLabel?: string
}

const WRITE_CANCELLED = 'Write cancelled.'

/** The user closed the guard dialog. Callers stay silent. */
export class WriteCancelled extends Error {
	constructor() {
		super(WRITE_CANCELLED)
		this.name = 'WriteCancelled'
	}
}

export function isWriteCancelled(err: unknown): boolean {
	if (err instanceof WriteCancelled) return true
	const message = err instanceof Error ? err.message : err
	return message === WRITE_CANCELLED
}

export async function guardLoop<T>(
	run: (confirmObjectName?: string) => Promise<T>,
	deps: {
		ask: (prompt: WriteGuardPrompt) => Promise<WriteGuardDecision>
		unlock: () => Promise<void>
	},
	options: GuardOptions = {},
): Promise<T> {
	let confirmObjectName = options.confirmObjectName
	for (;;) {
		try {
			return await run(confirmObjectName)
		} catch (e) {
			const prompt = writeGuardPrompt(e)
			if (!prompt) throw e
			const decision = await deps.ask(
				options.confirmLabel
					? { ...prompt, confirmLabel: options.confirmLabel }
					: prompt,
			)
			if (!decision.ok) throw new WriteCancelled()
			if (prompt.kind === 'unlock') {
				await deps.unlock()
				continue
			}
			confirmObjectName =
				prompt.kind === 'typeConfirm'
					? decision.typedName
					: (prompt.objectName ?? undefined)
		}
	}
}
