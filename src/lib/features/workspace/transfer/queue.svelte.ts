/**
 * Background transfers: exports and imports run off the dialog, one at a
 * time per connection, with live row counts and cancel. Finished ones stay
 * listed until dismissed so the result (or the error) is not lost.
 *
 * Pure scheduling: the work is a function the caller hands in, so the
 * rules have tests without IPC.
 */

export type TransferKind = 'export' | 'import'
export type TransferStatus = 'queued' | 'running' | 'done' | 'failed' | 'cancelled'

export interface TransferTask {
	id: string
	kind: TransferKind
	connectionId: string
	/** "users → users.csv" */
	label: string
	/** Rows written so far. */
	rows: number
	status: TransferStatus
	/** Result line for a finished task, or the error. */
	message: string | null
	/** Includes the tab's uncommitted changes. */
	throughTransaction: boolean
}

export type TransferOutcome = { rows: number; message?: string | null }

/** The work. `executionId` is the cancel token; `onRows` reports progress. */
export type TransferRun = (
	executionId: string,
	onRows: (rows: number) => void,
) => Promise<TransferOutcome>

type Pending = {
	run: TransferRun
	executionId: string
	resolve: (task: TransferTask) => void
}

export type TransferQueueDeps = {
	/** Stop a running transfer by its execution id. */
	cancel: (executionId: string) => Promise<void>
	newId?: () => string
}

export class TransferQueue {
	tasks = $state<TransferTask[]>([])
	readonly #pending = new Map<string, Pending>()
	readonly #deps: TransferQueueDeps

	constructor(deps: TransferQueueDeps) {
		this.#deps = deps
	}

	/** Queued or running. */
	get active(): TransferTask[] {
		return this.tasks.filter((t) => t.status === 'queued' || t.status === 'running')
	}

	get hasActive(): boolean {
		return this.active.length > 0
	}

	/**
	 * Add a transfer. It starts now when its connection is free, else after
	 * the ones before it. The promise settles when it finishes, fails or is
	 * cancelled; it never rejects.
	 */
	enqueue(
		spec: Pick<TransferTask, 'kind' | 'connectionId' | 'label'> & {
			throughTransaction?: boolean
		},
		run: TransferRun,
	): { id: string; done: Promise<TransferTask> } {
		const id = this.#newId()
		const task: TransferTask = {
			id,
			kind: spec.kind,
			connectionId: spec.connectionId,
			label: spec.label,
			rows: 0,
			status: 'queued',
			message: null,
			throughTransaction: !!spec.throughTransaction,
		}
		let resolve!: (task: TransferTask) => void
		const done = new Promise<TransferTask>((r) => (resolve = r))
		this.#pending.set(id, { run, executionId: this.#newId(), resolve })
		this.tasks = [...this.tasks, task]
		this.#pump(spec.connectionId)
		return { id, done }
	}

	/** Cancel a queued task outright, or ask the backend to stop a running one. */
	async cancel(id: string): Promise<void> {
		const task = this.#find(id)
		const pending = this.#pending.get(id)
		if (!task || !pending) return
		if (task.status === 'queued') {
			this.#finish(id, 'cancelled', 'Cancelled before it started.')
			this.#pump(task.connectionId)
			return
		}
		if (task.status === 'running') {
			try {
				await this.#deps.cancel(pending.executionId)
			} catch {
				/* the run may already have finished */
			}
		}
	}

	dismiss(id: string) {
		this.tasks = this.tasks.filter((t) => t.id !== id || isActive(t))
	}

	clearFinished() {
		this.tasks = this.tasks.filter(isActive)
	}

	#newId(): string {
		return this.#deps.newId ? this.#deps.newId() : crypto.randomUUID()
	}

	#find(id: string): TransferTask | undefined {
		return this.tasks.find((t) => t.id === id)
	}

	#patch(id: string, patch: Partial<TransferTask>) {
		this.tasks = this.tasks.map((t) => (t.id === id ? { ...t, ...patch } : t))
	}

	#finish(id: string, status: TransferStatus, message: string | null, rows?: number) {
		this.#patch(id, { status, message, ...(rows === undefined ? {} : { rows }) })
		const pending = this.#pending.get(id)
		this.#pending.delete(id)
		const task = this.#find(id)
		if (pending && task) pending.resolve(task)
	}

	/** Start the connection's next queued task when none of its tasks runs. */
	#pump(connectionId: string) {
		const mine = this.tasks.filter((t) => t.connectionId === connectionId)
		if (mine.some((t) => t.status === 'running')) return
		const next = mine.find((t) => t.status === 'queued')
		if (!next) return
		const pending = this.#pending.get(next.id)
		if (!pending) return
		this.#patch(next.id, { status: 'running' })
		void this.#start(next.id, connectionId, pending)
	}

	async #start(id: string, connectionId: string, pending: Pending) {
		try {
			const outcome = await pending.run(pending.executionId, (rows) => {
				const task = this.#find(id)
				if (task?.status === 'running' && rows > task.rows) this.#patch(id, { rows })
			})
			this.#finish(id, 'done', outcome.message ?? null, outcome.rows)
		} catch (err) {
			const message = err instanceof Error ? err.message : String(err)
			const cancelled = /cancel/i.test(message)
			this.#finish(id, cancelled ? 'cancelled' : 'failed', message)
		} finally {
			this.#pump(connectionId)
		}
	}
}

function isActive(t: TransferTask): boolean {
	return t.status === 'queued' || t.status === 'running'
}
