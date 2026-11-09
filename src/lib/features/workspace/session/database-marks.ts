/**
 * Mark a database production (or clear its mark) from the explorer. The
 * backend stores the mark on the profile and its guard uses it from the
 * next write; the profile list and the open session pick up the new row.
 */
import { errorMessage } from '#core/ipc'
import { connectionProfiles, setProfileDatabaseEnvironment } from '#features/connections'
import { showToast } from '#features/toast'
import { workspaceSession } from './session.svelte'

export async function markDatabase(
	profileId: string,
	database: string,
	environment: string | null,
): Promise<void> {
	try {
		const row = await setProfileDatabaseEnvironment({
			profileId,
			database,
			schema: null,
			environment,
		})
		connectionProfiles.upsert(row)
		workspaceSession.updateProfile(row)
	} catch (e) {
		showToast('danger', errorMessage(e))
	}
}
