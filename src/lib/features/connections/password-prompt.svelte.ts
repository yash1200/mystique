/** The connect password prompt. `PasswordPromptModal` renders it once, app-wide. */

import type {
	OpenSessionRequest,
	SessionInfo,
	TestResult,
} from '#core/contracts'
import {
	ensureEngineSession,
	openEngineSession,
	testEngineConnection,
} from '#core/ipc'
import {
	connectWithPassword,
	createPasswordAsker,
	type PasswordTarget,
} from './password-prompt'

class PasswordPromptStore {
	target = $state<PasswordTarget | null>(null)
	#done: ((password: string | null) => void) | null = null

	ask = createPasswordAsker((target, done) => {
		this.target = target
		this.#done = done
	})

	submit = (password: string) => this.#finish(password)

	cancel = () => this.#finish(null)

	#finish(password: string | null) {
		const done = this.#done
		this.#done = null
		this.target = null
		done?.(password)
	}
}

export const passwordPrompt = new PasswordPromptStore()

/** `ensureEngineSession`, asking for the password when the profile does not save it. */
export function ensureSessionWithPassword(
	request: OpenSessionRequest,
	name?: string,
): Promise<SessionInfo> {
	return connectWithPassword(request, ensureEngineSession, passwordPrompt.ask, name)
}

/** `openEngineSession` (always a fresh pool), with the same prompt. */
export function openSessionWithPassword(
	request: OpenSessionRequest,
	name?: string,
): Promise<SessionInfo> {
	return connectWithPassword(request, openEngineSession, passwordPrompt.ask, name)
}

/** The pre-connect test. A typed password that passes is kept for the session. */
export function testSessionWithPassword(
	request: OpenSessionRequest,
	name?: string,
): Promise<TestResult> {
	return connectWithPassword(request, testEngineConnection, passwordPrompt.ask, name)
}
