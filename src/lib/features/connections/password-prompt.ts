/**
 * Connect with a password prompt. A profile that does not save its password
 * fails the first connect of a session with a `password_required` error; we ask once,
 * retry with what was typed, and the backend keeps it until the app quits.
 */
import type { OpenSessionRequest } from '#core/contracts'
import { isPasswordRequired } from '#core/ipc'

export interface PasswordTarget {
	/** Profile id. One prompt per id at a time. */
	id: string
	name: string
}

/** Resolves to the typed password, or null when the user cancels. */
export type AskPassword = (target: PasswordTarget) => Promise<string | null>

/** The user closed the prompt. Callers stay silent: no toast, no error UI. */
export class ConnectCancelled extends Error {
	constructor() {
		super('Connect cancelled.')
		this.name = 'ConnectCancelled'
	}
}

export function isConnectCancelled(err: unknown): boolean {
	return err instanceof ConnectCancelled
}

/** Run `open`; on `password_required`, ask once and retry with the answer. */
export async function connectWithPassword<T>(
	request: OpenSessionRequest,
	open: (request: OpenSessionRequest) => Promise<T>,
	ask: AskPassword,
	name?: string,
): Promise<T> {
	try {
		return await open(request)
	} catch (err) {
		if (!isPasswordRequired(err)) throw err
		const id = request.profileId ?? request.sessionId ?? ''
		const password = await ask({ id, name: name?.trim() || 'this connection' })
		if (password == null) throw new ConnectCancelled()
		return open({ ...request, password })
	}
}

/**
 * One prompt on screen at a time. Callers asking for the same connection
 * share the open prompt; others wait their turn.
 */
export function createPasswordAsker(
	show: (target: PasswordTarget, done: (password: string | null) => void) => void,
): AskPassword {
	const pending = new Map<string, Promise<string | null>>()
	let queue: Promise<unknown> = Promise.resolve()
	return (target) => {
		const shared = pending.get(target.id)
		if (shared) return shared
		const answer = queue.then(
			() =>
				new Promise<string | null>((resolve) => {
					show(target, (password) => {
						const typed = password ?? ''
						resolve(typed.length > 0 ? typed : null)
					})
				}),
		)
		pending.set(target.id, answer)
		const settled = answer.finally(() => pending.delete(target.id))
		queue = settled
		return settled
	}
}
