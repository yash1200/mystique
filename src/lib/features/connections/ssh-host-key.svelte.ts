/**
 * The SSH host-key prompt. A connect or test that reaches an unknown (or
 * changed) bastion key hands the key here with a retry; `SshHostKeyModal`
 * renders it once, app-wide. Trusting the key stores it and runs the retry.
 */
import type { SshHostKeyPrompt } from '#core/contracts'
import { errorMessage, sshHostKeyPrompt, trustSshHostKey } from '#core/ipc'

class SshHostKeyStore {
	prompt = $state<SshHostKeyPrompt | null>(null)
	busy = $state(false)
	error = $state<string | null>(null)
	#retry: (() => unknown) | null = null

	/**
	 * Show the prompt when `source` carries one: a test result's `sshHostKey`
	 * or a thrown `IpcError`. True when it did, so the caller stops there.
	 */
	offer = (source: unknown, retry: () => unknown): boolean => {
		const prompt = promptOf(source)
		if (!prompt) return false
		this.prompt = prompt
		this.error = null
		this.#retry = retry
		return true
	}

	cancel = () => {
		if (this.busy) return
		this.#clear()
	}

	accept = async () => {
		const prompt = this.prompt
		if (!prompt || this.busy) return
		this.busy = true
		this.error = null
		try {
			await trustSshHostKey(prompt)
		} catch (err) {
			this.error = errorMessage(err)
			return
		} finally {
			this.busy = false
		}
		const retry = this.#retry
		this.#clear()
		await retry?.()
	}

	#clear() {
		this.prompt = null
		this.error = null
		this.#retry = null
	}
}

function promptOf(source: unknown): SshHostKeyPrompt | null {
	if (source && typeof source === 'object' && 'sshHostKey' in source) {
		return (source as { sshHostKey?: SshHostKeyPrompt | null }).sshHostKey ?? null
	}
	return sshHostKeyPrompt(source)
}

export const sshHostKeys = new SshHostKeyStore()
