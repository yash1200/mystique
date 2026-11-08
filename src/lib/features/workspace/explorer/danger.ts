/**
 * Drop / truncate from the explorer menu: exactly one dialog.
 * Local and staging: this dialog, then the run carries the name so the
 * backend asks nothing more. Production: no dialog here; the backend's
 * type-to-confirm is the one dialog (and it unlocks the session).
 */
import type { WriteGuardPrompt } from '#core/contracts'

export type DangerAction = 'drop' | 'truncate'

export function menuDangerPrompt(input: {
	action: DangerAction
	name: string
	noun: string
	environment: string
	connectionName: string
}): WriteGuardPrompt | null {
	if (input.environment.trim().toLowerCase() === 'production') return null
	const drop = input.action === 'drop'
	return {
		code: 'writeGuard',
		kind: 'confirm',
		environment: input.environment,
		connectionName: input.connectionName,
		title: `${drop ? 'Drop' : 'Truncate'} ${input.noun}`,
		preview: drop
			? `Drop “${input.name}”.`
			: `Delete every row in “${input.name}”.`,
		objectName: input.name,
		confirmLabel: drop ? 'Drop' : 'Truncate',
	}
}
