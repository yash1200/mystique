/**
 * What closing a tab or resetting the app would lose, and the order the
 * prompts come in. Pure so the rules have tests.
 */
import type { SaveOutcome } from './tab-save'
import type { WorkspaceTab } from './types'

/** Pinned (manual-commit) state of one query tab. */
export type TabTxn = { pinned: boolean; inTransaction: boolean }

export type CloseStep = 'transaction' | 'unsaved' | 'close'

/** A tab's manual-commit flags, read straight from the store (no async gap). */
export function tabTxnView(
	txns: Readonly<Record<string, TabTxn | undefined>>,
	tabId: string,
): { manualCommit: boolean; transactionOpen: boolean } {
	const txn = txns[tabId]
	return { manualCommit: txn?.pinned ?? false, transactionOpen: txn?.inTransaction ?? false }
}

/** An open transaction is asked about first, then unsaved edits. */
export function closeStep(
	tab: Pick<WorkspaceTab, 'kind' | 'dirty'>,
	txn: TabTxn | undefined,
): CloseStep {
	if (tab.kind === 'query' && txn?.inTransaction) return 'transaction'
	if (tab.dirty) return 'unsaved'
	return 'close'
}

/** Save, then close only when the save committed. */
export async function saveThenClose(
	save: () => Promise<SaveOutcome>,
	close: () => void,
): Promise<SaveOutcome> {
	let outcome: SaveOutcome
	try {
		outcome = await save()
	} catch (err) {
		outcome = { ok: false, error: err instanceof Error ? err.message : String(err) }
	}
	if (outcome.ok) close()
	return outcome
}

export type PendingLoss = { dirty: string[]; transactions: string[] }

/** Tabs a reset would drop. Both lists empty means no confirm is needed. */
export function pendingLoss(
	tabs: readonly Pick<WorkspaceTab, 'id' | 'kind' | 'title' | 'dirty'>[],
	txns: Readonly<Record<string, TabTxn | undefined>>,
): PendingLoss {
	return {
		dirty: tabs.filter((tab) => tab.dirty).map((tab) => tab.title),
		transactions: tabs
			.filter((tab) => tab.kind === 'query' && txns[tab.id]?.inTransaction)
			.map((tab) => tab.title),
	}
}

export function hasPendingLoss(loss: PendingLoss): boolean {
	return loss.dirty.length > 0 || loss.transactions.length > 0
}

/** `pendingLoss` where a table tab also counts its unsaved grid edits. */
export function workspaceLoss(
	tabs: readonly Pick<WorkspaceTab, 'id' | 'kind' | 'title' | 'dirty'>[],
	txns: Readonly<Record<string, TabTxn | undefined>>,
	gridEditsDirty: (tabId: string) => boolean,
): PendingLoss {
	return pendingLoss(
		tabs.map((tab) => ({
			...tab,
			dirty: tab.dirty || (tab.kind === 'table' && gridEditsDirty(tab.id)),
		})),
		txns,
	)
}
