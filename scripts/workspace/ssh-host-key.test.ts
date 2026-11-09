import { expect, test, vi } from 'vitest'
import { sshHostKeys } from '../../src/lib/features/connections/ssh-host-key.svelte'
import { mockIpc } from './mock-ipc'

const key = {
	kind: 'unknown',
	host: 'bastion',
	port: 22,
	keyType: 'ssh-ed25519',
	fingerprint: 'abc',
} as const

test('a test result with a host key opens the prompt; trusting it retries', async () => {
	const calls = mockIpc({ trust_ssh_host_key: () => null })
	const retry = vi.fn()
	expect(sshHostKeys.offer({ ok: false, sshHostKey: null }, retry)).toBe(false)
	expect(sshHostKeys.offer({ ok: false, sshHostKey: key }, retry)).toBe(true)
	expect(sshHostKeys.prompt).toEqual(key)
	await sshHostKeys.accept()
	expect(calls).toEqual([{ command: 'trust_ssh_host_key', args: { prompt: key } }])
	expect(retry).toHaveBeenCalledTimes(1)
	expect(sshHostKeys.prompt).toBeNull()
})

test('an error carrying the prompt opens it too; a failed trust keeps it open', async () => {
	mockIpc({
		trust_ssh_host_key: () => {
			throw { kind: 'internal', message: 'disk full' }
		},
	})
	const retry = vi.fn()
	const err = { kind: 'sshHostKey', message: 'trust it', prompt: { type: 'sshHostKey', ...key } }
	expect(sshHostKeys.offer(err, retry)).toBe(true)
	await sshHostKeys.accept()
	expect(retry).not.toHaveBeenCalled()
	expect(sshHostKeys.prompt).toEqual(key)
	expect(sshHostKeys.error).toContain('disk full')
	sshHostKeys.cancel()
	expect(sshHostKeys.prompt).toBeNull()
	expect(sshHostKeys.error).toBeNull()
})
