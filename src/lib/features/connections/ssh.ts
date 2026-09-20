import type { ConnectionFormDraft, SshHostKeyPrompt, SshTunnelRequest } from '$core/contracts'
import { trustSshHostKey } from '$core/ipc'

export function sshTunnelFromDraft(
	draft: ConnectionFormDraft,
): SshTunnelRequest | undefined {
	if (!draft.sshEnabled || !draft.sshHost?.trim()) return undefined
	return {
		host: draft.sshHost.trim(),
		port: Number(draft.sshPort) || 22,
		user: draft.sshUser?.trim() ?? '',
		auth: draft.sshAuth ?? 'password',
		password: draft.sshPassword || null,
		privateKeyPath: draft.sshPrivateKeyPath || null,
		passphrase: draft.sshPassphrase || null,
	}
}

export function parseSshHostKey(err: unknown): SshHostKeyPrompt | null {
	let raw: string
	if (typeof err === 'string') raw = err
	else if (err instanceof Error) raw = err.message
	else if (err && typeof err === 'object' && 'message' in err) {
		raw = String((err as { message: unknown }).message)
	} else return null
	const start = raw.indexOf('{')
	if (start < 0) return null
	try {
		const parsed = JSON.parse(raw.slice(start)) as SshHostKeyPrompt
		if (parsed?.code === 'sshHostKey' && parsed.fingerprint) return parsed
	} catch {
		/* not JSON */
	}
	return null
}

export async function acceptSshHostKey(
	prompt: SshHostKeyPrompt,
): Promise<void> {
	await trustSshHostKey(prompt)
}

export function invokeErrorMessage(err: unknown): string {
	if (typeof err === 'string') return err
	if (err instanceof Error) return err.message
	if (err && typeof err === 'object' && 'message' in err) {
		return String((err as { message: unknown }).message)
	}
	return String(err)
}

export function prettifyConnectError(message: string): string {
	const lower = message.toLowerCase()
	if (
		lower.includes('password missing') ||
		lower.includes('stored password is missing')
	) {
		return 'This server requires a password. Edit the connection and save one.'
	}
	return message
}
