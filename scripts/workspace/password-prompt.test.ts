import assert from 'node:assert/strict'
import { test } from 'vitest'

import type { OpenSessionRequest } from '../../src/lib/core/contracts/engines.ts'
import { mongodb } from '../../src/lib/engines/mongodb/index.ts'
import { mysql } from '../../src/lib/engines/mysql/index.ts'
import { postgres } from '../../src/lib/engines/postgres/index.ts'
import { sqlite } from '../../src/lib/engines/sqlite/index.ts'
import {
	connectWithPassword,
	createPasswordAsker,
	isConnectCancelled,
	isPasswordRequired,
	type PasswordTarget,
} from '../../src/lib/features/connections/password-prompt.ts'

const REQUIRED =
	'password_required: This connection asks for its password once per session.'

function request(): OpenSessionRequest {
	return {
		engine: 'postgres',
		pluginConfig: { host: 'db.internal' },
		profileId: '7',
		sessionId: '7',
	}
}

/** The backend: asks until a password arrives, then opens. */
function fakeOpen() {
	const calls: OpenSessionRequest[] = []
	const open = async (req: OpenSessionRequest) => {
		calls.push(req)
		if (!req.password) throw REQUIRED
		return { sessionId: req.sessionId }
	}
	return { calls, open }
}

/** A caller as the connect paths are written: toast any error but a cancel. */
async function connectLikeTheApp(
	run: () => Promise<unknown>,
	toasts: string[],
): Promise<void> {
	try {
		await run()
	} catch (err) {
		if (isConnectCancelled(err)) return
		toasts.push(String(err))
	}
}

test('C-06 every engine draft saves the password by default', () => {
	for (const adapter of [postgres, mysql, sqlite, mongodb]) {
		assert.equal(adapter.connection.emptyDraft().savePassword, true, adapter.id)
	}
})

test('C-06 password_required asks once, then retries with the typed password', async () => {
	const { calls, open } = fakeOpen()
	const asked: PasswordTarget[] = []
	const session = await connectWithPassword(
		request(),
		open,
		async (target) => {
			asked.push(target)
			return 's3cret'
		},
		'Prod DB',
	)
	assert.deepEqual(session, { sessionId: '7' })
	assert.deepEqual(asked, [{ id: '7', name: 'Prod DB' }])
	assert.equal(calls.length, 2)
	assert.equal(calls[0].password, undefined)
	assert.equal(calls[1].password, 's3cret')
	assert.equal(calls[1].profileId, '7')
})

test('C-06 cancel stops the connect without an error toast', async () => {
	const { calls, open } = fakeOpen()
	const toasts: string[] = []
	await connectLikeTheApp(
		() => connectWithPassword(request(), open, async () => null),
		toasts,
	)
	assert.deepEqual(toasts, [])
	assert.equal(calls.length, 1, 'no retry after cancel')
})

test('C-06 other connect errors pass through without a prompt', async () => {
	let asked = 0
	const toasts: string[] = []
	await connectLikeTheApp(
		() =>
			connectWithPassword(
				request(),
				async () => {
					throw new Error('connection refused')
				},
				async () => {
					asked += 1
					return 'pw'
				},
			),
		toasts,
	)
	assert.equal(asked, 0)
	assert.deepEqual(toasts, ['Error: connection refused'])
	assert.equal(isPasswordRequired(REQUIRED), true)
	assert.equal(isPasswordRequired(new Error(REQUIRED)), true)
	assert.equal(isPasswordRequired('password authentication failed'), false)
})

test('C-06 one prompt per connection; others wait their turn', async () => {
	const shown: Array<{ target: PasswordTarget; done: (pw: string | null) => void }> = []
	const ask = createPasswordAsker((target, done) => shown.push({ target, done }))

	const first = ask({ id: '1', name: 'one' })
	const again = ask({ id: '1', name: 'one' })
	const second = ask({ id: '2', name: 'two' })
	await Promise.resolve()
	assert.equal(shown.length, 1, 'same id shares the open prompt')

	shown[0].done('pw-1')
	assert.equal(await first, 'pw-1')
	assert.equal(await again, 'pw-1')
	await new Promise((r) => setTimeout(r, 0))
	assert.equal(shown.length, 2)
	assert.equal(shown[1].target.id, '2')

	shown[1].done('')
	assert.equal(await second, null, 'an empty answer counts as cancel')
})
