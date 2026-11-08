import type { ConnectionFormDraft, SshHostKeyPrompt, SshTunnelRequest } from '#core/contracts'
import { errorMessage, isPasswordRequired, trustSshHostKey } from '#core/ipc'

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

export async function acceptSshHostKey(
	prompt: SshHostKeyPrompt,
): Promise<void> {
	await trustSshHostKey(prompt)
}

/** A failed connect or test, worded for the toast. */
export function connectErrorMessage(err: unknown): string {
	if (isPasswordRequired(err)) {
		return 'Enter the password. This connection does not save it.'
	}
	return prettifyConnectError(errorMessage(err))
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
