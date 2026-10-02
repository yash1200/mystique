import type { WriteGuardPrompt } from '#core/contracts'
import { unlockEngineWrites } from '#core/ipc'

function invokeErrorMessage(err: unknown): string {
	if (typeof err === 'string') return err
	if (err instanceof Error) return err.message
	if (err && typeof err === 'object' && 'message' in err) {
		return String((err as { message: unknown }).message)
	}
	return String(err)
}

export type WriteGuardDecision = { ok: true; typedName?: string } | { ok: false }

let pending: {
	prompt: WriteGuardPrompt
	resolve: (d: WriteGuardDecision) => void
} | null = null
const listeners = new Set<() => void>()

function emit() {
	for (const fn of listeners) fn()
}

export function subscribeWriteGuard(fn: () => void): () => void {
	listeners.add(fn)
	return () => listeners.delete(fn)
}

export function currentWriteGuard(): WriteGuardPrompt | null {
	return pending?.prompt ?? null
}

export function parseWriteGuard(err: unknown): WriteGuardPrompt | null {
	const raw = invokeErrorMessage(err)
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

export function askWriteGuard(prompt: WriteGuardPrompt): Promise<WriteGuardDecision> {
	return new Promise((resolve) => {
		pending?.resolve({ ok: false })
		pending = { prompt, resolve }
		emit()
	})
}

export function resolveWriteGuard(decision: WriteGuardDecision) {
	pending?.resolve(decision)
	pending = null
	emit()
}

export async function runWithWriteGuard<T>(
	sessionId: string,
	run: (confirmObjectName?: string) => Promise<T>,
): Promise<T> {
	let confirmObjectName: string | undefined
	for (;;) {
		try {
			return await run(confirmObjectName)
		} catch (e) {
			const prompt = parseWriteGuard(e)
			if (!prompt) throw e
			const decision = await askWriteGuard(prompt)
			if (!decision.ok) {
				throw new Error('Write cancelled.')
			}
			if (prompt.kind === 'unlock') {
				await unlockEngineWrites(sessionId)
				continue
			}
			confirmObjectName = decision.typedName
		}
	}
}
