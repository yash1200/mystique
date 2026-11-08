/**
 * ⌘/Ctrl+Shift+R. Resets at once when nothing would be lost; otherwise
 * lists the unsaved tabs and open transactions and asks first.
 */
import { hasPendingLoss, pendingLoss, type PendingLoss } from './close-guard'
import { tabEditsAreDirty } from './edit-buffer'
import { resetAppToHome } from './reset'
import { workspaceSession } from './session.svelte'

class ResetGuard {
	/** Set while the confirm is open. */
	loss = $state<PendingLoss | null>(null)

	request() {
		const tabs = workspaceSession.openTabs.map((tab) => ({
			...tab,
			dirty: tab.dirty || (tab.kind === 'table' && tabEditsAreDirty(tab.id)),
		}))
		const loss = pendingLoss(tabs, workspaceSession.tabTxn)
		if (!hasPendingLoss(loss)) {
			void resetAppToHome()
			return
		}
		this.loss = loss
	}

	confirm() {
		this.loss = null
		void resetAppToHome()
	}

	cancel() {
		this.loss = null
	}
}

export const resetGuard = new ResetGuard()
