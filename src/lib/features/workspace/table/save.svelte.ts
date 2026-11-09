/**
 * Saving one table tab: the save itself (with the optional review dialog),
 * Undo / Rollback of a save by its backend id, the close / import prompt
 * waiting on a save, and the live "Show SQL" plan.
 *
 * The write guard is the session's client (`askWriteGuard`, and the backend
 * prompt `persistTableEdits` routes through `runWithWriteGuard`). The pure
 * decisions live in `save-flow.ts`.
 */
import type { TableMeta } from '#core/contracts'
import { errorMessage, unlockEngineWrites } from '#core/ipc'
import { latestRun } from '#core/util'
import { showToast } from '#features/toast'
import { askWriteGuard, isWriteCancelled, type SaveOutcome } from '#features/workspace/session'
import { appPrefs } from '#lib/app'
import { loadLatestSaveId, persistTableEdits, rollbackTableEdits } from './api'
import type { TableEdits } from './edits.svelte'
import { reviewPrompt, reviewsEverySave, savedMessage, undoSave } from './save-flow'

export type SaverContext = {
	edits: TableEdits
	table: () => TableMeta
	engine: () => string
	connectionId: () => string | null
	sessionId: () => string | null
	/** Re-open a dead engine session; returns the new session id. */
	reconnect: () => (() => Promise<string | null>) | undefined
	production: () => boolean
	connectionName: () => string
	/** False (after telling the user) on a read-only connection. */
	writable: () => boolean
	/** Commit the cell being edited, if any. */
	commitEdit: () => void
	/** Reload the page from the server; false when the grid has no loader. */
	reload: () => Promise<boolean>
	/** No loader: the saved rows become the page. */
	adoptSaved: () => void
	/** The saved baseline moved. */
	baselineChanged: () => void
	report: (err: unknown) => void
}

export class TableSaver {
	saving = $state(false)
	/** Backend id of the save the Rollback button undoes. */
	lastSaveId = $state<string | null>(null)
	/** Live "Show SQL" panel. */
	sqlStatements = $state<string[]>([])
	sqlError = $state<string | null>(null)
	/** Waiting close / import prompt. Settled when the save commits, fails, or is cancelled. */
	#waiter: ((outcome: SaveOutcome) => void) | null = null
	/** Connection + table the latest-save lookup belongs to. */
	#key = ''
	readonly #ctx: SaverContext

	readonly #sqlPlan = latestRun({
		delayMs: 300,
		run: async () => {
			const sid = this.#ctx.sessionId() ?? ''
			if (this.#ctx.edits.dirtyCount === 0 || !sid) return []
			const planned = await persistTableEdits(this.#payload(sid, true))
			return planned.statements ?? []
		},
		onResult: (statements) => {
			this.sqlStatements = statements
			this.sqlError = null
		},
		onError: (err) => {
			this.sqlError = errorMessage(err)
		},
	})

	constructor(ctx: SaverContext) {
		this.#ctx = ctx
		$effect(() => {
			if (!appPrefs.gridSqlPanel) return
			// Replan whenever the pending edits change.
			void ctx.edits.rows
			void ctx.edits.dirtyKeys
			void ctx.edits.newRows
			void ctx.edits.deleted
			this.#sqlPlan.schedule()
			return () => this.#sqlPlan.cancel()
		})
	}

	#payload(sessionId: string, dryRun: boolean) {
		return {
			sessionId,
			engine: this.#ctx.engine(),
			table: this.#ctx.table(),
			...this.#ctx.edits.payload(),
			dryRun,
		}
	}

	async #liveSessionId(): Promise<string> {
		let sid = this.#ctx.sessionId()
		const reconnect = this.#ctx.reconnect()
		if (!sid && reconnect) sid = await reconnect()
		return sid ?? ''
	}

	#settle(outcome: SaveOutcome) {
		const waiter = this.#waiter
		this.#waiter = null
		waiter?.(outcome)
	}

	/** Save for a close or import prompt; resolves when the save settles. */
	saveForPrompt(): Promise<SaveOutcome> {
		this.#settle({ ok: false, error: null })
		return new Promise((resolve) => {
			this.#waiter = resolve
			void this.save()
		})
	}

	/** The grid unmounts: a waiting prompt counts as cancelled. */
	dispose() {
		this.#settle({ ok: false, error: null })
	}

	/** "Review before saving: always": the one dialog, shown before the save. */
	async #review(sid: string): Promise<'save' | 'empty' | 'cancel'> {
		const planned = await persistTableEdits(this.#payload(sid, true))
		const statements = planned.statements ?? []
		if (statements.length === 0) return 'empty'
		const production = this.#ctx.production()
		const decision = await askWriteGuard(
			reviewPrompt({ production, connectionName: this.#ctx.connectionName(), statements }),
		)
		if (!decision.ok) return 'cancel'
		// Production: this dialog is also the unlock, so the backend asks nothing more.
		if (production) await unlockEngineWrites(sid)
		return 'save'
	}

	/** Saves straight away. Production asks once per session (the backend guard). */
	async save() {
		if (this.saving) return
		const { edits } = this.#ctx
		if (!this.#ctx.writable()) {
			this.#settle({ ok: false, error: 'Connection is read-only' })
			return
		}
		this.#ctx.commitEdit()
		if (edits.dirtyCount === 0) {
			this.#settle({ ok: true })
			return
		}
		this.saving = true
		try {
			const sid = await this.#liveSessionId()
			if (reviewsEverySave(appPrefs.reviewBeforeSaving)) {
				const step = await this.#review(sid)
				if (step !== 'save') {
					this.#settle(step === 'empty' ? { ok: true } : { ok: false, error: null })
					return
				}
			}
			const pendingDeletes = edits.pendingDeletes()
			// Do not retry after a dropped connection: the save may have landed.
			const result = await persistTableEdits(this.#payload(sid, false))
			if (pendingDeletes > 0 && (result.deleted ?? 0) < pendingDeletes) {
				throw new Error('Delete did not apply. Restart Mystique and try the delete again.')
			}
			this.lastSaveId = result.saveId ?? null
			edits.clearHistory()
			if (!(await this.#ctx.reload())) this.#ctx.adoptSaved()
			const saveId = result.saveId
			showToast(
				'success',
				savedMessage(result),
				saveId ? { action: { label: 'Undo', run: () => this.rollback(saveId) } } : undefined,
			)
			this.#ctx.baselineChanged()
			this.#settle({ ok: true })
		} catch (err) {
			if (isWriteCancelled(err)) {
				this.#settle({ ok: false, error: null })
			} else {
				this.#ctx.report(err)
				this.#settle({ ok: false, error: errorMessage(err) })
			}
		} finally {
			this.saving = false
		}
	}

	/** Undo one save by its backend id (Rollback button and the Undo toast). */
	async rollback(saveId: string) {
		if (this.saving) return
		if (!this.#ctx.writable()) return
		this.saving = true
		try {
			const sid = await this.#liveSessionId()
			const outcome = await undoSave(
				() => rollbackTableEdits({ sessionId: sid, saveId }),
				isWriteCancelled,
			)
			// A stale or used save is gone on the backend; offer the next one.
			this.refreshLatest()
			if (!outcome.ok) {
				if (outcome.message) showToast('danger', outcome.message)
				return
			}
			if (this.lastSaveId === saveId) this.lastSaveId = null
			this.#ctx.edits.clearHistory()
			await this.#ctx.reload()
			showToast('success', 'Rolled back the save')
			this.#ctx.baselineChanged()
		} catch (err) {
			this.#ctx.report(err)
		} finally {
			this.saving = false
		}
	}

	/** Another connection or table: forget the last save and look up its latest one. */
	forTable(key: string) {
		this.#key = key
		this.lastSaveId = null
		this.refreshLatest()
	}

	/** Ask the backend which save Rollback would undo now. */
	refreshLatest() {
		const id = this.#ctx.connectionId()
		if (!id) return
		const key = this.#key
		void loadLatestSaveId(id, this.#ctx.table())
			.then((saveId) => {
				if (this.#key !== key) return
				this.lastSaveId = saveId
			})
			.catch(() => {})
	}
}
