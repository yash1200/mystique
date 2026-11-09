/** Text for the one write dialog. Plain TS so tests can read it. */
import type { WriteGuardPrompt } from '#core/contracts'

export type GuardCopy = {
	title: string
	description: string
	confirm: string
	/** Type-to-confirm input is shown. */
	typed: boolean
	/** SQL to show; falls back to the short preview. */
	body: string
	/** Production only: an UPDATE / DELETE that changes every row. Not a block. */
	warnings: string[]
}

function envLabel(environment: string): string {
	const env = environment.trim().toLowerCase()
	if (env === 'production') return 'Production'
	if (env === 'staging') return 'Staging'
	return 'Local'
}

export function guardCopy(prompt: WriteGuardPrompt): GuardCopy {
	const env = envLabel(prompt.environment)
	const production = env === 'Production'
	const where = `“${prompt.connectionName}” is ${env.toLowerCase()}.`
	const statements = prompt.statements ?? []
	const body = statements.length > 0 ? statements.join(';\n') : prompt.preview
	const name = prompt.objectName?.trim() || ''
	const unlockNote = 'Writes stay allowed until you disconnect.'
	const warnings = prompt.warnings ?? []
	if (prompt.kind === 'typeConfirm') {
		return {
			title: prompt.title ?? `Confirm ${env} write`,
			description: `${where} This cannot be undone. Type “${name}” to continue.${production ? ` ${unlockNote}` : ''}`,
			confirm: prompt.confirmLabel ?? 'Run',
			typed: true,
			body,
			warnings,
		}
	}
	if (prompt.kind === 'unlock') {
		return {
			title: prompt.title ?? `Allow writes on ${env}`,
			description: `${where} ${unlockNote}`,
			confirm: prompt.confirmLabel ?? 'Allow writes this session',
			typed: false,
			body,
			warnings,
		}
	}
	if (prompt.kind === 'review') {
		return {
			title: prompt.title ?? (production ? 'Save to Production' : 'Save changes'),
			description: production ? `${where} ${unlockNote}` : where,
			confirm: prompt.confirmLabel ?? 'Save',
			typed: false,
			body,
			warnings,
		}
	}
	return {
		title: prompt.title ?? `Run on ${env}`,
		description: `${where} This cannot be undone.`,
		confirm: prompt.confirmLabel ?? 'Run',
		typed: false,
		body,
		warnings,
	}
}

/** Typed text matches the expected name (case-insensitive, trimmed). */
export function typedNameOk(prompt: WriteGuardPrompt, typed: string): boolean {
	if (prompt.kind !== 'typeConfirm') return true
	const expected = prompt.objectName?.trim().toLowerCase() ?? ''
	return typed.trim().toLowerCase() === expected
}
