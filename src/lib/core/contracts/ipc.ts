/** The IPC error envelope (`mystique_connector_api::IpcError`). */
import type { SourcePosition, SshHostKeyPrompt, WriteGuardPrompt } from './engines'

export type IpcErrorKind =
	| 'invalid_config'
	| 'auth'
	| 'connect'
	| 'pool'
	| 'timeout'
	| 'not_supported'
	| 'query'
	| 'policy'
	| 'write_guard'
	| 'ssh_host_key'
	| 'password_required'
	| 'rollback_stale'
	| 'not_found'
	| 'invalid_input'
	| 'storage'
	| 'secret_store'
	| 'internal'

export type IpcPrompt =
	| ({ type: 'writeGuard' } & WriteGuardPrompt)
	| ({ type: 'sshHostKey' } & SshHostKeyPrompt)
	| { type: 'passwordRequired' }

/** A manual-commit tab's transaction state. */
export interface PinStatus {
	pinned: boolean
	inTransaction: boolean
	/** An open transaction was discarded (unpin, idle timeout, disconnect). */
	rolledBack: boolean
}

export interface IpcErrorPayload {
	kind: IpcErrorKind
	message: string
	position?: SourcePosition
	statementIndex?: number
	prompt?: IpcPrompt
	transactionOutcome?: PinStatus
}
