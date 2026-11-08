/**
 * Retry loop for writes the backend guard stopped. Plain TS so tests can
 * drive it with fake `ask` / `unlock`.
 */
import type { WriteGuardPrompt } from '#core/contracts'

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

function errorText(err: unknown): string {
	if (typeof err === 'string') return err
	if (err instanceof Error) return err.message
	if (err && typeof err === 'object' && 'message' in err) {
		return String((err as { message: unknown }).message)
	}
	return String(err)
}

export function parseWriteGuard(err: unknown): WriteGuardPrompt | null {
	const raw = errorText(err)
	const start = raw.indexOf('{')
	if (start < 0) return null
	try {
		const parsed = JSON.parse(raw.slice(start)) as WriteGuardPrompt
		if (parsed?.code === 'writeGuard' && parsed.kind) return parsed
	} catch {
		/* not JSON */
	}
	return null
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
			const prompt = parseWriteGuard(e)
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
