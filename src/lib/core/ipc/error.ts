/**
 * The one IPC error envelope (`mystique_connector_api::IpcError`).
 *
 * Every command rejects with `{ kind, message, position?, statementIndex?,
 * prompt?, transactionOutcome? }`. Code branches on `kind` and `prompt`,
 * never on the message text. `message` is human text: show it as is.
 */
import type {
	IpcErrorKind,
	IpcErrorPayload,
	IpcPrompt,
	PinStatus,
	SourcePosition,
	SshHostKeyPrompt,
	WriteGuardPrompt,
} from '../contracts'

export type { IpcErrorKind, IpcErrorPayload, IpcPrompt }

/**
 * An `Error` so stack traces, `instanceof Error` and `String(err)` keep
 * working in code that has not moved to `errorMessage`.
 */
export class IpcError extends Error implements IpcErrorPayload {
	readonly kind: IpcErrorKind
	readonly position?: SourcePosition
	readonly statementIndex?: number
	readonly prompt?: IpcPrompt
	readonly transactionOutcome?: PinStatus

	constructor(payload: IpcErrorPayload) {
		super(payload.message)
		this.name = 'IpcError'
		this.kind = payload.kind
		if (payload.position) this.position = payload.position
		if (typeof payload.statementIndex === 'number') this.statementIndex = payload.statementIndex
		if (payload.prompt) this.prompt = payload.prompt
		if (payload.transactionOutcome) this.transactionOutcome = payload.transactionOutcome
	}

	override toString(): string {
		return this.message
	}
}

function isPosition(value: unknown): value is SourcePosition {
	if (!value || typeof value !== 'object') return false
	const pos = value as Record<string, unknown>
	return (
		typeof pos.offset === 'number' &&
		typeof pos.line === 'number' &&
		typeof pos.column === 'number'
	)
}

function isPayload(value: unknown): value is IpcErrorPayload {
	if (!value || typeof value !== 'object') return false
	const record = value as Record<string, unknown>
	return typeof record.kind === 'string' && typeof record.message === 'string'
}

/**
 * Any rejection as an `IpcError`. The envelope passes through; a plain
 * `Error` or string (a frontend throw, an old backend) becomes `internal`
 * with its text as the message.
 */
export function toIpcError(err: unknown): IpcError {
	if (err instanceof IpcError) return err
	if (isPayload(err)) {
		const record = err as IpcErrorPayload
		return new IpcError({
			kind: record.kind,
			message: record.message,
			position: isPosition(record.position) ? record.position : undefined,
			statementIndex:
				typeof record.statementIndex === 'number' ? record.statementIndex : undefined,
			prompt: record.prompt ?? undefined,
			transactionOutcome: record.transactionOutcome ?? undefined,
		})
	}
	if (err instanceof Error) return new IpcError({ kind: 'internal', message: err.message })
	if (typeof err === 'string') return new IpcError({ kind: 'internal', message: err })
	if (err && typeof err === 'object' && typeof (err as { message?: unknown }).message === 'string') {
		return new IpcError({ kind: 'internal', message: (err as { message: string }).message })
	}
	return new IpcError({ kind: 'internal', message: err == null ? 'Unknown error' : String(err) })
}

/** Text to show for any thrown value. */
export function errorMessage(err: unknown): string {
	return toIpcError(err).message
}

/** The error's kind, or `internal` for anything that is not an envelope. */
export function errorKind(err: unknown): IpcErrorKind {
	return toIpcError(err).kind
}

/** The write-guard dialog this error asks for, if any. */
export function writeGuardPrompt(err: unknown): WriteGuardPrompt | null {
	const prompt = toIpcError(err).prompt
	if (prompt?.type !== 'writeGuard') return null
	const { type: _type, ...rest } = prompt
	return rest
}

/** The SSH host key this error asks the user to trust, if any. */
export function sshHostKeyPrompt(err: unknown): SshHostKeyPrompt | null {
	const prompt = toIpcError(err).prompt
	if (prompt?.type !== 'sshHostKey') return null
	const { type: _type, ...rest } = prompt
	return rest
}

/** The profile does not save its password and none is held this session. */
export function isPasswordRequired(err: unknown): boolean {
	const e = toIpcError(err)
	return e.kind === 'password_required' || e.prompt?.type === 'passwordRequired'
}

/** A policy refused the request (read-only, Read mode, …). */
export function isPolicyError(err: unknown): boolean {
	return toIpcError(err).kind === 'policy'
}
